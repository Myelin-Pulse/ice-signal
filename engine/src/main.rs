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
use crate::host::{
    analytics::SessionAnalytics, events, events::Event, meter, resample::Resampler,
    ws::Broadcaster,
};

const HTTP_PORT: u16 = 9714;
const WS_PORT: u16 = 9715;
static DASHBOARD: &str = include_str!("host/dashboard.html");

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
    let (broadcast, http_port, _) = Broadcaster::start(HTTP_PORT, WS_PORT, DASHBOARD)?;
    let emit = |line: String| {
        broadcast.send(&line);
        println!("{line}");
    };

    eprintln!("events: JSONL on stdout | meter: stderr | Ctrl+C to end session");
    eprintln!("dashboard: http://localhost:{http_port}\n");
    emit(Event::SessionStart.to_json());

    let mut analytics = SessionAnalytics::new();
    let mut device_samples: u64 = 0;
    while running.load(Ordering::SeqCst) {
        // Blocks until the capture callback delivers more mono samples.
        let Some(chunk) = capture.recv() else { break };
        device_samples += chunk.len() as u64;

        for sample in resampler.push(&chunk) {
            let Some(report) = engine.tick(sample) else { continue };
            // `sample` and this frame's audio are gone; only the report remains.
            for event in events::from_report(&report) {
                analytics.observe(&event);
                emit(event.to_json());
            }
            meter::render(&report, device_samples);
        }
    }

    let duration_ms = engine.frames_processed() * FRAME_MS as u64;
    let summary = analytics.finalize(duration_ms);
    emit(summary.to_json());
    emit(Event::PrivacySummary {
        t: duration_ms,
        frames_processed: engine.frames_processed(),
        audio_samples_discarded: device_samples,
    }
    .to_json());
    emit(Event::SessionEnd { t: duration_ms }.to_json());

    eprintln!("\n\n{}", summary.card());
    eprintln!("privacy proof");
    eprintln!("  frames processed:  {}", engine.frames_processed());
    eprintln!(
        "  audio discarded:   {device_samples} samples ({} KiB never stored)",
        device_samples * 4 / 1024
    );
    eprintln!("  audio persisted:   0 bytes");
    eprintln!("  words transcribed: 0");
    Ok(())
}
