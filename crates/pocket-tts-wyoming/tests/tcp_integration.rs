use anyhow::{Context, Result, ensure};
use pocket_tts_wyoming::{
    assets,
    protocol::{self, Event},
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Service {
    child: Child,
    address: String,
    log: PathBuf,
    ready: PathBuf,
}
impl Service {
    fn start(
        root: &Path,
        voices: &Path,
        tag: &str,
        default: &str,
        download: bool,
        explicit: bool,
    ) -> Result<Self> {
        Self::start_mode(root, voices, tag, default, download, explicit, true)
    }
    fn start_mode(
        root: &Path,
        voices: &Path,
        tag: &str,
        default: &str,
        download: bool,
        explicit: bool,
        quantize: bool,
    ) -> Result<Self> {
        let output = root.join("artifacts/wyoming-tests");
        fs::create_dir_all(&output)?;
        let ready = output.join(format!("{tag}-ready.json"));
        let log = output.join(format!("{tag}-server.jsonl"));
        if ready.exists() {
            fs::remove_file(&ready)?;
        }
        let mut c = Command::new(root.join("target/release/pocket-tts-wyoming"));
        c.current_dir(root)
            .env("POCKET_TTS_MODELS_DIR", root.join("models"))
            .env("POCKET_TTS_VOICES_DIR", voices)
            .env(
                "POCKET_TTS_QUANTIZE",
                if quantize { "true" } else { "false" },
            )
            .env("POCKET_TTS_THREADS", "2")
            .env("POCKET_TTS_VOICE", default)
            .env("POCKET_TTS_LANGUAGE", "german")
            .env("WYOMING_HOST", "127.0.0.1")
            .env("WYOMING_PORT", "0")
            .env("RUST_LOG", "info")
            .env(
                "POCKET_TTS_DOWNLOAD_VOICES",
                if download { "true" } else { "false" },
            )
            .env("POCKET_TTS_READY_FILE", &ready)
            .stdout(Stdio::from(File::create(&log)?))
            .stderr(Stdio::from(File::create(
                output.join(format!("{tag}-stderr.log")),
            )?));
        if explicit {
            c.env("POCKET_TTS_MODEL_PATH", root.join("models/german"));
        } else {
            c.env_remove("POCKET_TTS_MODEL_PATH");
        }
        let mut child = c.spawn()?;
        let start = Instant::now();
        loop {
            if ready.is_file() {
                let v: Value = serde_json::from_slice(&fs::read(&ready)?)?;
                return Ok(Self {
                    child,
                    address: v["address"].as_str().context("ready address")?.to_owned(),
                    log,
                    ready,
                });
            }
            if let Some(exit) = child.try_wait()? {
                anyhow::bail!(
                    "service exited before readiness: {exit}; inspect {}",
                    log.display()
                );
            }
            if start.elapsed() > Duration::from_secs(180) {
                let _ = child.kill();
                anyhow::bail!("service startup timeout");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn terminate(&mut self) -> Result<f64> {
        let start = Instant::now();
        let status = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status()?;
        ensure!(status.success(), "SIGTERM failed");
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(status.success(), "server shutdown error {status}");
                break;
            }
            ensure!(
                start.elapsed() < Duration::from_secs(5),
                "shutdown exceeded 5 seconds"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(start.elapsed().as_secs_f64())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
struct Client {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}
impl Client {
    fn connect(address: &str) -> Result<Self> {
        let writer = TcpStream::connect(address)?;
        writer.set_nodelay(true)?;
        writer.set_read_timeout(Some(Duration::from_secs(120)))?;
        writer.set_write_timeout(Some(Duration::from_secs(5)))?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Self { writer, reader })
    }
    fn send(&mut self, kind: &str, data: Value) -> Result<()> {
        // Real upstream-style additional-data framing, independently constructed.
        let data = serde_json::to_vec(&data)?;
        let header = format!("{{\"type\":\"{kind}\",\"data_length\":{}}}\n", data.len());
        self.writer.write_all(header.as_bytes())?;
        self.writer.write_all(&data)?;
        self.writer.flush()?;
        Ok(())
    }
    fn next(&mut self) -> Result<Event> {
        protocol::read_event(&mut self.reader)?.context("unexpected TCP EOF")
    }
    fn describe(&mut self) -> Result<Value> {
        self.send("describe", json!({}))?;
        let event = self.next()?;
        ensure!(event.kind == "info", "describe failed");
        Ok(event.data)
    }
    fn run(
        &mut self,
        text: &str,
        voice: Option<Value>,
        path: &Path,
        delay_ms: u64,
    ) -> Result<Value> {
        let start = Instant::now();
        let mut data = json!({"text":text});
        if let Some(v) = voice {
            data["voice"] = v;
        }
        if delay_ms > 0 {
            socket2::SockRef::from(&self.writer).set_recv_buffer_size(4096)?;
        }
        self.send("synthesize", data)?;
        self.drain(start, path, delay_ms)
    }
    fn drain(&mut self, start: Instant, path: &Path, delay_ms: u64) -> Result<Value> {
        let first = self.next()?;
        self.drain_started(start, path, delay_ms, first)
    }
    fn drain_started(
        &mut self,
        start: Instant,
        path: &Path,
        delay_ms: u64,
        first: Event,
    ) -> Result<Value> {
        ensure!(
            first.kind == "audio-start",
            "expected audio-start, got {} {:?}",
            first.kind,
            first.data
        );
        for (k, n) in [("rate", 24000), ("width", 2), ("channels", 1)] {
            ensure!(first.data[k] == n, "incorrect audio-start {k}");
        }
        let mut wav = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        let mut events = File::create(path.with_extension("consumer.jsonl"))?;
        writeln!(
            events,
            "{}",
            json!({"event":"audio_start","elapsed_us":start.elapsed().as_micros()})
        )?;
        let mut first_ms = None;
        let mut chunks = 0usize;
        let mut samples = 0usize;
        let mut peak = 0i32;
        loop {
            let event = self.next()?;
            match event.kind.as_str() {
                "audio-chunk" => {
                    ensure!(
                        event.data["rate"] == 24000
                            && event.data["width"] == 2
                            && event.data["channels"] == 1,
                        "invalid chunk format"
                    );
                    ensure!(
                        event.data["timestamp"] == samples * 1000 / 24000,
                        "chunk timestamp mismatch"
                    );
                    ensure!(
                        !event.payload.is_empty()
                            && event.payload.len() <= 3840
                            && event.payload.len() % 2 == 0,
                        "invalid PCM chunk"
                    );
                    let elapsed = start.elapsed();
                    if first_ms.is_none() {
                        first_ms = Some(elapsed.as_secs_f64() * 1000.0);
                    }
                    writeln!(
                        events,
                        "{}",
                        json!({"event":"pcm_chunk","index":chunks,"elapsed_us":elapsed.as_micros()})
                    )?;
                    for bytes in event.payload.chunks_exact(2) {
                        let v = i16::from_le_bytes([bytes[0], bytes[1]]);
                        peak = peak.max((v as i32).abs());
                        wav.write_sample(v)?;
                    }
                    wav.flush()?;
                    samples += event.payload.len() / 2;
                    chunks += 1;
                    if delay_ms > 0 {
                        std::thread::sleep(Duration::from_millis(delay_ms));
                    }
                }
                "audio-stop" => {
                    ensure!(
                        event.data["timestamp"] == samples * 1000 / 24000,
                        "audio-stop timestamp mismatch"
                    );
                    writeln!(
                        events,
                        "{}",
                        json!({"event":"audio_stop","elapsed_us":start.elapsed().as_micros()})
                    )?;
                    break;
                }
                _ => anyhow::bail!("unexpected {} {:?}", event.kind, event.data),
            }
        }
        wav.finalize()?;
        ensure!(chunks > 1 && peak > 32, "empty/invalid audio");
        let seconds = start.elapsed().as_secs_f64();
        let summary = json!({"ttfa_consumer_ms":first_ms.unwrap(),"seconds":seconds,"audio_seconds":samples as f64/24000.0,"chunks":chunks,"samples":samples,"peak_pcm":peak,"wav":path,"sha256":assets::hash(path)?});
        fs::write(
            path.with_extension("client.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        Ok(summary)
    }
}
fn records(path: &Path) -> Result<Vec<Value>> {
    fs::read_to_string(path)?
        .lines()
        .map(|l| serde_json::from_str(l).map_err(Into::into))
        .collect()
}
fn verify_streaming(logs: &[Value]) -> Result<Vec<Value>> {
    let complete = logs
        .iter()
        .filter(|e| e["event"] == "synthesis_complete")
        .collect::<Vec<_>>();
    ensure!(!complete.is_empty(), "no complete syntheses");
    for completion in &complete {
        let id = &completion["request"];
        let rows = logs
            .iter()
            .filter(|e| &e["request"] == id)
            .collect::<Vec<_>>();
        let time = |name: &str| -> Result<u64> {
            rows.iter()
                .find(|e| e["event"] == name)
                .and_then(|e| e["monotonic_us"].as_u64())
                .context(format!("missing {name} for {id}"))
        };
        let first = time("audio_chunk_sent")?;
        ensure!(
            time("synthesize_received")? < time("audio_start_sent")?
                && time("audio_start_sent")? <= first,
            "audio-start order"
        );
        ensure!(
            first < time("inference_end")?,
            "first PCM came after full inference"
        );
        for stage in ["flow_step_start", "mimi_output"] {
            ensure!(
                rows.iter().any(|e| e["event"] == stage
                    && e["monotonic_us"].as_u64().is_some_and(|t| t > first)),
                "no later real {stage}"
            );
        }
        let last = rows
            .iter()
            .filter(|e| e["event"] == "audio_chunk_sent")
            .map(|e| e["monotonic_us"].as_u64().unwrap())
            .max()
            .unwrap();
        ensure!(last < time("audio_stop_sent")?, "stop before last chunk");
        ensure!(
            completion["data"]["nonfinite_values"] == 0
                && completion["data"]["all_segments_eos"] == true,
            "invalid inference completion"
        );
    }
    Ok(complete.into_iter().cloned().collect())
}
#[test]
#[ignore = "requires pinned real model assets, official voices, and release binary"]
fn real_tcp_voices_streaming_cache_endurance_and_shutdown() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let output = root.join("artifacts/wyoming-tests");
    fs::create_dir_all(&output)?;
    let voice_root = output.join("voices");
    fs::create_dir_all(voice_root.join("official"))?;
    fs::create_dir_all(voice_root.join("local"))?;
    let manifest = assets::manifest();
    for voice in &manifest.voices {
        let source = root
            .join("voices/official/german")
            .join(&manifest.revision)
            .join(format!("{}.safetensors", voice.name));
        let old = root
            .join("voices/official")
            .join(format!("{}.safetensors", voice.name));
        let source = if source.is_file() { source } else { old };
        ensure!(
            source.is_file(),
            "download official voice assets first: {}",
            voice.name
        );
        let dest = voice_root
            .join("official")
            .join(format!("{}.safetensors", voice.name));
        if !dest.exists() {
            fs::hard_link(source, dest)?;
        }
    }
    let default = "juergen";
    let default_record = manifest
        .voices
        .iter()
        .find(|v| v.name == default)
        .context("configured default absent from current official manifest")?;
    let alias = voice_root.join("local/installed-copy.safetensors");
    if !alias.exists() {
        fs::hard_link(
            voice_root
                .join("official")
                .join(format!("{}.safetensors", default_record.name)),
            &alias,
        )?;
    }
    fs::write(
        voice_root.join("local/invalid.safetensors"),
        b"not a valid voice state",
    )?;
    for (name, revision, format, quantization) in [
        (
            "foreign-revision",
            "another-revision",
            "pocket-tts-kv-v1",
            "fp32-kv",
        ),
        (
            "wrong-quantization",
            manifest.revision.as_str(),
            "pocket-tts-kv-v1",
            "fp32",
        ),
        (
            "unknown-format",
            manifest.revision.as_str(),
            "unknown-format",
            "fp32-kv",
        ),
    ] {
        let path = voice_root.join(format!("local/{name}.safetensors"));
        if !path.exists() {
            fs::hard_link(&alias, &path)?;
        }
        fs::write(
            path.with_extension("voice.json"),
            serde_json::to_vec(
                &json!({"model_sha256":manifest.model_sha256,"model_revision":revision,"state_format":format,"quantization":quantization,"language":"german"}),
            )?,
        )?;
    }
    let mut service = Service::start(&root, &voice_root, "main", default, true, false)?;
    let mut client = Client::connect(&service.address)?;
    let info = client.describe()?;
    ensure!(
        info["ready"] == true && info["tts"][0]["name"] == "pocket-tts",
        "service metadata wrong"
    );
    let names = info["tts"][0]["voices"]
        .as_array()
        .context("voices metadata")?
        .iter()
        .map(|v| {
            ensure!(
                v["installed"] == true && v["languages"] == json!(["de-DE"]),
                "voice unavailable/languages wrong"
            );
            Ok(v["name"].as_str().unwrap().to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    for v in &manifest.voices {
        ensure!(names.contains(&v.name), "Describe missing {}", v.name);
    }
    ensure!(
        names.contains(&"local/installed-copy".into()) && !names.contains(&"local/invalid".into()),
        "local discovery wrong"
    );
    for name in ["foreign-revision", "wrong-quantization", "unknown-format"] {
        ensure!(
            !names.contains(&format!("local/{name}")),
            "incompatible state advertised: {name}"
        );
    }
    fs::write(
        output.join("describe.json"),
        serde_json::to_vec_pretty(&info)?,
    )?;
    let long = fs::read_to_string(root.join("tests/fixtures/long-german.txt"))?;
    let short = fs::read_to_string(root.join("tests/fixtures/short-german.txt"))?;
    let mut selected = vec![default.to_owned()];
    selected.extend(
        manifest
            .voices
            .iter()
            .filter(|v| v.name != default)
            .take(2)
            .map(|v| v.name.clone()),
    );
    ensure!(selected.len() == 3, "need two additional official voices");
    let mut examples = Vec::new();
    for (i, name) in selected.iter().enumerate() {
        let choice = if i == 0 {
            None
        } else {
            Some(json!({"name":name}))
        };
        let path = output.join(format!("{name}-long.wav"));
        let summary = client.run(&long, choice, &path, 0)?;
        examples.push(json!({"voice":name,"client":summary}));
    }
    ensure!(
        examples[0]["client"]["sha256"] != examples[1]["client"]["sha256"]
            && examples[1]["client"]["sha256"] != examples[2]["client"]["sha256"],
        "voice selection did not change waveform"
    );
    client.run(
        &short,
        Some(json!({"name":"german","speaker":selected[1]})),
        &output.join("group-speaker.wav"),
        0,
    )?;
    client.run(
        &short,
        Some(json!({"name":"local/installed-copy"})),
        &output.join("local-installed.wav"),
        0,
    )?;
    client.send(
        "synthesize",
        json!({"text":short,"voice":{"name":"missing-voice"}}),
    )?;
    let error = client.next()?;
    ensure!(
        error.kind == "error" && error.data["code"] == "unknown-voice",
        "unknown voice silently fell back"
    );
    client.send("synthesize", json!({"text":short,"text_format":"ssml"}))?;
    ensure!(client.next()?.kind == "error", "SSML not rejected");
    let busy_start = Instant::now();
    client.send("synthesize", json!({"text":long,"voice":{"name":default}}))?;
    let audio_start = client.next()?;
    ensure!(
        audio_start.kind == "audio-start",
        "primary synthesis not admitted"
    );
    let mut second = Client::connect(&service.address)?;
    second.send("synthesize", json!({"text":short}))?;
    let busy = second.next()?;
    ensure!(
        busy.kind == "error" && busy.data["code"] == "busy",
        "concurrent synthesis not bounded"
    );
    drop(second);
    client.drain_started(busy_start, &output.join("busy-primary.wav"), 0, audio_start)?;
    drop(client);
    // Slow actual TCP receiver exercises kernel+bounded-channel backpressure.
    let mut slow = Client::connect(&service.address)?;
    slow.run(
        &long,
        Some(json!({"name":default})),
        &output.join("slow-tcp.wav"),
        120,
    )?;
    drop(slow);
    // Client disconnect must release the worker and preserve the loaded model.
    let mut abort = Client::connect(&service.address)?;
    abort.send("synthesize", json!({"text":long}))?;
    ensure!(abort.next()?.kind == "audio-start", "abort request start");
    ensure!(abort.next()?.kind == "audio-chunk", "abort request chunk");
    abort.writer.shutdown(std::net::Shutdown::Both)?;
    drop(abort);
    let mut client = Client::connect(&service.address)?;
    let wait = Instant::now();
    loop {
        if client.describe()?["pocket_tts"]["busy"] == false {
            break;
        }
        ensure!(
            wait.elapsed() < Duration::from_secs(5),
            "disconnect did not release synthesis"
        );
        std::thread::sleep(Duration::from_millis(30));
    }
    // Sequential soak, rotating all official voices, forcing LRU evictions.
    for n in 0..60 {
        let voice = &manifest.voices[n % manifest.voices.len()].name;
        let text = if n % 10 == 0 { &long } else { &short };
        client.run(
            text,
            Some(json!({"name":voice})),
            &output.join(format!("soak-{n:02}.wav")),
            0,
        )?;
    }
    drop(client);
    let shutdown = service.terminate()?;
    let logs = records(&service.log)?;
    let completed = verify_streaming(&logs)?;
    ensure!(
        logs.iter().filter(|e| e["event"] == "model_loaded").count() == 1,
        "model reloaded per request"
    );
    ensure!(
        !logs.iter().any(|e| e["event"] == "asset_downloaded"),
        "valid installed assets downloaded again"
    );
    ensure!(
        logs.iter().any(|e| e["event"] == "voice_rejected"),
        "invalid voice not warned"
    );
    ensure!(
        logs.iter().any(|e| e["event"] == "voice_cache_evicted"),
        "LRU not exercised"
    );
    for row in logs.iter().filter(|e| e["event"] == "voice_cache_loaded") {
        ensure!(
            row["data"]["cache_bytes"].as_u64().unwrap() <= 32 * 1024 * 1024,
            "voice cache exceeds budget"
        );
    }
    ensure!(
        logs.iter().any(|e| e["event"] == "pcm_enqueued"
            && e["data"]["blocked_us"].as_u64().is_some_and(|v| v > 50000)),
        "slow TCP consumer did not trigger backpressure"
    );
    let rss = completed
        .iter()
        .map(|e| e["data"]["rss_kib"].as_u64().unwrap())
        .collect::<Vec<_>>();
    let first_max = *rss.iter().take(12).max().unwrap();
    let last_max = *rss.iter().rev().take(12).max().unwrap();
    ensure!(
        last_max <= first_max + 96 * 1024,
        "sustained RSS growth during soak"
    );
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(
            &json!({"examples":examples,"official_count":manifest.voices.len(),"available_count":names.len(),"completed_requests":completed.len(),"shutdown_seconds":shutdown,"first12_max_rss_kib":first_max,"last12_max_rss_kib":last_max,"server_metrics":completed.iter().map(|e|&e["data"]).collect::<Vec<_>>()}),
        )?,
    )?;
    // Restart with another configured default proves absence of a Juergen-only code path.
    let mut restart = Service::start(&root, &voice_root, "restart", &selected[1], false, true)?;
    let mut c = Client::connect(&restart.address)?;
    ensure!(
        c.describe()?["pocket_tts"]["default_voice"] == selected[1],
        "default config ignored"
    );
    c.run(&short, None, &output.join("alternate-default.wav"), 0)?;
    drop(c);
    restart.terminate()?;
    let restarted = records(&restart.log)?;
    ensure!(
        restarted
            .iter()
            .any(|e| e["event"] == "synthesis_complete" && e["data"]["voice"] == selected[1]),
        "configured alternate default not synthesized"
    );
    // No valid voices => process serves diagnostics, health is false.
    let empty = output.join("empty-voices");
    fs::create_dir_all(&empty)?;
    fs::write(empty.join("broken.safetensors"), b"broken")?;
    let mut unavailable = Service::start(&root, &empty, "empty", default, false, true)?;
    let mut c = Client::connect(&unavailable.address)?;
    let info = c.describe()?;
    ensure!(
        info["ready"] == false && info["tts"][0]["voices"].as_array().unwrap().is_empty(),
        "empty registry ready"
    );
    let address: std::net::SocketAddr = unavailable.address.parse()?;
    ensure!(
        pocket_tts_wyoming::server::healthcheck("127.0.0.1", address.port()).is_err(),
        "health passed without voice"
    );
    c.send("synthesize", json!({"text":short}))?;
    ensure!(
        c.next()?.kind == "error",
        "empty registry synthesis accepted"
    );
    drop(c);
    unavailable.terminate()?;
    // SIGTERM also interrupts an active request and an idle connection.
    let mut active = Service::start(&root, &voice_root, "active-shutdown", default, false, true)?;
    let mut c = Client::connect(&active.address)?;
    c.send("synthesize", json!({"text":long}))?;
    ensure!(c.next()?.kind == "audio-start", "active shutdown start");
    ensure!(c.next()?.kind == "audio-chunk", "active shutdown PCM");
    let _idle = Client::connect(&active.address)?;
    let seconds = active.terminate()?;
    ensure!(seconds < 2.0, "active/idle connections delayed shutdown");
    ensure!(
        serde_json::from_slice::<Value>(&fs::read(&active.ready)?)?["ready"] == false,
        "shutdown readiness remained true"
    );
    let mut fp32 = Service::start_mode(&root, &voice_root, "fp32", default, false, true, false)?;
    let mut c = Client::connect(&fp32.address)?;
    c.run(&long, None, &output.join("fp32-juergen-long.wav"), 0)?;
    drop(c);
    fp32.terminate()?;
    verify_streaming(&records(&fp32.log)?)?;
    Ok(())
}
