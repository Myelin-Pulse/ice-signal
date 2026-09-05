//! Formats core FrameReports as the JSONL metadata contract in
//! docs/event-schema.md. This stream (stdout) is the only data that leaves
//! the engine.
//!
//! Times are frame-derived (`frame_index * 20 ms`) — monotonic session time,
//! never wall-clock, per the schema.

use crate::core::{Band, FrameReport, FRAME_MS};

fn ms(frame_index: u64) -> u64 {
    frame_index * FRAME_MS as u64
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

pub fn session_start() -> String {
    format!(
        r#"{{"t":0,"event":"SESSION_START","frame_ms":{FRAME_MS},"engine_version":"{}"}}"#,
        env!("CARGO_PKG_VERSION")
    )
}

/// The 0..n schema events triggered by one frame's strobes.
pub fn report_lines(r: &FrameReport) -> Vec<String> {
    let t = ms(r.frame_index);
    let mut lines = Vec::new();
    if r.speaking_start {
        lines.push(format!(r#"{{"t":{t},"event":"SPEAKING_START"}}"#));
    }
    if r.speaking_stop {
        let utterance_ms = r.utterance_frames as u64 * FRAME_MS as u64;
        lines.push(format!(
            r#"{{"t":{t},"event":"SPEAKING_STOP","utterance_ms":{utterance_ms}}}"#
        ));
    }
    if r.silence {
        let since_ms = r.silence_frames as u64 * FRAME_MS as u64;
        lines.push(format!(r#"{{"t":{t},"event":"SILENCE","since_ms":{since_ms}}}"#));
    }
    if r.long_pause {
        let pause_ms = r.silence_frames as u64 * FRAME_MS as u64;
        lines.push(format!(r#"{{"t":{t},"event":"LONG_PAUSE","pause_ms":{pause_ms}}}"#));
    }
    if let Some((band, avg_power)) = r.energy_band {
        let band = match band {
            Band::Low => "LOW",
            Band::Med => "MED",
            Band::High => "HIGH",
        };
        lines.push(format!(
            r#"{{"t":{t},"event":"ENERGY_LEVEL","band":"{band}","dbfs":{:.1}}}"#,
            dbfs(avg_power)
        ));
    }
    lines
}

pub fn session_end(frames_processed: u64, device_samples_discarded: u64) -> [String; 2] {
    let t = ms(frames_processed);
    [
        format!(
            r#"{{"t":{t},"event":"PRIVACY_SUMMARY","frames_processed":{frames_processed},"audio_samples_discarded":{device_samples_discarded},"audio_bytes_persisted":0,"words_transcribed":0}}"#
        ),
        format!(r#"{{"t":{t},"event":"SESSION_END","duration_ms":{t}}}"#),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbfs_matches_known_levels() {
        // Full-scale square wave: mean square = 32767², ~0 dBFS.
        let full = (320u64 * 32767 * 32767 >> 8) as u32;
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
        assert_eq!(
            report_lines(&r),
            vec![r#"{"t":12340,"event":"SPEAKING_STOP","utterance_ms":3200}"#.to_string()]
        );
    }
}
