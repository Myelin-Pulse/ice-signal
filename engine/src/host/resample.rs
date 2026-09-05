//! Demo-only: converts the device stream (any rate, f32 mono) into the
//! core's fixed 16 kHz i16 stream, by linear interpolation.
//!
//! This stage does not port. On hardware the MEMS mic's PDM decimator feeds
//! the core at 16 kHz directly, and this file's job disappears.

pub struct Resampler {
    /// Output spacing measured in input samples (in_rate / out_rate).
    step: f64,
    /// Fractional position of the next output between `prev` and the
    /// current input sample.
    pos: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        Self {
            step: in_rate as f64 / out_rate as f64,
            pos: 0.0,
            prev: 0.0,
        }
    }

    pub fn push(&mut self, input: &[f32]) -> Vec<i16> {
        let mut out = Vec::with_capacity(input.len() * 2 / self.step.ceil() as usize + 2);
        for &x in input {
            while self.pos < 1.0 {
                let s = self.prev + (x - self.prev) * self.pos as f32;
                out.push((s * 32767.0).clamp(-32768.0, 32767.0) as i16);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.prev = x;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsamples_48k_to_16k_by_three() {
        let mut r = Resampler::new(48_000, 16_000);
        let out = r.push(&vec![0.5f32; 4800]);
        assert!((1598..=1602).contains(&out.len()), "got {}", out.len());
        // Constant input stays constant (first sample interpolates from 0).
        assert!(out[10..].iter().all(|&s| (s - 16383).abs() <= 1));
    }

    #[test]
    fn handles_non_integer_ratios_and_chunk_boundaries() {
        let mut r = Resampler::new(44_100, 16_000);
        let mut total = 0usize;
        for _ in 0..441 {
            total += r.push(&[0.25f32; 100]).len();
        }
        // 44100 input samples ≈ 16000 output samples.
        assert!((15990..=16010).contains(&total), "got {total}");
    }
}
