//! Bounded streaming WSOLA at 24 kHz; identity speed bypasses all DSP.
use anyhow::{Context, Result, ensure};
const H: usize = 480;
const SEARCH: usize = 240;
pub fn parse(value: Option<&str>) -> Result<f64> {
    let s = value.unwrap_or("1.0");
    let x: f64 = s
        .parse()
        .with_context(|| format!("POCKET_TTS_SPEED={s:?}: expected finite number in 0.8..=1.2"))?;
    ensure!(
        x.is_finite() && (0.8..=1.2).contains(&x),
        "POCKET_TTS_SPEED={s:?}: expected finite number in 0.8..=1.2; no clamping"
    );
    Ok(x)
}
pub struct Stream {
    speed: f64,
    input: Vec<f32>,
    base: usize,
    received: usize,
    produced: usize,
    next: f64,
    tail: Vec<f32>,
    started: bool,
    pub chunks: usize,
    pub peak_buffer_samples: usize,
}
impl Stream {
    pub fn new(speed: f64) -> Result<Self> {
        parse(Some(&speed.to_string()))?;
        Ok(Self {
            speed,
            input: Vec::new(),
            base: 0,
            received: 0,
            produced: 0,
            next: 0.,
            tail: vec![0.; H],
            started: false,
            chunks: 0,
            peak_buffer_samples: 0,
        })
    }
    pub fn samples(&self) -> usize {
        self.produced
    }
    pub fn push(
        &mut self,
        bytes: Vec<u8>,
        sink: &mut dyn FnMut(Vec<u8>) -> Result<()>,
    ) -> Result<()> {
        ensure!(
            bytes.len() % 2 == 0 && bytes.len() <= 3840,
            "invalid PCM for speed processing"
        );
        self.received += bytes.len() / 2;
        if self.speed == 1.0 {
            self.produced += bytes.len() / 2;
            self.chunks += 1;
            return sink(bytes);
        }
        self.input.extend(
            bytes
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32),
        );
        self.peak_buffer_samples = self.peak_buffer_samples.max(self.input.len());
        self.process(false, sink)
    }
    pub fn finish(&mut self, sink: &mut dyn FnMut(Vec<u8>) -> Result<()>) -> Result<()> {
        if self.speed == 1.0 {
            return Ok(());
        }
        self.input.resize(self.input.len() + 2 * H + SEARCH, 0.);
        self.peak_buffer_samples = self.peak_buffer_samples.max(self.input.len());
        self.process(true, sink)
    }
    fn process(&mut self, finish: bool, sink: &mut dyn FnMut(Vec<u8>) -> Result<()>) -> Result<()> {
        let target = (self.received as f64 / self.speed).round() as usize;
        let mut output = Vec::with_capacity(3840);
        while self.produced < target {
            let nominal = self.next.round() as usize;
            let lo = nominal.saturating_sub(SEARCH).max(self.base);
            let available = self.base + self.input.len();
            if available < 2 * H || (!finish && available < nominal + SEARCH + 2 * H) {
                break;
            }
            let hi = (nominal + SEARCH).min(available.saturating_sub(2 * H));
            if lo > hi {
                break;
            }
            let chosen = if !self.started {
                0
            } else {
                let mut best = lo;
                let mut score = f64::NEG_INFINITY;
                let energy: f64 = self
                    .tail
                    .iter()
                    .step_by(4)
                    .map(|x| (*x as f64).powi(2))
                    .sum();
                for c in lo..=hi {
                    let window = &self.input[c - self.base..c - self.base + H];
                    let mut dot = 0.;
                    let mut norm = 0.;
                    for (&x, &y) in self.tail.iter().zip(window).step_by(4) {
                        dot += x as f64 * y as f64;
                        norm += (y as f64).powi(2)
                    }
                    let correlation = if energy * norm > 1. {
                        dot / (energy * norm).sqrt()
                    } else {
                        0.
                    };
                    let candidate = correlation - 0.000001 * (c as f64 - self.next).abs();
                    if candidate > score {
                        score = candidate;
                        best = c
                    }
                }
                best
            };
            let n = H.min(target - self.produced);
            for j in 0..n {
                let x = self.input[chosen - self.base + j];
                let value = if self.started {
                    let w = j as f32 / H as f32;
                    self.tail[j] * (1. - w) + x * w
                } else {
                    x
                };
                let sample = value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                output.extend_from_slice(&sample.to_le_bytes());
            }
            self.tail
                .copy_from_slice(&self.input[chosen - self.base + H..chosen - self.base + 2 * H]);
            self.started = true;
            self.produced += n;
            self.next += H as f64 * self.speed;
            if output.len() >= 3840 {
                self.chunks += 1;
                sink(std::mem::take(&mut output))?
            }
        }
        if !output.is_empty() {
            self.chunks += 1;
            sink(output)?
        }
        let remove = (self.next.floor() as usize)
            .saturating_sub(SEARCH)
            .saturating_sub(self.base)
            .min(self.input.len());
        self.input.drain(..remove);
        self.base += remove;
        if finish {
            ensure!(
                self.produced == target,
                "WSOLA incomplete tail: {} vs {}",
                self.produced,
                target
            )
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_configuration() {
        for x in [
            "0.8", "0.85", "0.9", "0.95", "1.0", "1.05", "1.1", "1.15", "1.2",
        ] {
            assert!(parse(Some(x)).is_ok());
        }
        for x in [
            "0", "0.5", "0.79", "1.21", "1.5", "2.0", "-1", "abc", "NaN", "inf", "",
        ] {
            assert!(parse(Some(x)).is_err());
        }
        assert_eq!(parse(None).unwrap(), 1.0);
    }
    #[test]
    fn identity_is_byte_exact() {
        let bytes: Vec<_> = (-1920i16..0).flat_map(i16::to_le_bytes).collect();
        for speed in [parse(None).unwrap(), 1.0] {
            let mut s = Stream::new(speed).unwrap();
            let mut out = vec![];
            s.push(bytes.clone(), &mut |b| {
                out.extend(b);
                Ok(())
            })
            .unwrap();
            s.finish(&mut |b| {
                out.extend(b);
                Ok(())
            })
            .unwrap();
            assert_eq!(bytes, out);
        }
    }
    #[test]
    fn streaming_duration_pitch_and_memory() {
        for speed in [0.8, 0.9, 1.0, 1.1, 1.2] {
            let mut s = Stream::new(speed).unwrap();
            let mut out = vec![];
            let samples: Vec<i16> = (0..48000)
                .map(|i| {
                    (12000. * (i as f64 * std::f64::consts::TAU * 200. / 24000.).sin()).round()
                        as i16
                })
                .collect();
            for (frame, i) in samples.chunks(1920).zip(0..) {
                s.push(
                    frame.iter().flat_map(|x| x.to_le_bytes()).collect(),
                    &mut |b| {
                        out.extend(b.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
                        Ok(())
                    },
                )
                .unwrap();
                if i == 0 {
                    assert!(!out.is_empty());
                }
            }
            s.finish(&mut |b| {
                out.extend(b.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
                Ok(())
            })
            .unwrap();
            assert_eq!(out.len(), (samples.len() as f64 / speed).round() as usize);
            let crossings = out[2400..out.len() - 2400]
                .windows(2)
                .filter(|w| w[0] <= 0 && w[1] > 0)
                .count();
            let hz = crossings as f64 * 24000. / (out.len() - 4800) as f64;
            assert!((hz - 200.).abs() < 3., "{speed}: {hz}");
            assert!(s.peak_buffer_samples < 7000);
        }
    }
}
