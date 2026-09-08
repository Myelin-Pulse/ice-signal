//! Voice-activity state machine, driven by the LIF neuron's spike train.
//! Two states and three counters — a direct FSM in RTL.

/// Frames without a spike before an utterance is considered over (400 ms).
pub const HANGOVER_FRAMES: u32 = 20;
/// Quiet frames before a SILENCE strobe (1.5 s).
pub const SILENCE_FRAMES: u32 = 75;
/// Quiet frames before a LONG_PAUSE strobe (4 s).
pub const LONG_PAUSE_FRAMES: u32 = 200;

#[derive(Clone, Copy, Debug, Default)]
pub struct VadOutput {
    pub active: bool,
    pub speaking_start: bool,
    pub speaking_stop: bool,
    /// Valid only when `speaking_stop` is set; hangover tail excluded.
    pub utterance_frames: u32,
    pub silence: bool,
    pub silence_frames: u32,
    pub long_pause: bool,
}

#[derive(PartialEq)]
enum State {
    Silence,
    Speech,
}

pub struct Vad {
    state: State,
    frames_in_state: u32,
    frames_since_spike: u32,
}

impl Vad {
    pub fn new() -> Self {
        Self {
            state: State::Silence,
            frames_in_state: 0,
            frames_since_spike: 0,
        }
    }

    pub fn step(&mut self, spike: bool) -> VadOutput {
        let mut out = VadOutput::default();
        match self.state {
            State::Silence => {
                if spike {
                    self.state = State::Speech;
                    self.frames_in_state = 0;
                    self.frames_since_spike = 0;
                    out.speaking_start = true;
                    out.active = true;
                } else {
                    self.frames_in_state += 1;
                    if self.frames_in_state == SILENCE_FRAMES {
                        out.silence = true;
                        out.silence_frames = self.frames_in_state;
                    }
                    if self.frames_in_state == LONG_PAUSE_FRAMES {
                        out.long_pause = true;
                        out.silence_frames = self.frames_in_state;
                    }
                }
            }
            State::Speech => {
                self.frames_in_state += 1;
                if spike {
                    self.frames_since_spike = 0;
                } else {
                    self.frames_since_spike += 1;
                }
                if self.frames_since_spike >= HANGOVER_FRAMES {
                    out.speaking_stop = true;
                    // At least 1: the starting spike frame itself was voiced.
                    out.utterance_frames = self
                        .frames_in_state
                        .saturating_sub(self.frames_since_spike)
                        .max(1);
                    self.state = State::Silence;
                    self.frames_in_state = 0;
                } else {
                    out.active = true;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_starts_speech_and_hangover_ends_it() {
        let mut vad = Vad::new();
        let out = vad.step(true);
        assert!(out.speaking_start && out.active);

        // Spike every 3rd frame for 30 frames keeps the utterance alive.
        for i in 1..=30 {
            let out = vad.step(i % 3 == 0);
            assert!(out.active && !out.speaking_stop, "dropped at frame {i}");
        }

        // Then silence: stop fires exactly at the hangover boundary.
        for i in 1..HANGOVER_FRAMES {
            assert!(!vad.step(false).speaking_stop, "early stop at +{i}");
        }
        let out = vad.step(false);
        assert!(out.speaking_stop);
        // 31 frames of speech + 19 quiet, minus the 20-frame hangover tail.
        assert_eq!(out.utterance_frames, 30);
    }

    #[test]
    fn silence_and_long_pause_strobe_once_each() {
        let mut vad = Vad::new();
        let outs: Vec<VadOutput> = (0..LONG_PAUSE_FRAMES + 50).map(|_| vad.step(false)).collect();
        assert_eq!(outs.iter().filter(|o| o.silence).count(), 1);
        assert_eq!(outs.iter().filter(|o| o.long_pause).count(), 1);
        assert!(outs[SILENCE_FRAMES as usize - 1].silence);
    }
}
