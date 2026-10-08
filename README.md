# pocket-tts-german-wyoming

<p align="center">
  <img src="icons/pocket-tts-hq.png" alt="pocket-tts-german-wyoming" width="320">
</p>

Native Rust German Pocket TTS as a Wyoming TTS service for Home Assistant.
CPU-only, tested on Intel N100 (linux/amd64, Gracemont/AVX2). The Docker runtime
has no Python, PyTorch, GPU libraries, Cargo or compiler.

- Pinned official German model, Q8 by default; FP32 remains available.
- Genuine incremental FlowLM → stateful Mimi → PCM → Wyoming streaming.
- 24,000 Hz, mono, signed PCM16 little endian; no output resampling.
- 27 validated official voice states advertised with locale **de-DE** only.
- Juergen default; voice selection through Wyoming; unknown voices fail clearly.
- Native German SPAN/Priority text normalization and streaming speech-speed control.
- Persistent model/voice storage, atomic validated downloads, separate Q8 derivatives.
- Native readiness healthcheck, graceful shutdown, one synthesis at a time.

The regular multi-stage image and Home Assistant listening test passed on N100.
This repository prepares a locally buildable Docker project; no registry image
or Unraid Community Applications listing is claimed to be published.

## Build and Docker

```bash
docker build --platform linux/amd64 -t pocket-tts-german-wyoming:local .
```

The build pins Rust 1.99.0 and Cargo dependencies. Debian build packages use
Bookworm repositories, not a frozen package snapshot. Runtime is `scratch` with
the actual ELF library closure, CA certificates and license records.
The fixed Gracemont target requires a compatible AVX2 CPU; this is not a generic
image for older x86 CPUs or ARM.

Create dedicated writable model and voice directories owned by the container
user (default UID 99 / GID 100). Do not change permissions on other projects.

```bash
docker run -d --name pocket-tts \
  --restart unless-stopped --read-only --cap-drop ALL \
  --security-opt no-new-privileges \
  -p 10204:10204 \
  -v /absolute/path/to/dedicated/models:/app/models \
  -v /absolute/path/to/dedicated/voices:/app/voices \
  -e POCKET_TTS_SPEED=1.0 \
  pocket-tts-german-wyoming:local
```

Health is ready only after the model/default voice are available and Wyoming
responds. First startup may download assets and produce Q8; subsequent startup
validates and reuses them. The original model is never overwritten.
See [docker/README.md](docker/README.md) and [docker/compose.yaml](docker/compose.yaml).
The prepared Unraid template is [templates/pocket-tts-german-wyoming.xml](templates/pocket-tts-german-wyoming.xml),
with public icon [icons/pocket-tts.png](icons/pocket-tts.png). Its future image path
`ghcr.io/marco-taylor/pocket-tts-german-wyoming:latest` and GitHub raw URLs are
prepared only; the image and CA listing are **not yet published**.
It maps Wyoming TCP 10204 and dedicated models/voices directories under
`/mnt/user/appdata/pocket-tts-german-wyoming/` to `/app/models` and `/app/voices`.
Ensure those directories are writable by UID 99 / GID 100. Speech speed remains
configurable from 0.8 to 1.2, default 1.0; intermediate values are allowed.

In Home Assistant, manually add **Wyoming Protocol**, using the Docker host and
published port (default 10204). Select `pocket-tts`, German (Germany), and a voice.
No mDNS advertisement is implemented. Home Assistant itself is not configured
by this project.

## Configuration

[./.env.example](./.env.example) documents settings; the binary does not load it
automatically. Pass ENV to Docker, Compose or your process.

| Variable | Default | Meaning |
|---|---|---|
| POCKET_TTS_LANGUAGE | german | This release supports the pinned German architecture |
| POCKET_TTS_MODEL_PATH | unset | Explicit local model directory; disables automatic replacement/download |
| POCKET_TTS_MODELS_DIR | /app/models | Persistent model root |
| POCKET_TTS_VOICES_DIR | /app/voices | Persistent official/local voice root |
| POCKET_TTS_VOICE | juergen | Configurable default voice |
| POCKET_TTS_QUANTIZE | true | Q8; false selects FP32 |
| POCKET_TTS_THREADS | 2 | Backend threads, allowed 1–4; N100 tested |
| POCKET_TTS_NORMALIZE | true | German normalization before Pocket tokenizer |
| POCKET_TTS_SPEED | 1.0 | Speech speed, continuous range **0.8–1.2** |
| POCKET_TTS_VOICE_CACHE_MB | 32 | Lazy LRU voice cache, allowed 16–512 MiB |
| POCKET_TTS_DOWNLOAD_VOICES | true | Manage pinned public official voices automatically |
| WYOMING_HOST | 0.0.0.0 | Listen interface |
| WYOMING_PORT | 10204 | Container/process TCP port |
| RUST_LOG | info | error, warn, info, debug or trace |

Advanced existing options: `POCKET_TTS_NORMALIZER=safe` is the tested release
profile. `span`, `misaki` and `final` are historical comparison profiles, not
recommended for normal deployment; their broader experimental rules are not
active under `safe`. `POCKET_TTS_READY_FILE` optionally writes a diagnostic
readiness snapshot; its parent must exist and be writable.

### Speech speed

| Value | Meaning |
|---|---|
| 0.8 | Slower |
| 1.0 | Normal / reference |
| 1.2 | Faster |

Intermediate values such as 0.85 and 1.05 are allowed. Unset means 1.0.
Non-finite, malformed, empty or out-of-range values cause a startup/configuration
error: no clamping and no silent fallback.

Native bounded streaming WSOLA follows Mimi when speed differs from 1.0.
The output remains 24 kHz mono PCM16 LE; it does not reinterpret the sample rate
or wait for a full waveform. Local waveform alignment largely preserves pitch,
although time-stretch artifacts can occur. At 1.0 the original PCM passes through
unchanged; the tested reference was byte-identical. ENV is read at startup:
changing speed requires recreating the Docker container with the new ENV,
not merely restarting it.

### German normalization

Existing rules cover numbers, negatives, comma decimals, German thousands,
calendar-validated numerical/named-month dates with contextual ordinal forms,
times, temperatures, percentages and common Home Assistant units.
Structured spans are consumed once; protected technical tokens take priority.
The default includes nine narrowly tested additions informed by Misaki:
Euro, Cent, `z. B.`, `ca.` before numbers, `bzw.`, `inkl.`, context-dependent
`Nr.`, `etc.` and month abbreviations. Euro/Cent precision is preserved.
`Dr.`, `Prof.`, `Fr.` and `AG` expansions remain excluded in the default profile.
This is our Rust normalizer, not Misaki G2P, espeak-ng or a Python service.

RAW, normalized and tokenizer strings are currently logged at INFO for diagnosis.
Use `RUST_LOG=warn` when those potentially private texts should not be logged.
Recognition of free-form entity names/ambiguous technical strings is conservative
but cannot protect every unknown format. Model pronunciation is not guaranteed
for arbitrary text.

## Models and voices

Sources, revisions, sizes and SHA256 are pinned in [./assets.lock.json](./assets.lock.json)
and [assets/german-voices.lock.json](assets/german-voices.lock.json).
Public official states require no HF_TOKEN. No assets are included in Git/image.
Downloads use owned temporary files, verification and no-overwrite atomic rename;
manifests are written last. Existing invalid files are reported, never silently
replaced. Interrupted partial files are detected and retained for inspection.

An explicit local model directory needs compatible `config.yaml`, `tokenizer.json`
and `model.safetensors`; incompatible/incomplete data fail without automatic
fallback. Separate Q8 files also need their provenance/hash sidecar.
Q8 quantizes the supported FlowLM projection/FFN tensors; Mimi and KV remain FP32.

Official states live under `/app/voices/official/german/<revision>/`.
Local states are discovered recursively on startup, for example
`/app/voices/local/my_voice.safetensors`, advertised as `local/my_voice`.
They need compatible embedded metadata or a sibling `my_voice.voice.json`:

```json
{
  "model_sha256": "<original model SHA256>",
  "model_revision": "<pinned model revision>",
  "language": "german",
  "state_format": "pocket-tts-kv-v1",
  "quantization": "fp32-kv"
}
```

KV shapes, offsets, padding and finite values are validated. Incompatible voices
are skipped with warnings. Missing valid/default voice keeps readiness false.
Only the default is prewarmed/pinned; others use a bounded lazy LRU. Its budget
is not a total-process RAM limit. WAV voice cloning is not implemented.

## Development and tests

Rust 1.99.0, an x86_64 Linux C linker and Gracemont-compatible CPU are required.

```bash
cargo fmt --all --check
cargo test --locked --workspace
cargo build --locked --release -p pocket-tts-wyoming
```

Unit/regression tests require no downloaded weights. Explicitly ignored
hardware/TCP tests need compatible local assets and should be run only in your
own isolated environment. Generated WAVs/logs belong under ignored artifacts.
Output streaming is distinct from input text streaming: Wyoming
`supports_synthesize_streaming=false` refers to unsupported streaming text input.
Disconnects cancel synthesis; concurrent requests return `busy`.

## Acknowledgements and License / Third-party licenses

Thanks to [Kyutai Pocket TTS](https://github.com/kyutai-labs/pocket-tts),
[gradium-ai XN / xn-ptts](https://github.com/gradium-ai/xn-ptts) and
[Misaki](https://github.com/hexgrad/misaki) contributors. Misaki served as an
analysis/reference source for selected normalization rules, not as a runtime or
phonemizer. Historical adapted comparison source retains its Apache-2.0 license.

Our original code: **MIT**, Copyright (c) 2026 Marco Taylor, [./LICENSE](./LICENSE).
Third-party code and assets keep their own licenses; see
[./THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
In particular, the pinned Kyutai model/voice repository declares **CC BY 4.0**:
models/voices are **not** relicensed under our MIT license. Local custom assets
may have different licenses. Vendored xn-ptts remains MIT OR Apache-2.0;
Misaki-adapted files remain Apache-2.0.
