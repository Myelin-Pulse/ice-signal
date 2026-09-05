//! Microphone capture via cpal. The audio callback downmixes to mono and
//! forwards samples to the processing thread; nothing is buffered beyond the
//! channel in flight and nothing ever touches disk.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

pub struct Capture {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    rx: Receiver<Vec<f32>>,
    _stream: cpal::Stream,
}

impl Capture {
    /// Next chunk of mono samples. Returns an empty chunk on a quiet timeout
    /// (so callers can poll their shutdown flag) and `None` once capture ends.
    pub fn recv(&self) -> Option<Vec<f32>> {
        match self.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => Some(chunk),
            Err(RecvTimeoutError::Timeout) => Some(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
}

pub fn start_capture() -> Result<Capture> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device — check microphone permissions"))?;
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());
    let config = device
        .default_input_config()
        .context("querying default input config")?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    let (tx, rx) = mpsc::channel();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), channels, tx)?,
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), channels, tx)?,
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), channels, tx)?,
        other => return Err(anyhow!("unsupported sample format {other:?}")),
    };
    stream.play().context("starting input stream")?;

    Ok(Capture {
        device_name,
        sample_rate,
        channels,
        rx,
        _stream: stream,
    })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: u16,
    tx: mpsc::Sender<Vec<f32>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1) as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|frame| {
                    frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32
                })
                .collect();
            let _ = tx.send(mono);
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}
