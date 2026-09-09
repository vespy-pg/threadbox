use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Stream, StreamConfig,
};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Clone, Default)]
pub struct NativeAudioRecorder {
    active: Arc<Mutex<Option<ActiveRecording>>>,
}

#[derive(Clone, Default)]
pub struct NativeAudioPlayer {
    generation: Arc<AtomicU64>,
}

struct ActiveRecording {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    started_at: Instant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRecordingResult {
    base64: String,
    duration_seconds: f64,
}

impl NativeAudioRecorder {
    pub fn start(&self, input_mode: &str) -> AppResult<()> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| audio_error("The audio recorder state is unavailable"))?;
        if active.is_some() {
            return Err(audio_error("A recording is already in progress"));
        }

        let host = preferred_host()?;
        let device = if input_mode == "system" {
            host.input_devices()
                .map_err(|error| audio_error(format!("Could not list audio inputs: {error}")))?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| {
                            let name = description.name().to_lowercase();
                            name.contains("monitor") || name.contains("loopback")
                        })
                })
                .ok_or_else(|| audio_error("No system audio monitor is available. Enable a monitor source in PipeWire or PulseAudio."))?
        } else {
            host.default_input_device()
                .ok_or_else(|| audio_error("No default microphone is available"))?
        };
        let supported = device.default_input_config().map_err(|error| {
            audio_error(format!("Could not read the audio input format: {error}"))
        })?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let stream = build_stream(&device, &config, sample_format, samples.clone())?;
        stream
            .play()
            .map_err(|error| audio_error(format!("Could not start the audio input: {error}")))?;

        *active = Some(ActiveRecording {
            stream,
            samples,
            sample_rate: config.sample_rate,
            channels: config.channels,
            started_at: Instant::now(),
        });
        Ok(())
    }

    pub fn stop(&self) -> AppResult<NativeRecordingResult> {
        // PulseAudio and PipeWire monitor sources can deliver captured samples well after the
        // user-facing stop event. Keep the stream alive briefly so their queued tail reaches the
        // callback before the WAV is finalized.
        std::thread::sleep(Duration::from_secs(2));
        let recording = self
            .active
            .lock()
            .map_err(|_| audio_error("The audio recorder state is unavailable"))?
            .take()
            .ok_or_else(|| audio_error("No recording is in progress"))?;
        let duration_seconds = recording.started_at.elapsed().as_secs_f64();
        drop(recording.stream);

        let interleaved = recording
            .samples
            .lock()
            .map_err(|_| audio_error("The recorded audio is unavailable"))?
            .clone();
        let mono = mix_to_mono(&interleaved, recording.channels);
        let samples = downsample(&mono, recording.sample_rate, 16_000);
        let wav = encode_wav(&samples, 16_000)?;

        Ok(NativeRecordingResult {
            base64: STANDARD.encode(wav),
            duration_seconds,
        })
    }
}

#[cfg(target_os = "linux")]
fn preferred_host() -> AppResult<cpal::Host> {
    cpal::host_from_id(cpal::HostId::PulseAudio)
        .map_err(|error| audio_error(format!("Could not connect to PulseAudio: {error}")))
}

pub fn play_reminder_sound() -> AppResult<()> {
    let host = preferred_host()?;
    let device = host
        .default_output_device()
        .ok_or_else(|| audio_error("No default audio output is available"))?;
    let supported = device
        .default_output_config()
        .map_err(|error| audio_error(format!("Could not read the audio output format: {error}")))?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let stream = build_reminder_stream(&device, &config, sample_format)?;
    stream
        .play()
        .map_err(|error| audio_error(format!("Could not play the reminder: {error}")))?;
    std::thread::sleep(Duration::from_millis(1_250));
    drop(stream);
    Ok(())
}

impl NativeAudioPlayer {
    pub fn play(&self, wav_base64: &str) -> AppResult<()> {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        play_recording(wav_base64, &self.generation, generation)
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

fn play_recording(
    wav_base64: &str,
    current_generation: &AtomicU64,
    generation: u64,
) -> AppResult<()> {
    let bytes = STANDARD
        .decode(wav_base64)
        .map_err(|error| audio_error(format!("Invalid recording data: {error}")))?;
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|error| audio_error(format!("Invalid WAV recording: {error}")))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(audio_error("Threadbox expects 16-bit mono WAV recordings"));
    }
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / 32_768.0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| audio_error(format!("Could not decode the recording: {error}")))?;
    if samples.is_empty() {
        return Err(audio_error("The recording is empty"));
    }

    let host = preferred_host()?;
    let device = host
        .default_output_device()
        .ok_or_else(|| audio_error("No default audio output is available"))?;
    let supported = device
        .default_output_config()
        .map_err(|error| audio_error(format!("Could not read the audio output format: {error}")))?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let duration = Duration::from_secs_f64(samples.len() as f64 / f64::from(spec.sample_rate));
    let stream = build_recording_stream(
        &device,
        &config,
        sample_format,
        Arc::new(samples),
        spec.sample_rate,
    )?;
    stream
        .play()
        .map_err(|error| audio_error(format!("Could not play the recording: {error}")))?;
    let started_at = Instant::now();
    let playback_duration = duration + Duration::from_millis(100);
    while started_at.elapsed() < playback_duration
        && current_generation.load(Ordering::Acquire) == generation
    {
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(stream);
    Ok(())
}

fn build_recording_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    samples: Arc<Vec<f32>>,
    input_rate: u32,
) -> AppResult<Stream> {
    let channels = usize::from(config.channels);
    let step = f64::from(input_rate) / f64::from(config.sample_rate);
    let stream = match format {
        SampleFormat::F32 => {
            let mut position = 0.0;
            device.build_output_stream(
                *config,
                move |output: &mut [f32], _| {
                    fill_recording(output, channels, &samples, step, &mut position, |sample| {
                        sample
                    })
                },
                output_stream_error,
                None,
            )
        }
        SampleFormat::I16 => {
            let mut position = 0.0;
            device.build_output_stream(
                *config,
                move |output: &mut [i16], _| {
                    fill_recording(output, channels, &samples, step, &mut position, |sample| {
                        (sample * i16::MAX as f32) as i16
                    })
                },
                output_stream_error,
                None,
            )
        }
        SampleFormat::U16 => {
            let mut position = 0.0;
            device.build_output_stream(
                *config,
                move |output: &mut [u16], _| {
                    fill_recording(output, channels, &samples, step, &mut position, |sample| {
                        ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16
                    })
                },
                output_stream_error,
                None,
            )
        }
        other => {
            return Err(audio_error(format!(
                "Unsupported audio output format: {other}"
            )))
        }
    };
    stream.map_err(|error| audio_error(format!("Could not open the audio output: {error}")))
}

fn fill_recording<T>(
    output: &mut [T],
    channels: usize,
    samples: &[f32],
    step: f64,
    position: &mut f64,
    convert: impl Fn(f32) -> T,
) {
    for frame in output.chunks_mut(channels.max(1)) {
        let sample = samples.get(*position as usize).copied().unwrap_or(0.0);
        for channel in frame {
            *channel = convert(sample);
        }
        *position += step;
    }
}

fn build_reminder_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
) -> AppResult<Stream> {
    let sample_rate = config.sample_rate as f32;
    let channels = usize::from(config.channels);
    let stream = match format {
        SampleFormat::F32 => {
            let mut sample_index = 0_u64;
            device.build_output_stream(
                *config,
                move |output: &mut [f32], _| {
                    fill_reminder(output, channels, sample_rate, &mut sample_index, |sample| {
                        sample
                    })
                },
                output_stream_error,
                None,
            )
        }
        SampleFormat::I16 => {
            let mut sample_index = 0_u64;
            device.build_output_stream(
                *config,
                move |output: &mut [i16], _| {
                    fill_reminder(output, channels, sample_rate, &mut sample_index, |sample| {
                        (sample * i16::MAX as f32) as i16
                    })
                },
                output_stream_error,
                None,
            )
        }
        SampleFormat::U16 => {
            let mut sample_index = 0_u64;
            device.build_output_stream(
                *config,
                move |output: &mut [u16], _| {
                    fill_reminder(output, channels, sample_rate, &mut sample_index, |sample| {
                        ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16
                    })
                },
                output_stream_error,
                None,
            )
        }
        other => {
            return Err(audio_error(format!(
                "Unsupported audio output format: {other}"
            )))
        }
    };
    stream.map_err(|error| audio_error(format!("Could not open the audio output: {error}")))
}

fn fill_reminder<T>(
    output: &mut [T],
    channels: usize,
    sample_rate: f32,
    sample_index: &mut u64,
    convert: impl Fn(f32) -> T,
) {
    for frame in output.chunks_mut(channels.max(1)) {
        let time = *sample_index as f32 / sample_rate;
        let sample = reminder_sample(time);
        for channel in frame {
            *channel = convert(sample);
        }
        *sample_index += 1;
    }
}

fn reminder_sample(time: f32) -> f32 {
    let (local_time, frequency) = if (0.0..0.30).contains(&time) {
        (time, 660.0)
    } else if (0.38..0.72).contains(&time) {
        (time - 0.38, 880.0)
    } else if (0.82..1.16).contains(&time) {
        (time - 0.82, 660.0)
    } else {
        return 0.0;
    };
    let envelope = (local_time / 0.025).min(1.0) * ((0.34 - local_time) / 0.08).clamp(0.0, 1.0);
    (std::f32::consts::TAU * frequency * local_time).sin() * envelope * 0.22
}

fn output_stream_error(error: cpal::Error) {
    eprintln!("Reminder audio stream error: {error}");
}

#[cfg(not(target_os = "linux"))]
fn preferred_host() -> AppResult<cpal::Host> {
    Ok(cpal::default_host())
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    samples: Arc<Mutex<Vec<f32>>>,
) -> AppResult<Stream> {
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            *config,
            move |data: &[f32], _| extend_samples(&samples, data.iter().copied()),
            stream_error,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            *config,
            move |data: &[i16], _| {
                extend_samples(
                    &samples,
                    data.iter().map(|sample| *sample as f32 / 32_768.0),
                )
            },
            stream_error,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            *config,
            move |data: &[u16], _| {
                extend_samples(
                    &samples,
                    data.iter()
                        .map(|sample| (*sample as f32 - 32_768.0) / 32_768.0),
                )
            },
            stream_error,
            None,
        ),
        other => {
            return Err(audio_error(format!(
                "Unsupported microphone format: {other}"
            )))
        }
    };
    stream.map_err(|error| audio_error(format!("Could not open the microphone: {error}")))
}

fn extend_samples(samples: &Mutex<Vec<f32>>, values: impl Iterator<Item = f32>) {
    if let Ok(mut target) = samples.lock() {
        target.extend(values);
    }
}

fn stream_error(error: cpal::Error) {
    eprintln!("Microphone stream error: {error}");
}

fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn downsample(samples: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input_rate == output_rate {
        return samples.to_vec();
    }
    let ratio = input_rate as f64 / output_rate as f64;
    let output_len = (samples.len() as f64 / ratio).floor() as usize;
    (0..output_len)
        .map(|index| {
            let start = (index as f64 * ratio).floor() as usize;
            let end = (((index + 1) as f64 * ratio).floor() as usize)
                .max(start + 1)
                .min(samples.len());
            samples[start..end].iter().sum::<f32>() / (end - start) as f32
        })
        .collect()
}

fn encode_wav(samples: &[f32], sample_rate: u32) -> AppResult<Vec<u8>> {
    let data_size = samples
        .len()
        .checked_mul(2)
        .and_then(|size| u32::try_from(size).ok())
        .ok_or_else(|| audio_error("The recording is too long to encode"))?;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        let sample = sample.clamp(-1.0, 1.0);
        let value = if sample < 0.0 {
            (sample * 32_768.0) as i16
        } else {
            (sample * 32_767.0) as i16
        };
        wav.extend_from_slice(&value.to_le_bytes());
    }
    Ok(wav)
}

fn audio_error(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_mono_pcm_wav() {
        let wav = encode_wav(&[0.0, 0.5, -0.5], 16_000).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
    }

    #[test]
    fn mixes_interleaved_stereo_to_mono() {
        assert_eq!(mix_to_mono(&[1.0, -1.0, 0.5, 0.5], 2), [0.0, 0.5]);
    }

    #[test]
    fn reminder_chime_contains_tones_and_silence() {
        assert_ne!(reminder_sample(0.1), 0.0);
        assert_eq!(reminder_sample(0.35), 0.0);
        assert_eq!(reminder_sample(1.2), 0.0);
    }
}
