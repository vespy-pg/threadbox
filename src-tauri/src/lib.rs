mod database;
mod documents;
mod error;
mod integration;
mod media;
mod native_audio;
mod native_messaging;
mod providers;
mod screenshot;
mod secrets;
mod settings;
mod speech;
mod workspace;

use std::path::PathBuf;

use base64::Engine;
use database::{Database, Task, TaskInput};
use documents::{ProjectDocument, ProjectDocumentInput};
use error::AppResult;
use native_audio::{NativeAudioPlayer, NativeAudioRecorder, NativeRecordingResult};
use providers::{LanguageModelStatus, ProviderProbe};
use serde_json::Value;
use settings::AppSettings;
use speech::ModelStatus;
use tauri_plugin_autostart::ManagerExt;
use workspace::{
    Organization, OrganizationInput, Person, PersonInput, Project, ProjectInput, ProjectLanguage,
    ProjectMember,
};

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
) -> AppResult<String> {
    let speech_settings = AppSettings::load().unwrap_or_default().speech;
    let model = model.unwrap_or_else(|| speech::DEFAULT_MODEL.to_string());
    let language = language.unwrap_or(speech_settings.language);
    tauri::async_runtime::spawn_blocking(move || speech::transcribe(&wav_base64, &model, &language))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
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
    tauri::async_runtime::spawn_blocking(move || player.play(&wav_base64))
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
        .manage(database)
        .manage(NativeAudioRecorder::default())
        .manage(NativeAudioPlayer::default())
        .setup(|app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--autostart"]),
            ))?;

            let settings = AppSettings::load().unwrap_or_default();
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
            list_project_people,
            add_project_person,
            remove_project_person,
            list_project_documents,
            create_project_document,
            update_project_document,
            delete_project_document,
            media_root,
            project_language,
            save_data_url,
            open_media,
            speech_models,
            download_model,
            transcribe_wav,
            language_model_status,
            test_language_model,
            set_language_model_key,
            delete_language_model_key,
            get_settings,
            update_settings,
            capture_screenshot,
            start_native_recording,
            warm_up_audio,
            stop_native_recording,
            test_reminder_sound,
            play_recording,
            stop_recording_playback
        ])
        .run(tauri::generate_context!())
        .expect("error while running Threadbox");
}

pub fn run_native_messaging() -> AppResult<()> {
    native_messaging::run()
}
