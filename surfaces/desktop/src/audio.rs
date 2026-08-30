//! Microphone capture and the pure audio conversions the pill needs.
//!
//! Capture runs on the default input device at whatever format it offers. The
//! conversions below turn that into the 16 kHz mono PCM WAV every
//! ai-tools-api accepts, and they stay free of device handles so they
//! are testable without hardware.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use cpal::{
    Device, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use thiserror::Error;

/// Called when an open capture stream fails after it started.
pub type ErrorSink = Arc<dyn Fn(String) + Send + Sync>;

/// Opens microphone captures. Injectable so failure paths are testable.
pub trait Recorder: Send + Sync {
    /// Starts capturing, reporting any later stream failure to `on_error`.
    fn start(&self, on_error: ErrorSink) -> Result<Box<dyn Capture>, RecorderError>;
}

/// One in-progress capture.
pub trait Capture: Send + Sync {
    /// Returns how long this capture has been running.
    fn elapsed(&self) -> Duration;
    /// Returns and clears the loudest level seen since the last call.
    fn take_level(&self) -> f32;
    /// Stops capture and returns the audio as a 16 kHz mono PCM WAV.
    fn finish(self: Box<Self>) -> Vec<u8>;
    /// Stops capture and throws the audio away.
    fn discard(self: Box<Self>);
}

/// The real microphone, through CPAL's default host and input device.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpalRecorder;

impl Recorder for CpalRecorder {
    fn start(&self, on_error: ErrorSink) -> Result<Box<dyn Capture>, RecorderError> {
        Ok(Box::new(Recording::start(on_error)?))
    }
}

/// Sample rate every transcription request uses.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Longest single recording the companion keeps.
pub const MAX_RECORDING: Duration = Duration::from_secs(120);

/// Failure to record from the local microphone.
#[derive(Debug, Error)]
pub enum RecorderError {
    /// The host has no usable default input device.
    #[error("no microphone is available")]
    NoInputDevice,
    /// The device refused every supported capture configuration.
    #[error("the microphone offers no supported capture format: {0}")]
    UnsupportedFormat(String),
    /// The capture stream failed.
    #[error("microphone capture failed: {0}")]
    Stream(String),
}

#[derive(Debug, Default)]
struct Buffer {
    samples: Vec<f32>,
    peak: f32,
}

/// One in-progress microphone recording.
pub struct Recording {
    stream: Stream,
    capture: Arc<Mutex<Buffer>>,
    sample_rate: u32,
    channels: u16,
    started: Instant,
}

impl Recording {
    /// Starts capturing from the default input device.
    pub fn start(on_error: ErrorSink) -> Result<Self, RecorderError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(RecorderError::NoInputDevice)?;
        Self::start_on(&device, on_error)
    }

    fn start_on(device: &Device, on_error: ErrorSink) -> Result<Self, RecorderError> {
        let supported = device
            .default_input_config()
            .map_err(|error| RecorderError::UnsupportedFormat(error.to_string()))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let capture = Arc::new(Mutex::new(Buffer::default()));
        let sink = Arc::clone(&capture);
        // A stream that dies after it started must reach the state machine, or
        // the pill keeps claiming to record with no capture behind it.
        let on_error = move |error: cpal::StreamError| {
            on_error(RecorderError::Stream(error.to_string()).to_string());
        };
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| append(&sink, data.iter().copied()),
                on_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    append(&sink, data.iter().map(|value| f32::from(*value) / 32_768.0))
                },
                on_error,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    append(
                        &sink,
                        data.iter()
                            .map(|value| (f32::from(*value) - 32_768.0) / 32_768.0),
                    )
                },
                on_error,
                None,
            ),
            other => {
                return Err(RecorderError::UnsupportedFormat(format!("{other:?}")));
            }
        }
        .map_err(|error| RecorderError::Stream(error.to_string()))?;
        stream
            .play()
            .map_err(|error| RecorderError::Stream(error.to_string()))?;
        Ok(Self {
            stream,
            capture,
            sample_rate: config.sample_rate.0,
            channels: config.channels,
            started: Instant::now(),
        })
    }
}

impl Capture for Recording {
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn take_level(&self) -> f32 {
        let mut capture = self
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let peak = capture.peak;
        capture.peak = 0.0;
        peak.clamp(0.0, 1.0)
    }

    fn finish(self: Box<Self>) -> Vec<u8> {
        drop(self.stream);
        let capture = self
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mono = downmix(&capture.samples, self.channels);
        let resampled = resample(&mono, self.sample_rate, TARGET_SAMPLE_RATE);
        encode_wav(&resampled, TARGET_SAMPLE_RATE)
    }

    fn discard(self: Box<Self>) {
        drop(self.stream);
    }
}

fn append(capture: &Arc<Mutex<Buffer>>, samples: impl Iterator<Item = f32>) {
    let mut capture = capture.lock().unwrap_or_else(|error| error.into_inner());
    let limit = MAX_RECORDING.as_secs() as usize * 192_000 * 2;
    for sample in samples {
        capture.peak = capture.peak.max(sample.abs());
        if capture.samples.len() < limit {
            capture.samples.push(sample);
        }
    }
}

/// Averages interleaved channels into one mono track.
pub fn downmix(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resamples one mono track by nearest-neighbour selection.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if samples.is_empty() || from == 0 || to == 0 || from == to {
        return samples.to_vec();
    }
    let target = (samples.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    (0..target)
        .map(|index| {
            let source = index as u64 * u64::from(from) / u64::from(to);
            samples[(source as usize).min(samples.len() - 1)]
        })
        .collect()
}

/// Encodes one mono track as a 16-bit PCM WAV file.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_bytes = samples.len() * 2;
    let mut wav = Vec::with_capacity(44 + data_bytes);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&((36 + data_bytes) as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for sample in samples {
        let clamped = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        wav.extend_from_slice(&clamped.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_frames_average_into_one_mono_track() {
        assert_eq!(downmix(&[1.0, -1.0, 0.5, 0.5], 2), vec![0.0, 0.5]);
        assert_eq!(downmix(&[0.25, 0.5], 1), vec![0.25, 0.5]);
    }

    #[test]
    fn resampling_reaches_the_transcription_rate() {
        let source: Vec<f32> = (0..48_000).map(|index| index as f32 / 48_000.0).collect();
        let resampled = resample(&source, 48_000, TARGET_SAMPLE_RATE);
        assert_eq!(resampled.len(), 16_000);
        assert_eq!(resample(&source, 16_000, 16_000).len(), 48_000);
        assert!(resample(&[], 48_000, 16_000).is_empty());
    }

    #[test]
    fn encoded_audio_is_a_readable_mono_pcm_wav() {
        let wav = encode_wav(&[0.0, 1.0, -1.0], TARGET_SAMPLE_RATE);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 42);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().unwrap()),
            TARGET_SAMPLE_RATE
        );
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(i16::from_le_bytes(wav[46..48].try_into().unwrap()), 32_767);
        assert_eq!(i16::from_le_bytes(wav[48..50].try_into().unwrap()), -32_767);
    }
}
