//! Ice Signal engine.
//!
//! Layout mirrors the eventual hardware split:
//!   core/ — fixed-point, allocation-free signal chain (ports ~1-1 to FPGA)
//!   host/ — capture, resampling, JSONL output, meter (stays on computer/phone)
//!
//! Privacy invariant: raw audio exists only inside the sample currently being
//! processed. Nothing is written to disk. Only derived scalar metadata leaves
//! this process, as JSONL events on stdout (see docs/event-schema.md).

mod core;
mod host;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;

use crate::core::{Engine, CORE_SAMPLE_RATE, FRAME_MS};
use crate::host::{events, meter, resample::Resampler};

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))?;
    }

    let capture = host::audio::start_capture()?;
    let mut resampler = Resampler::new(capture.sample_rate, CORE_SAMPLE_RATE);
    let mut engine = Engine::new();

    eprintln!(
        "ice-engine | device: {} @ {} Hz, {} ch → core @ {CORE_SAMPLE_RATE} Hz, {FRAME_MS}ms frames",
        capture.device_name, capture.sample_rate, capture.channels
    );
    eprintln!("events: JSONL on stdout | meter: stderr | Ctrl+C to end session\n");
    println!("{}", events::session_start());

    let mut device_samples: u64 = 0;
    while running.load(Ordering::SeqCst) {
        // Blocks until the capture callback delivers more mono samples.
        let Some(chunk) = capture.recv() else { break };
        device_samples += chunk.len() as u64;

        for sample in resampler.push(&chunk) {
            let Some(report) = engine.tick(sample) else { continue };
            // `sample` and this frame's audio are gone; only the report remains.
            for line in events::report_lines(&report) {
                println!("{line}");
            }
            meter::render(&report, device_samples);
        }
    }

    for line in events::session_end(engine.frames_processed(), device_samples) {
        println!("{line}");
    }

    let duration_ms = engine.frames_processed() * FRAME_MS as u64;
    eprintln!("\n\nsession summary");
    eprintln!("  duration:          {:.1}s", duration_ms as f64 / 1000.0);
    eprintln!("  frames processed:  {}", engine.frames_processed());
    eprintln!(
        "  audio discarded:   {device_samples} samples ({} KiB never stored)",
        device_samples * 4 / 1024
    );
    eprintln!("  audio persisted:   0 bytes");
    eprintln!("  words transcribed: 0");
    Ok(())
}
