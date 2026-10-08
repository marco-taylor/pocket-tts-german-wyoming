use serde_json::{Value, json};
use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Instant,
};
#[derive(Clone)]
pub struct Log {
    origin: Instant,
    level: u8,
    lock: Arc<Mutex<()>>,
}
fn rank(s: &str) -> u8 {
    match s {
        "error" => 0,
        "warn" => 1,
        "debug" => 3,
        "trace" => 4,
        _ => 2,
    }
}
impl Log {
    pub fn new(level: &str) -> Self {
        Self {
            origin: Instant::now(),
            level: rank(level),
            lock: Arc::new(Mutex::new(())),
        }
    }
    pub fn stamp(&self) -> u64 {
        self.origin.elapsed().as_micros() as u64
    }
    pub fn event(&self, level: &str, event: &str, request: Option<u64>, data: Value) -> u64 {
        let us = self.stamp();
        if rank(level) <= self.level {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let _ = writeln!(
                std::io::stdout().lock(),
                "{}",
                json!({"level":level,"event":event,"request":request,"monotonic_us":us,"data":data})
            );
        }
        us
    }
    pub fn warn(&self, event: &str, text: impl std::fmt::Display) {
        self.event("warn", event, None, json!({"text":text.to_string()}));
    }
}
pub fn memory_kib(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with(key)).map(str::to_owned))
        .and_then(|s| s.split_whitespace().nth(1)?.parse().ok())
        .unwrap_or(0)
}
