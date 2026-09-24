//! Typed metadata events and their JSONL form (docs/event-schema.md).
//!
//! The JSONL stream on stdout is the only data that ever leaves the engine.
//! The analytics layer consumes these same typed events — never frame data —
//! so everything the app computes is provably derivable from the metadata
//! contract alone. On hardware, this enum is what crosses the USB/BLE link.

use crate::core::{Band, FrameReport, FRAME_MS};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    SessionStart,
    SpeakingStart {
        t: u64,
    },
    SpeakingStop {
        t: u64,
        utterance_ms: u64,
    },
    Silence {
        t: u64,
        since_ms: u64,
    },
    LongPause {
        t: u64,
        pause_ms: u64,
    },
    EnergyLevel {
        t: u64,
        band: Band,
        dbfs: f32,
    },
    PrivacySummary {
        t: u64,
        frames_processed: u64,
        audio_samples_discarded: u64,
    },
    SessionEnd {
        t: u64,
    },
    SocialConfig {
        dual_channel: bool,
        demo: bool,
    },
    ChannelActivity {
        t: u64,
        active_a: bool,
        active_b: bool,
    },
    TurnTaken {
        t: u64,
        speaker: &'static str,
        turn_ms: u64,
    },
    OverlapDetected {
        t: u64,
        overlap_ms: u64,
    },
    Interruption {
        t: u64,
        speaker: &'static str,
        interrupted: &'static str,
        overlap_ms: u64,
    },
}

impl Event {
    pub fn to_json(self) -> String {
        match self {
            Event::SocialConfig { dual_channel, demo } => format!(
                r#"{{"t":0,"event":"SOCIAL_CONFIG","mode":"{}","estimated":true,"demo":{demo}}}"#,
                if dual_channel { "DUAL_CHANNEL" } else { "MONO" }
            ),
            Event::ChannelActivity {
                t,
                active_a,
                active_b,
            } => format!(
                r#"{{"t":{t},"event":"CHANNEL_ACTIVITY","active_a":{active_a},"active_b":{active_b}}}"#
            ),
            Event::TurnTaken {
                t,
                speaker,
                turn_ms,
            } => format!(
                r#"{{"t":{t},"event":"TURN_TAKEN","speaker":"{speaker}","turn_ms":{turn_ms},"estimated":true}}"#
            ),
            Event::OverlapDetected { t, overlap_ms } => format!(
                r#"{{"t":{t},"event":"OVERLAP_DETECTED","overlap_ms":{overlap_ms},"estimated":true}}"#
            ),
            Event::Interruption {
                t,
                speaker,
                interrupted,
                overlap_ms,
            } => format!(
                r#"{{"t":{t},"event":"INTERRUPTION","speaker":"{speaker}","interrupted":"{interrupted}","overlap_ms":{overlap_ms},"estimated":true}}"#
            ),
            Event::SessionStart => format!(
                r#"{{"t":0,"event":"SESSION_START","frame_ms":{FRAME_MS},"engine_version":"{}"}}"#,
                env!("CARGO_PKG_VERSION")
            ),
            Event::SpeakingStart { t } => {
                format!(r#"{{"t":{t},"event":"SPEAKING_START"}}"#)
            }
            Event::SpeakingStop { t, utterance_ms } => {
                format!(r#"{{"t":{t},"event":"SPEAKING_STOP","utterance_ms":{utterance_ms}}}"#)
            }
            Event::Silence { t, since_ms } => {
                format!(r#"{{"t":{t},"event":"SILENCE","since_ms":{since_ms}}}"#)
            }
            Event::LongPause { t, pause_ms } => {
                format!(r#"{{"t":{t},"event":"LONG_PAUSE","pause_ms":{pause_ms}}}"#)
            }
            Event::EnergyLevel { t, band, dbfs } => format!(
                r#"{{"t":{t},"event":"ENERGY_LEVEL","band":"{}","dbfs":{dbfs:.1}}}"#,
                band_str(band)
            ),
            Event::PrivacySummary {
                t,
                frames_processed,
                audio_samples_discarded,
            } => format!(
                r#"{{"t":{t},"event":"PRIVACY_SUMMARY","frames_processed":{frames_processed},"audio_samples_discarded":{audio_samples_discarded},"audio_bytes_persisted":0,"words_transcribed":0}}"#
            ),
            Event::SessionEnd { t } => {
                format!(r#"{{"t":{t},"event":"SESSION_END","duration_ms":{t}}}"#)
            }
        }
    }
}

fn band_str(band: Band) -> &'static str {
    match band {
        Band::Low => "LOW",
        Band::Med => "MED",
        Band::High => "HIGH",
    }
}

fn ms(frame_index: u64) -> u64 {
    frame_index * FRAME_MS as u64
}

/// The 0..n schema events triggered by one frame's strobes.
pub fn from_report(r: &FrameReport) -> Vec<Event> {
    let t = ms(r.frame_index);
    let mut events = Vec::new();
    if r.speaking_start {
        events.push(Event::SpeakingStart { t });
    }
    if r.speaking_stop {
        events.push(Event::SpeakingStop {
            t,
            utterance_ms: r.utterance_frames as u64 * FRAME_MS as u64,
        });
    }
    if r.silence {
        events.push(Event::Silence {
            t,
            since_ms: r.silence_frames as u64 * FRAME_MS as u64,
        });
    }
    if r.long_pause {
        events.push(Event::LongPause {
            t,
            pause_ms: r.silence_frames as u64 * FRAME_MS as u64,
        });
    }
    if let Some((band, avg_power)) = r.energy_band {
        events.push(Event::EnergyLevel {
            t,
            band,
            dbfs: dbfs(avg_power),
        });
    }
    events
}

/// Display-side conversion of core frame power back to dBFS.
/// frame_power = sum_sq >> 8 over 320 samples, so mean square = 0.8 × power.
pub fn dbfs(frame_power: u32) -> f32 {
    if frame_power == 0 {
        return -90.0;
    }
    let mean_sq = frame_power as f64 * 256.0 / 320.0;
    (10.0 * (mean_sq / (32768.0f64 * 32768.0)).log10()).max(-90.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbfs_matches_known_levels() {
        // Full-scale square wave: mean square = 32767², ~0 dBFS.
        let full = ((320u64 * 32767 * 32767) >> 8) as u32;
        assert!(dbfs(full).abs() < 0.1, "got {}", dbfs(full));
        assert_eq!(dbfs(0), -90.0);
    }

    #[test]
    fn speaking_stop_line_matches_schema() {
        let r = FrameReport {
            frame_index: 617,
            speaking_stop: true,
            utterance_frames: 160,
            ..Default::default()
        };
        let events = from_report(&r);
        assert_eq!(
            events,
            vec![Event::SpeakingStop {
                t: 12340,
                utterance_ms: 3200
            }]
        );
        assert_eq!(
            events[0].to_json(),
            r#"{"t":12340,"event":"SPEAKING_STOP","utterance_ms":3200}"#
        );
    }
}
