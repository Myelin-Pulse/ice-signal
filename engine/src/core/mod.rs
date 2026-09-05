//! Portable signal core — the part that ports ~1-1 to FPGA logic.
//!
//! Rules for everything under `core/` (the synthesizable subset):
//!   - integer arithmetic only: no floats, no division except power-of-two shifts
//!   - no heap allocation, no std collections — all state is fixed-size registers
//!   - one `tick()` per audio sample, mirroring one clock cycle per sample
//!   - outputs are a plain struct of strobes + values, mirroring RTL output ports
//!     (an `Option<T>` field maps to a valid bit plus a value bus)
//!
//! Everything outside `core/` is host/testbench code and never needs porting.

pub mod energy;
pub mod lif;
pub mod vad;

/// The core runs at a fixed rate; the host resamples the device stream to this.
/// On hardware this is the PDM decimator's output rate.
pub const CORE_SAMPLE_RATE: u32 = 16_000;
pub const FRAME_MS: u32 = 20;
pub const FRAME_LEN: u32 = CORE_SAMPLE_RATE * FRAME_MS / 1000; // 320 samples

/// `frame_power` unit: per-frame sum of squares of i16 samples, >> 8.
/// The shift approximates dividing by FRAME_LEN (320) without a divider;
/// the constant ~1.25x factor is baked into every threshold below.

/// Energy-level report cadence: power of two so the average is a shift.
pub const ENERGY_WINDOW_FRAMES: u32 = 64; // 1.28 s

/// Band boundaries in frame-power units (~ -50 dBFS and -35 dBFS).
pub const BAND_MED_MIN: u32 = 13_400;
pub const BAND_HIGH_MIN: u32 = 424_000;

/// Noise-floor tracker (frame-power units).
pub const NOISE_FLOOR_INIT: u32 = 4_096; // ~ -55 dBFS starting guess
pub const NOISE_FLOOR_MIN: u32 = 16;
/// The VAD gate never drops below this, so a dead-silent room cannot spike.
pub const ABS_GATE_MIN: u32 = 2_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Band {
    Low,
    Med,
    High,
}

/// Per-frame output of the core — the module's "output ports".
/// Emitted once per completed 20 ms frame; `bool` fields are one-frame strobes.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameReport {
    pub frame_index: u64,
    pub frame_power: u32,
    pub noise_floor: u32,
    pub spike: bool,
    pub vad_active: bool,
    pub speaking_start: bool,
    /// Strobe; `utterance_frames` is valid only while this is set.
    pub speaking_stop: bool,
    pub utterance_frames: u32,
    /// Strobe when sustained quiet crosses the SILENCE threshold.
    pub silence: bool,
    pub silence_frames: u32,
    /// Strobe when quiet crosses the LONG_PAUSE threshold.
    pub long_pause: bool,
    /// (band, average frame power) every ENERGY_WINDOW_FRAMES frames.
    pub energy_band: Option<(Band, u32)>,
}

pub struct Engine {
    acc: energy::EnergyAccumulator,
    lif: lif::LifNeuron,
    vad: vad::Vad,
    noise_floor: u32,
    frame_index: u64,
    window_power_sum: u64,
    window_frames: u32,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            acc: energy::EnergyAccumulator::new(),
            lif: lif::LifNeuron::vad_default(),
            vad: vad::Vad::new(),
            noise_floor: NOISE_FLOOR_INIT,
            frame_index: 0,
            window_power_sum: 0,
            window_frames: 0,
        }
    }

    /// Process one 16 kHz mono sample. Returns a report on each frame boundary.
    /// The sample is consumed here and nowhere retained — this is the privacy
    /// boundary the FPGA enforces in hardware.
    pub fn tick(&mut self, sample: i16) -> Option<FrameReport> {
        let sum_sq = self.acc.push(sample)?;

        // Frame boundary: the whole per-frame chain below is combinational
        // logic plus a handful of state registers.
        let frame_power = (sum_sq >> 8).min(u32::MAX as u64) as u32;

        // Gate the neuron's drive by the tracked noise floor (+~6 dB).
        let gate = self.noise_floor.saturating_mul(4).max(ABS_GATE_MIN);
        let drive = frame_power.saturating_sub(gate).min(lif::INPUT_CAP);
        let spike = self.lif.step(drive);
        let v = self.vad.step(spike);

        // Track the noise floor while quiet; only creep downward during
        // speech so sustained loud input cannot freeze the gate forever.
        if !v.active {
            self.noise_floor = ema_step(self.noise_floor, frame_power, 6);
        } else if frame_power < self.noise_floor {
            self.noise_floor = ema_step(self.noise_floor, frame_power, 9);
        }
        self.noise_floor = self.noise_floor.max(NOISE_FLOOR_MIN);

        // Coarse energy level over a power-of-two window of frames.
        self.window_power_sum += frame_power as u64;
        self.window_frames += 1;
        let energy_band = if self.window_frames == ENERGY_WINDOW_FRAMES {
            let avg = (self.window_power_sum >> 6) as u32; // / ENERGY_WINDOW_FRAMES
            self.window_power_sum = 0;
            self.window_frames = 0;
            let band = if avg >= BAND_HIGH_MIN {
                Band::High
            } else if avg >= BAND_MED_MIN {
                Band::Med
            } else {
                Band::Low
            };
            Some((band, avg))
        } else {
            None
        };

        let report = FrameReport {
            frame_index: self.frame_index,
            frame_power,
            noise_floor: self.noise_floor,
            spike,
            vad_active: v.active,
            speaking_start: v.speaking_start,
            speaking_stop: v.speaking_stop,
            utterance_frames: v.utterance_frames,
            silence: v.silence,
            silence_frames: v.silence_frames,
            long_pause: v.long_pause,
            energy_band,
        };
        self.frame_index += 1;
        Some(report)
    }

    pub fn frames_processed(&self) -> u64 {
        self.frame_index
    }
}

/// One step of a shift-based exponential moving average:
/// `current += (target - current) >> shift`, nudged by 1 so small
/// deltas still converge instead of sticking below the shift resolution.
fn ema_step(current: u32, target: u32, shift: u32) -> u32 {
    if target >= current {
        let delta = (target - current) >> shift;
        current + delta.max(if target > current { 1 } else { 0 })
    } else {
        let delta = (current - target) >> shift;
        current - delta.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(engine: &mut Engine, samples: impl Iterator<Item = i16>) -> Vec<FrameReport> {
        samples.filter_map(|s| engine.tick(s)).collect()
    }

    /// ±8000 square wave: frame_power ≈ 8000² * 320 >> 8 = 8e7, loud speech.
    fn loud(n_frames: u32) -> impl Iterator<Item = i16> {
        (0..n_frames * FRAME_LEN).map(|i| if i % 2 == 0 { 8000 } else { -8000 })
    }

    fn quiet(n_frames: u32) -> impl Iterator<Item = i16> {
        (0..n_frames * FRAME_LEN).map(|_| 0)
    }

    #[test]
    fn silence_emits_no_speech_events() {
        let mut engine = Engine::new();
        let reports = feed(&mut engine, quiet(100));
        assert_eq!(reports.len(), 100);
        assert!(reports.iter().all(|r| !r.speaking_start && !r.vad_active));
        // Initial quiet crosses the SILENCE threshold exactly once.
        assert_eq!(reports.iter().filter(|r| r.silence).count(), 1);
    }

    #[test]
    fn speech_then_silence_produces_full_event_sequence() {
        let mut engine = Engine::new();
        let mut reports = feed(&mut engine, quiet(50));
        reports.extend(feed(&mut engine, loud(50)));
        reports.extend(feed(&mut engine, quiet(250)));

        let start = reports.iter().position(|r| r.speaking_start).expect("start");
        assert!((50..60).contains(&start), "onset at frame {start}");

        let stop = reports.iter().find(|r| r.speaking_stop).expect("stop");
        assert!(
            (30..=50).contains(&stop.utterance_frames),
            "utterance {} frames",
            stop.utterance_frames
        );

        assert!(reports.iter().any(|r| r.silence));
        assert!(reports.iter().any(|r| r.long_pause));
    }

    #[test]
    fn energy_band_reports_on_window_cadence() {
        let mut engine = Engine::new();
        let reports = feed(&mut engine, quiet(64));
        let bands: Vec<_> = reports.iter().filter_map(|r| r.energy_band).collect();
        assert_eq!(bands, vec![(Band::Low, 0)]);

        let reports = feed(&mut engine, loud(64));
        let bands: Vec<_> = reports.iter().filter_map(|r| r.energy_band).collect();
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].0, Band::High);
    }
}
