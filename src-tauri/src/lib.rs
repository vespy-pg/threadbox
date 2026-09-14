mod analysis;
mod cloud_speech;
mod database;
mod documents;
mod error;
mod gmail;
mod google;
mod google_calendar;
mod integration;
pub mod integrations;
mod media;
mod meetings;
mod microsoft;
mod microsoft_graph;
mod native_audio;
mod native_messaging;
mod providers;
mod screenshot;
mod secrets;
mod settings;
mod speech;
mod standard_mail;
mod transcriptions;
mod vocabulary;
mod workspace;

use std::path::PathBuf;

use analysis::MeetingAnalysis;
use base64::Engine;
use database::{Database, Task, TaskInput};
use documents::{ProjectDocument, ProjectDocumentInput};
use error::AppResult;
use gmail::{
    ExternalMailLabel, ExternalMailMessage, MailActionResult, MailDraftInput, MailListInput,
    MailPage, MailSyncResult, ProjectMailItem,
};
use google_calendar::{
    AvailableSlot, CalendarEventDraft, ExternalCalendar, ExternalCalendarEvent, FindTimeInput,
};
use integrations::{
    ExternalActionInput, IntegrationConnectionInput, IntegrationSnapshot, PrivacyReceipt,
    CAPABILITY_CALENDAR_FREE_BUSY, CAPABILITY_CALENDAR_READ, CAPABILITY_CALENDAR_WRITE,
    CAPABILITY_MAIL_COMPOSE, CAPABILITY_MAIL_CONTENT_READ, CAPABILITY_MAIL_METADATA_READ,
    CAPABILITY_MAIL_SEND,
};
use meetings::{Meeting, MeetingInput};
use native_audio::{
    MeetingAudioRecorder, NativeAudioPlayer, NativeAudioRecorder, NativeRecordingResult,
};
use providers::{LanguageModelStatus, ProviderProbe};
use serde::Serialize;
use serde_json::Value;
use settings::AppSettings;
use speech::ModelStatus;
use standard_mail::{MailDiagnostic, StandardMailConnectionInput};
use tauri_plugin_autostart::ManagerExt;
use transcriptions::{MeetingTranscript, ProcessingJob, TranscriptionJobConfig};
use vocabulary::{
    VocabularyCandidate, VocabularySet, VocabularySetInput, VocabularyTerm, VocabularyTermInput,
};
use workspace::{
    Organization, OrganizationInput, Person, PersonInput, Project, ProjectInput, ProjectLanguage,
    ProjectMember,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpeechCloudStatus {
    provider: String,
    model: String,
    configured: bool,
    key_present: bool,
}

fn configured_google_client_id() -> AppResult<String> {
    let configured = AppSettings::load()?.google_oauth_client_id;
    if !configured.trim().is_empty() {
        return Ok(configured.trim().to_string());
    }
    Ok(option_env!("THREADBOX_GOOGLE_CLIENT_ID")
        .unwrap_or_default()
        .to_string())
}

fn configured_microsoft_client_id() -> AppResult<String> {
    let configured = AppSettings::load()?.microsoft_oauth_client_id;
    if !configured.trim().is_empty() {
        return Ok(configured.trim().to_string());
    }
    Ok(option_env!("THREADBOX_MICROSOFT_CLIENT_ID")
        .unwrap_or_default()
        .to_string())
}

fn google_capability_scope(capability: &str) -> AppResult<&'static str> {
    match capability {
        CAPABILITY_CALENDAR_READ => Ok(google::SCOPE_CALENDAR_READ),
        CAPABILITY_CALENDAR_WRITE => Ok(google::SCOPE_CALENDAR_WRITE),
        CAPABILITY_CALENDAR_FREE_BUSY => Ok(google::SCOPE_CALENDAR_FREE_BUSY),
        CAPABILITY_MAIL_METADATA_READ => Ok(google::SCOPE_GMAIL_METADATA),
        CAPABILITY_MAIL_CONTENT_READ => Ok(google::SCOPE_GMAIL_READONLY),
        CAPABILITY_MAIL_COMPOSE => Ok(google::SCOPE_GMAIL_COMPOSE),
        CAPABILITY_MAIL_SEND => Ok(google::SCOPE_GMAIL_SEND),
        _ => Err(error::AppError::InvalidInput(format!(
            "Unknown Google capability: {capability}"
        ))),
    }
}

fn google_scopes(
    snapshot: &IntegrationSnapshot,
    added: Option<&str>,
) -> AppResult<Vec<&'static str>> {
    let mut scopes = google::SCOPE_IDENTITY.to_vec();
    let capabilities = snapshot
        .capabilities
        .iter()
        .filter(|item| item.status == "granted")
        .map(|item| item.capability.as_str())
        .chain(added)
        .collect::<Vec<_>>();
    for capability in &capabilities {
        match *capability {
            CAPABILITY_CALENDAR_READ
            | CAPABILITY_CALENDAR_WRITE
            | CAPABILITY_CALENDAR_FREE_BUSY => {
                let scope = google_capability_scope(capability)?;
                if !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
            CAPABILITY_MAIL_METADATA_READ
            | CAPABILITY_MAIL_CONTENT_READ
            | CAPABILITY_MAIL_COMPOSE
            | CAPABILITY_MAIL_SEND => {}
            _ => {
                return Err(error::AppError::InvalidInput(format!(
                    "Unknown Google capability: {capability}"
                )))
            }
        }
    }
    let has = |capability: &str| capabilities.contains(&capability);
    if has(CAPABILITY_MAIL_CONTENT_READ) {
        scopes.push(google::SCOPE_GMAIL_READONLY);
    } else if has(CAPABILITY_MAIL_METADATA_READ) {
        scopes.push(google::SCOPE_GMAIL_METADATA);
    }
    if has(CAPABILITY_MAIL_COMPOSE) {
        scopes.push(google::SCOPE_GMAIL_COMPOSE);
    } else if has(CAPABILITY_MAIL_SEND) {
        scopes.push(google::SCOPE_GMAIL_SEND);
    }
    Ok(scopes)
}

fn microsoft_capability_scope(capability: &str) -> AppResult<&'static str> {
    match capability {
        CAPABILITY_MAIL_METADATA_READ => Ok(microsoft::SCOPE_MAIL_METADATA),
        CAPABILITY_MAIL_CONTENT_READ => Ok(microsoft::SCOPE_MAIL_READ),
        CAPABILITY_MAIL_COMPOSE => Ok(microsoft::SCOPE_MAIL_COMPOSE),
        CAPABILITY_MAIL_SEND => Ok(microsoft::SCOPE_MAIL_SEND),
        _ => Err(error::AppError::InvalidInput(format!(
            "Unknown Microsoft capability: {capability}"
        ))),
    }
}

fn microsoft_scopes(
    snapshot: &IntegrationSnapshot,
    added: Option<&str>,
) -> AppResult<Vec<&'static str>> {
    let capabilities = snapshot
        .capabilities
        .iter()
        .filter(|item| item.status == "granted")
        .map(|item| item.capability.as_str())
        .chain(added)
        .collect::<Vec<_>>();
    let mut scopes = microsoft::SCOPE_IDENTITY.to_vec();
    let has = |capability: &str| capabilities.contains(&capability);
    if has(CAPABILITY_MAIL_CONTENT_READ) || has(CAPABILITY_MAIL_COMPOSE) {
        scopes.push(if has(CAPABILITY_MAIL_COMPOSE) {
            microsoft::SCOPE_MAIL_COMPOSE
        } else {
            microsoft::SCOPE_MAIL_READ
        });
    } else if has(CAPABILITY_MAIL_METADATA_READ) {
        scopes.push(microsoft::SCOPE_MAIL_METADATA);
    }
    if has(CAPABILITY_MAIL_SEND) {
        scopes.push(microsoft::SCOPE_MAIL_SEND);
    }
    for capability in capabilities {
        microsoft_capability_scope(capability)?;
    }
    Ok(scopes)
}

#[tauri::command]
fn list_tasks(database: tauri::State<'_, Database>) -> AppResult<Vec<Task>> {
    database.list_tasks()
}

#[tauri::command]
fn create_task(database: tauri::State<'_, Database>, input: TaskInput) -> AppResult<Task> {
    database.create_task(input)
}

#[tauri::command]
fn update_task(database: tauri::State<'_, Database>, patch: Value) -> AppResult<Task> {
    database.update_task(patch)
}

#[tauri::command]
fn delete_task(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_task(&id)
}

#[tauri::command]
fn restore_task(database: tauri::State<'_, Database>, id: String) -> AppResult<Task> {
    database.restore_task(&id)
}

#[tauri::command]
fn delete_tasks(
    database: tauri::State<'_, Database>,
    ids: Vec<String>,
    permanently: bool,
) -> AppResult<()> {
    database.delete_tasks(&ids, permanently)
}

#[tauri::command]
fn export_backup(database: tauri::State<'_, Database>, path: PathBuf) -> AppResult<()> {
    database.export(&path)
}

#[tauri::command]
fn list_organizations(database: tauri::State<'_, Database>) -> AppResult<Vec<Organization>> {
    database.list_organizations()
}

#[tauri::command]
fn create_organization(
    database: tauri::State<'_, Database>,
    input: OrganizationInput,
) -> AppResult<Organization> {
    database.create_organization(input)
}

#[tauri::command]
fn update_organization(
    database: tauri::State<'_, Database>,
    patch: Value,
) -> AppResult<Organization> {
    database.update_organization(patch)
}

#[tauri::command]
fn delete_organization(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_organization(&id)
}

#[tauri::command]
fn list_projects(
    database: tauri::State<'_, Database>,
    organization_id: Option<String>,
) -> AppResult<Vec<Project>> {
    database.list_projects(organization_id)
}

#[tauri::command]
fn create_project(database: tauri::State<'_, Database>, input: ProjectInput) -> AppResult<Project> {
    database.create_project(input)
}

#[tauri::command]
fn update_project(database: tauri::State<'_, Database>, patch: Value) -> AppResult<Project> {
    database.update_project(patch)
}

#[tauri::command]
fn delete_project(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_project(&id)
}

#[tauri::command]
fn project_context_scope(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<String>> {
    database.project_context_scope(&project_id)
}

#[tauri::command]
fn link_projects(
    database: tauri::State<'_, Database>,
    project_id: String,
    linked_project_id: String,
) -> AppResult<()> {
    database.link_projects(&project_id, &linked_project_id)
}

#[tauri::command]
fn unlink_projects(
    database: tauri::State<'_, Database>,
    project_id: String,
    linked_project_id: String,
) -> AppResult<()> {
    database.unlink_projects(&project_id, &linked_project_id)
}

#[tauri::command]
fn list_project_links(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<Project>> {
    database.list_project_links(&project_id)
}

#[tauri::command]
fn list_people(database: tauri::State<'_, Database>) -> AppResult<Vec<Person>> {
    database.list_people()
}

#[tauri::command]
fn create_person(database: tauri::State<'_, Database>, input: PersonInput) -> AppResult<Person> {
    database.create_person(input)
}

/// The person record representing the user of this installation, if one has been marked.
#[tauri::command]
fn self_person(database: tauri::State<'_, Database>) -> AppResult<Option<Person>> {
    database.self_person()
}

#[tauri::command]
fn update_person(database: tauri::State<'_, Database>, patch: Value) -> AppResult<Person> {
    database.update_person(patch)
}

#[tauri::command]
fn delete_person(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_person(&id)
}

#[tauri::command]
fn list_organization_people(
    database: tauri::State<'_, Database>,
    organization_id: String,
) -> AppResult<Vec<workspace::OrganizationMember>> {
    database.list_organization_people(&organization_id)
}

#[tauri::command]
fn add_organization_person(
    database: tauri::State<'_, Database>,
    organization_id: String,
    person_id: String,
    role: Option<String>,
) -> AppResult<()> {
    database.add_organization_person(&organization_id, &person_id, role)
}

#[tauri::command]
fn remove_organization_person(
    database: tauri::State<'_, Database>,
    organization_id: String,
    person_id: String,
) -> AppResult<()> {
    database.remove_organization_person(&organization_id, &person_id)
}

#[tauri::command]
fn list_project_people(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<ProjectMember>> {
    database.list_project_people(&project_id)
}

#[tauri::command]
fn add_project_person(
    database: tauri::State<'_, Database>,
    project_id: String,
    person_id: String,
    role: Option<String>,
) -> AppResult<()> {
    database.add_project_person(&project_id, &person_id, role)
}

#[tauri::command]
fn remove_project_person(
    database: tauri::State<'_, Database>,
    project_id: String,
    person_id: String,
) -> AppResult<()> {
    database.remove_project_person(&project_id, &person_id)
}

/// Documents and notes held on a project, which are both reference material for the user and the
/// context a model reads for that project.
#[tauri::command]
fn list_project_documents(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<ProjectDocument>> {
    database.list_project_documents(&project_id)
}

#[tauri::command]
fn create_project_document(
    database: tauri::State<'_, Database>,
    input: ProjectDocumentInput,
) -> AppResult<ProjectDocument> {
    database.create_project_document(input)
}

#[tauri::command]
fn update_project_document(
    database: tauri::State<'_, Database>,
    patch: Value,
) -> AppResult<ProjectDocument> {
    database.update_project_document(patch)
}

#[tauri::command]
fn delete_project_document(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_project_document(&id)
}

#[tauri::command]
fn list_meetings(
    database: tauri::State<'_, Database>,
    project_id: Option<String>,
) -> AppResult<Vec<Meeting>> {
    database.list_meetings(project_id.as_deref())
}

#[tauri::command]
fn create_meeting(database: tauri::State<'_, Database>, input: MeetingInput) -> AppResult<Meeting> {
    database.create_meeting(input)
}

#[tauri::command]
fn update_meeting(database: tauri::State<'_, Database>, patch: Value) -> AppResult<Meeting> {
    database.update_meeting(patch)
}

#[tauri::command]
fn delete_meeting(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_meeting(&id)
}

#[tauri::command]
async fn start_meeting_recording(
    database: tauri::State<'_, Database>,
    recorder: tauri::State<'_, MeetingAudioRecorder>,
    id: String,
) -> AppResult<Meeting> {
    database.get_meeting(&id)?;
    let owned_id = id.clone();
    let recorder = recorder.inner().clone();
    let recording_worker = recorder.clone();
    tauri::async_runtime::spawn_blocking(move || recording_worker.start(&owned_id))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))??;
    match database.mark_meeting_recording(&id) {
        Ok(meeting) => Ok(meeting),
        Err(error) => {
            let _ = recorder.cancel(&id);
            Err(error)
        }
    }
}

#[tauri::command]
async fn stop_meeting_recording(
    database: tauri::State<'_, Database>,
    recorder: tauri::State<'_, MeetingAudioRecorder>,
    id: String,
) -> AppResult<Meeting> {
    let owned_id = id.clone();
    let recorder = recorder.inner().clone();
    let recording = tauri::async_runtime::spawn_blocking(move || recorder.stop(&owned_id))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))??;
    match database.store_meeting_recording(&id, &recording.wav, recording.duration_seconds) {
        Ok(meeting) => Ok(meeting),
        Err(error) => {
            let _ = database.reset_meeting_after_failed_recording(&id);
            Err(error)
        }
    }
}

#[tauri::command]
fn meeting_transcript(
    database: tauri::State<'_, Database>,
    id: String,
) -> AppResult<Option<MeetingTranscript>> {
    database.meeting_transcript(&id)
}

#[tauri::command]
fn meeting_transcription_jobs(
    database: tauri::State<'_, Database>,
    id: String,
) -> AppResult<Vec<ProcessingJob>> {
    database.list_meeting_jobs(&id)
}

#[tauri::command]
async fn transcribe_meeting(
    database: tauri::State<'_, Database>,
    id: String,
    provider: Option<String>,
) -> AppResult<MeetingTranscript> {
    let settings = AppSettings::load().unwrap_or_default().speech;
    let provider = provider.unwrap_or(settings.provider);
    let model_id = match provider.as_str() {
        providers::SPEECH_LOCAL => settings.model,
        providers::SPEECH_OPENAI => settings.cloud_model,
        _ => {
            return Err(error::AppError::InvalidInput(format!(
                "Unknown speech provider: {provider}"
            )))
        }
    };
    let config = TranscriptionJobConfig {
        provider,
        model_id,
        language: settings.language,
    };
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let job = database.enqueue_meeting_transcription(&id, &config)?;
        database.process_transcription_job(&job.id)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn meeting_analysis(
    database: tauri::State<'_, Database>,
    id: String,
) -> AppResult<Option<MeetingAnalysis>> {
    database.meeting_analysis(&id)
}

#[tauri::command]
fn meeting_analysis_jobs(
    database: tauri::State<'_, Database>,
    id: String,
) -> AppResult<Vec<ProcessingJob>> {
    database.list_meeting_analysis_jobs(&id)
}

#[tauri::command]
async fn analyse_meeting(
    database: tauri::State<'_, Database>,
    id: String,
) -> AppResult<MeetingAnalysis> {
    let settings = AppSettings::load().unwrap_or_default().language_model;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let job = database.enqueue_meeting_analysis(&id)?;
        database.process_analysis_job(&job.id, &settings)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn list_vocabulary_sets(
    database: tauri::State<'_, Database>,
    project_id: Option<String>,
) -> AppResult<Vec<VocabularySet>> {
    database.list_vocabulary_sets(project_id.as_deref())
}

#[tauri::command]
fn create_vocabulary_set(
    database: tauri::State<'_, Database>,
    input: VocabularySetInput,
) -> AppResult<VocabularySet> {
    database.create_vocabulary_set(input)
}

#[tauri::command]
fn update_vocabulary_set(
    database: tauri::State<'_, Database>,
    id: String,
    name: String,
    always_active: bool,
    project_ids: Vec<String>,
) -> AppResult<VocabularySet> {
    database.update_vocabulary_set(&id, &name, always_active, project_ids)
}

#[tauri::command]
fn delete_vocabulary_set(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_vocabulary_set(&id)
}

#[tauri::command]
fn list_vocabulary_terms(
    database: tauri::State<'_, Database>,
    set_id: String,
) -> AppResult<Vec<VocabularyTerm>> {
    database.list_vocabulary_terms(&set_id)
}

#[tauri::command]
fn create_vocabulary_term(
    database: tauri::State<'_, Database>,
    input: VocabularyTermInput,
) -> AppResult<VocabularyTerm> {
    database.create_vocabulary_term(input)
}

#[tauri::command]
fn update_vocabulary_term(
    database: tauri::State<'_, Database>,
    id: String,
    input: VocabularyTermInput,
) -> AppResult<VocabularyTerm> {
    database.update_vocabulary_term(input, &id)
}

#[tauri::command]
fn delete_vocabulary_term(database: tauri::State<'_, Database>, id: String) -> AppResult<()> {
    database.delete_vocabulary_term(&id)
}

#[tauri::command]
fn vocabulary_candidates(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<VocabularyCandidate>> {
    database.vocabulary_candidates(&project_id)
}

#[tauri::command]
fn dismiss_vocabulary_candidate(
    database: tauri::State<'_, Database>,
    project_id: String,
    text: String,
) -> AppResult<()> {
    database.dismiss_vocabulary_candidate(&project_id, &text)
}

#[tauri::command]
fn correct_transcript_segment(
    database: tauri::State<'_, Database>,
    id: String,
    text: String,
) -> AppResult<()> {
    database.correct_transcript_segment(&id, &text)
}

/// Stored media is recorded relative to this directory, so the interface needs it to display a file.
#[tauri::command]
fn media_root(database: tauri::State<'_, Database>) -> AppResult<String> {
    Ok(database.media_root().to_string_lossy().into_owned())
}

/// Writes an attachment to a path the user chose. `source` is either inline data or a stored media
/// reference.
#[tauri::command]
fn save_data_url(
    database: tauri::State<'_, Database>,
    path: PathBuf,
    data_url: String,
) -> AppResult<()> {
    let bytes = if data_url.starts_with("data:") {
        let encoded = data_url
            .split_once(',')
            .map(|(_, value)| value)
            .ok_or_else(|| error::AppError::InvalidInput("Invalid attachment data".into()))?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| {
                error::AppError::InvalidInput(format!("Invalid attachment data: {error}"))
            })?
    } else {
        database.read_media(&data_url)?
    };
    std::fs::write(path, bytes)?;
    Ok(())
}

#[tauri::command]
fn open_media(database: tauri::State<'_, Database>, path: String) -> AppResult<()> {
    database.open_media(&path)
}

/// Every speech model on offer, with its size and whether it is already downloaded.
#[tauri::command]
fn speech_models() -> AppResult<Vec<ModelStatus>> {
    speech::statuses()
}

#[tauri::command]
async fn download_model(id: String) -> AppResult<ModelStatus> {
    tauri::async_runtime::spawn_blocking(move || speech::download(&id))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

/// Transcribes one recording. Without an explicit model this is a voice note, which always uses the
/// small model so that capture stays fast whatever is configured for meetings; the language comes
/// from the settings unless the caller states one.
#[tauri::command]
async fn transcribe_wav(
    wav_base64: String,
    model: Option<String>,
    language: Option<String>,
    provider: Option<String>,
) -> AppResult<String> {
    let speech_settings = AppSettings::load().unwrap_or_default().speech;
    let language = language.unwrap_or(speech_settings.language);
    let provider = provider.unwrap_or_else(|| providers::SPEECH_LOCAL.into());
    tauri::async_runtime::spawn_blocking(move || match provider.as_str() {
        providers::SPEECH_LOCAL => {
            let model = model.unwrap_or_else(|| speech::DEFAULT_MODEL.to_string());
            speech::transcribe(&wav_base64, &model, &language)
        }
        providers::SPEECH_OPENAI => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(wav_base64)
                .map_err(|error| {
                    error::AppError::InvalidInput(format!("Invalid audio data: {error}"))
                })?;
            let key = secrets::read(&secrets::speech_account(providers::SPEECH_OPENAI))?
                .ok_or_else(|| {
                    error::AppError::InvalidInput(
                        "Add an OpenAI speech API key in Settings before using cloud transcription"
                            .into(),
                    )
                })?;
            let transcript = cloud_speech::transcribe_openai_wav_channel(
                &bytes,
                providers::OPENAI_SPEECH_MODEL,
                &language,
                0,
                None,
                &key,
            )?;
            Ok(transcript
                .segments
                .into_iter()
                .map(|segment| segment.text)
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string())
        }
        _ => Err(error::AppError::InvalidInput(format!(
            "Unknown speech provider: {provider}"
        ))),
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

/// Cloud speech credentials are separate from language model credentials, so either permission can
/// be revoked independently.
#[tauri::command]
fn speech_cloud_status() -> SpeechCloudStatus {
    let settings = AppSettings::load().unwrap_or_default().speech;
    let key_present = secrets::is_present(&secrets::speech_account(providers::SPEECH_OPENAI));
    SpeechCloudStatus {
        provider: providers::SPEECH_OPENAI.into(),
        model: settings.cloud_model,
        configured: key_present,
        key_present,
    }
}

#[tauri::command]
fn set_speech_cloud_key(provider: String, key: String) -> AppResult<SpeechCloudStatus> {
    if provider != providers::SPEECH_OPENAI {
        return Err(error::AppError::InvalidInput(format!(
            "Unknown cloud speech provider: {provider}"
        )));
    }
    secrets::store(&secrets::speech_account(&provider), &key)?;
    Ok(speech_cloud_status())
}

#[tauri::command]
fn delete_speech_cloud_key(provider: String) -> AppResult<SpeechCloudStatus> {
    if provider != providers::SPEECH_OPENAI {
        return Err(error::AppError::InvalidInput(format!(
            "Unknown cloud speech provider: {provider}"
        )));
    }
    secrets::delete(&secrets::speech_account(&provider))?;
    Ok(speech_cloud_status())
}

/// What will answer the next analysis request, so the interface can say so before anything is sent.
#[tauri::command]
fn language_model_status() -> AppResult<LanguageModelStatus> {
    Ok(providers::language_model_status(
        &AppSettings::load()?.language_model,
    ))
}

/// Asks the configured provider whether it is reachable. Only ever called from the test button.
#[tauri::command]
async fn test_language_model() -> AppResult<ProviderProbe> {
    let settings = AppSettings::load()?.language_model;
    tauri::async_runtime::spawn_blocking(move || providers::probe(&settings))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

/// Stores an API key in the operating system credential store. The key is never written to the
/// settings file and never read back out to the interface.
#[tauri::command]
fn set_language_model_key(provider: String, key: String) -> AppResult<LanguageModelStatus> {
    if !providers::is_known_api_provider(&provider) {
        return Err(error::AppError::InvalidInput(format!(
            "Unknown provider: {provider}"
        )));
    }
    secrets::store(&secrets::language_model_account(&provider), &key)?;
    language_model_status()
}

/// Deletes the stored key rather than blanking a field.
#[tauri::command]
fn delete_language_model_key(provider: String) -> AppResult<LanguageModelStatus> {
    secrets::delete(&secrets::language_model_account(&provider))?;
    language_model_status()
}

/// Which language recognition will use for a project, with every fallback already applied.
#[tauri::command]
fn project_language(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<ProjectLanguage> {
    database.project_language(&project_id)
}

#[tauri::command]
fn get_settings() -> AppResult<AppSettings> {
    AppSettings::load()
}

#[tauri::command]
fn update_settings(settings: AppSettings) -> AppResult<AppSettings> {
    settings.save()?;
    Ok(settings)
}

#[tauri::command]
fn list_integration_connections(
    database: tauri::State<'_, Database>,
    organization_id: String,
) -> AppResult<Vec<IntegrationSnapshot>> {
    database.integration_snapshots(&organization_id)
}

#[tauri::command]
fn list_privacy_receipts(
    database: tauri::State<'_, Database>,
    organization_id: String,
) -> AppResult<Vec<PrivacyReceipt>> {
    database.privacy_receipts(&organization_id, 100)
}

#[tauri::command]
async fn connect_google(
    database: tauri::State<'_, Database>,
    organization_id: String,
    connection_id: Option<String>,
) -> AppResult<IntegrationSnapshot> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let existing = connection_id
            .as_deref()
            .map(|id| database.integration_snapshot(id))
            .transpose()?;
        if existing
            .as_ref()
            .is_some_and(|item| item.connection.organization_id != organization_id)
        {
            return Err(error::AppError::InvalidInput(
                "That Google connection belongs to another organisation".into(),
            ));
        }
        let scopes = existing
            .as_ref()
            .map(|snapshot| google_scopes(snapshot, None))
            .transpose()?
            .unwrap_or_else(|| google::SCOPE_IDENTITY.to_vec());
        let login_hint = existing
            .as_ref()
            .map(|item| item.connection.account_identifier.as_str());
        let (token, profile) = google::authorize(&client_id, &scopes, login_hint)?;
        if login_hint.is_some_and(|expected| !profile.email.eq_ignore_ascii_case(expected)) {
            return Err(error::AppError::InvalidInput(
                "Google authorized a different account than the connection being repaired".into(),
            ));
        }
        let snapshot = database.upsert_integration_connection(IntegrationConnectionInput {
            organization_id,
            provider: "google".into(),
            account_identifier: profile.email.clone(),
            display_name: if profile.name.trim().is_empty() {
                profile.email
            } else {
                profile.name
            },
        })?;
        google::store_authorized_token(&snapshot.connection.id, token)?;
        Ok(snapshot)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn grant_google_capability(
    database: tauri::State<'_, Database>,
    connection_id: String,
    capability: String,
) -> AppResult<IntegrationSnapshot> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = database.integration_snapshot(&connection_id)?;
        if snapshot.connection.provider != "google" {
            return Err(error::AppError::InvalidInput(
                "That is not a Google connection".into(),
            ));
        }
        let scope = google_capability_scope(&capability)?;
        let scopes = google_scopes(&snapshot, Some(&capability))?;
        let (token, profile) = google::authorize(
            &client_id,
            &scopes,
            Some(&snapshot.connection.account_identifier),
        )?;
        if !profile
            .email
            .eq_ignore_ascii_case(&snapshot.connection.account_identifier)
        {
            return Err(error::AppError::InvalidInput(
                "Google authorized a different account".into(),
            ));
        }
        if !token.grants(&scopes) {
            return Err(error::AppError::InvalidInput(
                "Google did not grant every permission selected in Threadbox".into(),
            ));
        }
        google::store_authorized_token(&connection_id, token)?;
        database.set_integration_capability(&connection_id, &capability, scope, true)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn revoke_google_capability(
    database: tauri::State<'_, Database>,
    connection_id: String,
    capability: String,
) -> AppResult<IntegrationSnapshot> {
    let scope = google_capability_scope(&capability)?;
    database.set_integration_capability(&connection_id, &capability, scope, false)
}

#[tauri::command]
fn disconnect_google(database: tauri::State<'_, Database>, connection_id: String) -> AppResult<()> {
    google::delete_token(&connection_id)?;
    database.disconnect_integration(&connection_id)
}

#[tauri::command]
async fn connect_microsoft(
    database: tauri::State<'_, Database>,
    organization_id: String,
    connection_id: Option<String>,
) -> AppResult<IntegrationSnapshot> {
    let client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let existing = connection_id
            .as_deref()
            .map(|id| database.integration_snapshot(id))
            .transpose()?;
        if existing
            .as_ref()
            .is_some_and(|item| item.connection.organization_id != organization_id)
        {
            return Err(error::AppError::InvalidInput(
                "That Microsoft connection belongs to another organisation".into(),
            ));
        }
        let scopes = existing
            .as_ref()
            .map(|snapshot| microsoft_scopes(snapshot, None))
            .transpose()?
            .unwrap_or_else(|| microsoft::SCOPE_IDENTITY.to_vec());
        let login_hint = existing
            .as_ref()
            .map(|item| item.connection.account_identifier.as_str());
        let (token, profile) = microsoft::authorize(&client_id, &scopes, login_hint)?;
        let email = profile.mail.unwrap_or(profile.user_principal_name);
        if login_hint.is_some_and(|expected| !email.eq_ignore_ascii_case(expected)) {
            return Err(error::AppError::InvalidInput(
                "Microsoft authorized a different account".into(),
            ));
        }
        let snapshot = database.upsert_integration_connection(IntegrationConnectionInput {
            organization_id,
            provider: "microsoft".into(),
            account_identifier: email.clone(),
            display_name: if profile.display_name.trim().is_empty() {
                email
            } else {
                profile.display_name
            },
        })?;
        microsoft::store_authorized_token(&snapshot.connection.id, token)?;
        Ok(snapshot)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn grant_microsoft_capability(
    database: tauri::State<'_, Database>,
    connection_id: String,
    capability: String,
) -> AppResult<IntegrationSnapshot> {
    let client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = database.integration_snapshot(&connection_id)?;
        if snapshot.connection.provider != "microsoft" {
            return Err(error::AppError::InvalidInput(
                "That is not a Microsoft connection".into(),
            ));
        }
        let scope = microsoft_capability_scope(&capability)?;
        let scopes = microsoft_scopes(&snapshot, Some(&capability))?;
        let (token, profile) = microsoft::authorize(
            &client_id,
            &scopes,
            Some(&snapshot.connection.account_identifier),
        )?;
        let email = profile.mail.unwrap_or(profile.user_principal_name);
        if !email.eq_ignore_ascii_case(&snapshot.connection.account_identifier) {
            return Err(error::AppError::InvalidInput(
                "Microsoft authorized a different account".into(),
            ));
        }
        if !token.grants(&scopes) {
            return Err(error::AppError::InvalidInput(
                "Microsoft did not grant every permission selected in Threadbox".into(),
            ));
        }
        microsoft::store_authorized_token(&connection_id, token)?;
        database.set_integration_capability(&connection_id, &capability, scope, true)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn revoke_microsoft_capability(
    database: tauri::State<'_, Database>,
    connection_id: String,
    capability: String,
) -> AppResult<IntegrationSnapshot> {
    let scope = microsoft_capability_scope(&capability)?;
    database.set_integration_capability(&connection_id, &capability, scope, false)
}

#[tauri::command]
fn disconnect_microsoft(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<()> {
    microsoft::delete_token(&connection_id)?;
    database.disconnect_integration(&connection_id)
}

#[tauri::command]
fn connect_standard_mail(
    database: tauri::State<'_, Database>,
    input: StandardMailConnectionInput,
) -> AppResult<IntegrationSnapshot> {
    database.upsert_standard_mail_connection(&input)
}

#[tauri::command]
async fn diagnose_standard_mail(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<MailDiagnostic> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || database.diagnose_standard_mail(&connection_id))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn disconnect_standard_mail(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<()> {
    secrets::delete(&secrets::integration_account("imap", &connection_id))?;
    secrets::delete(&secrets::integration_account("smtp", &connection_id))?;
    database.disconnect_integration(&connection_id)
}

#[tauri::command]
async fn list_google_calendars(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<Vec<ExternalCalendar>> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.google_calendars(&connection_id, &client_id)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn list_google_calendar_events(
    database: tauri::State<'_, Database>,
    connection_id: String,
    calendar_id: String,
    time_min: String,
    time_max: String,
) -> AppResult<Vec<ExternalCalendarEvent>> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.google_calendar_events(
            &connection_id,
            &client_id,
            &calendar_id,
            &time_min,
            &time_max,
        )
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn create_google_calendar_event(
    database: tauri::State<'_, Database>,
    draft: CalendarEventDraft,
) -> AppResult<ExternalCalendarEvent> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let action = database.create_approved_external_action(&ExternalActionInput {
            organization_id: draft.organization_id.clone(),
            project_id: draft.project_id.clone(),
            connection_id: draft.connection_id.clone(),
            capability: CAPABILITY_CALENDAR_WRITE.into(),
            kind: "calendar.event.create".into(),
            payload: serde_json::to_value(&draft)?,
        })?;
        let attempt = database.start_external_action_attempt(&action.id)?;
        let result = database.create_google_calendar_event(&client_id, &draft);
        match result {
            Ok(event) => {
                database.finish_external_action_attempt(
                    &action.id,
                    attempt,
                    Some(&event.id),
                    None,
                )?;
                Ok(event)
            }
            Err(error) => {
                database.finish_external_action_attempt(
                    &action.id,
                    attempt,
                    None,
                    Some(&error.to_string()),
                )?;
                Err(error)
            }
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn import_google_calendar_event(
    database: tauri::State<'_, Database>,
    connection_id: String,
    project_id: String,
    event: ExternalCalendarEvent,
) -> AppResult<Meeting> {
    database.require_integration_capability(&connection_id, CAPABILITY_CALENDAR_READ)?;
    let snapshot = database.integration_snapshot(&connection_id)?;
    let project = database.get_project(&project_id)?;
    if project.organization_id != snapshot.connection.organization_id {
        return Err(error::AppError::InvalidInput(
            "The event and project must belong to the same organisation".into(),
        ));
    }
    let external_id = format!("{}:{}", event.calendar_id, event.id);
    if let Some(meeting_id) = database.external_calendar_meeting_id(&connection_id, &external_id)? {
        return database.get_meeting(&meeting_id);
    }
    let meeting = database.create_meeting(MeetingInput {
        project_id: Some(project_id.clone()),
        title: event.summary.clone(),
        scheduled_start: (!event.all_day).then_some(event.start.clone()),
    })?;
    database.link_external_calendar_event(
        &connection_id,
        &external_id,
        &project_id,
        &meeting.id,
        event.etag.as_deref(),
    )?;
    Ok(meeting)
}

#[tauri::command]
async fn find_google_calendar_time(
    database: tauri::State<'_, Database>,
    input: FindTimeInput,
) -> AppResult<Vec<AvailableSlot>> {
    let client_id = configured_google_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.find_google_calendar_time(&client_id, &input)
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn list_mail_labels(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<Vec<ExternalMailLabel>> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        match database
            .integration_snapshot(&connection_id)?
            .connection
            .provider
            .as_str()
        {
            "google" => database.google_mail_labels(&connection_id, &google_client_id),
            "microsoft" => database.microsoft_mail_labels(&connection_id, &microsoft_client_id),
            "standard_mail" => database.standard_mail_labels(&connection_id),
            provider => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn list_mail(
    database: tauri::State<'_, Database>,
    input: MailListInput,
) -> AppResult<MailPage> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        match database
            .integration_snapshot(&input.connection_id)?
            .connection
            .provider
            .as_str()
        {
            "google" => database.google_mail_page(&google_client_id, &input),
            "microsoft" => database.microsoft_mail_page(&microsoft_client_id, &input),
            "standard_mail" => database.standard_mail_page(&input),
            provider => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn sync_mail(
    database: tauri::State<'_, Database>,
    connection_id: String,
) -> AppResult<MailSyncResult> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        match database
            .integration_snapshot(&connection_id)?
            .connection
            .provider
            .as_str()
        {
            "google" => database.sync_google_mail_headers(&connection_id, &google_client_id),
            "microsoft" => {
                database.sync_microsoft_mail_headers(&connection_id, &microsoft_client_id)
            }
            "standard_mail" => database.sync_standard_mail_headers(&connection_id),
            provider => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn get_mail_message(
    database: tauri::State<'_, Database>,
    connection_id: String,
    message_id: String,
) -> AppResult<ExternalMailMessage> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        match database
            .integration_snapshot(&connection_id)?
            .connection
            .provider
            .as_str()
        {
            "google" => {
                database.google_mail_message(&connection_id, &google_client_id, &message_id)
            }
            "microsoft" => {
                database.microsoft_mail_message(&connection_id, &microsoft_client_id, &message_id)
            }
            "standard_mail" => database.standard_mail_message(&connection_id, &message_id),
            provider => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn get_mail_attachment(
    database: tauri::State<'_, Database>,
    connection_id: String,
    message_id: String,
    attachment_id: String,
    mime_type: String,
) -> AppResult<String> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        match database
            .integration_snapshot(&connection_id)?
            .connection
            .provider
            .as_str()
        {
            "google" => database.google_mail_attachment(
                &connection_id,
                &google_client_id,
                &message_id,
                &attachment_id,
                &mime_type,
            ),
            "microsoft" => database.microsoft_mail_attachment(
                &connection_id,
                &microsoft_client_id,
                &message_id,
                &attachment_id,
                &mime_type,
            ),
            "standard_mail" => database.standard_mail_attachment(
                &connection_id,
                &message_id,
                &attachment_id,
                &mime_type,
            ),
            provider => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn import_mail_message(
    database: tauri::State<'_, Database>,
    connection_id: String,
    project_id: String,
    message: ExternalMailMessage,
) -> AppResult<ProjectMailItem> {
    database.import_mail_message(&connection_id, &project_id, &message)
}

#[tauri::command]
fn list_project_mail(
    database: tauri::State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<ProjectMailItem>> {
    database.project_mail_items(&project_id)
}

#[tauri::command]
async fn create_mail_draft(
    database: tauri::State<'_, Database>,
    input: MailDraftInput,
) -> AppResult<MailActionResult> {
    execute_mail_action(database.inner().clone(), input, false).await
}

#[tauri::command]
async fn send_mail(
    database: tauri::State<'_, Database>,
    input: MailDraftInput,
) -> AppResult<MailActionResult> {
    execute_mail_action(database.inner().clone(), input, true).await
}

async fn execute_mail_action(
    database: Database,
    input: MailDraftInput,
    send: bool,
) -> AppResult<MailActionResult> {
    let google_client_id = configured_google_client_id()?;
    let microsoft_client_id = configured_microsoft_client_id()?;
    tauri::async_runtime::spawn_blocking(move || {
        let capability = if send {
            CAPABILITY_MAIL_SEND
        } else {
            CAPABILITY_MAIL_COMPOSE
        };
        let action = database.create_approved_external_action(&ExternalActionInput {
            organization_id: input.organization_id.clone(),
            project_id: input.project_id.clone(),
            connection_id: input.connection_id.clone(),
            capability: capability.into(),
            kind: if send {
                "mail.message.send".into()
            } else {
                "mail.draft.create".into()
            },
            payload: serde_json::to_value(&input)?,
        })?;
        let attempt = database.start_external_action_attempt(&action.id)?;
        let provider = database
            .integration_snapshot(&input.connection_id)?
            .connection
            .provider;
        let result = match (provider.as_str(), send) {
            ("google", true) => database.send_google_mail(&google_client_id, &input),
            ("google", false) => database.create_google_mail_draft(&google_client_id, &input),
            ("microsoft", true) => database.send_microsoft_mail(&microsoft_client_id, &input),
            ("microsoft", false) => {
                database.create_microsoft_mail_draft(&microsoft_client_id, &input)
            }
            ("standard_mail", true) => database.send_standard_mail(&input),
            ("standard_mail", false) => Err(error::AppError::InvalidInput(
                "Standard SMTP accounts do not have a remote drafts API".into(),
            )),
            _ => Err(error::AppError::InvalidInput(format!(
                "Mail is not implemented for {provider}"
            ))),
        };
        match result {
            Ok(result) => {
                let receipt = result.draft_id.as_deref().unwrap_or(&result.id);
                database.finish_external_action_attempt(
                    &action.id,
                    attempt,
                    Some(receipt),
                    None,
                )?;
                Ok(result)
            }
            Err(error) => {
                database.finish_external_action_attempt(
                    &action.id,
                    attempt,
                    None,
                    Some(&error.to_string()),
                )?;
                Err(error)
            }
        }
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn capture_screenshot() -> AppResult<String> {
    screenshot::capture().await
}

#[tauri::command]
async fn start_native_recording(
    recorder: tauri::State<'_, NativeAudioRecorder>,
    input_mode: String,
) -> AppResult<()> {
    let recorder = recorder.inner().clone();
    tauri::async_runtime::spawn_blocking(move || recorder.start(&input_mode))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn warm_up_audio() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(native_audio::warm_up)
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn stop_native_recording(
    recorder: tauri::State<'_, NativeAudioRecorder>,
) -> AppResult<NativeRecordingResult> {
    let recorder = recorder.inner().clone();
    tauri::async_runtime::spawn_blocking(move || recorder.stop())
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn test_reminder_sound() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(native_audio::play_reminder_sound)
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn play_recording(
    source: String,
    start_seconds: Option<f64>,
    database: tauri::State<'_, Database>,
    player: tauri::State<'_, NativeAudioPlayer>,
) -> AppResult<()> {
    let wav_base64 = if source.starts_with("data:") {
        source
            .split_once(',')
            .map(|(_, value)| value.to_string())
            .unwrap_or_default()
    } else {
        base64::engine::general_purpose::STANDARD.encode(database.read_media(&source)?)
    };
    let player = player.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player.play(&wav_base64, start_seconds.unwrap_or_default())
    })
    .await
    .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
fn stop_recording_playback(player: tauri::State<'_, NativeAudioPlayer>) {
    player.stop();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let database = Database::open_default().expect("failed to initialize the Threadbox database");
    let resume_database = database.clone();
    if let Err(error) = integration::register_firefox_native_host() {
        eprintln!("Could not register the Firefox native messaging host: {error}");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            integration::show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(database)
        .manage(NativeAudioRecorder::default())
        .manage(MeetingAudioRecorder::default())
        .manage(NativeAudioPlayer::default())
        .setup(move |app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--autostart"]),
            ))?;

            let settings = AppSettings::load().unwrap_or_default();
            let resume_language_model = settings.language_model.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Ok(connection) = resume_database.connect() {
                    if let Err(error) = transcriptions::recover_interrupted_jobs(&connection) {
                        eprintln!("Could not recover interrupted transcriptions: {error}");
                    }
                }
                if let Err(error) = resume_database.resume_transcription_jobs() {
                    eprintln!("Could not resume pending transcriptions: {error}");
                }
                if let Err(error) = resume_database.resume_analysis_jobs(&resume_language_model) {
                    eprintln!("Could not resume pending meeting analyses: {error}");
                }
            });
            let autostart = app.autolaunch();
            let autostart_result = if settings.start_at_login {
                autostart.enable()
            } else {
                autostart.disable()
            };
            if let Err(error) = autostart_result {
                eprintln!("Could not synchronize the autostart setting: {error}");
            }

            integration::configure_app(app)?;
            if !std::env::args().any(|argument| argument == "--autostart") {
                integration::show_main_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_tasks,
            create_task,
            update_task,
            delete_task,
            restore_task,
            delete_tasks,
            export_backup,
            list_organizations,
            create_organization,
            update_organization,
            delete_organization,
            list_projects,
            create_project,
            update_project,
            delete_project,
            project_context_scope,
            link_projects,
            unlink_projects,
            list_project_links,
            list_people,
            create_person,
            self_person,
            update_person,
            delete_person,
            list_organization_people,
            add_organization_person,
            remove_organization_person,
            list_project_people,
            add_project_person,
            remove_project_person,
            list_project_documents,
            create_project_document,
            update_project_document,
            delete_project_document,
            list_meetings,
            create_meeting,
            update_meeting,
            delete_meeting,
            start_meeting_recording,
            stop_meeting_recording,
            meeting_transcript,
            meeting_transcription_jobs,
            transcribe_meeting,
            meeting_analysis,
            meeting_analysis_jobs,
            analyse_meeting,
            list_vocabulary_sets,
            create_vocabulary_set,
            update_vocabulary_set,
            delete_vocabulary_set,
            list_vocabulary_terms,
            create_vocabulary_term,
            update_vocabulary_term,
            delete_vocabulary_term,
            vocabulary_candidates,
            dismiss_vocabulary_candidate,
            correct_transcript_segment,
            media_root,
            project_language,
            save_data_url,
            open_media,
            speech_models,
            download_model,
            transcribe_wav,
            speech_cloud_status,
            set_speech_cloud_key,
            delete_speech_cloud_key,
            language_model_status,
            test_language_model,
            set_language_model_key,
            delete_language_model_key,
            get_settings,
            update_settings,
            list_integration_connections,
            list_privacy_receipts,
            connect_google,
            grant_google_capability,
            revoke_google_capability,
            disconnect_google,
            connect_microsoft,
            grant_microsoft_capability,
            revoke_microsoft_capability,
            disconnect_microsoft,
            connect_standard_mail,
            diagnose_standard_mail,
            disconnect_standard_mail,
            list_google_calendars,
            list_google_calendar_events,
            create_google_calendar_event,
            import_google_calendar_event,
            find_google_calendar_time,
            list_mail_labels,
            list_mail,
            sync_mail,
            get_mail_message,
            get_mail_attachment,
            import_mail_message,
            list_project_mail,
            create_mail_draft,
            send_mail,
            capture_screenshot,
            start_native_recording,
            warm_up_audio,
            stop_native_recording,
            test_reminder_sound,
            play_recording,
            stop_recording_playback
        ])
        .build(tauri::generate_context!())
        .expect("error while building Threadbox")
        .run(|_app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                providers::shutdown_managed_server();
            }
        });
}

pub fn run_native_messaging() -> AppResult<()> {
    native_messaging::run()
}
