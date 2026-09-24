//! Microphone capture via cpal. The audio callback selects two channels or downmixes to mono and
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
    rx: Receiver<Vec<[f32; 2]>>,
    _stream: cpal::Stream,
}

impl Capture {
    /// Next chunk of selected channel pairs (mono uses only the first slot). Returns an empty chunk on a quiet timeout
    /// (so callers can poll their shutdown flag) and `None` once capture ends.
    pub fn recv(&self) -> Option<Vec<[f32; 2]>> {
        match self.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => Some(chunk),
            Err(RecvTimeoutError::Timeout) => Some(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
}

pub fn start_capture(dual: bool) -> Result<Capture> {
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

    if dual && channels < 2 {
        return Err(anyhow!(
            "--dual-channel requires two separate participant channels; this device has {channels}"
        ));
    }

    let (tx, rx) = mpsc::channel();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &config.into(), channels, dual, tx)?
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &config.into(), channels, dual, tx)?
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &config.into(), channels, dual, tx)?
        }
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
    dual: bool,
    tx: mpsc::Sender<Vec<[f32; 2]>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1) as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let samples: Vec<[f32; 2]> = data
                .chunks_exact(channels)
                .map(|frame| select_channels(frame, dual))
                .collect();
            let _ = tx.send(samples);
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

fn select_channels<T: SizedSample>(frame: &[T], dual: bool) -> [f32; 2]
where
    f32: FromSample<T>,
{
    if dual {
        [f32::from_sample(frame[0]), f32::from_sample(frame[1])]
    } else {
        [
            frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / frame.len() as f32,
            0.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_selection_preserves_participant_separation() {
        assert_eq!(select_channels(&[0.25f32, -0.75, 1.0], true), [0.25, -0.75]);
        assert_eq!(select_channels(&[0.25f32, -0.75], false), [-0.25, 0.0]);
        assert_eq!(select_channels(&[0.5f32], false), [0.5, 0.0]);
        assert_eq!(select_channels(&[16384i16, -16384], true), [0.5, -0.5]);
    }
}
