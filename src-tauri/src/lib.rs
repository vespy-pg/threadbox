mod database;
mod error;
mod integration;
mod native_audio;
mod native_messaging;
mod screenshot;
mod settings;
mod speech;

use std::path::PathBuf;

use base64::Engine;
use database::{Database, Task, TaskInput};
use error::AppResult;
use native_audio::{NativeAudioPlayer, NativeAudioRecorder, NativeRecordingResult};
use serde_json::Value;
use settings::AppSettings;
use speech::ModelStatus;
use tauri_plugin_autostart::ManagerExt;

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
fn save_data_url(
    database: tauri::State<'_, Database>,
    path: PathBuf,
    data_url: String,
) -> AppResult<()> {
    if !data_url.starts_with("data:") {
        std::fs::write(
            path,
            database.read_media(PathBuf::from(data_url).as_path())?,
        )?;
        return Ok(());
    }
    let encoded = data_url
        .split_once(',')
        .map(|(_, value)| value)
        .ok_or_else(|| error::AppError::InvalidInput("Invalid attachment data".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            error::AppError::InvalidInput(format!("Invalid attachment data: {error}"))
        })?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[tauri::command]
fn open_media(database: tauri::State<'_, Database>, path: PathBuf) -> AppResult<()> {
    database.open_media(&path)
}

#[tauri::command]
fn model_status() -> AppResult<ModelStatus> {
    speech::status()
}

#[tauri::command]
async fn download_model() -> AppResult<ModelStatus> {
    tauri::async_runtime::spawn_blocking(speech::download)
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
}

#[tauri::command]
async fn transcribe_wav(wav_base64: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || speech::transcribe(&wav_base64))
        .await
        .map_err(|error| error::AppError::InvalidInput(error.to_string()))?
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
fn start_native_recording(
    recorder: tauri::State<'_, NativeAudioRecorder>,
    input_mode: String,
) -> AppResult<()> {
    recorder.start(&input_mode)
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
        base64::engine::general_purpose::STANDARD
            .encode(database.read_media(PathBuf::from(source).as_path())?)
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
            save_data_url,
            open_media,
            model_status,
            download_model,
            transcribe_wav,
            get_settings,
            update_settings,
            capture_screenshot,
            start_native_recording,
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
