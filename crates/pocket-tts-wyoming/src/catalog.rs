use crate::{
    assets::{self, Downloader, ModelFiles, VoiceManifest},
    logging::Log,
};
use anyhow::{Context, Result, ensure};
use pocket_tts_runtime::{engine::Precision, voice::OfficialVoice};
use ptts::tts_model::TTSConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::WalkDir;
pub const FORMAT: &str = "pocket-tts-kv-v1";
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub model: String,
    pub revision: String,
    pub model_sha256: String,
    pub runtime_sha256: String,
    pub voice: String,
    pub state_sha256: String,
    pub state_format: String,
    pub precision: Precision,
}
#[derive(Clone, Debug, Serialize)]
pub struct VoiceRecord {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
    pub official: bool,
    pub bytes: usize,
    pub offset: usize,
    pub cache_key: CacheKey,
}
#[derive(Default, Deserialize)]
struct Binding {
    #[serde(default)]
    model_sha256: String,
    #[serde(default)]
    model_revision: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    state_format: String,
    #[serde(default)]
    quantization: String,
}
fn binding(path: &Path, state: &OfficialVoice) -> Result<Binding> {
    let side = path.with_extension("voice.json");
    let mut b: Binding = if side.is_file() {
        serde_json::from_slice(&std::fs::read(side)?)?
    } else {
        Binding::default()
    };
    for (k, v) in &state.metadata {
        let target = match k.as_str() {
            "model_sha256" => &mut b.model_sha256,
            "model_revision" => &mut b.model_revision,
            "language" => &mut b.language,
            "state_format" => &mut b.state_format,
            "quantization" => &mut b.quantization,
            _ => continue,
        };
        ensure!(
            target.is_empty() || target == v,
            "voice metadata conflicts with sidecar: {k}"
        );
        *target = v.clone();
    }
    Ok(b)
}
fn validate(
    path: &Path,
    root: &Path,
    model: &ModelFiles,
    cfg: &TTSConfig,
    manifest: &VoiceManifest,
) -> Result<(VoiceRecord, OfficialVoice)> {
    let sha = assets::hash(path)?;
    let state = OfficialVoice::load(path, cfg)?;
    let b = binding(path, &state)?;
    let known = manifest.voices.iter().find(|v| {
        v.sha256 == sha
            && manifest.model_sha256 == model.model_sha256
            && manifest.revision == model.revision
    });
    if known.is_none() {
        ensure!(
            b.model_sha256 == model.model_sha256,
            "no verified model binding: add compatible .voice.json sidecar with model_sha256"
        );
        ensure!(
            b.model_revision == model.revision,
            "voice model revision missing or incompatible"
        );
        ensure!(
            b.state_format == FORMAT,
            "voice format version missing or incompatible"
        );
    }
    for (actual, expected, label) in [
        (&b.model_sha256, &model.model_sha256, "model hash"),
        (&b.model_revision, &model.revision, "model revision"),
        (&b.language, &model.language, "language"),
    ] {
        ensure!(
            actual.is_empty() || actual == expected,
            "incompatible voice {label}"
        );
    }
    ensure!(
        b.state_format.is_empty() || b.state_format == FORMAT,
        "unsupported state format"
    );
    ensure!(
        b.quantization.is_empty()
            || b.quantization == "fp32-kv"
            || b.quantization == model.precision.name(),
        "voice quantization binding differs from runtime"
    );
    let relative = path
        .strip_prefix(root)?
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");
    let official = known.is_some() && relative.starts_with("official/");
    let name = if official {
        known.unwrap().name.clone()
    } else {
        relative
    };
    ensure!(!name.is_empty() && name.len() <= 160, "invalid voice name");
    let source = known
        .map(|v| v.url.clone())
        .unwrap_or_else(|| path.display().to_string());
    let record = VoiceRecord {
        name: name.clone(),
        path: path.to_owned(),
        source,
        official,
        bytes: state.byte_size(),
        offset: state.offset,
        cache_key: CacheKey {
            model: model.language.clone(),
            revision: model.revision.clone(),
            model_sha256: model.model_sha256.clone(),
            runtime_sha256: model.runtime_sha256.clone(),
            voice: name,
            state_sha256: sha,
            state_format: FORMAT.into(),
            precision: model.precision,
        },
    };
    Ok((record, state))
}
pub fn discover(
    root: &Path,
    model: &ModelFiles,
    cfg: &TTSConfig,
    manifest: &VoiceManifest,
    log: &Log,
) -> BTreeMap<String, VoiceRecord> {
    let mut records = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let e = match entry {
            Ok(e) => e,
            Err(e) => {
                log.warn("voice_scan_warning", e);
                continue;
            }
        };
        if !e.file_type().is_file() || e.path().extension().is_none_or(|x| x != "safetensors") {
            continue;
        }
        match validate(e.path(), root, model, cfg, manifest) {
            Ok((r, _)) => {
                if records.contains_key(&r.name) {
                    log.warn(
                        "voice_duplicate",
                        format!(
                            "duplicate voice {} at {}; retaining first",
                            r.name,
                            r.path.display()
                        ),
                    );
                    continue;
                }
                log.event(
                    "info",
                    "voice_validated",
                    None,
                    serde_json::to_value(&r).unwrap(),
                );
                records.insert(r.name.clone(), r);
            }
            Err(error) => log.warn(
                "voice_rejected",
                format!("{}: {error:#}", e.path().display()),
            ),
        }
    }
    records
}
pub fn download_official(
    root: &Path,
    model: &ModelFiles,
    cfg: &TTSConfig,
    manifest: &VoiceManifest,
    existing: &BTreeMap<String, VoiceRecord>,
    log: &Log,
) -> Result<()> {
    if model.model_sha256 != manifest.model_sha256 || model.revision != manifest.revision {
        log.warn(
            "official_voices_skipped",
            "selected custom model is not bound to the official voice revision",
        );
        return Ok(());
    }
    let downloader = Downloader::new(log.clone())?;
    for asset in &manifest.voices {
        ensure!(!assets::stopping(), "official voice downloads cancelled");
        if existing
            .values()
            .any(|r| r.cache_key.state_sha256 == asset.sha256)
        {
            continue;
        }
        let path = root
            .join("official")
            .join(&model.language)
            .join(&model.revision)
            .join(format!("{}.safetensors", asset.name));
        if let Err(e) = downloader.fetch(&path, &asset.url, &asset.sha256, Some(asset.size)) {
            log.warn("voice_download_failed", format!("{}: {e:#}", asset.name));
            continue;
        }
        if let Err(e) = validate(&path, root, model, cfg, manifest) {
            log.warn(
                "voice_download_incompatible",
                format!("{}: {e:#}", asset.name),
            );
        }
    }
    Ok(())
}
pub struct VoiceCache {
    entries: HashMap<CacheKey, Arc<OfficialVoice>>,
    order: VecDeque<CacheKey>,
    bytes: usize,
    limit: usize,
    pinned: Option<CacheKey>,
    cfg: TTSConfig,
    log: Log,
}
impl VoiceCache {
    pub fn new(limit: usize, cfg: TTSConfig, log: Log) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            limit,
            pinned: None,
            cfg,
            log,
        }
    }
    pub fn pin(&mut self, r: &VoiceRecord) -> Result<()> {
        self.pinned = Some(r.cache_key.clone());
        self.get(r)?;
        Ok(())
    }
    pub fn get(&mut self, r: &VoiceRecord) -> Result<Arc<OfficialVoice>> {
        let key = &r.cache_key;
        if let Some(v) = self.entries.get(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());
            self.log.event(
                "debug",
                "voice_cache_hit",
                None,
                serde_json::json!({"voice":r.name,"cache_bytes":self.bytes}),
            );
            return Ok(v.clone());
        }
        let start = std::time::Instant::now();
        ensure!(
            assets::hash(&r.path)? == key.state_sha256,
            "voice changed since discovery; restart required"
        );
        let state = Arc::new(OfficialVoice::load(&r.path, &self.cfg)?);
        let size = state.byte_size();
        ensure!(
            size <= self.limit,
            "voice cache too small for requested voice"
        );
        while self.bytes + size > self.limit {
            let old=self.order.iter().find(|k|Some(*k)!=self.pinned.as_ref()).cloned().context("cache budget cannot hold pinned default and requested voice; increase POCKET_TTS_VOICE_CACHE_MB")?;
            self.order.retain(|k| k != &old);
            if let Some(v) = self.entries.remove(&old) {
                self.bytes -= v.byte_size();
                self.log.event(
                    "info",
                    "voice_cache_evicted",
                    None,
                    serde_json::json!({"voice":old.voice,"cache_bytes":self.bytes}),
                );
            }
        }
        self.bytes += size;
        self.entries.insert(key.clone(), state.clone());
        self.order.push_back(key.clone());
        self.log.event("info","voice_cache_loaded",None,serde_json::json!({"voice":r.name,"ms":start.elapsed().as_secs_f64()*1000.0,"cache_bytes":self.bytes,"cache_limit_bytes":self.limit}));
        Ok(state)
    }
}
