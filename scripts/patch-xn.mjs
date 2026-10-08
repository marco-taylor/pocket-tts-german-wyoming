// Reproducible compatibility edits against upstream 83dfe119. Run only on a fresh copy.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const root = (process.argv[2] ?? fileURLToPath(new URL('../vendor/xn-ptts/ptts/src/', import.meta.url))).replace(/\/$/, '') + '/';
if (readFileSync(root + 'mimi.rs', 'utf8').includes('pub inner_dim:')) {
  throw new Error('Source already patched; refusing to apply any edits');
}
const pending = new Map();
function edit(file, replacements) {
  let s = readFileSync(root + file, 'utf8');
  for (const [a,b] of replacements) {
    if (!s.includes(a)) throw new Error(`${file}: missing patch context: ${a}`);
    s = s.replace(a,b);
  }
  pending.set(file, s);
}
edit('mimi.rs', [
  ['    pub downsample_channel_wise: bool,','    pub downsample_channel_wise: bool,\n    #[serde(default)]\n    pub inner_dim: Option<usize>,\n    #[serde(default)]\n    pub outer_dim: Option<usize>,'],
  ['                cfg.downsample_channel_wise,','                cfg.downsample_channel_wise,\n                cfg.inner_dim,']
]);
edit('resample.rs', [
  ['pub fn load(vb: &Path<B>, stride: usize, dimension: usize, depthwise: bool)', 'pub fn load(vb: &Path<B>, stride: usize, dimension: usize, depthwise: bool, out_dimension: Option<usize>)'],
  ['            dimension,\n            dimension,\n            2 * stride,', '            dimension,\n            out_dimension.unwrap_or(dimension),\n            2 * stride,']
]);
edit('tts_model.rs', [
  ['    pub voices: Vec<VoiceConfig>,','    pub voices: Vec<VoiceConfig>,\n    #[serde(default)]\n    pub insert_bos_before_voice: bool,'],
  ['                downsample_channel_wise: false,','                downsample_channel_wise: false,\n                inner_dim: None,\n                outer_dim: None,'],
  ['            voices: vec![],','            voices: vec![],\n            insert_bos_before_voice: false,'],
  ['    speaker_proj: Option<Linear<f32, Q::B>>,','    speaker_proj: Option<Linear<f32, Q::B>>,\n    bos_before_voice: Option<Tensor<Q::T, Q::B>>,'],
  ['        let speaker_proj = crate::loader::load_speaker_proj(vb, cfg)?;', '        let speaker_proj = crate::loader::load_speaker_proj(vb, cfg)?;\n        let bos_before_voice = if cfg.insert_bos_before_voice {\n            Some(vb.pp("flow_lm").tensor("bos_before_voice", (1, 1, cfg.flow_lm.d_model))?)\n        } else { None };'],
  ['            speaker_proj,\n            lsd_decode_steps:', '            speaker_proj,\n            bos_before_voice,\n            lsd_decode_steps:'],
  ['        let text_embeddings = Tensor::cat(&[&self.empty_text()?, audio_conditioning], 1)?;', '        // Only raw embeddings pass through here. Imported official KV states bypass\n        // prompting completely: their BOS is already represented in the cache.\n        let text_embeddings = match &self.bos_before_voice {\n            Some(bos) => Tensor::cat(&[bos, audio_conditioning], 1)?,\n            None => audio_conditioning.clone(),\n        };'],
  ['            (cfg.flow_lm.d_model, mimi_cfg.dimension),','            (cfg.flow_lm.d_model, mimi_cfg.inner_dim.unwrap_or(mimi_cfg.dimension)),']
]);
edit('loader.rs', [
  ['    let shape = (cfg.flow_lm.d_model, cfg.speaker_mimi_cfg().dimension);','    let mimi = cfg.speaker_mimi_cfg();\n    let shape = (cfg.flow_lm.d_model, mimi.inner_dim.unwrap_or(mimi.dimension));']
]);
edit('transformer.rs', [
  ['        // At single-token decode every cached key is visible, so the causal mask is identically\n        // zero: the FlowLm branch masks nothing at all, and the Mimi branch trims its cache to\n        // `context` before this point, which makes its window condition vacuous too. Building\n        // that mask means filling a Vec on the host, uploading it and adding it per layer, all\n        // to add zero.', '        // FlowLM single-token decode sees all cached keys. Mimi retains `context`\n        // previous keys, but current upstream requires delta < context, so its\n        // oldest key must still be masked even when decoding a single token.'],
  ['let mask = if seq_len == 1 {', 'let mask = if seq_len == 1 && !matches!(state.layer_states.first(), Some(LayerAttentionState::Mimi(_))) {'],
  ['if seq_idx.saturating_sub(context) <= attn_idx\n                                    && attn_idx <= seq_idx', 'if attn_idx <= seq_idx && seq_idx - attn_idx < context']
]);
// Do not change any file unless every upstream context has validated.
for (const [file, contents] of pending) writeFileSync(root + file, contents);
