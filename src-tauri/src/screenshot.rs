#[cfg(target_os = "linux")]
use std::{path::PathBuf, process::Command, time::Duration};

#[cfg(target_os = "linux")]
use ashpd::desktop::screenshot::Screenshot;
#[cfg(target_os = "linux")]
use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(target_os = "linux")]
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub async fn capture() -> AppResult<String> {
    #[cfg(target_os = "linux")]
    {
        capture_linux().await
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(AppError::InvalidInput(
            "Screenshot capture is not implemented on this platform yet".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
async fn capture_linux() -> AppResult<String> {
    if uses_x11() && command_exists("import") {
        return tauri::async_runtime::spawn_blocking(capture_x11_area)
            .await
            .map_err(screenshot_error)?;
    }
    capture_with_portal().await
}

#[cfg(target_os = "linux")]
fn uses_x11() -> bool {
    let declared_x11 = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .eq_ignore_ascii_case("x11");
    let display_only =
        std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none();
    declared_x11 || display_only
}

#[cfg(target_os = "linux")]
fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(target_os = "linux")]
fn capture_x11_area() -> AppResult<String> {
    // Give the compositor enough time to unmap and repaint the Threadbox window.
    std::thread::sleep(Duration::from_millis(500));
    let path = temporary_screenshot_path();
    let status = Command::new("import")
        .arg("-silent")
        .arg(&path)
        .status()
        .map_err(screenshot_error)?;
    if !status.success() {
        return Err(AppError::InvalidInput(
            "Screenshot selection was cancelled".into(),
        ));
    }
    encode_file(path)
}

#[cfg(target_os = "linux")]
async fn capture_with_portal() -> AppResult<String> {
    let request = Screenshot::request()
        .interactive(true)
        .modal(false)
        .send()
        .await
        .map_err(screenshot_error)?;
    let screenshot = request.response().map_err(screenshot_error)?;
    let url = url::Url::parse(screenshot.uri().as_str())
        .map_err(|error| AppError::InvalidInput(format!("Invalid screenshot URI: {error}")))?;
    let path = url.to_file_path().map_err(|_| {
        AppError::InvalidInput("The screenshot portal returned a non-file URI".into())
    })?;
    encode_file(path)
}

#[cfg(target_os = "linux")]
fn temporary_screenshot_path() -> PathBuf {
    std::env::temp_dir().join(format!("threadbox-{}.png", Uuid::new_v4()))
}

#[cfg(target_os = "linux")]
fn encode_file(path: PathBuf) -> AppResult<String> {
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(path);
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

#[cfg(target_os = "linux")]
fn screenshot_error(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(format!("Screenshot was not captured: {error}"))
}
