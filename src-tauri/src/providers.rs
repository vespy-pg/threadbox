//! Which speech and language model providers answer a request, and how each one is configured.
//!
//! Two independent choices, both changeable at any time without losing data: local speech
//! recognition, where the model size and the language behaviour are settings rather than constants,
//! and the language model used for analysis, which is either a server on this machine, an external
//! provider reached with the user's own API key, or an agent command line tool they have already
//! installed and signed in to. `docs/model-providers.md` states why the list is exactly this.

use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    secrets, speech,
};

/// Nothing chosen yet. The first run asks, because no default suits everyone.
pub const KIND_UNSET: &str = "unset";
/// A server on this machine speaking the OpenAI-compatible protocol.
pub const KIND_LOCAL: &str = "local";
/// An external provider reached with the user's own key.
pub const KIND_API: &str = "api";
/// A command line tool the user has already authorised, invoked per request.
pub const KIND_AGENT: &str = "agent";

const API_PROVIDERS: [&str; 4] = ["anthropic", "openai", "openrouter", "compatible"];

/// Recognition happens on this machine, so the only decisions are which model and which language.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSettings {
    /// Model used for meetings and other long material. Voice notes always use the small model, so
    /// that capturing a thought does not become slow when a larger model is chosen here.
    #[serde(default = "default_speech_model")]
    pub model: String,
    /// `auto` for detection per recording, or an ISO 639-1 code to state it.
    #[serde(default = "default_speech_language")]
    pub language: String,
    /// The language the terminology is in when it differs from the spoken language, which is the
    /// normal case for technical work in another language. `None` means the same as `language`.
    #[serde(default)]
    pub terminology_language: Option<String>,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            model: default_speech_model(),
            language: default_speech_language(),
            terminology_language: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelSettings {
    #[serde(default = "default_language_model_kind")]
    pub kind: String,
    #[serde(default)]
    pub local: LocalModelSettings,
    #[serde(default)]
    pub api: ApiModelSettings,
    #[serde(default)]
    pub agent: AgentModelSettings,
}

// Written out rather than derived: a derived `Default` would leave `kind` an empty string, and a
// settings file that omits the whole section falls back to this, not to the serde field default.
impl Default for LanguageModelSettings {
    fn default() -> Self {
        Self {
            kind: default_language_model_kind(),
            local: LocalModelSettings::default(),
            api: ApiModelSettings::default(),
            agent: AgentModelSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelSettings {
    /// Base of the OpenAI-compatible endpoint, without a trailing slash.
    #[serde(default = "default_local_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// Whether Threadbox starts and stops the server itself. When false the user runs it, typically
    /// as a system service, and Threadbox must never terminate it.
    #[serde(default)]
    pub managed: bool,
    /// Command used to start the server when `managed` is set.
    #[serde(default)]
    pub command: String,
    /// Minutes without a request after which a server Threadbox started is stopped, which is what
    /// keeps an unused local model from holding memory all day. Zero keeps it loaded.
    #[serde(default = "default_idle_timeout_minutes")]
    pub idle_timeout_minutes: u64,
}

impl Default for LocalModelSettings {
    fn default() -> Self {
        Self {
            base_url: default_local_base_url(),
            model: String::new(),
            managed: false,
            command: String::new(),
            idle_timeout_minutes: default_idle_timeout_minutes(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiModelSettings {
    /// One of `anthropic`, `openai`, `openrouter`, `compatible`. The key itself lives in the
    /// operating system credential store, never here.
    #[serde(default)]
    pub provider: String,
    /// Only used by `compatible`, where the endpoint is whatever the user points at.
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelSettings {
    /// The tool to invoke, for example `claude` or `codex`. Its credentials stay inside it.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

/// What the interface needs to show about the selected provider without ever holding a key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelStatus {
    pub kind: String,
    /// A sentence naming what will answer the next request, shown before anything is sent.
    pub summary: String,
    /// Whether the configuration is complete enough to be used.
    pub configured: bool,
    /// Whether a key is stored for the currently selected external provider.
    pub key_present: bool,
    /// Which external providers have a key stored, so switching does not look like data loss.
    pub providers_with_keys: Vec<String>,
}

/// The result of a test the user explicitly asked for. A network failure is a result, not an error:
/// the reason belongs in front of the user rather than in a stack trace.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProbe {
    pub reachable: bool,
    pub detail: String,
    pub models: Vec<String>,
}

pub fn validate_speech(settings: &SpeechSettings) -> AppResult<()> {
    if !speech::models()
        .iter()
        .any(|model| model.id == settings.model)
    {
        return Err(AppError::InvalidInput(format!(
            "Unknown speech model: {}",
            settings.model
        )));
    }
    validate_speech_language(&settings.language)?;
    if let Some(language) = settings.terminology_language.as_deref() {
        validate_speech_language(language)?;
    }
    Ok(())
}

pub fn validate_language_model(settings: &LanguageModelSettings) -> AppResult<()> {
    match settings.kind.as_str() {
        KIND_UNSET => Ok(()),
        KIND_LOCAL => {
            validate_base_url(&settings.local.base_url)?;
            if settings.local.model.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "Name the model the local server should load".into(),
                ));
            }
            if settings.local.managed && settings.local.command.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "A managed local server needs the command that starts it".into(),
                ));
            }
            if settings.local.idle_timeout_minutes > 1_440 {
                return Err(AppError::InvalidInput(
                    "The idle timeout must be 1440 minutes or less".into(),
                ));
            }
            Ok(())
        }
        KIND_API => {
            if !API_PROVIDERS.contains(&settings.api.provider.as_str()) {
                return Err(AppError::InvalidInput(format!(
                    "Unknown provider: {}",
                    settings.api.provider
                )));
            }
            if settings.api.provider == "compatible" {
                validate_base_url(&settings.api.base_url)?;
            }
            if settings.api.model.trim().is_empty() {
                return Err(AppError::InvalidInput("Choose a model".into()));
            }
            Ok(())
        }
        KIND_AGENT => {
            if settings.agent.command.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "Name the command line tool to invoke".into(),
                ));
            }
            Ok(())
        }
        other => Err(AppError::InvalidInput(format!(
            "Unknown language model provider: {other}"
        ))),
    }
}

/// States plainly what will answer the next request, which is what makes "nothing outbound happens
/// silently" true in the interface rather than only in the documentation.
pub fn language_model_status(settings: &LanguageModelSettings) -> LanguageModelStatus {
    let providers_with_keys = API_PROVIDERS
        .iter()
        .filter(|provider| secrets::is_present(&secrets::language_model_account(provider)))
        .map(|provider| (*provider).to_string())
        .collect::<Vec<_>>();
    let key_present = providers_with_keys.contains(&settings.api.provider);
    let configured = validate_language_model(settings).is_ok()
        && settings.kind != KIND_UNSET
        && (settings.kind != KIND_API || key_present);
    let summary = match settings.kind.as_str() {
        KIND_LOCAL => format!(
            "{} on this machine, at {}. Nothing leaves the computer.",
            display_model(&settings.local.model),
            settings.local.base_url
        ),
        KIND_API => {
            let base = format!(
                "{} through {}",
                display_model(&settings.api.model),
                settings.api.provider
            );
            if key_present {
                format!("{base}, with your own API key. Requests leave this computer.")
            } else {
                format!("{base}, but no API key is stored yet.")
            }
        }
        KIND_AGENT => format!(
            "The {} command line tool, using the account you signed in to there.",
            settings.agent.command
        ),
        _ => "No language model chosen yet, so nothing is analysed.".into(),
    };
    LanguageModelStatus {
        kind: settings.kind.clone(),
        summary,
        configured,
        key_present,
        providers_with_keys,
    }
}

/// Asks the configured provider whether it is there. Called only when the user presses the test
/// button, never on its own.
pub fn probe(settings: &LanguageModelSettings) -> AppResult<ProviderProbe> {
    validate_language_model(settings)?;
    match settings.kind.as_str() {
        KIND_LOCAL => Ok(list_models(
            &settings.local.base_url,
            &[],
            "The local server answered",
        )),
        KIND_API => {
            let provider = settings.api.provider.as_str();
            let key =
                secrets::read(&secrets::language_model_account(provider))?.ok_or_else(|| {
                    AppError::InvalidInput(format!("No API key is stored for {provider}"))
                })?;
            let base_url = api_base_url(provider, &settings.api.base_url);
            let headers: Vec<(String, String)> = if provider == "anthropic" {
                vec![
                    ("x-api-key".into(), key),
                    ("anthropic-version".into(), "2023-06-01".into()),
                ]
            } else {
                vec![("authorization".into(), format!("Bearer {key}"))]
            };
            Ok(list_models(&base_url, &headers, "The provider answered"))
        }
        KIND_AGENT => Ok(probe_command(&settings.agent.command)),
        _ => Err(AppError::InvalidInput(
            "Choose a language model provider first".into(),
        )),
    }
}

fn list_models(base_url: &str, headers: &[(String, String)], success: &str) -> ProviderProbe {
    let endpoint = format!("{}/models", base_url.trim_end_matches('/'));
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(error) => return ProviderProbe::failed(error.to_string()),
    };
    let mut request = client.get(&endpoint);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let response = match request.send() {
        Ok(response) => response,
        Err(error) => {
            return ProviderProbe::failed(format!("Could not reach {endpoint}: {error}"));
        }
    };
    let status = response.status();
    if !status.is_success() {
        return ProviderProbe::failed(format!("{endpoint} answered {status}"));
    }
    let models = response
        .json::<Value>()
        .ok()
        .as_ref()
        .and_then(|body| body.get("data"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                .take(40)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ProviderProbe {
        reachable: true,
        detail: format!("{success} with {} models.", models.len()),
        models,
    }
}

fn probe_command(command: &str) -> ProviderProbe {
    match Command::new(command).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("").trim().to_string();
            ProviderProbe {
                reachable: true,
                detail: if first_line.is_empty() {
                    format!("{command} is installed.")
                } else {
                    format!("{command} reports {first_line}.")
                },
                models: Vec::new(),
            }
        }
        Ok(output) => ProviderProbe::failed(format!(
            "{command} exited with {}",
            output
                .status
                .code()
                .map_or_else(|| "a signal".to_string(), |code| code.to_string())
        )),
        Err(error) => ProviderProbe::failed(format!("Could not run {command}: {error}")),
    }
}

impl ProviderProbe {
    fn failed(detail: String) -> Self {
        Self {
            reachable: false,
            detail,
            models: Vec::new(),
        }
    }
}

/// Every provider except `compatible` has a fixed endpoint, so the user is not asked for one.
pub fn api_base_url(provider: &str, configured: &str) -> String {
    match provider {
        "anthropic" => "https://api.anthropic.com/v1".into(),
        "openai" => "https://api.openai.com/v1".into(),
        "openrouter" => "https://openrouter.ai/api/v1".into(),
        _ => configured.trim_end_matches('/').to_string(),
    }
}

pub fn is_known_api_provider(provider: &str) -> bool {
    API_PROVIDERS.contains(&provider)
}

fn display_model(model: &str) -> String {
    if model.trim().is_empty() {
        "An unnamed model".into()
    } else {
        model.trim().to_string()
    }
}

/// Accepts `auto` or a language code. Shared with per-project language settings.
pub fn validate_speech_language(language: &str) -> AppResult<()> {
    if language == "auto" {
        return Ok(());
    }
    let plausible = (2..=5).contains(&language.len())
        && language
            .chars()
            .all(|value| value.is_ascii_lowercase() || value == '-');
    if plausible {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Language must be auto or a language code such as pl or en, not {language}"
        )))
    }
}

fn validate_base_url(value: &str) -> AppResult<()> {
    let parsed = url::Url::parse(value.trim()).map_err(|error| {
        AppError::InvalidInput(format!("That is not a usable address: {error}"))
    })?;
    if !["http", "https"].contains(&parsed.scheme()) {
        return Err(AppError::InvalidInput(
            "The endpoint must be an http or https address".into(),
        ));
    }
    Ok(())
}

fn default_speech_model() -> String {
    speech::DEFAULT_MODEL.into()
}

fn default_speech_language() -> String {
    "auto".into()
}

fn default_language_model_kind() -> String {
    KIND_UNSET.into()
}

fn default_local_base_url() -> String {
    "http://127.0.0.1:11434/v1".into()
}

fn default_idle_timeout_minutes() -> u64 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_defaults() {
        validate_speech(&SpeechSettings::default()).unwrap();
        validate_language_model(&LanguageModelSettings::default()).unwrap();
    }

    #[test]
    fn refuses_an_unknown_speech_model_or_language() {
        let settings = SpeechSettings {
            model: "enormous".into(),
            ..Default::default()
        };
        assert!(validate_speech(&settings).is_err());

        let mut settings = SpeechSettings {
            language: "Polish".into(),
            ..Default::default()
        };
        assert!(validate_speech(&settings).is_err());

        settings.language = "pl-PL".into();
        assert!(
            validate_speech(&settings).is_err(),
            "codes are lower case, so a region suffix has to be written pl-pl"
        );
        settings.language = "pt-br".into();
        validate_speech(&settings).unwrap();
    }

    #[test]
    fn a_local_provider_needs_an_endpoint_and_a_model() {
        let mut settings = LanguageModelSettings {
            kind: KIND_LOCAL.into(),
            ..Default::default()
        };
        assert!(
            validate_language_model(&settings).is_err(),
            "the default configuration names no model"
        );
        settings.local.model = "qwen2.5:14b".into();
        validate_language_model(&settings).unwrap();

        settings.local.base_url = "not an address".into();
        assert!(validate_language_model(&settings).is_err());
        settings.local.base_url = "file:///models".into();
        assert!(validate_language_model(&settings).is_err());
    }

    #[test]
    fn a_managed_local_server_needs_its_command() {
        let settings = LanguageModelSettings {
            kind: KIND_LOCAL.into(),
            local: LocalModelSettings {
                model: "qwen2.5:14b".into(),
                managed: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(validate_language_model(&settings).is_err());
    }

    #[test]
    fn an_external_provider_must_be_one_we_know() {
        let mut settings = LanguageModelSettings {
            kind: KIND_API.into(),
            api: ApiModelSettings {
                provider: "somebodys-endpoint".into(),
                model: "claude-opus-5".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(validate_language_model(&settings).is_err());

        settings.api.provider = "anthropic".into();
        validate_language_model(&settings).unwrap();

        settings.api.provider = "compatible".into();
        assert!(
            validate_language_model(&settings).is_err(),
            "a compatible endpoint has no fixed address, so one must be given"
        );
        settings.api.base_url = "https://models.example.com/v1".into();
        validate_language_model(&settings).unwrap();
    }

    #[test]
    fn only_a_compatible_endpoint_uses_the_configured_address() {
        assert_eq!(
            api_base_url("anthropic", "https://ignored.example.com"),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(
            api_base_url("compatible", "https://models.example.com/v1/"),
            "https://models.example.com/v1"
        );
    }

    #[test]
    fn an_unconfigured_provider_says_so_rather_than_pretending() {
        let status = language_model_status(&LanguageModelSettings::default());
        assert!(!status.configured);
        assert!(status.summary.contains("No language model chosen"));

        let settings = LanguageModelSettings {
            kind: KIND_LOCAL.into(),
            local: LocalModelSettings {
                model: "qwen2.5:14b".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let status = language_model_status(&settings);
        assert!(status.configured);
        assert!(status.summary.contains("Nothing leaves the computer"));
    }
}
