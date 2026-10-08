# Pinned official Rust 1.99.0 Debian Bookworm amd64 manifest.
FROM rust:1.99.0-slim-bookworm@sha256:ba81bc3eaa4422af576c0262515d96b0111a628a6ccc2c86557cf55c9a4bbee0 AS build
ARG TARGETARCH=amd64
RUN test "$TARGETARCH" = amd64
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config ca-certificates binutils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY crates crates
COPY vendor vendor
COPY tests/fixtures tests/fixtures
COPY assets.lock.json ./
COPY assets/german-voices.lock.json assets/german-voices.lock.json
COPY licenses licenses
COPY LICENSE THIRD_PARTY_NOTICES.md ./
COPY docker/runtime-rootfs.sh /build/docker/runtime-rootfs.sh
# Explicit Gracemont, never target-cpu=native or build-host feature detection.
ENV CARGO_BUILD_JOBS=2 RUSTFLAGS="-C target-cpu=gracemont"
RUN cargo build --locked --release -p pocket-tts-wyoming \
    && cargo test --locked -p pocket-tts-runtime \
    && cargo test --locked -p pocket-tts-wyoming --lib \
    && sh /build/docker/runtime-rootfs.sh

FROM scratch AS runtime
COPY --from=build /runtime-rootfs/ /
LABEL org.opencontainers.image.title="pocket-tts-german-wyoming" \
      org.opencontainers.image.description="Native Rust Pocket TTS german, Wyoming streaming, CPU-only Intel N100" \
      pocket-tts.build.cpu="gracemont" \
      pocket-tts.build.rust="1.99.0" \
      pocket-tts.model.revision="1e08e6a23401048648a9fdcfde2f89348215c2a7"
ENV POCKET_TTS_LANGUAGE=german \
    POCKET_TTS_QUANTIZE=true \
    POCKET_TTS_NORMALIZE=true \
    POCKET_TTS_NORMALIZER=safe \
    POCKET_TTS_THREADS=2 \
    POCKET_TTS_SPEED=1.0 \
    POCKET_TTS_VOICE=juergen \
    POCKET_TTS_MODELS_DIR=/app/models \
    POCKET_TTS_VOICES_DIR=/app/voices \
    POCKET_TTS_MODEL_PATH="" \
    POCKET_TTS_VOICE_CACHE_MB=32 \
    POCKET_TTS_DOWNLOAD_VOICES=true \
    WYOMING_HOST=0.0.0.0 \
    WYOMING_PORT=10204 \
    RUST_LOG=info \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
WORKDIR /app
USER 99:100
EXPOSE 10204/tcp
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=3s --start-period=10m --start-interval=2s --retries=3 \
    CMD ["/usr/local/bin/pocket-tts-wyoming", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/pocket-tts-wyoming"]
