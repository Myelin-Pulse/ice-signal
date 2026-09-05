//! Ice Signal engine — Week 1: live mic capture, 20ms frame processing,
//! RMS energy, terminal output.
//!
//! Privacy invariant: raw audio exists only inside the frame currently being
//! processed. Frames are never written to disk, buffered beyond one frame
//! length, or sent anywhere. Only derived scalar metadata leaves this process.

mod audio;
mod energy;
mod frame;

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::energy::EnergyReading;
use crate::frame::Framer;

const FRAME_MS: u32 = 20;

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))?;
    }

    let capture = audio::start_capture()?;
    let frame_len = (capture.sample_rate * FRAME_MS / 1000) as usize;
    let mut framer = Framer::new(frame_len);

    eprintln!(
        "ice-engine | device: {} | {} Hz, {} ch | frame: {FRAME_MS}ms ({frame_len} samples)",
        capture.device_name, capture.sample_rate, capture.channels
    );
    eprintln!("listening — raw audio is processed per-frame and discarded. Ctrl+C to end session.\n");

    let session_start = Instant::now();
    let mut frames_processed: u64 = 0;
    let mut samples_discarded: u64 = 0;
    let mut peak_dbfs: f32 = f32::NEG_INFINITY;

    while running.load(Ordering::SeqCst) {
        // Blocks until the capture callback delivers more mono samples.
        let Some(chunk) = capture.recv() else { break };

        for frame in framer.push(&chunk) {
            let reading = energy::rms(&frame);
            frames_processed += 1;
            samples_discarded += frame.len() as u64;
            peak_dbfs = peak_dbfs.max(reading.dbfs);
            render_meter(&reading, frames_processed, samples_discarded);
            // `frame` drops here: the raw audio for these 20ms is gone.
        }
    }

    let elapsed = session_start.elapsed();
    println!("\n\nsession summary");
    println!("  duration:          {:.1}s", elapsed.as_secs_f32());
    println!("  frames processed:  {frames_processed}");
    println!("  peak level:        {peak_dbfs:.1} dBFS");
    println!(
        "  audio discarded:   {} samples ({} KiB of raw audio never stored)",
        samples_discarded,
        samples_discarded * 4 / 1024
    );
    println!("  audio persisted:   0 bytes");
    Ok(())
}

/// Live single-line meter: `[########----------------] -32.4 dBFS`
fn render_meter(reading: &EnergyReading, frames: u64, discarded: u64) {
    const WIDTH: usize = 32;
    // Map -60..0 dBFS onto the bar.
    let fill = (((reading.dbfs + 60.0) / 60.0).clamp(0.0, 1.0) * WIDTH as f32) as usize;
    let bar: String = "#".repeat(fill) + &"-".repeat(WIDTH - fill);
    print!(
        "\r[{bar}] {:>6.1} dBFS | frames: {frames} | discarded: {} KiB ",
        reading.dbfs,
        discarded * 4 / 1024
    );
    let _ = std::io::stdout().flush();
}
