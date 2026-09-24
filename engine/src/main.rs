//! Ice Signal: fixed-point signal cores, host capture, and metadata-only analytics.
mod core;
mod host;

use crate::core::{CORE_SAMPLE_RATE, FRAME_LEN, FRAME_MS};
use crate::host::{
    demo, events::Event, meter, resample::Resampler, session::Session, ws::Broadcaster,
};
use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const HTTP_PORT: u16 = 9714;
const WS_PORT: u16 = 9715;
static DASHBOARD: &str = include_str!("host/dashboard.html");

fn main() -> Result<()> {
    let mut dual = false;
    let mut demo_mode = false;
    let mut fast = false;
    let mut headless = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dual-channel" => dual = true,
            "--demo" => {
                demo_mode = true;
                dual = true;
            }
            "--fast" => fast = true,
            "--headless" => headless = true,
            "--help" | "-h" => {
                eprintln!("ice-engine [--dual-channel] [--demo [--fast]] [--headless]\n\nDefault: mono microphone, aggregate analytics, social attribution unavailable.\n--dual-channel  Input channels 1/2 must each carry one participant (A/B).\n--demo          Synthetic 20-second conversation, no microphone access.\n--fast          Run demo without real-time pacing.\n--headless      JSONL only; disable dashboard server and live meter.\n\nSocial signals are estimates; channels are not voice identities.");
                return Ok(());
            }
            _ => bail!("unknown option {arg}; use --help"),
        }
    }
    if fast && !demo_mode {
        bail!("--fast requires --demo");
    }
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))?;
    }
    let capture = if demo_mode {
        None
    } else {
        Some(host::audio::start_capture(dual)?)
    };
    let broadcast = if headless {
        None
    } else {
        let (b, http_port, _) = Broadcaster::start(HTTP_PORT, WS_PORT, DASHBOARD)?;
        eprintln!("dashboard: http://localhost:{http_port}");
        Some(b)
    };
    let emit = |line: String| {
        if let Some(b) = &broadcast {
            b.send(&line);
        }
        println!("{line}");
    };
    emit(Event::SessionStart.to_json());
    emit(
        Event::SocialConfig {
            dual_channel: dual,
            demo: demo_mode,
        }
        .to_json(),
    );
    eprintln!(
        "social signals: {}",
        if dual {
            "estimated from separate A/B channels"
        } else {
            "unavailable with mono input"
        }
    );
    let mut session = Session::new(dual);
    let mut device_samples = 0u64;
    if let Some(capture) = capture {
        eprintln!("ice-engine | device: {} @ {} Hz, {} ch → core @ {CORE_SAMPLE_RATE} Hz, {FRAME_MS}ms frames", capture.device_name, capture.sample_rate, capture.channels);
        let mut resamplers = [
            Resampler::new(capture.sample_rate, CORE_SAMPLE_RATE),
            Resampler::new(capture.sample_rate, CORE_SAMPLE_RATE),
        ];
        while running.load(Ordering::SeqCst) {
            let Some(chunk) = capture.recv() else { break };
            // Count every hardware channel sample consumed by the callback.
            device_samples += chunk.len() as u64 * capture.channels as u64;
            let a = resamplers[0].push(&chunk.iter().map(|s| s[0]).collect::<Vec<_>>());
            let b = if dual {
                resamplers[1].push(&chunk.iter().map(|s| s[1]).collect::<Vec<_>>())
            } else {
                vec![0; a.len()]
            };
            for (a, b) in a.into_iter().zip(b) {
                if let Some(report) = session.tick([a, b], &emit) {
                    if !headless {
                        meter::render(&report, device_samples);
                    }
                }
            }
        }
    } else {
        eprintln!("ice-engine | SYNTHETIC DEMO | no microphone accessed");
        let start = std::time::Instant::now();
        for frame in 0..demo::FRAMES {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            device_samples += FRAME_LEN as u64 * 2;
            for samples in demo::frame(frame) {
                if let Some(report) = session.tick(samples, &emit) {
                    if !headless {
                        meter::render(&report, device_samples);
                    }
                }
            }
            if !fast {
                let deadline = Duration::from_millis((frame as u64 + 1) * FRAME_MS as u64);
                std::thread::sleep(deadline.saturating_sub(start.elapsed()));
            }
        }
    }
    let (summary, social) = session.finish(device_samples, &emit);
    eprintln!("\n\n{}\n{}", summary.card(), social.card());
    eprintln!("privacy proof\n  audio discarded: {device_samples} samples\n  audio persisted: 0 bytes\n  words transcribed: 0");
    // Keep the completed demo card available for browsers opened after playback.
    if demo_mode && !fast && !headless {
        eprintln!("Demo complete. Dashboard remains available; Ctrl+C to exit.");
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
