use anyhow::{Context, Result, ensure};
use ptts::{
    transformer::LayerAttentionState,
    tts_model::{TTSConfig, TTSModel, TTSState},
};
use safetensors::{Dtype, SafeTensors, tensor::TensorView};
use std::{collections::HashMap, path::Path};
use xn::{BackendQ, CpuDevice, Tensor};

/// Current official state, immutable and independent of mutable request caches.
pub struct OfficialVoice {
    pub offset: usize,
    pub heads: usize,
    pub head_dim: usize,
    pub metadata: HashMap<String, String>,
    layers: Vec<(Vec<f32>, Vec<f32>)>,
}
fn scalar_i64(t: TensorView<'_>, name: &str) -> Result<i64> {
    ensure!(
        t.dtype() == Dtype::I64 && t.shape() == [1],
        "{name}: expected I64 [1]"
    );
    Ok(i64::from_le_bytes(t.data().try_into()?))
}

impl OfficialVoice {
    pub fn byte_size(&self) -> usize {
        self.layers
            .iter()
            .map(|(k, v)| (k.len() + v.len()) * std::mem::size_of::<f32>())
            .sum()
    }
    pub fn load(path: &Path, cfg: &TTSConfig) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("read voice {}", path.display()))?;
        Self::from_bytes(&bytes, cfg)
    }
    pub fn from_bytes(bytes: &[u8], cfg: &TTSConfig) -> Result<Self> {
        let st = SafeTensors::deserialize(bytes).context("validate official voice safetensors")?;
        let (_, header) = SafeTensors::read_metadata(bytes)?;
        let metadata = header.metadata().clone().unwrap_or_default();
        if let Some(language) = metadata.get("language") {
            ensure!(language == "german", "voice metadata language mismatch");
        }
        if let (Some(expected), Some(recorded)) = (cfg.model_ext(), metadata.get("model_ext")) {
            ensure!(&expected == recorded, "voice metadata model_ext mismatch");
        }
        let f = &cfg.flow_lm;
        ensure!(
            f.num_heads > 0 && f.d_model.is_multiple_of(f.num_heads),
            "invalid head geometry"
        );
        ensure!(
            st.names().len() == 3 * f.num_layers,
            "expected exactly cache/offset/pad per layer; incompatible voice-state schema"
        );
        let mut offset = None;
        let mut layers = Vec::with_capacity(f.num_layers);
        for i in 0..f.num_layers {
            let prefix = format!("transformer.layers.{i}.self_attn");
            let pos = scalar_i64(st.tensor(&format!("{prefix}/offset"))?, "offset")?;
            let pad = scalar_i64(st.tensor(&format!("{prefix}/pad"))?, "pad")?;
            ensure!(pos > 0, "voice offset must be positive");
            // Juergen is a single unpadded row. Never silently reinterpret batched/padded states.
            ensure!(
                pad == 0,
                "padded/batched voice state unsupported in this Juergen milestone"
            );
            let pos = usize::try_from(pos)?;
            if let Some(previous) = offset {
                ensure!(previous == pos, "cache offsets differ between layers");
            }
            offset = Some(pos);
            let cache = st.tensor(&format!("{prefix}/cache"))?;
            let shape = cache.shape();
            ensure!(
                cache.dtype() == Dtype::F32 && shape.len() == 5,
                "{prefix}: expected F32 [2,1,T,H,D]"
            );
            ensure!(
                shape[0] == 2
                    && shape[1] == 1
                    && shape[3] == f.num_heads
                    && shape[4] == f.d_model / f.num_heads,
                "{prefix}: incompatible KV shape {shape:?}"
            );
            ensure!(pos <= shape[2], "{prefix}: offset beyond allocated cache");
            let stride = shape[2] * f.d_model * 4;
            let valid_bytes = pos * f.d_model * 4;
            let read = |b: &[u8]| -> Result<Vec<f32>> {
                let values: Vec<_> = b
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                ensure!(
                    values.iter().all(|x| x.is_finite()),
                    "non-finite valid voice cache at layer {i}"
                );
                Ok(values)
            };
            // Unused capacity may contain upstream NaN sentinels; only valid prefix is imported.
            layers.push((
                read(&cache.data()[..valid_bytes])?,
                read(&cache.data()[stride..stride + valid_bytes])?,
            ));
        }
        Ok(Self {
            offset: offset.context("empty voice state")?,
            heads: f.num_heads,
            head_dim: f.d_model / f.num_heads,
            metadata,
            layers,
        })
    }
    pub fn new_state<Q: BackendQ<T = f32, B = CpuDevice>>(
        &self,
        model: &TTSModel<Q>,
        capacity: usize,
    ) -> Result<TTSState<Q>> {
        ensure!(
            capacity >= self.offset,
            "request cache too small for voice prefix"
        );
        let mut state = model.init_flow_lm_state(1, capacity)?;
        ensure!(
            state.flow_lm_state.transformer_state.layer_states.len() == self.layers.len(),
            "voice/model layer count mismatch"
        );
        for (layer, (keys, values)) in state
            .flow_lm_state
            .transformer_state
            .layer_states
            .iter_mut()
            .zip(&self.layers)
        {
            let LayerAttentionState::FlowLm(layer) = layer else {
                anyhow::bail!("voice state requires FlowLM attention")
            };
            let shape = (1, self.offset, self.heads, self.head_dim);
            layer
                .k_cache
                .slice_set(&Tensor::from_vec(keys.clone(), shape, &CpuDevice)?, 1, 0)?;
            layer
                .v_cache
                .slice_set(&Tensor::from_vec(values.clone(), shape, &CpuDevice)?, 1, 0)?;
            layer.current_end = self.offset;
        }
        // No prompt_audio call and no BOS insertion: cache is already fully prepared.
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::tensor::{TensorView, serialize};
    fn fixture(offset: i64, pad: i64, shape: Vec<usize>, values: Vec<f32>) -> Vec<u8> {
        let f: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
        let offset_bytes = offset.to_le_bytes();
        let pad_bytes = pad.to_le_bytes();
        let views = vec![
            (
                "transformer.layers.0.self_attn/cache",
                TensorView::new(Dtype::F32, shape, &f).unwrap(),
            ),
            (
                "transformer.layers.0.self_attn/offset",
                TensorView::new(Dtype::I64, vec![1], &offset_bytes).unwrap(),
            ),
            (
                "transformer.layers.0.self_attn/pad",
                TensorView::new(Dtype::I64, vec![1], &pad_bytes).unwrap(),
            ),
        ];
        serialize(views, None).unwrap()
    }
    fn cfg() -> TTSConfig {
        let mut cfg = TTSConfig::v202601();
        cfg.flow_lm.num_layers = 1;
        cfg.flow_lm.num_heads = 1;
        cfg.flow_lm.d_model = 2;
        cfg
    }
    #[test]
    fn validates_cache_and_position_and_ignores_unused_nan_capacity() {
        let good = fixture(
            1,
            0,
            vec![2, 1, 2, 1, 2],
            vec![1., 2., f32::NAN, f32::NAN, 3., 4., f32::NAN, f32::NAN],
        );
        let voice = OfficialVoice::from_bytes(&good, &cfg()).unwrap();
        assert_eq!(voice.layers[0], (vec![1., 2.], vec![3., 4.]));
        assert_eq!(voice.offset, 1);
        assert!(
            OfficialVoice::from_bytes(&fixture(3, 0, vec![2, 1, 2, 1, 2], vec![0.; 8]), &cfg())
                .is_err()
        );
        assert!(
            OfficialVoice::from_bytes(&fixture(1, 1, vec![2, 1, 2, 1, 2], vec![0.; 8]), &cfg())
                .is_err()
        );
        assert!(
            OfficialVoice::from_bytes(
                &fixture(1, 0, vec![2, 1, 2, 1, 2], vec![f32::NAN; 8]),
                &cfg()
            )
            .is_err()
        );
        assert!(
            OfficialVoice::from_bytes(&fixture(1, 0, vec![2, 1, 2, 2, 1], vec![0.; 8]), &cfg())
                .is_err()
        );
    }
}
