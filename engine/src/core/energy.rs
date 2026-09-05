//! Short-time energy as a per-sample running accumulator.
//!
//! No frame buffer exists: each sample is squared into the accumulator and
//! gone. In RTL this is one multiplier and a 39-bit accumulator register
//! (320 * 32767² < 2^39), cleared on each frame strobe.

use super::FRAME_LEN;

pub struct EnergyAccumulator {
    sum_sq: u64,
    count: u32,
}

impl EnergyAccumulator {
    pub fn new() -> Self {
        Self { sum_sq: 0, count: 0 }
    }

    /// Accumulate one sample. Returns the frame's sum of squares when this
    /// sample completes a frame, then resets for the next frame.
    pub fn push(&mut self, sample: i16) -> Option<u64> {
        let s = sample as i64;
        self.sum_sq += (s * s) as u64;
        self.count += 1;
        if self.count == FRAME_LEN {
            let out = self.sum_sq;
            self.sum_sq = 0;
            self.count = 0;
            Some(out)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_exactly_on_frame_boundary() {
        let mut acc = EnergyAccumulator::new();
        for _ in 0..FRAME_LEN - 1 {
            assert_eq!(acc.push(1000), None);
        }
        assert!(acc.push(1000).is_some());
        assert_eq!(acc.push(1000), None); // next frame started fresh
    }

    #[test]
    fn full_scale_square_wave_energy_is_exact() {
        let mut acc = EnergyAccumulator::new();
        let mut out = None;
        for i in 0..FRAME_LEN {
            let s = if i % 2 == 0 { 32767 } else { -32767 };
            out = acc.push(s);
        }
        assert_eq!(out, Some(FRAME_LEN as u64 * 32767 * 32767));
    }

    #[test]
    fn silence_is_zero() {
        let mut acc = EnergyAccumulator::new();
        let mut out = None;
        for _ in 0..FRAME_LEN {
            out = acc.push(0);
        }
        assert_eq!(out, Some(0));
    }
}
