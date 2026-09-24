//! Deterministic, synthesized input. Uses the same cores and metadata path as capture.
use crate::core::{CORE_SAMPLE_RATE, FRAME_LEN};
pub const FRAMES: u32 = 1000; // 20 seconds

pub fn frame(index: u32) -> Vec<[i16; 2]> {
    let active = match index {
        50..=199 => [true, false],
        220..=369 => [false, true],
        400..=499 => [true, false],
        500..=529 => [true, true],
        530..=679 => [false, true],
        720..=849 => [true, false],
        _ => [false, false],
    };
    (0..FRAME_LEN)
        .map(|offset| {
            let sample_index = index * FRAME_LEN + offset;
            std::array::from_fn(|i| {
                if !active[i] {
                    return 0;
                }
                let phase = sample_index as f64 * [180.0, 250.0][i] / CORE_SAMPLE_RATE as f64;
                (phase * std::f64::consts::TAU).sin().mul_add(8000.0, 0.0) as i16
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::session::Session;
    #[test]
    fn demo_exercises_the_complete_social_pipeline() {
        let mut session = Session::new(true);
        for index in 0..FRAMES {
            for samples in frame(index) {
                session.tick(samples, &|_| {});
            }
        }
        let (summary, social) = session.finish(FRAMES as u64 * FRAME_LEN as u64 * 2, &|_| {});
        assert_eq!(summary.duration_ms, 20000);
        assert_eq!(social.turns, 5);
        assert_eq!(social.overlaps, 1);
        assert_eq!(social.interruptions, 1);
        assert_eq!(social.suggestion, "LEAVE_SPACE");
    }
}
