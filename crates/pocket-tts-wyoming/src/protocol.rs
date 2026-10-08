use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
const MAX_HEADER: usize = 16384;
#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "empty")]
    pub data: Value,
    #[serde(skip)]
    pub payload: Vec<u8>,
}
fn empty() -> Value {
    json!({})
}
impl Event {
    pub fn new(kind: &str, data: Value) -> Self {
        Self {
            kind: kind.into(),
            data,
            payload: vec![],
        }
    }
    pub fn error(code: &str, text: impl std::fmt::Display) -> Self {
        Self::new("error", json!({"code":code,"text":text.to_string()}))
    }
}
pub fn read_event(reader: &mut impl BufRead) -> Result<Option<Event>> {
    let mut header = Vec::new();
    let n = (&mut *reader)
        .take((MAX_HEADER + 1) as u64)
        .read_until(b'\n', &mut header)?;
    if n == 0 {
        return Ok(None);
    }
    ensure!(
        n <= MAX_HEADER && header.last() == Some(&b'\n'),
        "Wyoming header too long or incomplete"
    );
    let mut raw: Value = serde_json::from_slice(&header).context("invalid Wyoming JSON header")?;
    ensure!(raw.is_object(), "Wyoming header must be object");
    let length = |key: &str, max: usize| -> Result<usize> {
        match raw.get(key) {
            None => Ok(0),
            Some(v) => {
                let n = v.as_u64().context(format!("invalid {key}"))?;
                ensure!(n <= max as u64, "{key} exceeds limit");
                Ok(n as usize)
            }
        }
    };
    let dl = length("data_length", 65536)?;
    let pl = length("payload_length", 1048576)?;
    let mut data = raw.get("data").cloned().unwrap_or_else(empty);
    ensure!(data.is_object(), "event data must be object");
    if dl > 0 {
        let mut b = vec![0; dl];
        reader.read_exact(&mut b)?;
        let extra: Value = serde_json::from_slice(&b).context("invalid Wyoming additional data")?;
        let extra = extra
            .as_object()
            .context("additional data must be object")?;
        data.as_object_mut().unwrap().extend(extra.clone());
    }
    let mut payload = vec![0; pl];
    reader.read_exact(&mut payload)?;
    raw["data"] = data;
    let mut event: Event = serde_json::from_value(raw)?;
    ensure!(!event.kind.is_empty(), "empty event type");
    event.payload = payload;
    Ok(Some(event))
}
pub fn write_event(writer: &mut impl Write, event: &Event) -> Result<()> {
    let mut v = json!({"type":event.kind,"data":event.data});
    if !event.payload.is_empty() {
        v["payload_length"] = json!(event.payload.len());
    }
    let mut header = serde_json::to_vec(&v)?;
    header.push(b'\n');
    writer.write_all(&header)?;
    writer.write_all(&event.payload)?;
    writer.flush()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_data_payload_and_next_event() {
        let extra = r#"{"text":"Grüße","x":2}"#.as_bytes();
        let mut b=format!("{{\"type\":\"synthesize\",\"data\":{{\"x\":1}},\"data_length\":{},\"payload_length\":2}}\n",extra.len()).into_bytes();
        b.extend(extra);
        b.extend([0, 255]);
        b.extend(b"{\"type\":\"describe\"}\n");
        let mut r = std::io::Cursor::new(b);
        let e = read_event(&mut r).unwrap().unwrap();
        assert_eq!(e.data["x"], 2);
        assert_eq!(e.data["text"], "Grüße");
        assert_eq!(e.payload, [0, 255]);
        assert_eq!(read_event(&mut r).unwrap().unwrap().kind, "describe");
    }
    #[test]
    fn rejects_unbounded_or_truncated_frames() {
        for b in [
            b"{\"type\":\"x\",\"payload_length\":1048577}\n".as_slice(),
            b"{\"type\":\"x\",\"data_length\":-1}\n",
            b"{\"type\":\"x\",\"payload_length\":2}\n\x00",
            b"{\"type\":\"x\"}",
        ] {
            assert!(read_event(&mut std::io::Cursor::new(b)).is_err());
        }
    }
}
