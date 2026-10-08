use anyhow::{Context, Result, ensure};
use pocket_tts_runtime::engine::Precision;
use std::{env, path::PathBuf};
#[derive(Clone, Debug)]
pub struct Settings {
    pub language: String,
    pub models_dir: PathBuf,
    pub voices_dir: PathBuf,
    pub model_path: Option<PathBuf>,
    pub voice: String,
    pub precision: Precision,
    pub threads: usize,
    pub speed: f64,
    pub host: String,
    pub port: u16,
    pub voice_cache_bytes: usize,
    pub download_voices: bool,
    pub normalize: bool,
    pub normalizer: String,
    pub ready_file: Option<PathBuf>,
    pub log_level: String,
}
fn value(k: &str, d: &str) -> String {
    env::var(k)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| d.into())
}
fn boolean(k: &str, d: bool) -> Result<bool> {
    match env::var(k).as_deref() {
        Ok("true" | "1") => Ok(true),
        Ok("false" | "0") => Ok(false),
        Err(_) => Ok(d),
        _ => anyhow::bail!("{k} must be true or false"),
    }
}
impl Settings {
    pub fn from_env() -> Result<Self> {
        let language = value("POCKET_TTS_LANGUAGE", "german");
        ensure!(
            language == "german",
            "only german is supported in this release milestone"
        );
        let normalizer = value("POCKET_TTS_NORMALIZER", "safe");
        ensure!(
            ["span", "misaki", "final", "safe"].contains(&normalizer.as_str()),
            "normalizer must be span, misaki, final or safe"
        );
        let threads = value("POCKET_TTS_THREADS", "2")
            .parse()
            .context("POCKET_TTS_THREADS")?;
        ensure!((1..=4).contains(&threads), "threads must be 1..4 on N100");
        let cache_mb: usize = value("POCKET_TTS_VOICE_CACHE_MB", "32")
            .parse()
            .context("POCKET_TTS_VOICE_CACHE_MB")?;
        ensure!(
            (16..=512).contains(&cache_mb),
            "voice cache must be 16..512 MiB"
        );
        let log_level = value("RUST_LOG", "info");
        ensure!(
            ["error", "warn", "info", "debug", "trace"].contains(&log_level.as_str()),
            "RUST_LOG must be error/warn/info/debug/trace"
        );
        Ok(Self {
            language,
            models_dir: value("POCKET_TTS_MODELS_DIR", "/app/models").into(),
            voices_dir: value("POCKET_TTS_VOICES_DIR", "/app/voices").into(),
            model_path: env::var("POCKET_TTS_MODEL_PATH")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            voice: value("POCKET_TTS_VOICE", "juergen"),
            precision: if boolean("POCKET_TTS_QUANTIZE", true)? {
                Precision::Q8
            } else {
                Precision::Fp32
            },
            threads,
            speed: crate::speed::parse(env::var("POCKET_TTS_SPEED").ok().as_deref())?,
            host: value("WYOMING_HOST", "0.0.0.0"),
            port: value("WYOMING_PORT", "10204")
                .parse()
                .context("WYOMING_PORT")?,
            voice_cache_bytes: cache_mb * 1024 * 1024,
            download_voices: boolean("POCKET_TTS_DOWNLOAD_VOICES", true)?,
            normalize: boolean("POCKET_TTS_NORMALIZE", true)?,
            normalizer,
            ready_file: env::var("POCKET_TTS_READY_FILE").ok().map(PathBuf::from),
            log_level,
        })
    }
}
