use crate::{config::GermanConfig, pcm::to_pcm16, text, voice::OfficialVoice};
use anyhow::{Result, ensure};
use ptts::{
    flow_lm::{NormalRng, StepInput},
    tts_model::TTSModel,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use xn::{BackendQ, CpuDevice, Tensor};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Fp32,
    Q8,
}
impl Precision {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fp32 => "fp32",
            Self::Q8 => "q8",
        }
    }
}
#[derive(Serialize, Debug)]
pub struct InferenceStats {
    pub chunks: usize,
    pub samples: usize,
    pub flow_steps: usize,
    pub segments: usize,
    pub nonfinite_values: usize,
    pub all_segments_eos: bool,
    pub seconds: f64,
    pub max_abs_f32: f32,
}
pub trait Engine {
    fn synthesize(
        &self,
        text: &str,
        voice: &OfficialVoice,
        seed: u64,
        cancel: &AtomicBool,
        stop: &AtomicBool,
        pcm: &mut dyn FnMut(Vec<u8>) -> Result<()>,
        event: &mut dyn FnMut(&str, serde_json::Value),
    ) -> Result<InferenceStats>;
}
struct Runtime<Q: BackendQ<T = f32, B = CpuDevice>> {
    cfg: GermanConfig,
    model: TTSModel<Q>,
    tokenizer: ptts::tok::Tok,
}
pub fn load(
    cfg_path: &Path,
    weights: &Path,
    tokenizer: &Path,
    precision: Precision,
) -> Result<Box<dyn Engine>> {
    match precision {
        Precision::Fp32 => {
            load_typed::<xn::Unquantized<f32, CpuDevice>>(cfg_path, weights, tokenizer)
        }
        Precision::Q8 => load_typed::<xn::quantized::Q80F32>(cfg_path, weights, tokenizer),
    }
}
fn load_typed<Q: BackendQ<T = f32, B = CpuDevice> + 'static>(
    cfg_path: &Path,
    weights: &Path,
    tokenizer: &Path,
) -> Result<Box<dyn Engine>> {
    let cfg = GermanConfig::load(cfg_path)?;
    let vb = ptts::loader::load_weights::<Q>(weights, &CpuDevice)?;
    ensure!(
        vb.shape("flow_lm.speaker_proj_weight")
            .is_some_and(|s| s.dims() == [1024, 32]),
        "invalid speaker projection"
    );
    ensure!(
        vb.shape("mimi.downsample.conv.conv.weight")
            .is_some_and(|s| s.dims() == [32, 512, 32]),
        "invalid Mimi downsampler"
    );
    let model = TTSModel::<Q>::load(
        &vb,
        Box::new(ptts::tok::Tok::open(tokenizer)?),
        &cfg.runtime_config(),
        &HashMap::new(),
    )?;
    Ok(Box::new(Runtime {
        cfg,
        model,
        tokenizer: ptts::tok::Tok::open(tokenizer)?,
    }))
}
impl<Q: BackendQ<T = f32, B = CpuDevice>> Engine for Runtime<Q> {
    fn synthesize(
        &self,
        input: &str,
        voice: &OfficialVoice,
        seed: u64,
        cancel: &AtomicBool,
        stop: &AtomicBool,
        pcm: &mut dyn FnMut(Vec<u8>) -> Result<()>,
        event: &mut dyn FnMut(&str, serde_json::Value),
    ) -> Result<InferenceStats> {
        let start = Instant::now();
        let parts = text::chunks_observed(
            input,
            &self.cfg,
            &self.tokenizer,
            50,
            &mut |phase, input| {
                event(
                    "TOKENIZER_TEXT",
                    serde_json::json!({"phase":phase,"text":input,"escaped":text::diagnostic_escaped(input)}),
                );
            },
        )?;
        let mut stats = InferenceStats {
            chunks: 0,
            samples: 0,
            flow_steps: 0,
            segments: parts.len(),
            nonfinite_values: 0,
            all_segments_eos: false,
            seconds: 0.0,
            max_abs_f32: 0.0,
        };
        let mut rng = NormalRng::new(0.3, seed)?;
        for (segment, chunk) in parts.iter().enumerate() {
            ensure!(
                !cancel.load(Ordering::Acquire) && !stop.load(Ordering::Acquire),
                "synthesis cancelled"
            );
            let budget = ptts::plan::frame_budget(chunk.tokens.len(), self.cfg.mimi.frame_rate);
            let mut state =
                voice.new_state(&self.model, voice.offset + chunk.tokens.len() + budget)?;
            let mut mimi = self.model.init_mimi_state(1)?;
            self.model.prompt_text(&mut state, &chunk.tokens)?;
            let mut previous: Option<Tensor<f32, CpuDevice>> = None;
            let mut eos_step = None;
            let mut ended = false;
            for step in 0..budget {
                ensure!(
                    !cancel.load(Ordering::Acquire) && !stop.load(Ordering::Acquire),
                    "synthesis cancelled"
                );
                event(
                    "flow_step_start",
                    serde_json::json!({"segment":segment,"step":step}),
                );
                let (latent, eos) = self.model.generate_step_parts(
                    &mut state,
                    match &previous {
                        Some(x) => StepInput::Latent(x),
                        None => StepInput::Bos { batch: 1 },
                    },
                    &mut rng,
                )?;
                ensure!(
                    latent.dims() == [1, 1, 32] && latent.to_vec()?.iter().all(|x| x.is_finite()),
                    "invalid/nonfinite latent"
                );
                let eos = eos.to_vec()?;
                ensure!(eos.len() == 1 && eos[0].is_finite(), "invalid EOS logit");
                stats.flow_steps += 1;
                event(
                    "latent_generated",
                    serde_json::json!({"segment":segment,"step":step}),
                );
                if self.model.eos_from_logit(&eos) && eos_step.is_none() && step >= 6 {
                    eos_step = Some(step);
                }
                if eos_step.is_some_and(|s| step >= s + chunk.frames_after_eos) {
                    ended = true;
                    event(
                        "segment_eos",
                        serde_json::json!({"segment":segment,"step":step}),
                    );
                    break;
                }
                let audio = self.model.decode_latent(&latent, &mut mimi)?;
                ensure!(
                    audio.dims().len() == 3 && audio.dims()[0] == 1 && audio.dims()[1] == 1,
                    "invalid mono decoder output"
                );
                let samples = audio.to_vec()?;
                ensure!(
                    !samples.is_empty()
                        && samples.len() <= 1920
                        && samples.iter().all(|x| x.is_finite()),
                    "invalid/nonfinite decoder output"
                );
                stats.max_abs_f32 = samples
                    .iter()
                    .fold(stats.max_abs_f32, |a, x| a.max(x.abs()));
                event(
                    "mimi_output",
                    serde_json::json!({"segment":segment,"step":step,"samples":samples.len()}),
                );
                stats.samples += samples.len();
                stats.chunks += 1;
                pcm(to_pcm16(&samples))?;
                previous = Some(latent);
            }
            ensure!(
                ended,
                "segment {segment} exhausted frame budget without EOS"
            );
        }
        stats.all_segments_eos = true;
        stats.seconds = start.elapsed().as_secs_f64();
        event("flow_end", serde_json::json!({"steps":stats.flow_steps}));
        event("mimi_end", serde_json::json!({"chunks":stats.chunks}));
        event("inference_end", serde_json::json!({"chunks":stats.chunks}));
        Ok(stats)
    }
}
