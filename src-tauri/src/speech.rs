//! Local speech recognition through whisper.cpp.
//!
//! Neither the model nor the language is a constant any more. Model size is a genuine trade between
//! accuracy and speed on hardware the application cannot assume, and a meeting is often in one
//! language while its terminology is in another. `docs/model-providers.md` states the reasoning.

use std::{fs, io::Cursor, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use directories::ProjectDirs;
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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
    let model = status(model_id)?;
    if !model.installed {
        return Err(AppError::InvalidInput(format!(
            "Download the {} speech model in Settings before recording",
            model.label
        )));
    }
    let bytes = STANDARD
        .decode(wav_base64)
        .map_err(|error| AppError::InvalidInput(format!("Invalid audio data: {error}")))?;
    let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        return Err(AppError::InvalidInput(
            "Threadbox expects 16 kHz mono audio".into(),
        ));
    }
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()?
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
    state
        .full(params, &samples)
        .map_err(|error| AppError::Speech(error.to_string()))?;
    let mut transcript = String::new();
    for index in 0..state
        .full_n_segments()
        .map_err(|error| AppError::Speech(error.to_string()))?
    {
        transcript.push_str(
            &state
                .full_get_segment_text(index)
                .map_err(|error| AppError::Speech(error.to_string()))?,
        );
    }
    Ok(transcript.trim().to_string())
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
}
