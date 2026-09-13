//! Cloud speech recognition adapters.
//!
//! The first adapter deliberately uses OpenAI's timestamp-capable Whisper endpoint. Threadbox
//! uploads each recording channel independently and in bounded chunks, preserving the same source
//! timeline and microphone/system distinction as local recognition.

use std::time::Duration;

use reqwest::blocking::multipart::{Form, Part};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    speech::{self, SpeechSegment, SpeechTranscript, LANGUAGE_AUTO},
};

const OPENAI_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const MAX_UPLOAD_BYTES: usize = 25_000_000;

#[derive(Debug, Deserialize)]
struct OpenAiTranscript {
    #[serde(default)]
    language: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    segments: Vec<OpenAiSegment>,
}

#[derive(Debug, Deserialize)]
struct OpenAiSegment {
    start: f64,
    end: f64,
    text: String,
}

pub(crate) fn transcribe_openai_wav_channel(
    bytes: &[u8],
    model: &str,
    language: &str,
    channel: usize,
    prompt: Option<&str>,
    api_key: &str,
) -> AppResult<SpeechTranscript> {
    let chunks = speech::mono_wav_chunks(bytes, channel)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;
    let mut detected_language = String::new();
    let mut segments = Vec::new();
    for (chunk_index, (offset_ms, chunk)) in chunks.into_iter().enumerate() {
        if chunk.len() > MAX_UPLOAD_BYTES {
            return Err(AppError::InvalidInput(
                "A cloud transcription chunk exceeds the 25 MB provider limit".into(),
            ));
        }
        let chunk_duration_ms = ((chunk.len().saturating_sub(44) / 2) as i64) / 16;
        let part = Part::bytes(chunk)
            .file_name(format!("threadbox-channel-{channel}-{chunk_index}.wav"))
            .mime_str("audio/wav")?;
        let mut form = Form::new()
            .part("file", part)
            .text("model", model.to_string())
            .text("response_format", "verbose_json")
            .text("timestamp_granularities[]", "segment");
        if language != LANGUAGE_AUTO && !language.trim().is_empty() {
            form = form.text("language", language.to_string());
        }
        if let Some(prompt) = prompt.filter(|value| !value.trim().is_empty()) {
            form = form.text("prompt", prompt.to_string());
        }
        let response: OpenAiTranscript = client
            .post(OPENAI_TRANSCRIPTIONS_URL)
            .bearer_auth(api_key)
            .multipart(form)
            .send()?
            .error_for_status()?
            .json()?;
        if detected_language.is_empty() && !response.language.is_empty() {
            detected_language = response.language.clone();
        }
        let fallback_end = response
            .segments
            .last()
            .map(|segment| offset_ms + seconds_to_ms(segment.end))
            .unwrap_or(offset_ms + chunk_duration_ms);
        if response.segments.is_empty() && !response.text.trim().is_empty() {
            segments.push(SpeechSegment {
                start_ms: offset_ms,
                end_ms: fallback_end.max(offset_ms),
                text: response.text.trim().to_string(),
            });
        } else {
            segments.extend(response.segments.into_iter().filter_map(|segment| {
                let text = segment.text.trim().to_string();
                (!text.is_empty()).then_some(SpeechSegment {
                    start_ms: offset_ms + seconds_to_ms(segment.start),
                    end_ms: offset_ms + seconds_to_ms(segment.end),
                    text,
                })
            }));
        }
    }
    Ok(SpeechTranscript {
        language: if detected_language.is_empty() {
            language.to_string()
        } else {
            detected_language
        },
        segments,
    })
}

fn seconds_to_ms(value: f64) -> i64 {
    (value.max(0.0) * 1_000.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_conversion_rounds_and_rejects_negative_offsets() {
        assert_eq!(seconds_to_ms(1.2346), 1_235);
        assert_eq!(seconds_to_ms(-2.0), 0);
    }

    #[test]
    fn verbose_response_keeps_timestamped_segments() {
        let response: OpenAiTranscript = serde_json::from_str(
            r#"{"language":"pl","text":"Hello world","segments":[{"start":0.25,"end":1.5,"text":" Hello world "}]}"#,
        )
        .unwrap();
        assert_eq!(response.language, "pl");
        assert_eq!(seconds_to_ms(response.segments[0].start), 250);
        assert_eq!(response.segments[0].text.trim(), "Hello world");
    }
}
