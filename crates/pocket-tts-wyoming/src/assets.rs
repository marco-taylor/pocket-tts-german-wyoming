use crate::{logging::Log, settings::Settings};
use anyhow::{Context, Result, ensure};
use pocket_tts_runtime::{config::GermanConfig, engine::Precision};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::{
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
pub fn set_stop_flag(stop: Arc<AtomicBool>) {
    let _ = STOP.set(stop);
}
pub fn stopping() -> bool {
    STOP.get().is_some_and(|s| s.load(Ordering::Acquire))
}
#[derive(Clone, Deserialize, Serialize)]
pub struct VoiceAsset {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct VoiceManifest {
    pub repository: String,
    pub revision: String,
    pub language: String,
    pub model_sha256: String,
    pub state_format: String,
    pub voices: Vec<VoiceAsset>,
}
pub fn manifest() -> VoiceManifest {
    serde_json::from_str(include_str!("../../../assets/german-voices.lock.json"))
        .expect("embedded validated manifest")
}
pub fn hash(path: &Path) -> Result<String> {
    let mut r = BufReader::new(File::open(path)?);
    let mut sha = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = r.read(&mut b)?;
        if n == 0 {
            break;
        }
        sha.update(&b[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
#[derive(Clone, Serialize)]
pub struct ModelFiles {
    pub config: PathBuf,
    pub tokenizer: PathBuf,
    pub weights: PathBuf,
    pub model_sha256: String,
    pub runtime_sha256: String,
    pub revision: String,
    pub language: String,
    pub precision: Precision,
}
pub struct Downloader {
    client: reqwest::Client,
    runtime: tokio::runtime::Runtime,
    log: Log,
}
impl Downloader {
    pub fn new(log: Log) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(240))
                .https_only(true)
                .build()?,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
            log,
        })
    }
    pub fn fetch(&self, path: &Path, url: &str, expected: &str, size: Option<u64>) -> Result<()> {
        ensure!(!stopping(), "startup cancelled");
        let parent = path.parent().context("asset parent")?;
        if parent.is_dir() {
            for entry in fs::read_dir(parent)?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(".pocket-tts-partial-")
                    || name.contains(".partial-")
                    || name.ends_with(".partial")
                {
                    self.log.warn(
                        "incomplete_download_detected",
                        format!(
                            "{}: ignored; never reused or overwritten",
                            entry.path().display()
                        ),
                    );
                }
            }
        }
        if path.exists() {
            ensure!(
                hash(path)? == expected,
                "existing file has wrong hash; refusing overwrite: {}",
                path.display()
            );
            if let Some(n) = size {
                ensure!(
                    fs::metadata(path)?.len() == n,
                    "existing asset size mismatch"
                );
            }
            self.log.event(
                "info",
                "asset_local",
                None,
                serde_json::json!({"path":path,"sha256":expected}),
            );
            return Ok(());
        }
        fs::create_dir_all(parent)?;
        let start = std::time::Instant::now();
        self.runtime.block_on(async {
            let mut response = tokio::select! {
                result = self.client.get(url).send() => result?.error_for_status()?,
                _ = cancelled() => anyhow::bail!("startup cancelled"),
            };
            let mut temporary = pocket_tts_runtime::atomic_file::Temporary::new(parent)?;
            let mut sha = Sha256::new();
            let mut bytes = 0u64;
            loop {
                ensure!(!stopping(), "startup cancelled");
                let chunk = tokio::select! {
                    result = response.chunk() => result?,
                    _ = cancelled() => anyhow::bail!("startup cancelled"),
                };
                let Some(b) = chunk else { break };
                let n = b.len();
                bytes += n as u64;
                if let Some(max) = size {
                    ensure!(bytes <= max, "download larger than manifest");
                }
                temporary.file.write_all(&b)?;
                sha.update(&b);
            }
            if let Some(n) = size {
                ensure!(bytes == n, "download truncated: {bytes}/{n}");
            }
            ensure!(
                format!("{:x}", sha.finalize()) == expected,
                "download SHA256 mismatch"
            );
            temporary.publish(path)?;
            self.log.event(
                "info",
                "asset_downloaded",
                None,
                serde_json::json!({"path":path,"url":url,"bytes":bytes,"sha256":expected,"ms":start.elapsed().as_secs_f64()*1000.0}),
            );
            Ok(())
        })
    }
}
#[derive(Deserialize)]
struct AssetLock {
    upstream_revision: String,
    assets: Vec<LockedAsset>,
}
#[derive(Deserialize)]
struct LockedAsset {
    path: String,
    url: String,
    sha256: String,
    size: u64,
}
pub fn prepare_model(settings: &Settings, log: &Log) -> Result<ModelFiles> {
    let m = manifest();
    let base;
    let original;
    if let Some(local) = &settings.model_path {
        if local.is_dir() {
            base = local.clone();
            original = base.join("model.safetensors");
        } else {
            base = local.parent().context("local model parent")?.to_path_buf();
            original = local.clone();
        }
    } else {
        base = settings.models_dir.join(&settings.language);
        original = base.join("model.safetensors");
        let lock: AssetLock = serde_json::from_str(include_str!("../../../assets.lock.json"))?;
        let downloader = Downloader::new(log.clone())?;
        for asset in lock.assets.iter().filter(|a| a.path.starts_with("models/")) {
            let name = Path::new(&asset.path).file_name().context("asset name")?;
            downloader.fetch(
                &base.join(name),
                &asset.url,
                &asset.sha256,
                Some(asset.size),
            )?;
        }
        log.event("info","model_revision",None,serde_json::json!({"model_revision":m.revision,"upstream_revision":lock.upstream_revision}));
    }
    let config = base.join("config.yaml");
    let tokenizer = base.join("tokenizer.json");
    GermanConfig::load(&config)?;
    ensure!(tokenizer.is_file(), "local tokenizer.json missing");
    let model_sha256;
    let weights;
    if original.extension().is_some_and(|x| x == "gguf") {
        ensure!(
            settings.precision == Precision::Q8,
            "explicit GGUF requires quantize=true"
        );
        let side: serde_json::Value =
            serde_json::from_slice(&fs::read(original.with_extension("gguf.json"))?)
                .context("local derived Q8 requires provenance sidecar")?;
        ensure!(
            side["derived_sha256"].as_str() == Some(&hash(&original)?),
            "derived GGUF hash mismatch"
        );
        model_sha256 = side["source_sha256"]
            .as_str()
            .context("source SHA missing")?
            .to_owned();
        weights = original.clone();
    } else {
        ensure!(
            original.is_file(),
            "local model.safetensors missing; explicit model path never triggers a download"
        );
        model_sha256 = hash(&original)?;
        if settings.precision == Precision::Q8 {
            let derived = base.join("derived/model.q8_0.gguf");
            if !derived.exists() {
                let start = std::time::Instant::now();
                log.event(
                    "info",
                    "q8_creation_started",
                    None,
                    serde_json::json!({"source":original,"derived":derived}),
                );
                pocket_tts_runtime::quantize::convert_cancellable(&original, &derived, &stopping)?;
                log.event(
                    "info",
                    "q8_created",
                    None,
                    serde_json::json!({"ms":start.elapsed().as_secs_f64()*1000.0,"path":derived}),
                );
            } else {
                log.event(
                    "info",
                    "q8_reused",
                    None,
                    serde_json::json!({"path":derived}),
                );
            }
            let side: serde_json::Value =
                serde_json::from_slice(&fs::read(derived.with_extension("gguf.json"))?)?;
            ensure!(
                side["source_sha256"].as_str() == Some(&model_sha256),
                "derived model belongs to another source"
            );
            ensure!(
                side["derived_sha256"].as_str() == Some(&hash(&derived)?),
                "derived model corrupted"
            );
            weights = derived;
        } else {
            weights = original;
        }
    }
    let revision = if model_sha256 == m.model_sha256 {
        m.revision
    } else {
        format!("local-sha256:{model_sha256}")
    };
    Ok(ModelFiles {
        config,
        tokenizer,
        runtime_sha256: hash(&weights)?,
        weights,
        model_sha256,
        revision,
        language: settings.language.clone(),
        precision: settings.precision,
    })
}
async fn cancelled() {
    while !stopping() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Commit immutable installation receipts only after runtime/state validation.
/// The digest-named receipt is last, never a replacement for tensor validation.
pub fn commit_installation(
    settings: &Settings,
    model: &ModelFiles,
    voices: &std::collections::BTreeMap<String, crate::catalog::VoiceRecord>,
    log: &Log,
) -> Result<()> {
    let mut receipts = Vec::new();
    if settings.model_path.is_none() {
        receipts.push((model.config.parent().context("config parent")?.join(".pocket-tts/manifests"), serde_json::json!({"format_version":1,"kind":"model","model":model,"config_sha256":hash(&model.config)?,"tokenizer_sha256":hash(&model.tokenizer)?})));
    }
    if settings.download_voices {
        receipts.push((settings.voices_dir.join(".pocket-tts/manifests"), serde_json::json!({"format_version":1,"kind":"voices","model_revision":model.revision,"model_sha256":model.model_sha256,"precision":model.precision,"voices":voices})));
    }
    for (parent, receipt) in receipts {
        let data = serde_json::to_vec_pretty(&receipt)?;
        let digest = format!("{:x}", Sha256::digest(&data));
        let path = parent.join(format!("{digest}.json"));
        if path.exists() {
            ensure!(
                hash(&path)? == digest,
                "existing installation receipt corrupted; refusing overwrite"
            );
        } else {
            pocket_tts_runtime::atomic_file::write_new(&path, &data)?;
        }
        log.event(
            "info",
            "installation_manifest_verified",
            None,
            serde_json::json!({"path":path,"sha256":digest,"kind":receipt["kind"]}),
        );
    }
    Ok(())
}
