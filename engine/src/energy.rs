//! Short-time energy: the only thing the engine keeps from a frame.

pub struct EnergyReading {
    pub dbfs: f32,
}

/// Root-mean-square level of a frame expressed in dBFS
/// (0 dBFS = full scale, silence floors at -90).
pub fn rms(frame: &[f32]) -> EnergyReading {
    if frame.is_empty() {
        return EnergyReading { dbfs: -90.0 };
    }
    let mean_sq = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    let dbfs = 20.0 * mean_sq.sqrt().max(1e-9).log10().max(-4.5);
    EnergyReading { dbfs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_sine_is_about_minus_3_dbfs() {
        let frame: Vec<f32> = (0..960)
            .map(|i| (i as f32 * std::f32::consts::TAU / 48.0).sin())
            .collect();
        let reading = rms(&frame);
        assert!((reading.dbfs - (-3.01)).abs() < 0.1, "got {}", reading.dbfs);
    }

    #[test]
    fn silence_floors_at_minus_90() {
        let reading = rms(&[0.0; 960]);
        assert_eq!(reading.dbfs, -90.0);
    }
}
