# Vendored XN port

Base: https://github.com/gradium-ai/xn-ptts/tree/83dfe119e4e1f7a13277aa4186882c758664ebf9

Only the source of the ptts library and the dual MIT/Apache licenses are copied.
The standalone package manifest omits other workspace frontends, examples and their
development dependencies. Tensor backend: xn 0.2.10, pinned in ./Cargo.lock.

Compatibility patch: scripts/patch-xn.mjs (paths relative to this project's root).
It applies to a fresh upstream source copy, not an already patched tree.

Changes:

- vendor/xn-ptts/ptts/src/mimi.rs: inner_dim/outer_dim, current encoder downsample.
- vendor/xn-ptts/ptts/src/resample.rs: distinct downsample input/output channels.
- vendor/xn-ptts/ptts/src/loader.rs: 1024x32 speaker projection.
- vendor/xn-ptts/ptts/src/tts_model.rs: YAML-mapped voice BOS, speaker encoder width.
- vendor/xn-ptts/ptts/src/transformer.rs: Mimi's strict `delta < context` mask,
  including the single-token boundary case, matching current upstream.

The runtime intentionally does not call the upstream high-level synth API.
Its unbounded channels, old text preparation and EOS policy remain in the vendored
upstream module but are not part of this project's execution path.
