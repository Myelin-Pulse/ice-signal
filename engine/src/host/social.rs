//! Social estimates from anonymous channel-activity metadata only.
//! A/B identify input positions, never voices or persistent people.

use super::events::Event;

pub const MIN_OVERLAP_MS: u64 = 200;
pub const TAKEOVER_MS: u64 = 300;
const LABELS: [&str; 2] = ["A", "B"];

pub struct SocialSignals {
    available: bool,
    last_t: u64,
    active: [bool; 2],
    starts: [Option<u64>; 2],
    speech_ms: [u64; 2],
    turns: u64,
    overlap_start: Option<u64>,
    incumbent: Option<usize>,
    overlap_ms: u64,
    overlaps: u64,
    interruptions: u64,
    pending: Option<(usize, u64, u64)>, // newcomer, solo start, overlap duration
}

impl SocialSignals {
    pub fn new(available: bool) -> Self {
        Self {
            available,
            last_t: 0,
            active: [false; 2],
            starts: [None; 2],
            speech_ms: [0; 2],
            turns: 0,
            overlap_start: None,
            incumbent: None,
            overlap_ms: 0,
            overlaps: 0,
            interruptions: 0,
            pending: None,
        }
    }

    /// Advance the metadata clock, accounting for half-open intervals [start, end).
    pub fn advance(&mut self, t: u64) -> Vec<Event> {
        assert!(t >= self.last_t, "social event time must be monotonic");
        let elapsed = t - self.last_t;
        for (i, active) in self.active.iter().enumerate() {
            if *active {
                self.speech_ms[i] += elapsed;
            }
        }
        if self.active == [true, true] {
            self.overlap_ms += elapsed;
        }
        self.last_t = t;
        if let Some((newcomer, since, overlap_ms)) = self.pending {
            if t - since >= TAKEOVER_MS {
                self.pending = None;
                self.interruptions += 1;
                return vec![Event::Interruption {
                    t,
                    speaker: LABELS[newcomer],
                    interrupted: LABELS[1 - newcomer],
                    overlap_ms,
                }];
            }
        }
        Vec::new()
    }

    pub fn observe(&mut self, event: &Event) -> Vec<Event> {
        let Event::ChannelActivity {
            t,
            active_a,
            active_b,
        } = *event
        else {
            return Vec::new();
        };
        if !self.available {
            return Vec::new();
        }
        let mut events = self.advance(t);
        let next = [active_a, active_b];
        if next == self.active {
            return events;
        }
        // A pending takeover requires uninterrupted solo activity.
        self.pending = None;
        if self.active == [true, true] {
            let duration = t - self.overlap_start.take().unwrap();
            if duration >= MIN_OVERLAP_MS {
                self.overlaps += 1;
                events.push(Event::OverlapDetected {
                    t,
                    overlap_ms: duration,
                });
                if let Some(old) = self.incumbent {
                    if !next[old] && next[1 - old] {
                        self.pending = Some((1 - old, t, duration));
                    }
                }
            }
            self.incumbent = None;
        }
        if next == [true, true] {
            self.overlap_start = Some(t);
            self.incumbent = match self.active {
                [true, false] => Some(0),
                [false, true] => Some(1),
                _ => None,
            };
        }
        for i in 0..2 {
            if next[i] && !self.active[i] {
                self.starts[i] = Some(t);
            } else if !next[i] && self.active[i] {
                self.turns += 1;
                events.push(Event::TurnTaken {
                    t,
                    speaker: LABELS[i],
                    turn_ms: t - self.starts[i].take().unwrap(),
                });
            }
        }
        self.active = next;
        events
    }

    pub fn snapshot(&self, t: u64, final_summary: bool) -> SocialSummary {
        let total = self.speech_ms.iter().sum::<u64>();
        let shares = if self.available && total > 0 {
            Some([
                self.speech_ms[0] as f64 / total as f64,
                self.speech_ms[1] as f64 / total as f64,
            ])
        } else {
            None
        };
        let suggestion = if !self.available {
            "REFLECT_TOGETHER"
        } else if t < 10_000 || total < 5_000 {
            "KEEP_LISTENING"
        } else if self.interruptions > 0 {
            "LEAVE_SPACE"
        } else if shares.is_some_and(|s| s[0].max(s[1]) >= 0.70) {
            "INVITE_OTHER"
        } else {
            "ASK_FOLLOW_UP"
        };
        SocialSummary {
            t,
            final_summary,
            available: self.available,
            speech_ms: self.speech_ms,
            shares,
            turns: self.turns,
            overlap_ms: self.overlap_ms,
            overlaps: self.overlaps,
            interruptions: self.interruptions,
            suggestion,
        }
    }

    pub fn finish(&mut self, t: u64) -> Vec<Event> {
        // Close ongoing turns and overlap exactly once. Ending a session does
        // not establish that either participant interrupted the other.
        self.observe(&Event::ChannelActivity {
            t,
            active_a: false,
            active_b: false,
        })
    }
}

pub struct SocialSummary {
    pub t: u64,
    pub final_summary: bool,
    pub available: bool,
    pub speech_ms: [u64; 2],
    pub shares: Option<[f64; 2]>,
    pub turns: u64,
    pub overlap_ms: u64,
    pub overlaps: u64,
    pub interruptions: u64,
    pub suggestion: &'static str,
}

impl SocialSummary {
    pub fn suggestion_text(&self) -> &'static str {
        match self.suggestion {
            "KEEP_LISTENING" => {
                "Keep listening; there is not enough activity for a useful suggestion yet."
            }
            "LEAVE_SPACE" => {
                "Try leaving a short pause before responding so each person can finish."
            }
            "INVITE_OTHER" => {
                "Consider inviting the person who spoke less to share their perspective."
            }
            "ASK_FOLLOW_UP" => {
                "Consider asking an open follow-up question about something they shared."
            }
            _ => "Ask each other how the conversation felt and whether you would like to continue.",
        }
    }

    pub fn to_json(&self) -> String {
        let shares = self
            .shares
            .map(|s| format!("[{:.4},{:.4}]", s[0], s[1]))
            .unwrap_or("null".into());
        let counts = if self.available {
            format!(
                r#""speech_ms":[{},{}],"turns":{},"overlap_ms":{},"overlaps":{},"interruptions":{}"#,
                self.speech_ms[0],
                self.speech_ms[1],
                self.turns,
                self.overlap_ms,
                self.overlaps,
                self.interruptions
            )
        } else {
            r#""speech_ms":null,"turns":null,"overlap_ms":null,"overlaps":null,"interruptions":null"#.into()
        };
        format!(
            r#"{{"t":{},"event":"SOCIAL_SIGNALS","final":{},"available":{},"estimated":true,"reason":"{}","shares":{shares},{counts},"suggestion":"{}","suggestion_text":"{}"}}"#,
            self.t,
            self.final_summary,
            self.available,
            if self.available {
                "SEPARATE_CHANNELS"
            } else {
                "SEPARATE_CHANNELS_REQUIRED"
            },
            self.suggestion,
            self.suggestion_text()
        )
    }

    pub fn card(&self) -> String {
        let balance = self
            .shares
            .map(|s| format!("A {:.0}% / B {:.0}%", s[0] * 100.0, s[1] * 100.0))
            .unwrap_or_else(|| "unavailable (no attributed speech)".into());
        if !self.available {
            return format!("social signals: unavailable — separate participant channels required\nfollow-up: {}", self.suggestion_text());
        }
        format!("social estimates\n  speaking balance: {balance}\n  turns: {} | overlap: {:.1}s ({} episodes) | possible interruptions: {}\nfollow-up: {}",
            self.turns, self.overlap_ms as f64 / 1000.0, self.overlaps, self.interruptions, self.suggestion_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn activity(s: &mut SocialSignals, t: u64, a: bool, b: bool) -> Vec<Event> {
        s.observe(&Event::ChannelActivity {
            t,
            active_a: a,
            active_b: b,
        })
    }

    #[test]
    fn balanced_turns_and_silence_do_not_imply_overlap() {
        let mut s = SocialSignals::new(true);
        activity(&mut s, 0, true, false);
        activity(&mut s, 4000, false, false);
        activity(&mut s, 5000, false, true);
        s.finish(9000);
        let summary = s.snapshot(10000, true);
        assert_eq!(summary.speech_ms, [4000, 4000]);
        assert_eq!(summary.shares, Some([0.5, 0.5]));
        assert_eq!(
            (summary.turns, summary.overlaps, summary.interruptions),
            (2, 0, 0)
        );
        assert_eq!(summary.suggestion, "ASK_FOLLOW_UP");
    }

    #[test]
    fn interruption_requires_overlap_and_sustained_takeover() {
        let mut s = SocialSignals::new(true);
        activity(&mut s, 0, true, false);
        activity(&mut s, 1000, true, true);
        let events = activity(&mut s, 1400, false, true);
        assert!(events.contains(&Event::OverlapDetected {
            t: 1400,
            overlap_ms: 400
        }));
        assert!(s.advance(1699).is_empty());
        assert_eq!(
            s.advance(1700),
            vec![Event::Interruption {
                t: 1700,
                speaker: "B",
                interrupted: "A",
                overlap_ms: 400
            }]
        );
        assert!(s.advance(2000).is_empty());
        s.finish(10000);
        assert_eq!(s.snapshot(10000, true).suggestion, "LEAVE_SPACE");
    }

    #[test]
    fn backchannel_simultaneous_start_and_short_overlap_are_not_interruptions() {
        for (initial, end, duration) in [
            ([true, false], [true, false], 400),
            ([false, false], [false, true], 400),
            ([true, false], [false, true], 199),
        ] {
            let mut s = SocialSignals::new(true);
            activity(&mut s, 0, initial[0], initial[1]);
            activity(&mut s, 1000, true, true);
            activity(&mut s, 1000 + duration, end[0], end[1]);
            s.advance(3000);
            assert_eq!(s.snapshot(3000, false).interruptions, 0);
        }
    }

    #[test]
    fn interrupted_takeover_is_cancelled_and_end_closes_once() {
        let mut s = SocialSignals::new(true);
        activity(&mut s, 0, true, false);
        activity(&mut s, 1000, true, true);
        activity(&mut s, 1400, false, true);
        activity(&mut s, 1600, false, false);
        s.finish(5000);
        assert_eq!(s.snapshot(5000, true).interruptions, 0);
        activity(&mut s, 6000, true, true);
        let end = s.finish(7000);
        assert_eq!(end.len(), 3); // one overlap, two turns
        assert!(s.finish(7000).is_empty());
        assert_eq!(s.snapshot(7000, true).speech_ms, [2400, 1600]);
    }

    #[test]
    fn empty_mono_and_dominant_sessions_have_honest_suggestions() {
        assert_eq!(SocialSignals::new(true).snapshot(0, true).shares, None);
        let mono = SocialSignals::new(false).snapshot(10000, true);
        assert!(mono.to_json().contains(r#""shares":null,"speech_ms":null"#));
        assert_eq!(mono.suggestion, "REFLECT_TOGETHER");
        let mut s = SocialSignals::new(true);
        activity(&mut s, 0, true, false);
        s.finish(10000);
        assert_eq!(s.snapshot(10000, true).suggestion, "INVITE_OTHER");
    }
}
