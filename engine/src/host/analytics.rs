//! Session analytics — the app-side layer (this runs on the phone in the
//! product). It consumes ONLY schema events, never frame reports, so every
//! number here is provably derivable from the privacy-safe metadata contract.
//!
//! Definitions (kept in docs/event-schema.md):
//!   - utterance: one continuous stretch of voiced activity, any speaker
//!     (single channel — speaker attribution lands in Week 6)
//!   - energy trend: mean dBFS of the second half vs the first half, ±2 dB
//!   - momentum: speech density of the second half vs the first half, ±0.10

use crate::host::events::Event;

const TREND_DB: f32 = 2.0;
const MOMENTUM_RATIO: f64 = 0.10;

#[derive(Default)]
pub struct SessionAnalytics {
    /// Closed utterances as (start_t_ms, duration_ms).
    spans: Vec<(u64, u64)>,
    speaking_since: Option<u64>,
    long_pauses: u32,
    /// (t_ms, dBFS) from ENERGY_LEVEL events.
    energy: Vec<(u64, f32)>,
}

impl SessionAnalytics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, event: &Event) {
        match *event {
            Event::SpeakingStart { t } => self.speaking_since = Some(t),
            Event::SpeakingStop { utterance_ms, .. } => {
                if let Some(start) = self.speaking_since.take() {
                    self.spans.push((start, utterance_ms));
                }
            }
            Event::LongPause { .. } => self.long_pauses += 1,
            Event::EnergyLevel { t, dbfs, .. } => self.energy.push((t, dbfs)),
            _ => {}
        }
    }

    pub fn finalize(mut self, duration_ms: u64) -> Summary {
        // Close an utterance still open when the session ends.
        if let Some(start) = self.speaking_since.take() {
            self.spans.push((start, duration_ms.saturating_sub(start)));
        }

        let speech_ms: u64 = self.spans.iter().map(|&(_, d)| d).sum();
        let speech_ratio = if duration_ms > 0 {
            speech_ms as f64 / duration_ms as f64
        } else {
            0.0
        };
        let utterances = self.spans.len() as u32;
        let utterances_per_min = if duration_ms > 0 {
            utterances as f64 * 60_000.0 / duration_ms as f64
        } else {
            0.0
        };

        // Longest lull: biggest gap between utterances, including the gaps
        // before the first one and after the last one.
        let mut longest_lull_ms = 0u64;
        let mut cursor = 0u64;
        for &(start, dur) in &self.spans {
            longest_lull_ms = longest_lull_ms.max(start.saturating_sub(cursor));
            cursor = cursor.max(start + dur);
        }
        longest_lull_ms = longest_lull_ms.max(duration_ms.saturating_sub(cursor));

        // Momentum: speech density, second half vs first half.
        let mid = duration_ms / 2;
        let first: u64 = self.spans.iter().map(|&s| overlap(s, 0, mid)).sum();
        let second: u64 = self.spans.iter().map(|&s| overlap(s, mid, duration_ms)).sum();
        let momentum = if mid == 0 {
            "STEADY"
        } else {
            let diff = (second as f64 - first as f64) / mid as f64;
            if diff > MOMENTUM_RATIO {
                "GAINING"
            } else if diff < -MOMENTUM_RATIO {
                "FADING"
            } else {
                "STEADY"
            }
        };

        // Energy: overall average and first-half vs second-half trend.
        let (avg_dbfs, energy_trend) = if self.energy.is_empty() {
            (-90.0, "STEADY")
        } else {
            let avg = mean(self.energy.iter().map(|&(_, db)| db));
            let (a, b): (Vec<_>, Vec<_>) = self.energy.iter().partition(|&&(t, _)| t < mid);
            let trend = if a.is_empty() || b.is_empty() {
                "STEADY"
            } else {
                let delta = mean(b.iter().map(|&&(_, db)| db)) - mean(a.iter().map(|&&(_, db)| db));
                if delta > TREND_DB {
                    "RISING"
                } else if delta < -TREND_DB {
                    "FALLING"
                } else {
                    "STEADY"
                }
            };
            (avg, trend)
        };

        Summary {
            duration_ms,
            speech_ms,
            speech_ratio,
            utterances,
            utterances_per_min,
            longest_lull_ms,
            long_pauses: self.long_pauses,
            avg_dbfs,
            energy_trend,
            momentum,
        }
    }
}

/// Overlap in ms between a span and the window [a, b).
fn overlap((start, dur): (u64, u64), a: u64, b: u64) -> u64 {
    let end = start + dur;
    end.min(b).saturating_sub(start.max(a))
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = values.fold((0.0f32, 0u32), |(s, n), v| (s + v, n + 1));
    if n == 0 { 0.0 } else { sum / n as f32 }
}

pub struct Summary {
    pub duration_ms: u64,
    pub speech_ms: u64,
    pub speech_ratio: f64,
    pub utterances: u32,
    pub utterances_per_min: f64,
    pub longest_lull_ms: u64,
    pub long_pauses: u32,
    pub avg_dbfs: f32,
    pub energy_trend: &'static str,
    pub momentum: &'static str,
}

impl Summary {
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"t":{},"event":"SESSION_ANALYTICS","duration_ms":{},"speech_ms":{},"speech_ratio":{:.2},"utterances":{},"utterances_per_min":{:.1},"longest_lull_ms":{},"long_pauses":{},"avg_dbfs":{:.1},"energy_trend":"{}","momentum":"{}"}}"#,
            self.duration_ms,
            self.duration_ms,
            self.speech_ms,
            self.speech_ratio,
            self.utterances,
            self.utterances_per_min,
            self.longest_lull_ms,
            self.long_pauses,
            self.avg_dbfs,
            self.energy_trend,
            self.momentum,
        )
    }

    /// Human-readable Conversation Card for the terminal (stderr).
    pub fn card(&self) -> String {
        let dur = format!("{}m {:02}s", self.duration_ms / 60_000, self.duration_ms % 60_000 / 1000);
        let lull = self.longest_lull_ms as f64 / 1000.0;
        format!(
            "──── conversation card ─────────────────────\n\
             \x20 duration      {dur}\n\
             \x20 speech        {:.0}% · {} utterances · {:.1}/min\n\
             \x20 longest lull  {lull:.1}s · {} long pauses\n\
             \x20 energy        {:.1} dBFS avg · {}\n\
             \x20 momentum      {}\n\
             ────────────────────────────────────────────",
            self.speech_ratio * 100.0,
            self.utterances,
            self.utterances_per_min,
            self.long_pauses,
            self.avg_dbfs,
            self.energy_trend.to_lowercase(),
            self.momentum.to_lowercase(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Band;

    fn utterance(a: &mut SessionAnalytics, start: u64, dur: u64) {
        a.observe(&Event::SpeakingStart { t: start });
        a.observe(&Event::SpeakingStop { t: start + dur + 400, utterance_ms: dur });
    }

    #[test]
    fn ratio_lulls_and_pace() {
        let mut a = SessionAnalytics::new();
        utterance(&mut a, 2_000, 2_500); // ends 4 500
        utterance(&mut a, 12_000, 2_500); // ends 14 500
        a.observe(&Event::LongPause { t: 9_000, pause_ms: 4_000 });
        let s = a.finalize(20_000);

        assert_eq!(s.utterances, 2);
        assert_eq!(s.speech_ms, 5_000);
        assert!((s.speech_ratio - 0.25).abs() < 1e-9);
        assert!((s.utterances_per_min - 6.0).abs() < 1e-9);
        assert_eq!(s.longest_lull_ms, 7_500); // 4 500 → 12 000
        assert_eq!(s.long_pauses, 1);
        assert_eq!(s.momentum, "STEADY"); // 2 500 ms in each half
    }

    #[test]
    fn momentum_gaining_when_second_half_denser() {
        let mut a = SessionAnalytics::new();
        utterance(&mut a, 0, 1_000);
        utterance(&mut a, 10_000, 9_000);
        let s = a.finalize(20_000);
        assert_eq!(s.momentum, "GAINING");
    }

    #[test]
    fn open_utterance_is_closed_at_session_end() {
        let mut a = SessionAnalytics::new();
        a.observe(&Event::SpeakingStart { t: 18_000 });
        let s = a.finalize(20_000);
        assert_eq!(s.utterances, 1);
        assert_eq!(s.speech_ms, 2_000);
    }

    #[test]
    fn energy_trend_rises_with_louder_second_half() {
        let mut a = SessionAnalytics::new();
        for (t, dbfs) in [(1_000, -40.0), (3_000, -38.0), (5_000, -33.0), (7_000, -31.0)] {
            a.observe(&Event::EnergyLevel { t, band: Band::Med, dbfs });
        }
        let s = a.finalize(8_000);
        assert_eq!(s.energy_trend, "RISING");
        assert!((s.avg_dbfs - -35.5).abs() < 0.01);
    }

    #[test]
    fn empty_session_is_all_zeroes() {
        let s = SessionAnalytics::new().finalize(0);
        assert_eq!(s.speech_ratio, 0.0);
        assert_eq!(s.utterances, 0);
        assert_eq!(s.longest_lull_ms, 0);
        assert_eq!(s.energy_trend, "STEADY");
    }

    #[test]
    fn json_line_is_stable() {
        let mut a = SessionAnalytics::new();
        utterance(&mut a, 2_000, 2_500);
        utterance(&mut a, 12_000, 2_500);
        let json = a.finalize(20_000).to_json();
        assert_eq!(
            json,
            r#"{"t":20000,"event":"SESSION_ANALYTICS","duration_ms":20000,"speech_ms":5000,"speech_ratio":0.25,"utterances":2,"utterances_per_min":6.0,"longest_lull_ms":7500,"long_pauses":0,"avg_dbfs":-90.0,"energy_trend":"STEADY","momentum":"STEADY"}"#
        );
    }
}
