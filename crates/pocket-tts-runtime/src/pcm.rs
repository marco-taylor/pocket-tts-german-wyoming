/// Output is always signed 16-bit little endian. The normal inference path rejects
/// non-finite tensors first; this conversion is defensive for other consumers.
pub fn to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &x in samples {
        let x = if x.is_nan() { 0.0 } else { x.clamp(-1.0, 1.0) };
        let v = (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipping_rounding_nonfinite_and_endianness() {
        let p = to_pcm16(&[
            -2.0,
            -1.0,
            0.0,
            0.5,
            1.0,
            2.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1.5 / 32768.0,
        ]);
        let values: Vec<_> = p
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(
            values,
            [-32768, -32768, 0, 16384, 32767, 32767, 0, 32767, -32768, 2]
        );
        assert_eq!(&p[6..8], &[0, 64]);
    }
}
