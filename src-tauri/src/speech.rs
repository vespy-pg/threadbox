//! Local speech recognition through whisper.cpp.
//!
//! Neither the model nor the language is a constant any more. Model size is a genuine trade between
//! accuracy and speed on hardware the application cannot assume, and a meeting is often in one
//! language while its terminology is in another. `docs/model-providers.md` states the reasoning.

use std::{fs, io::Cursor, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use directories::ProjectDirs;
use serde::Serialize;
use whisper_rs::{
    get_lang_str, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::error::{AppError, AppResult};

/// The model voice notes always use, and the initial choice for everything else. Capturing a thought
/// has to stay fast even when a larger model is configured for meetings.
pub const DEFAULT_MODEL: &str = "small";

/// Detection per recording rather than a stated language.
pub const LANGUAGE_AUTO: &str = "auto";

pub struct SpeechModel {
    pub id: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    /// Download size, used both to show the cost before downloading and to tell a finished download
    /// from a truncated one.
    pub approximate_bytes: u64,
    /// An honest sentence about what the size buys, since the interface cannot measure the user's
    /// machine before they choose.
    pub note: &'static str,
}

const MODELS: [SpeechModel; 3] = [
    SpeechModel {
        id: "small",
        label: "Small",
        file_name: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        approximate_bytes: 488_000_000,
        note: "Fast, and enough for a dictated note. Thin on an hour of several speakers.",
    },
    SpeechModel {
        id: "medium",
        label: "Medium",
        file_name: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        approximate_bytes: 1_530_000_000,
        note: "Clearly better on meetings, and roughly three times slower than small.",
    },
    SpeechModel {
        id: "large",
        label: "Large (v3 turbo)",
        file_name: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        approximate_bytes: 1_620_000_000,
        note: "The most accurate option here, at close to medium's speed. The usual choice for meetings.",
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub note: String,
    pub installed: bool,
    pub path: String,
    pub size_bytes: Option<u64>,
    pub approximate_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechTranscript {
    pub language: String,
    pub segments: Vec<SpeechSegment>,
}

pub fn models() -> &'static [SpeechModel] {
    &MODELS
}

fn model(id: &str) -> AppResult<&'static SpeechModel> {
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown speech model: {id}")))
}

fn model_path(model: &SpeechModel) -> AppResult<PathBuf> {
    let dirs = ProjectDirs::from("com", "threadbox", "Threadbox").ok_or(AppError::DataDirectory)?;
    Ok(dirs.data_dir().join("models").join(model.file_name))
}

pub fn status(id: &str) -> AppResult<ModelStatus> {
    let model = model(id)?;
    let path = model_path(model)?;
    let metadata = fs::metadata(&path).ok();
    // A download interrupted part way leaves a short file; treating it as installed would fail later
    // inside whisper.cpp with nothing the user can act on.
    let complete = metadata
        .as_ref()
        .is_some_and(|item| item.len() >= model.approximate_bytes / 10 * 9);
    Ok(ModelStatus {
        id: model.id.into(),
        label: model.label.into(),
        note: model.note.into(),
        installed: complete,
        path: path.display().to_string(),
        size_bytes: metadata.map(|item| item.len()),
        approximate_bytes: model.approximate_bytes,
    })
}

pub fn statuses() -> AppResult<Vec<ModelStatus>> {
    MODELS.iter().map(|model| status(model.id)).collect()
}

pub fn download(id: &str) -> AppResult<ModelStatus> {
    let model = model(id)?;
    let path = model_path(model)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("download");
    let mut response = reqwest::blocking::Client::new()
        .get(model.url)
        .send()?
        .error_for_status()?;
    let mut output = fs::File::create(&temporary)?;
    std::io::copy(&mut response, &mut output)?;
    fs::rename(temporary, path)?;
    status(id)
}

/// `language` is [`LANGUAGE_AUTO`] for detection per recording, or an ISO 639-1 code.
pub fn transcribe(wav_base64: &str, model_id: &str, language: &str) -> AppResult<String> {
    let bytes = STANDARD
        .decode(wav_base64)
        .map_err(|error| AppError::InvalidInput(format!("Invalid audio data: {error}")))?;
    let result = transcribe_wav_bytes(&bytes, model_id, language, None, None)?;
    Ok(result
        .segments
        .into_iter()
        .map(|segment| segment.text)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string())
}

/// Transcribe one channel of a 16 kHz WAV while preserving Whisper's segment timestamps.
pub fn transcribe_wav_bytes(
    bytes: &[u8],
    model_id: &str,
    language: &str,
    channel: Option<usize>,
    initial_prompt: Option<&str>,
) -> AppResult<SpeechTranscript> {
    let model = status(model_id)?;
    if !model.installed {
        return Err(AppError::InvalidInput(format!(
            "Download the {} speech model in Settings before transcribing",
            model.label
        )));
    }
    let samples = channel_samples(bytes, channel.unwrap_or(0))?
        .into_iter()
        .map(|sample| sample as f32 / i16::MAX as f32)
        .collect::<Vec<_>>();
    let context = WhisperContext::new_with_params(&model.path, WhisperContextParameters::default())
        .map_err(|error| AppError::Speech(error.to_string()))?;
    let mut state = context
        .create_state()
        .map_err(|error| AppError::Speech(error.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let detect = language.trim().is_empty() || language == LANGUAGE_AUTO;
    params.set_language(Some(if detect { LANGUAGE_AUTO } else { language }));
    params.set_detect_language(detect);
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    if let Some(prompt) = initial_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        params.set_initial_prompt(prompt);
    }
    state
        .full(params, &samples)
        .map_err(|error| AppError::Speech(error.to_string()))?;
    let detected_language = if detect {
        let id = state
            .full_lang_id_from_state()
            .map_err(|error| AppError::Speech(error.to_string()))?;
        get_lang_str(id).unwrap_or(LANGUAGE_AUTO).to_string()
    } else {
        language.to_string()
    };
    let mut segments = Vec::new();
    for index in 0..state
        .full_n_segments()
        .map_err(|error| AppError::Speech(error.to_string()))?
    {
        let text = state
            .full_get_segment_text(index)
            .map_err(|error| AppError::Speech(error.to_string()))?
            .trim()
            .to_string();
        if !text.is_empty() {
            segments.push(SpeechSegment {
                start_ms: state
                    .full_get_segment_t0(index)
                    .map_err(|error| AppError::Speech(error.to_string()))?
                    * 10,
                end_ms: state
                    .full_get_segment_t1(index)
                    .map_err(|error| AppError::Speech(error.to_string()))?
                    * 10,
                text,
            });
        }
    }
    Ok(SpeechTranscript {
        language: detected_language,
        segments,
    })
}

/// Extracts one channel from Threadbox's recording format. Kept in one place so local and cloud
/// recognition enforce exactly the same input contract.
pub(crate) fn channel_samples(bytes: &[u8], selected_channel: usize) -> AppResult<Vec<i16>> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000
        || !matches!(spec.channels, 1 | 2)
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(AppError::InvalidInput(
            "Threadbox expects 16 kHz 16-bit mono or stereo WAV audio".into(),
        ));
    }
    if selected_channel >= spec.channels as usize {
        return Err(AppError::InvalidInput(
            "The requested audio channel does not exist".into(),
        ));
    }
    Ok(reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .skip(selected_channel)
        .step_by(spec.channels as usize)
        .collect())
}

/// Produces timestamped mono chunks that stay below the cloud API's 25 MB upload limit. Ten minutes
/// of 16 kHz PCM is about 19.2 MB including its WAV header.
pub(crate) fn mono_wav_chunks(
    bytes: &[u8],
    selected_channel: usize,
) -> AppResult<Vec<(i64, Vec<u8>)>> {
    const SAMPLES_PER_CHUNK: usize = 16_000 * 60 * 10;
    let samples = channel_samples(bytes, selected_channel)?;
    samples
        .chunks(SAMPLES_PER_CHUNK)
        .enumerate()
        .map(|(index, chunk)| {
            let mut cursor = Cursor::new(Vec::new());
            {
                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: 16_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
                for sample in chunk {
                    writer.write_sample(*sample)?;
                }
                writer.finalize()?;
            }
            Ok(((index * SAMPLES_PER_CHUNK / 16) as i64, cursor.into_inner()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_is_listed_once_and_downloads_its_own_file() {
        let ids = models().iter().map(|model| model.id).collect::<Vec<_>>();
        assert!(ids.contains(&DEFAULT_MODEL));
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        for model in models() {
            assert!(model.url.ends_with(model.file_name));
            assert!(model.approximate_bytes > 100_000_000);
        }
    }

    #[test]
    fn an_unknown_model_is_rejected_rather_than_defaulted() {
        assert!(status("enormous").is_err());
        assert!(status(DEFAULT_MODEL).is_ok());
    }

    #[test]
    fn cloud_chunks_are_mono_and_keep_their_timeline_offset() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut cursor,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: 16_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for index in 0..32_000 {
                writer.write_sample(index as i16).unwrap();
                writer.write_sample(-(index as i16)).unwrap();
            }
            writer.finalize().unwrap();
        }
        let chunks = mono_wav_chunks(&cursor.into_inner(), 1).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, 0);
        let reader = hound::WavReader::new(Cursor::new(&chunks[0].1)).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.duration(), 32_000);
    }
}
