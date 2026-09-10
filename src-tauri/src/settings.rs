use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub welcome_completed: bool,
    #[serde(default = "default_start_at_login")]
    pub start_at_login: bool,
    pub quick_capture_shortcut: String,
    pub overdue_reminders_enabled: bool,
    pub overdue_interval_minutes: u64,
    #[serde(default = "default_sticky_reminders_enabled")]
    pub sticky_reminders_enabled: bool,
    #[serde(default = "default_tomorrow_reminder_time")]
    pub tomorrow_reminder_time: String,
    #[serde(default = "default_clock_format")]
    pub clock_format: String,
    #[serde(default = "default_audio_input_mode")]
    pub audio_input_mode: String,
    #[serde(default = "default_task_retention_days")]
    pub task_retention_days: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            welcome_completed: false,
            start_at_login: default_start_at_login(),
            quick_capture_shortcut: "CommandOrControl+Shift+Space".into(),
            overdue_reminders_enabled: true,
            overdue_interval_minutes: 15,
            sticky_reminders_enabled: default_sticky_reminders_enabled(),
            tomorrow_reminder_time: default_tomorrow_reminder_time(),
            clock_format: default_clock_format(),
            audio_input_mode: default_audio_input_mode(),
            task_retention_days: default_task_retention_days(),
        }
    }
}

impl AppSettings {
    pub fn load() -> AppResult<Self> {
        let path = settings_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let settings: Self = serde_json::from_slice(&fs::read(path)?)?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self) -> AppResult<()> {
        self.validate()?;
        let path = settings_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    fn validate(&self) -> AppResult<()> {
        if self.quick_capture_shortcut.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Quick capture shortcut cannot be empty".into(),
            ));
        }
        if !(1..=1_440).contains(&self.overdue_interval_minutes) {
            return Err(AppError::InvalidInput(
                "Overdue reminder interval must be between 1 and 1440 minutes".into(),
            ));
        }
        if !["12h", "24h"].contains(&self.clock_format.as_str()) {
            return Err(AppError::InvalidInput(
                "Clock format must be 12h or 24h".into(),
            ));
        }
        if !["microphone", "system"].contains(&self.audio_input_mode.as_str()) {
            return Err(AppError::InvalidInput(
                "Audio input mode must be microphone or system".into(),
            ));
        }
        if !(1..=365).contains(&self.task_retention_days) {
            return Err(AppError::InvalidInput(
                "Task retention must be between 1 and 365 days".into(),
            ));
        }
        let valid_tomorrow_time = self
            .tomorrow_reminder_time
            .split_once(':')
            .and_then(|(hour, minute)| Some((hour.parse::<u8>().ok()?, minute.parse::<u8>().ok()?)))
            .is_some_and(|(hour, minute)| hour < 24 && minute < 60);
        if !valid_tomorrow_time {
            return Err(AppError::InvalidInput(
                "Tomorrow reminder time must use HH:MM format".into(),
            ));
        }
        Ok(())
    }
}

fn default_clock_format() -> String {
    "24h".into()
}

fn default_start_at_login() -> bool {
    true
}

fn default_audio_input_mode() -> String {
    "microphone".into()
}

fn default_task_retention_days() -> u64 {
    7
}

fn default_sticky_reminders_enabled() -> bool {
    true
}

fn default_tomorrow_reminder_time() -> String {
    "08:30".into()
}

fn settings_path() -> AppResult<PathBuf> {
    ProjectDirs::from("com", "threadbox", "Threadbox")
        .map(|dirs| dirs.config_dir().join("settings.json"))
        .ok_or(AppError::DataDirectory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_quarter_hour_overdue_reminders() {
        let settings = AppSettings::default();
        assert!(!settings.welcome_completed);
        assert!(settings.start_at_login);
        assert!(settings.overdue_reminders_enabled);
        assert_eq!(settings.overdue_interval_minutes, 15);
        assert!(settings.sticky_reminders_enabled);
        assert_eq!(settings.tomorrow_reminder_time, "08:30");
        assert_eq!(settings.clock_format, "24h");
        assert_eq!(settings.audio_input_mode, "microphone");
        assert_eq!(settings.task_retention_days, 7);
        assert_eq!(
            settings.quick_capture_shortcut,
            "CommandOrControl+Shift+Space"
        );
    }
}
