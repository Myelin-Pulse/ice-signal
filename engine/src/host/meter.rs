//! Live terminal meter on stderr — presentation only, never ported.

use std::io::Write;

use crate::core::FrameReport;
use crate::host::events::dbfs;

/// Single-line meter: `[########----------------] -32.4 dBFS ▌speaking`
pub fn render(r: &FrameReport, device_samples_discarded: u64) {
    const WIDTH: usize = 32;
    let level = dbfs(r.frame_power);
    // Map -60..0 dBFS onto the bar.
    let fill = (((level + 60.0) / 60.0).clamp(0.0, 1.0) * WIDTH as f32) as usize;
    let bar: String = "#".repeat(fill) + &"-".repeat(WIDTH - fill);
    let state = if r.vad_active { "▌speaking" } else { "  silent " };
    let spike = if r.spike { "⚡" } else { " " };
    eprint!(
        "\r[{bar}] {level:>6.1} dBFS {spike}{state} | floor: {:>6.1} | discarded: {} KiB ",
        dbfs(r.noise_floor),
        device_samples_discarded * 4 / 1024
    );
    let _ = std::io::stderr().flush();
}
