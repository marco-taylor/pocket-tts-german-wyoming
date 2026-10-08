# Docker / Unraid

Build from the repository root with ./Dockerfile:

```bash
docker build --platform linux/amd64 -t pocket-tts-german-wyoming:local .
```

The CPU target is Intel Gracemont/AVX2 (N100 tested), not target-cpu=native.
The scratch runtime has no shell, Python, PyTorch, Cargo or GPU backend.
Native healthchecks use the same Rust binary. Readiness requires a loaded model,
valid default voice and Wyoming readiness; a merely open TCP port is insufficient.

## Persistence and permissions

Mount dedicated writable directories to `/app/models` and `/app/voices`.
Default UID/GID is 99:100, suitable for Unraid nobody/users; directories must be
writable by this account. Use appropriate ownership and permissions; no chmod 777.
Do not change other projects' data. You may override Docker's user when the mount
permissions match. Models, derived Q8 and voices persist separately from the image.

Use [docker/compose.yaml](docker/compose.yaml) with explicit values for
`POCKET_TTS_IMAGE`, `POCKET_TTS_HOST_MODELS`, `POCKET_TTS_HOST_VOICES`.
These are Compose substitution variables, not extra application settings.
`POCKET_TTS_UID`, `POCKET_TTS_GID`, `POCKET_TTS_HOST_PORT` are also Compose-only.
The application settings/defaults are documented in ./README.md and ./.env.example.
Container port is 10204/tcp; host port defaults to 10204. Use a free alternative
host port for parallel tests instead of stopping another service.

Keep the image's native HEALTHCHECK and SIGTERM behavior. Recommended flags are
read-only rootfs, cap-drop ALL, no-new-privileges, and restart unless-stopped.
The two data mounts remain writable for installation. Already complete assets
can be mounted read-only with an explicit local model path and official voice
download disabled. No resampling or output-format change is required by HA.

## Speech Speed

`POCKET_TTS_SPEED`: default **1.0**, continuous allowed range **0.8–1.2**.
0.8 is slower, 1.0 normal, 1.2 faster; 0.85/1.05 are valid intermediate values.
Malformed, non-finite, empty or out-of-range values fail startup, with no clamp or
fallback. The output always remains 24 kHz mono PCM16 LE and genuinely streaming.
At 1.0 the processor passes original PCM through. To change ENV, recreate only
this container with the new value and preserve its configuration/mounts; restart
alone does not change Docker's stored ENV.

## Unraid Community Applications

A CA XML template still needs to be created. No template or registry image is
published here. The future template must expose `POCKET_TTS_SPEED` as an ENV
variable: display name Speech Speed, Key/Target POCKET_TTS_SPEED, default 1.0,
description “Speech speed / Sprechgeschwindigkeit. Valid range 0.8–1.2; default
1.0; intermediate values allowed.” Runtime validation enforces the limits;
no artificial template min/max workaround is required.

Map models/voices to dedicated appdata folders chosen by the operator and map
host TCP 10204 to container TCP 10204. In HA, manually add Wyoming Protocol with
the Docker host and mapped port. This project does not change HA configuration.
