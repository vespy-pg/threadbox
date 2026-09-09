use std::{fs, io::Cursor, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use directories::ProjectDirs;
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::{AppError, AppResult};

const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub installed: bool,
    pub path: String,
    pub size_bytes: Option<u64>,
}

fn model_path() -> AppResult<PathBuf> {
    let dirs = ProjectDirs::from("com", "threadbox", "Threadbox").ok_or(AppError::DataDirectory)?;
    Ok(dirs.data_dir().join("models").join("ggml-small.bin"))
}

pub fn status() -> AppResult<ModelStatus> {
    let path = model_path()?;
    let metadata = fs::metadata(&path).ok();
    Ok(ModelStatus {
        installed: metadata
            .as_ref()
            .is_some_and(|item| item.len() > 100_000_000),
        path: path.display().to_string(),
        size_bytes: metadata.map(|item| item.len()),
    })
}

pub fn download() -> AppResult<ModelStatus> {
    let path = model_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("download");
    let mut response = reqwest::blocking::Client::new()
        .get(MODEL_URL)
        .send()?
        .error_for_status()?;
    let mut output = fs::File::create(&temporary)?;
    std::io::copy(&mut response, &mut output)?;
    fs::rename(temporary, path)?;
    status()
}

pub fn transcribe(wav_base64: &str) -> AppResult<String> {
    let model = status()?;
    if !model.installed {
        return Err(AppError::InvalidInput(
            "Install the local speech model in Settings before recording".into(),
        ));
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
    params.set_language(Some("pl"));
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
