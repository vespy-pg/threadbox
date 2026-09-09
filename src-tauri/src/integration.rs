use std::{fs, path::PathBuf, time::Duration};

use serde_json::json;
#[cfg(not(target_os = "linux"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager,
};

use crate::{
    database::Database,
    error::{AppError, AppResult},
    native_audio,
    settings::AppSettings,
};

pub fn configure_app(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Open Threadbox").build(app)?;
    let reminders = MenuItemBuilder::with_id("reminders", "Reminder center").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &reminders, &quit])
        .build()?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("Threadbox has no application icon")?;
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Threadbox")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "reminders" => {
                show_main_window(app);
                let _ = app.emit("open-reminders", ());
            }
            "quit" => app.exit(0),
            _ => {}
        });
    #[cfg(not(target_os = "linux"))]
    let tray = tray
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    tray.build(app)?;

    start_reminder_worker(app.handle().clone());
    Ok(())
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn start_reminder_worker(app: AppHandle) {
    std::thread::spawn(move || loop {
        if let (Ok(database), Ok(settings)) = (Database::open_default(), AppSettings::load()) {
            if settings.overdue_reminders_enabled {
                if let Ok(tasks) = database.due_notifications(settings.overdue_interval_minutes) {
                    let has_due_tasks = !tasks.is_empty();
                    let sound_played = has_due_tasks
                        && native_audio::play_reminder_sound()
                            .inspect_err(|error| {
                                eprintln!("Could not play reminder sound: {error}")
                            })
                            .is_ok();
                    let notification_shown = if tasks.len() == 1 {
                        let task = &tasks[0];
                        let body = task
                            .source_label
                            .as_deref()
                            .map(|label| format!("From {label}"))
                            .unwrap_or_else(|| "Threadbox reminder".into());
                        show_system_notification(&task.title, &body)
                    } else if has_due_tasks {
                        let body = tasks
                            .iter()
                            .take(3)
                            .map(|task| task.title.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        show_system_notification(
                            &format!("{} Threadbox reminders", tasks.len()),
                            &body,
                        )
                    } else {
                        false
                    };
                    for task in tasks {
                        if sound_played || notification_shown || settings.sticky_reminders_enabled {
                            let _ = database.mark_notified(&task.id);
                        }
                    }
                    if has_due_tasks && settings.sticky_reminders_enabled {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_always_on_top(true);
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("show-sticky-reminder", ());
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(20));
    });
}

#[cfg(target_os = "linux")]
fn show_system_notification(title: &str, body: &str) -> bool {
    notify_rust::Notification::new()
        .appname("Threadbox")
        .summary(title)
        .body(body)
        .timeout(notify_rust::Timeout::Milliseconds(10_000))
        .show()
        .is_ok()
}

#[cfg(not(target_os = "linux"))]
fn show_system_notification(_title: &str, _body: &str) -> bool {
    false
}

pub fn register_firefox_native_host() -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        let home = directories::BaseDirs::new()
            .ok_or(AppError::DataDirectory)?
            .home_dir()
            .to_path_buf();
        let directory = home.join(".mozilla").join("native-messaging-hosts");
        write_manifest(directory)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn write_manifest(directory: PathBuf) -> AppResult<()> {
    fs::create_dir_all(&directory)?;
    let executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?);
    let manifest = json!({
        "name": "com.threadbox.capture",
        "description": "Threadbox browser capture bridge",
        "path": executable,
        "type": "stdio",
        "allowed_extensions": ["threadbox@threadbox.app"]
    });
    fs::write(
        directory.join("com.threadbox.capture.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_icon_is_32_px_rgba8() {
        let icon = include_bytes!("../icons/32x32.png");
        assert_eq!(&icon[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(icon[16..20].try_into().unwrap()), 32);
        assert_eq!(u32::from_be_bytes(icon[20..24].try_into().unwrap()), 32);
        assert_eq!(icon[24], 8, "tray icon must use 8-bit channels");
        assert_eq!(icon[25], 6, "tray icon must use RGBA color");
    }
}
