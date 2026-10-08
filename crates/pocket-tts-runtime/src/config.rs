use anyhow::{Context, Result, ensure};
use ptts::{flow_lm::FlowLMConfig, mimi::MimiConfig, tts_model::TTSConfig};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GermanConfig {
    pub weights_path: String,
    pub weights_path_without_voice_cloning: String,
    #[serde(default)]
    pub remove_semicolons: bool,
    #[serde(default)]
    pub replace_characters: BTreeMap<char, String>,
    #[serde(default)]
    pub pad_with_spaces_for_short_inputs: bool,
    #[serde(default = "yes")]
    pub append_terminal_punctuation: bool,
    #[serde(default = "yes")]
    pub capitalize_first_letter: bool,
    #[serde(default)]
    pub model_recommended_frames_after_eos: Option<usize>,
    pub flow_lm: FlowYaml,
    pub mimi: MimiYaml,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowYaml {
    pub insert_bos_before_voice: bool,
    pub dtype: String,
    pub flow: FlowNetYaml,
    pub transformer: FlowTransformerYaml,
    pub lookup_table: LookupYaml,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowNetYaml {
    pub depth: usize,
    pub dim: usize,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTransformerYaml {
    pub d_model: usize,
    pub hidden_scale: usize,
    pub max_period: f32,
    pub num_heads: usize,
    pub num_layers: usize,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupYaml {
    pub dim: usize,
    pub n_bins: usize,
    pub tokenizer: String,
    pub tokenizer_path: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MimiYaml {
    pub dtype: String,
    pub sample_rate: usize,
    pub inner_dim: usize,
    pub outer_dim: usize,
    pub channels: usize,
    pub frame_rate: f64,
    pub seanet: SeanetYaml,
    pub transformer: MimiTransformerYaml,
    pub quantizer: QuantizerYaml,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeanetYaml {
    pub dimension: usize,
    pub channels: usize,
    pub n_filters: usize,
    pub n_residual_layers: usize,
    pub ratios: Vec<usize>,
    pub kernel_size: usize,
    pub residual_kernel_size: usize,
    pub last_kernel_size: usize,
    pub dilation_base: usize,
    pub pad_mode: String,
    pub compress: usize,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MimiTransformerYaml {
    pub d_model: usize,
    pub num_heads: usize,
    pub num_layers: usize,
    pub layer_scale: f64,
    pub context: usize,
    pub dim_feedforward: usize,
    pub input_dimension: usize,
    pub output_dimensions: Vec<usize>,
    #[serde(default = "max_period")]
    pub max_period: f32,
}
fn max_period() -> f32 {
    10000.0
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantizerYaml {
    pub dimension: usize,
    pub output_dimension: usize,
}

impl GermanConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let cfg: Self = serde_yaml::from_str(&std::fs::read_to_string(path)?)
            .with_context(|| format!("parse current German YAML {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.flow_lm.dtype == "float32" && self.mimi.dtype == "float32",
            "reference requires FP32 compute"
        );
        ensure!(
            self.flow_lm.lookup_table.tokenizer == "tokenizers",
            "requires official JSON tokenizer"
        );
        let f = &self.flow_lm.transformer;
        ensure!(
            f.num_layers == 6 && f.d_model == 1024 && f.num_heads == 16,
            "this milestone supports current german (6 layers), not another preset"
        );
        ensure!(
            self.mimi.sample_rate == 24000
                && self.mimi.channels == 1
                && self.mimi.frame_rate == 12.5,
            "requires native 24kHz mono, 12.5Hz codec"
        );
        ensure!(
            self.mimi.inner_dim == 32 && self.mimi.outer_dim == 512,
            "current German codec requires inner_dim=32, outer_dim=512"
        );
        ensure!(
            self.mimi.outer_dim == self.mimi.seanet.dimension
                && self.mimi.outer_dim == self.mimi.quantizer.output_dimension,
            "unsupported decoder channel topology"
        );
        ensure!(
            self.mimi.seanet.pad_mode == "constant",
            "unsupported SEANet padding"
        );
        ensure!(
            self.flow_lm.insert_bos_before_voice,
            "current German voice BOS must be enabled"
        );
        Ok(())
    }
    pub fn runtime_config(&self) -> TTSConfig {
        let f = &self.flow_lm;
        let m = &self.mimi;
        let s = &m.seanet;
        let t = &m.transformer;
        // Every architecture field comes from YAML; no v202601/English preset.
        TTSConfig {
            flow_lm: FlowLMConfig {
                d_model: f.transformer.d_model,
                num_heads: f.transformer.num_heads,
                num_layers: f.transformer.num_layers,
                dim_feedforward: f.transformer.d_model * f.transformer.hidden_scale,
                max_period: f.transformer.max_period,
                n_bins: f.lookup_table.n_bins,
                lut_dim: f.lookup_table.dim,
                flow_dim: f.flow.dim,
                flow_depth: f.flow.depth,
                ldim: m.quantizer.dimension,
            },
            mimi: MimiConfig {
                channels: m.channels,
                sample_rate: m.sample_rate,
                frame_rate: m.frame_rate,
                dimension: s.dimension,
                quantizer_dimension: m.quantizer.dimension,
                quantizer_output_dimension: m.quantizer.output_dimension,
                n_filters: s.n_filters,
                n_residual_layers: s.n_residual_layers,
                ratios: s.ratios.clone(),
                kernel_size: s.kernel_size,
                last_kernel_size: s.last_kernel_size,
                residual_kernel_size: s.residual_kernel_size,
                dilation_base: s.dilation_base,
                compress: s.compress,
                transformer_d_model: t.d_model,
                transformer_num_heads: t.num_heads,
                transformer_num_layers: t.num_layers,
                transformer_layer_scale: t.layer_scale,
                transformer_context: t.context,
                transformer_max_period: t.max_period,
                transformer_dim_feedforward: t.dim_feedforward,
                downsample_channel_wise: false,
                inner_dim: Some(m.inner_dim),
                outer_dim: Some(m.outer_dim),
            },
            lsd_decode_steps: 1,
            eos_threshold: -4.0,
            model_id: None,
            audio_prompt_min_duration: 0.0,
            audio_prompt_max_duration: 30.0,
            cfg_null_audio_empty: false,
            speaker_mimi: None,
            conditioners: vec![],
            fuser: None,
            voices: vec![],
            insert_bos_before_voice: f.insert_bos_before_voice,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_current_yaml_maps_new_topology() {
        let cfg: GermanConfig =
            serde_yaml::from_str(include_str!("../../../tests/fixtures/german.yaml")).unwrap();
        cfg.validate().unwrap();
        let r = cfg.runtime_config();
        assert_eq!(
            (r.flow_lm.num_layers, r.flow_lm.d_model, r.flow_lm.ldim),
            (6, 1024, 32)
        );
        assert_eq!(r.mimi.inner_dim, Some(32));
        assert!(r.insert_bos_before_voice);
    }
}
