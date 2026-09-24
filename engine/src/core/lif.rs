//! Leaky integrate-and-fire (LIF) neuron in fixed point — the seed of the
//! SNN story. The membrane potential is one register; leak is a right shift;
//! firing is a compare-and-reset. This is exactly the synthesizable form.
//!
//! Stepped once per frame (50 Hz), driven by noise-gated frame power.
//! Sustained voice charges the membrane past threshold and produces a spike
//! train; the VAD downstream reads activity from that train, not from the
//! raw energy.

/// Drive is saturated here so one impulsive bang (clap, door) cannot dump
/// unbounded charge in a single frame.
pub const INPUT_CAP: u32 = 1 << 17;
pub const LEAK_SHIFT: u32 = 3; // lose 1/8 of the potential per frame
pub const SPIKE_THRESHOLD: u32 = 1 << 18;

pub struct LifNeuron {
    potential: u32,
    leak_shift: u32,
    threshold: u32,
}

impl LifNeuron {
    pub fn new(leak_shift: u32, threshold: u32) -> Self {
        Self {
            potential: 0,
            leak_shift,
            threshold,
        }
    }

    /// Tuning for the speech-onset neuron. Equilibrium potential is
    /// drive << LEAK_SHIFT, so it fires on sustained drive ≥ threshold >> 3
    /// (~ -44 dBFS above the gate) and within ~3 frames (60 ms) at the cap.
    pub fn vad_default() -> Self {
        Self::new(LEAK_SHIFT, SPIKE_THRESHOLD)
    }

    /// One step: leak, integrate, fire. Returns true on a spike.
    /// The leak drains at least 1 so the potential reaches exactly zero
    /// instead of parking below the shift resolution.
    pub fn step(&mut self, input: u32) -> bool {
        let leak = (self.potential >> self.leak_shift)
            .max(1)
            .min(self.potential);
        self.potential -= leak;
        self.potential = self.potential.saturating_add(input);
        if self.potential >= self.threshold {
            self.potential = 0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_drive_spikes_within_three_frames() {
        let mut n = LifNeuron::vad_default();
        let spikes: Vec<bool> = (0..3).map(|_| n.step(INPUT_CAP)).collect();
        assert!(spikes.contains(&true), "no spike in 3 frames: {spikes:?}");
    }

    #[test]
    fn weak_drive_never_spikes() {
        let mut n = LifNeuron::vad_default();
        // Equilibrium = 8 * 8000 = 64k, well under the 262k threshold.
        assert!((0..1000).all(|_| !n.step(8_000)));
    }

    #[test]
    fn potential_leaks_back_to_zero() {
        let mut n = LifNeuron::vad_default();
        n.step(INPUT_CAP);
        for _ in 0..200 {
            n.step(0);
        }
        assert_eq!(n.potential, 0);
    }

    #[test]
    fn sustained_drive_produces_a_spike_train() {
        let mut n = LifNeuron::vad_default();
        let spikes = (0..50).filter(|_| n.step(INPUT_CAP)).count();
        assert!(spikes >= 10, "only {spikes} spikes in 50 frames");
    }
}
