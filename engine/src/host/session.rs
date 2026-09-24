//! Shared live/demo processing path. Social analysis consumes public metadata.
use super::{
    analytics::{SessionAnalytics, Summary},
    events::{self, Event},
    social::{SocialSignals, SocialSummary},
};
use crate::core::{Engine, ABS_GATE_MIN, FRAME_MS};

pub struct Session {
    mixed: Engine,
    channels: [Engine; 2],
    dual: bool,
    activity: [bool; 2],
    quiet_frames: [u32; 2],
    analytics: SessionAnalytics,
    social: SocialSignals,
}

impl Session {
    pub fn new(dual: bool) -> Self {
        Self {
            mixed: Engine::new(),
            channels: [Engine::new(), Engine::new()],
            dual,
            activity: [false; 2],
            quiet_frames: [0; 2],
            analytics: SessionAnalytics::new(),
            social: SocialSignals::new(dual),
        }
    }

    pub fn tick(
        &mut self,
        samples: [i16; 2],
        emit: &impl Fn(String),
    ) -> Option<crate::core::FrameReport> {
        let reports = if self.dual {
            [
                self.channels[0].tick(samples[0]),
                self.channels[1].tick(samples[1]),
            ]
        } else {
            [None, None]
        };
        let mixed = if self.dual {
            ((samples[0] as i32 + samples[1] as i32) / 2) as i16
        } else {
            samples[0]
        };
        let report = self.mixed.tick(mixed)?;
        for event in events::from_report(&report) {
            self.analytics.observe(&event);
            emit(event.to_json());
        }
        let t = report.frame_index * FRAME_MS as u64;
        if self.dual {
            let mut next = [false; 2];
            for i in 0..2 {
                let r = reports[i]
                    .as_ref()
                    .expect("channel frames stay synchronized");
                // Remove the mixed VAD's 400 ms tail from overlap measurement.
                // Bridge at most two low-energy frames (40 ms) inside a turn.
                let voiced = r.vad_active
                    && r.frame_power > r.noise_floor.saturating_mul(4).max(ABS_GATE_MIN);
                self.quiet_frames[i] = if voiced {
                    0
                } else {
                    self.quiet_frames[i].saturating_add(1)
                };
                next[i] = voiced || (self.activity[i] && self.quiet_frames[i] < 3);
            }
            if next != self.activity {
                let event = Event::ChannelActivity {
                    t,
                    active_a: next[0],
                    active_b: next[1],
                };
                emit(event.to_json());
                for derived in self.social.observe(&event) {
                    emit(derived.to_json());
                }
                self.activity = next;
            } else {
                for derived in self.social.advance(t) {
                    emit(derived.to_json());
                }
            }
        }
        if (report.frame_index + 1) % 50 == 0 {
            emit(self.social.snapshot(t, false).to_json());
        }
        Some(report)
    }

    pub fn finish(
        mut self,
        samples_discarded: u64,
        emit: &impl Fn(String),
    ) -> (Summary, SocialSummary) {
        let frames = self.mixed.frames_processed();
        let t = frames * FRAME_MS as u64;
        // The closing transition is public, so downstream consumers can also
        // close any turns still open at shutdown.
        if self.dual && self.activity != [false; 2] {
            emit(
                Event::ChannelActivity {
                    t,
                    active_a: false,
                    active_b: false,
                }
                .to_json(),
            );
        }
        for event in self.social.finish(t) {
            emit(event.to_json());
        }
        let summary = self.analytics.finalize(t);
        let social = self.social.snapshot(t, true);
        emit(summary.to_json());
        emit(social.to_json());
        emit(
            Event::PrivacySummary {
                t,
                frames_processed: frames,
                audio_samples_discarded: samples_discarded,
            }
            .to_json(),
        );
        emit(Event::SessionEnd { t }.to_json());
        (summary, social)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FRAME_LEN;
    use std::cell::RefCell;

    #[test]
    fn mono_and_empty_sessions_do_not_invent_social_measurements() {
        for frames in [0, 100] {
            let lines = RefCell::new(Vec::new());
            let emit = |line| lines.borrow_mut().push(line);
            let mut session = Session::new(false);
            for _ in 0..frames * FRAME_LEN {
                session.tick([8000, 0], &emit);
            }
            let (_, social) = session.finish(frames as u64 * FRAME_LEN as u64, &emit);
            assert!(!social.available);
            assert_eq!(social.shares, None);
            for line in lines.borrow().iter() {
                assert!(!line.contains("CHANNEL_ACTIVITY"));
                assert!(!line.contains("TURN_TAKEN"));
                assert!(!line.contains("OVERLAP_DETECTED"));
                assert!(!line.contains("INTERRUPTION"));
            }
        }
    }

    #[test]
    fn full_pipeline_detects_handoff_without_vad_tail_overlap() {
        let lines = RefCell::new(Vec::new());
        let emit = |line| lines.borrow_mut().push(line);
        let mut session = Session::new(true);
        for frame in 0..600 {
            let samples = match frame {
                0..=199 => [8000, 0],
                200..=399 => [0, 8000],
                _ => [0, 0],
            };
            for _ in 0..FRAME_LEN {
                session.tick(samples, &emit);
            }
        }
        let (_, social) = session.finish(600 * FRAME_LEN as u64 * 2, &emit);
        assert_eq!(social.turns, 2);
        assert_eq!(social.overlaps, 0);
        assert_eq!(social.interruptions, 0);
        assert!((social.shares.unwrap()[0] - 0.5).abs() < 0.02);
        assert!(lines.borrow().last().unwrap().contains("SESSION_END"));
    }
}
