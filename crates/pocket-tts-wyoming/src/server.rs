use crate::{
    assets::{self, ModelFiles},
    catalog::{self, VoiceCache, VoiceRecord},
    logging::{Log, memory_kib},
    protocol::{self, Event},
    settings::Settings,
};
use anyhow::{Context, Result, ensure};
use pocket_tts_runtime::{
    config::GermanConfig,
    engine::{self, InferenceStats},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::BufReader,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

struct Job {
    id: u64,
    text: String,
    voice: String,
    received_us: u64,
    frames: SyncSender<Frame>,
    cancel: Arc<AtomicBool>,
}
enum Frame {
    Pcm(Vec<u8>),
    End(InferenceStats),
    Error(String),
}
struct Shared {
    settings: Settings,
    model: ModelFiles,
    voices: BTreeMap<String, VoiceRecord>,
    jobs: SyncSender<Job>,
    ready: AtomicBool,
    busy: AtomicBool,
    connections: AtomicUsize,
    sockets: Mutex<BTreeMap<usize, TcpStream>>,
    next_connection: AtomicUsize,
    next_id: AtomicU64,
    stop: Arc<AtomicBool>,
    log: Log,
}
struct ConnectionGuard(Arc<Shared>, usize);
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::AcqRel);
        self.0
            .sockets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.1);
    }
}
struct RequestGuard {
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.shared.busy.store(false, Ordering::Release);
    }
}
#[derive(Default, Deserialize)]
struct VoiceChoice {
    name: Option<String>,
    speaker: Option<String>,
    language: Option<String>,
}
fn choose_voice(
    data: &Value,
    default: &str,
    voices: &BTreeMap<String, VoiceRecord>,
) -> Result<String> {
    let choice: VoiceChoice = match data.get("voice") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .context("voice must be an object with name/speaker/language")?,
        _ => VoiceChoice::default(),
    };
    if let Some(language) = &choice.language {
        ensure!(
            ["de", "de-DE", "german"].contains(&language.as_str()),
            "unsupported language {language}"
        );
    }
    let selected = match (choice.name, choice.speaker) {
        (Some(name), Some(speaker)) if name == "german" => speaker,
        (Some(name), Some(speaker)) => {
            ensure!(
                name == speaker,
                "speaker {speaker} does not match voice {name}"
            );
            name
        }
        (Some(name), None) if name == "german" => default.into(),
        (Some(name), None) => name,
        (None, Some(speaker)) => speaker,
        (None, None) => default.into(),
    };
    ensure!(
        voices.contains_key(&selected),
        "unknown or incompatible voice '{selected}'; available voices: {}",
        voices.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    Ok(selected)
}
fn info(shared: &Shared) -> Event {
    let attribution =
        json!({"name":"Kyutai / Pocket TTS","url":"https://github.com/kyutai-labs/pocket-tts"});
    let ready = shared.ready.load(Ordering::Acquire) && !shared.stop.load(Ordering::Acquire);
    let voices=shared.voices.values().map(|v|json!({"name":v.name,"languages":["de-DE"],"attribution":attribution,"installed":true,"description":v.name,"version":shared.model.revision})).collect::<Vec<_>>();
    Event::new(
        "info",
        json!({"asr":[],"tts":[{"name":"pocket-tts","attribution":attribution,"installed":ready,"description":"Native Rust Pocket TTS, german, incremental PCM audio","version":env!("CARGO_PKG_VERSION"),"voices":voices,"supports_synthesize_streaming":false}],"wake":[],"handle":[],"intent":[],"mic":[],"snd":[],"ready":ready,"pocket_tts":{"model":shared.model.language,"model_revision":shared.model.revision,"precision":shared.model.precision,"default_voice":shared.settings.voice,"voice_count":shared.voices.len(),"speed":shared.settings.speed,"busy":shared.busy.load(Ordering::Acquire)}}),
    )
}
fn send_error(stream: &mut TcpStream, code: &str, error: impl std::fmt::Display) -> Result<()> {
    protocol::write_event(stream, &Event::error(code, error))
}
fn synthesize(stream: &mut TcpStream, data: &Value, shared: &Arc<Shared>) -> Result<()> {
    let received = Instant::now();
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let received_us = shared
        .log
        .event("info", "synthesize_received", Some(id), json!({}));
    let text = match data.get("text").and_then(Value::as_str) {
        Some(t) if !t.trim().is_empty() && t.len() <= 16384 => t.to_owned(),
        _ => {
            return send_error(
                stream,
                "invalid-text",
                "text must be nonempty UTF-8, at most 16384 bytes",
            );
        }
    };
    shared.log.event("info", "RAW_TEXT", Some(id), json!({
        "text":text,
        "escaped":pocket_tts_runtime::text::diagnostic_escaped(&text),
        "codepoints":text.char_indices().map(|(byte_offset,c)| json!({"byte_offset":byte_offset,"codepoint":format!("U+{:04X}",c as u32),"escaped":pocket_tts_runtime::text::diagnostic_escaped(&c.to_string())})).collect::<Vec<_>>()
    }));
    if data
        .get("text_format")
        .is_some_and(|v| !v.is_null() && v != "text")
    {
        return send_error(
            stream,
            "unsupported-text-format",
            "only plain text is supported",
        );
    }
    let voice = match choose_voice(data, &shared.settings.voice, &shared.voices) {
        Ok(v) => v,
        Err(e) => return send_error(stream, "unknown-voice", e),
    };
    if !shared.ready.load(Ordering::Acquire) {
        return send_error(
            stream,
            "not-ready",
            "model/default voice is not ready; inspect startup warnings",
        );
    }
    if shared
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return send_error(
            stream,
            "busy",
            "pocket-tts currently synthesizes another request; retry after it finishes",
        );
    }
    let normalize_start = Instant::now();
    let normalized = if shared.settings.normalize {
        if shared.settings.normalizer == "misaki" {
            crate::misaki_normalizer::normalize(&text)
        } else if shared.settings.normalizer == "safe" {
            crate::german_normalizer::normalize_safe(&text)
        } else if shared.settings.normalizer == "final" {
            crate::german_normalizer::normalize_final(&text)
        } else {
            crate::german_normalizer::normalize(&text)
        }
    } else {
        text.clone()
    };
    let normalize_us = normalize_start.elapsed().as_nanos() as f64 / 1000.0;
    shared.log.event("info", if shared.settings.normalizer == "misaki" {"MISAKI_NORMALIZED_TEXT"} else {"NORMALIZED_TEXT"}, Some(id), json!({"text":normalized,"escaped":pocket_tts_runtime::text::diagnostic_escaped(&normalized)}));
    shared.log.event(
        "debug",
        "text_normalized",
        Some(id),
        json!({"original":text,"normalized":normalized}),
    );
    shared.log.event("info", "normalization_complete", Some(id), json!({"enabled":shared.settings.normalize,"normalizer":shared.settings.normalizer,"elapsed_us":normalize_us,"input_bytes":text.len(),"output_bytes":normalized.len()}));
    let text = normalized;
    let cancel = Arc::new(AtomicBool::new(false));
    let _guard = RequestGuard {
        shared: shared.clone(),
        cancel: cancel.clone(),
    };
    let (tx, rx) = mpsc::sync_channel(1);
    shared
        .jobs
        .try_send(Job {
            id,
            text,
            voice: voice.clone(),
            received_us,
            frames: tx,
            cancel,
        })
        .context("inference worker unavailable")?;
    let format = json!({"rate":24000,"width":2,"channels":1,"timestamp":0});
    protocol::write_event(stream, &Event::new("audio-start", format))?;
    shared.log.event(
        "info",
        "audio_start_sent",
        Some(id),
        json!({"voice":voice,"request_received_us":received_us}),
    );
    let mut first = None;
    let mut chunks = 0usize;
    let mut samples = 0usize;
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return send_error(stream, "shutdown", "service shutting down");
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Frame::Pcm(bytes)) => {
                ensure!(
                    !bytes.is_empty() && bytes.len() <= 3840 && bytes.len() % 2 == 0,
                    "invalid runtime PCM chunk"
                );
                let n = bytes.len() / 2;
                let mut event = Event::new(
                    "audio-chunk",
                    json!({"rate":24000,"width":2,"channels":1,"timestamp":samples*1000/24000}),
                );
                event.payload = bytes;
                protocol::write_event(stream, &event)?;
                let now = shared.log.event(
                    "info",
                    "audio_chunk_sent",
                    Some(id),
                    json!({"voice":voice,"index":chunks,"samples":n}),
                );
                if first.is_none() {
                    first = Some((now, received.elapsed().as_secs_f64() * 1000.0));
                }
                samples += n;
                chunks += 1;
            }
            Ok(Frame::End(stats)) => {
                ensure!(
                    samples == stats.samples && chunks == stats.chunks && first.is_some(),
                    "runtime/consumer count mismatch"
                );
                protocol::write_event(
                    stream,
                    &Event::new("audio-stop", json!({"timestamp":samples*1000/24000})),
                )?;
                shared.log.event(
                    "info",
                    "audio_stop_sent",
                    Some(id),
                    json!({"voice":voice,"chunks":chunks}),
                );
                let seconds = received.elapsed().as_secs_f64();
                let duration = samples as f64 / 24000.0;
                let (first_us, ttfa) = first.unwrap();
                shared.log.event("info","synthesis_complete",Some(id),json!({"voice":voice,"precision":shared.model.precision,"threads":shared.settings.threads,"speed":shared.settings.speed,"received_us":received_us,"first_chunk_us":first_us,"ttfa_ms":ttfa,"synthesis_seconds":seconds,"audio_seconds":duration,"rtf":seconds/duration,"peak_rss_kib":memory_kib("VmHWM:"),"rss_kib":memory_kib("VmRSS:"),"chunks":chunks,"flow_steps":stats.flow_steps,"segments":stats.segments,"nonfinite_values":stats.nonfinite_values,"all_segments_eos":stats.all_segments_eos,"max_abs_f32":stats.max_abs_f32}));
                return Ok(());
            }
            Ok(Frame::Error(e)) => {
                shared
                    .log
                    .warn("synthesis_failed", format!("request {id}: {e}"));
                return send_error(stream, "inference-error", e);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return send_error(
                    stream,
                    "inference-worker-lost",
                    "inference worker disconnected",
                );
            }
        }
    }
}
fn connection(mut stream: TcpStream, shared: Arc<Shared>, connection_id: usize) -> Result<()> {
    let _counter = ConnectionGuard(shared.clone(), connection_id);
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    socket2::SockRef::from(&stream).set_send_buffer_size(16384)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    while !shared.stop.load(Ordering::Acquire) {
        let event = match protocol::read_event(&mut reader) {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                let _ = send_error(&mut stream, "invalid-event", e);
                break;
            }
        };
        match event.kind.as_str() {
            "describe" => protocol::write_event(&mut stream, &info(&shared))?,
            "synthesize" => {
                if !event.payload.is_empty() {
                    send_error(
                        &mut stream,
                        "unexpected-payload",
                        "synthesize has no binary payload",
                    )?;
                } else {
                    synthesize(&mut stream, &event.data, &shared)?;
                }
            }
            _ => send_error(
                &mut stream,
                "unsupported-event",
                format!("unsupported event '{}'", event.kind),
            )?,
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}
pub fn run(settings: Settings) -> Result<()> {
    // Configure before conversion, TLS/DNS threads and tensor pool initialization.
    xn::set_num_threads(settings.threads);
    let log = Log::new(&settings.log_level);
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    assets::set_stop_flag(stop.clone());
    let model = match assets::prepare_model(&settings, &log) {
        Ok(m) => m,
        Err(e) if stop.load(Ordering::Acquire) => {
            log.warn("startup_cancelled", e);
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let cfg = GermanConfig::load(&model.config)?.runtime_config();
    let manifest = assets::manifest();
    std::fs::create_dir_all(&settings.voices_dir)?;
    let existing = catalog::discover(&settings.voices_dir, &model, &cfg, &manifest, &log);
    if settings.download_voices {
        if let Err(e) = catalog::download_official(
            &settings.voices_dir,
            &model,
            &cfg,
            &manifest,
            &existing,
            &log,
        ) {
            log.warn("official_voice_downloads_incomplete", e);
        }
    }
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let voices = if settings.download_voices {
        catalog::discover(&settings.voices_dir, &model, &cfg, &manifest, &log)
    } else {
        existing
    };
    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<Job>(1);
    let (loaded_tx, loaded_rx) = mpsc::sync_channel::<Result<bool>>(1);
    let shared = Arc::new(Shared {
        settings: settings.clone(),
        model: model.clone(),
        voices,
        jobs: jobs_tx,
        ready: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        connections: AtomicUsize::new(0),
        sockets: Mutex::new(BTreeMap::new()),
        next_connection: AtomicUsize::new(1),
        next_id: AtomicU64::new(1),
        stop: stop.clone(),
        log: log.clone(),
    });
    let worker_shared = shared.clone();
    let worker=std::thread::Builder::new().name("pocket-inference".into()).spawn(move||{
  let s=worker_shared;let load=Instant::now();let engine=match engine::load(&s.model.config,&s.model.weights,&s.model.tokenizer,s.model.precision){Ok(e)=>e,Err(e)=>{let _=loaded_tx.send(Err(e));return}};
  s.log.event("info","model_loaded",None,json!({"ms":load.elapsed().as_secs_f64()*1000.0,"model":s.model,"rss_kib":memory_kib("VmRSS:"),"peak_rss_kib":memory_kib("VmHWM:")}));
  let mut cache=VoiceCache::new(s.settings.voice_cache_bytes,cfg,s.log.clone());
  let ready=match s.voices.get(&s.settings.voice){Some(r)=>match cache.pin(r){Ok(())=>true,Err(e)=>{s.log.warn("default_voice_unavailable",e);false}},None=>{s.log.warn("default_voice_unavailable",format!("default '{}' not in validated voice catalog",s.settings.voice));false}};
  let _=loaded_tx.send(Ok(ready));
  while !s.stop.load(Ordering::Acquire){let job=match jobs_rx.recv_timeout(Duration::from_millis(100)){Ok(j)=>j,Err(RecvTimeoutError::Timeout)=>continue,Err(RecvTimeoutError::Disconnected)=>break};
   if job.cancel.load(Ordering::Acquire){continue}
   let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->Result<InferenceStats>{
    let record=s.voices.get(&job.voice).context("voice catalog mismatch")?;let voice=cache.get(record)?;
    s.log.event("info","inference_started",Some(job.id),json!({"voice":job.voice,"voice_state_sha256":record.cache_key.state_sha256,"received_us":job.received_us,"cache_key":record.cache_key}));
    let mut emit=|bytes|->Result<()>{let t=Instant::now();job.frames.send(Frame::Pcm(bytes)).context("PCM consumer disconnected")?;s.log.event("info","pcm_enqueued",Some(job.id),json!({"blocked_us":t.elapsed().as_micros()}));Ok(())};
    let mut event=|name:&str,data:Value|{s.log.event("info",name,Some(job.id),data);};
    let mut speed=crate::speed::Stream::new(s.settings.speed)?;
    let mut stats=engine.synthesize(&job.text,&voice,42,&job.cancel,&s.stop,&mut |bytes|speed.push(bytes,&mut emit),&mut event)?;
    speed.finish(&mut emit)?; stats.samples=speed.samples();stats.chunks=speed.chunks;
    s.log.event("info","speed_complete",Some(job.id),json!({"speed":s.settings.speed,"method":if s.settings.speed==1.0{"identity"}else{"streaming_wsola"},"peak_buffer_samples":speed.peak_buffer_samples}));
    Ok(stats)
   }));
   match result{Ok(Ok(stats))=>{let _=job.frames.send(Frame::End(stats));},Ok(Err(e))=>{let _=job.frames.send(Frame::Error(format!("{e:#}")));},Err(_)=>{s.ready.store(false,Ordering::Release);let _=job.frames.send(Frame::Error("runtime panic; service no longer ready".into()));break;}}
  }
  s.ready.store(false,Ordering::Release);s.log.event("info","inference_worker_stopped",None,json!({}));
 })?;
    let ready = loaded_rx
        .recv()
        .context("runtime failed during initialization")??;
    if ready {
        if let Err(error) = assets::commit_installation(&settings, &model, &shared.voices, &log) {
            stop.store(true, Ordering::Release);
            let _ = worker.join();
            return Err(error);
        }
    }
    shared.ready.store(ready, Ordering::Release);
    let listener = TcpListener::bind((settings.host.as_str(), settings.port))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    log.event("info","server_listening",None,json!({"address":address.to_string(),"ready":ready,"voices":shared.voices.keys().collect::<Vec<_>>(),"queue_capacity":1,"max_syntheses":1,"voice_cache_limit_bytes":settings.voice_cache_bytes}));
    if let Some(path) = &settings.ready_file {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"address":address.to_string(),"ready":ready,"pid":std::process::id(),"voices":shared.voices}),
            )?,
        )?;
    }
    let mut handles = Vec::new();
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if !reserve_connection(&shared.connections) {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let _ = send_error(
                        &mut stream,
                        "connection-limit",
                        "maximum 16 connections reached",
                    );
                    continue;
                }
                let connection_id = shared.next_connection.fetch_add(1, Ordering::Relaxed);
                match stream.try_clone() {
                    Ok(socket) => {
                        shared
                            .sockets
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(connection_id, socket);
                    }
                    Err(e) => {
                        shared.connections.fetch_sub(1, Ordering::AcqRel);
                        log.warn("socket_clone_failed", e);
                        continue;
                    }
                }
                let clone = shared.clone();
                handles.push(std::thread::spawn(move || {
                    if let Err(e) = connection(stream, clone.clone(), connection_id) {
                        clone.log.warn("connection_closed", e);
                    }
                }));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(e) => {
                stop.store(true, Ordering::Release);
                log.warn("listener_failed", e);
            }
        }
        let mut i = 0;
        while i < handles.len() {
            if handles[i].is_finished() {
                let h = handles.swap_remove(i);
                let _ = h.join();
            } else {
                i += 1;
            }
        }
    }
    shared.ready.store(false, Ordering::Release);
    drop(listener);
    for socket in shared
        .sockets
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
    {
        let _ = socket.shutdown(Shutdown::Both);
    }
    log.event("info", "shutdown_started", None, json!({}));
    for handle in handles {
        let _ = handle.join();
    }
    let _ = worker.join();
    if let Some(path) = &settings.ready_file {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"address":address.to_string(),"ready":false,"pid":std::process::id()}),
            )?,
        )?;
    }
    log.event("info", "shutdown_complete", None, json!({}));
    Ok(())
}
pub fn healthcheck(host: &str, port: u16) -> Result<()> {
    let address = std::net::ToSocketAddrs::to_socket_addrs(&(host, port))?
        .next()
        .context("health address")?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    protocol::write_event(&mut stream, &Event::new("describe", json!({})))?;
    let response = protocol::read_event(&mut BufReader::new(stream))?.context("health EOF")?;
    ensure!(
        response.kind == "info" && response.data["ready"] == true,
        "service not ready"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_voice_never_falls_back() {
        assert!(
            choose_voice(
                &json!({"voice":{"name":"missing"}}),
                "juergen",
                &BTreeMap::new()
            )
            .unwrap_err()
            .to_string()
            .contains("missing")
        );
        assert!(
            choose_voice(
                &json!({"voice":{"name":"a","speaker":"b"}}),
                "a",
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}
fn reserve_connection(counter: &AtomicUsize) -> bool {
    let mut n = counter.load(Ordering::Acquire);
    loop {
        if n >= 16 {
            return false;
        }
        match counter.compare_exchange_weak(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(actual) => n = actual,
        }
    }
}
