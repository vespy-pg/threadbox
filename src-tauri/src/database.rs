use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::Engine;
use chrono::Utc;
use directories::ProjectDirs;
use rusqlite::{params, Connection, Row};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    error::{AppError, AppResult},
    settings::AppSettings,
};

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub status: String,
    pub priority: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_label: Option<String>,
    pub source_author: Option<String>,
    pub source_excerpt: Option<String>,
    pub due_at: Option<String>,
    pub remind_at: Option<String>,
    pub screenshots: Vec<String>,
    pub audio_attachments: Vec<AudioAttachment>,
    pub links: Vec<String>,
    pub file_attachments: Vec<FileAttachment>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAttachment {
    pub id: String,
    pub data_url: String,
    pub duration_seconds: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub data_url: String,
    pub size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_source")]
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_label: Option<String>,
    pub source_author: Option<String>,
    pub source_excerpt: Option<String>,
    pub due_at: Option<String>,
    pub remind_at: Option<String>,
    #[serde(default)]
    pub screenshot_data_url: Option<String>,
    #[serde(default)]
    pub screenshots: Vec<String>,
    #[serde(default)]
    pub audio_attachments: Vec<AudioAttachment>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub file_attachments: Vec<FileAttachment>,
}

fn default_status() -> String {
    "inbox".into()
}
fn default_priority() -> String {
    "mid".into()
}
fn default_source() -> String {
    "manual".into()
}

impl Database {
    pub fn default_path() -> AppResult<PathBuf> {
        let dirs =
            ProjectDirs::from("com", "threadbox", "Threadbox").ok_or(AppError::DataDirectory)?;
        Ok(dirs.data_dir().join("threadbox.sqlite3"))
    }

    pub fn open_default() -> AppResult<Self> {
        let database = Self {
            path: Self::default_path()?,
        };
        database.migrate()?;
        Ok(database)
    }

    fn connect(&self) -> AppResult<Connection> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(connection)
    }

    fn migrate(&self) -> AppResult<()> {
        let connection = self.connect()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'inbox' CHECK(status IN ('inbox', 'todo', 'waiting', 'done')),
                source_type TEXT NOT NULL DEFAULT 'manual',
                source_url TEXT,
                source_label TEXT,
                source_author TEXT,
                source_excerpt TEXT,
                due_at TEXT,
                remind_at TEXT,
                screenshot_data_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                last_notified_for TEXT,
                screenshots_json TEXT NOT NULL DEFAULT '[]',
                audio_attachments_json TEXT NOT NULL DEFAULT '[]',
                links_json TEXT NOT NULL DEFAULT '[]',
                file_attachments_json TEXT NOT NULL DEFAULT '[]',
                priority TEXT NOT NULL DEFAULT 'mid',
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_due_at ON tasks(due_at);
            CREATE INDEX IF NOT EXISTS idx_tasks_updated_at ON tasks(updated_at DESC);"
        )?;
        let columns = connection
            .prepare("PRAGMA table_info(tasks)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == "last_notified_for") {
            connection.execute("ALTER TABLE tasks ADD COLUMN last_notified_for TEXT", [])?;
        }
        if !columns.iter().any(|name| name == "screenshots_json") {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN screenshots_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !columns.iter().any(|name| name == "audio_attachments_json") {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN audio_attachments_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !columns.iter().any(|name| name == "links_json") {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN links_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !columns.iter().any(|name| name == "file_attachments_json") {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN file_attachments_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !columns.iter().any(|name| name == "priority") {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'mid'",
                [],
            )?;
        }
        if !columns.iter().any(|name| name == "deleted_at") {
            connection.execute("ALTER TABLE tasks ADD COLUMN deleted_at TEXT", [])?;
        }
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority);
             CREATE INDEX IF NOT EXISTS idx_tasks_deleted_at ON tasks(deleted_at);",
        )?;
        Ok(())
    }

    pub fn list_tasks(&self) -> AppResult<Vec<Task>> {
        let retention_days = AppSettings::load().unwrap_or_default().task_retention_days;
        self.list_tasks_with_retention(retention_days)
    }

    fn list_tasks_with_retention(&self, retention_days: u64) -> AppResult<Vec<Task>> {
        let connection = self.connect()?;
        let expired_ids = purge_expired_tasks(&connection, retention_days)?;
        for id in expired_ids {
            let directory = self.media_root().join(id);
            if directory.exists() {
                fs::remove_dir_all(directory)?;
            }
        }
        let mut statement = connection.prepare(
            "SELECT id, title, notes, status, source_type, source_url, source_label,
                    source_author, source_excerpt, due_at, remind_at, screenshot_data_url,
                    created_at, updated_at, completed_at, screenshots_json, audio_attachments_json,
                    links_json, file_attachments_json, priority, deleted_at
             FROM tasks ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'mid' THEN 1 ELSE 2 END,
                    created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], task_from_row)?;
        let mut tasks = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        let mut migrated = false;
        for task in &mut tasks {
            if self.persist_media(task)? {
                self.replace(task)?;
                migrated = true;
            }
        }
        if migrated {
            self.connect()?.execute_batch("VACUUM")?;
        }
        Ok(tasks)
    }

    pub fn get_task(&self, id: &str) -> AppResult<Task> {
        self.connect()?
            .query_row(
                "SELECT id, title, notes, status, source_type, source_url, source_label,
                    source_author, source_excerpt, due_at, remind_at, screenshot_data_url,
                    created_at, updated_at, completed_at, screenshots_json, audio_attachments_json,
                    links_json, file_attachments_json, priority, deleted_at
                    FROM tasks WHERE id = ?1",
                [id],
                task_from_row,
            )
            .map_err(Into::into)
    }

    pub fn create_task(&self, input: TaskInput) -> AppResult<Task> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(AppError::InvalidInput("A task title is required".into()));
        }
        validate_status(&input.status)?;
        validate_priority(&input.priority)?;
        let now = Utc::now().to_rfc3339();
        let mut screenshots = input.screenshots;
        if let Some(screenshot) = input.screenshot_data_url {
            if !screenshots.contains(&screenshot) {
                screenshots.push(screenshot);
            }
        }
        let mut task = Task {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            notes: input.notes,
            status: input.status,
            priority: input.priority,
            source_type: input.source_type,
            source_url: input.source_url,
            source_label: input.source_label,
            source_author: input.source_author,
            source_excerpt: input.source_excerpt,
            due_at: input.due_at,
            remind_at: input.remind_at,
            screenshots,
            audio_attachments: input.audio_attachments,
            links: input.links,
            file_attachments: input.file_attachments,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
            deleted_at: None,
        };
        self.persist_media(&mut task)?;
        self.insert(&task)?;
        Ok(task)
    }

    pub fn update_task(&self, patch: Value) -> AppResult<Task> {
        let object = patch
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("The task update must be an object".into()))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("A task ID is required".into()))?;
        let mut task = self.get_task(id)?;
        set_string(object, "title", &mut task.title);
        set_string(object, "notes", &mut task.notes);
        set_string(object, "status", &mut task.status);
        set_string(object, "priority", &mut task.priority);
        set_string(object, "sourceType", &mut task.source_type);
        set_nullable(object, "sourceUrl", &mut task.source_url);
        set_nullable(object, "sourceLabel", &mut task.source_label);
        set_nullable(object, "sourceAuthor", &mut task.source_author);
        set_nullable(object, "sourceExcerpt", &mut task.source_excerpt);
        set_nullable(object, "dueAt", &mut task.due_at);
        set_nullable(object, "remindAt", &mut task.remind_at);
        set_json_array(object, "screenshots", &mut task.screenshots)?;
        set_json_array(object, "audioAttachments", &mut task.audio_attachments)?;
        set_json_array(object, "links", &mut task.links)?;
        set_json_array(object, "fileAttachments", &mut task.file_attachments)?;
        if task.title.trim().is_empty() {
            return Err(AppError::InvalidInput("A task title is required".into()));
        }
        validate_status(&task.status)?;
        validate_priority(&task.priority)?;
        task.updated_at = Utc::now().to_rfc3339();
        task.completed_at = if task.status == "done" {
            task.completed_at.or_else(|| Some(task.updated_at.clone()))
        } else {
            None
        };
        self.persist_media(&mut task)?;
        self.replace(&task)?;
        self.remove_orphaned_media(&task)?;
        Ok(task)
    }

    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE tasks SET deleted_at = ?2, due_at = NULL, remind_at = NULL,
                last_notified_for = NULL, updated_at = ?2 WHERE id = ?1",
            [id, &now],
        )?;
        Ok(())
    }

    pub fn restore_task(&self, id: &str) -> AppResult<Task> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE tasks SET deleted_at = NULL, status = 'inbox', completed_at = NULL,
                updated_at = ?2 WHERE id = ?1",
            [id, &now],
        )?;
        self.get_task(id)
    }

    pub fn delete_tasks(&self, ids: &[String], permanently: bool) -> AppResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        if permanently {
            let mut statement = transaction.prepare("DELETE FROM tasks WHERE id = ?1")?;
            for id in ids {
                statement.execute([id])?;
            }
        } else {
            let now = Utc::now().to_rfc3339();
            let mut statement = transaction.prepare(
                "UPDATE tasks SET deleted_at = ?2, due_at = NULL, remind_at = NULL,
                    last_notified_for = NULL, updated_at = ?2 WHERE id = ?1",
            )?;
            for id in ids {
                statement.execute(params![id, now])?;
            }
        }
        transaction.commit()?;
        if permanently {
            for id in ids {
                let directory = self.media_root().join(id);
                if directory.exists() {
                    fs::remove_dir_all(directory)?;
                }
            }
        }
        Ok(())
    }

    pub fn due_notifications(&self, interval_minutes: u64) -> AppResult<Vec<Task>> {
        let now = Utc::now();
        let repeat_after = now - chrono::Duration::minutes(interval_minutes as i64);
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, title, notes, status, source_type, source_url, source_label,
                    source_author, source_excerpt, due_at, remind_at, screenshot_data_url,
                    created_at, updated_at, completed_at, screenshots_json, audio_attachments_json,
                    links_json, file_attachments_json, priority, deleted_at,
                    last_notified_for
             FROM tasks
             WHERE status != 'done' AND deleted_at IS NULL AND COALESCE(remind_at, due_at) IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((task_from_row(row)?, row.get::<_, Option<String>>(21)?))
        })?;
        let tasks = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(tasks
            .into_iter()
            .filter(|(task, last_notified_at)| {
                let is_overdue = task
                    .remind_at
                    .as_deref()
                    .or(task.due_at.as_deref())
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .is_some_and(|due| due <= now);
                let can_repeat = last_notified_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .is_none_or(|last| last <= repeat_after);
                is_overdue && can_repeat
            })
            .map(|(task, _)| task)
            .collect())
    }

    pub fn mark_notified(&self, id: &str) -> AppResult<()> {
        let notified_at = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE tasks SET last_notified_for = ?2 WHERE id = ?1",
            [id, &notified_at],
        )?;
        Ok(())
    }

    pub fn export(&self, path: &Path) -> AppResult<()> {
        let mut tasks = self.list_tasks()?;
        for task in &mut tasks {
            task.screenshots
                .iter_mut()
                .for_each(make_archive_media_path);
            task.audio_attachments
                .iter_mut()
                .for_each(|attachment| make_archive_media_path(&mut attachment.data_url));
            task.file_attachments
                .iter_mut()
                .for_each(|attachment| make_archive_media_path(&mut attachment.data_url));
        }
        let payload = serde_json::json!({
            "format": "threadbox-backup",
            "version": 4,
            "exportedAt": Utc::now().to_rfc3339(),
            "tasks": tasks,
        });
        let file = fs::File::create(path)?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("backup.json", options)?;
        std::io::Write::write_all(&mut archive, &serde_json::to_vec_pretty(&payload)?)?;
        let media_root = self.media_root();
        if media_root.exists() {
            append_directory_to_zip(&mut archive, &media_root, &media_root, options)?;
        }
        archive.finish()?;
        Ok(())
    }

    pub fn open_media(&self, path: &Path) -> AppResult<()> {
        let canonical = self.validated_media_path(path)?;
        open::that(canonical).map_err(AppError::Io)
    }

    pub fn read_media(&self, path: &Path) -> AppResult<Vec<u8>> {
        Ok(fs::read(self.validated_media_path(path)?)?)
    }

    fn validated_media_path(&self, path: &Path) -> AppResult<PathBuf> {
        let canonical = path.canonicalize()?;
        let root = self.media_root().canonicalize()?;
        if !canonical.starts_with(root) {
            return Err(AppError::InvalidInput(
                "Attachment path is outside Threadbox storage".into(),
            ));
        }
        Ok(canonical)
    }

    fn media_root(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("media")
    }

    fn persist_media(&self, task: &mut Task) -> AppResult<bool> {
        let directory = self.media_root().join(&task.id);
        let mut changed = false;
        for (index, screenshot) in task.screenshots.iter_mut().enumerate() {
            if screenshot.starts_with("data:") {
                fs::create_dir_all(&directory)?;
                let path =
                    directory.join(format!("screenshot-{}-{}.png", index + 1, Uuid::new_v4()));
                write_data_url(&path, screenshot)?;
                *screenshot = path.to_string_lossy().into_owned();
                changed = true;
            }
        }
        for attachment in &mut task.audio_attachments {
            if attachment.data_url.starts_with("data:") {
                fs::create_dir_all(&directory)?;
                let path = directory.join(format!("{}.wav", attachment.id));
                write_data_url(&path, &attachment.data_url)?;
                attachment.data_url = path.to_string_lossy().into_owned();
                changed = true;
            }
        }
        for attachment in &mut task.file_attachments {
            if attachment.data_url.starts_with("data:") {
                fs::create_dir_all(&directory)?;
                let extension = Path::new(&attachment.name)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("bin");
                let path = directory.join(format!("{}.{}", attachment.id, extension));
                write_data_url(&path, &attachment.data_url)?;
                attachment.data_url = path.to_string_lossy().into_owned();
                changed = true;
            }
        }
        Ok(changed)
    }

    fn remove_orphaned_media(&self, task: &Task) -> AppResult<()> {
        let directory = self.media_root().join(&task.id);
        if !directory.exists() {
            return Ok(());
        }
        let referenced = task
            .screenshots
            .iter()
            .cloned()
            .chain(
                task.audio_attachments
                    .iter()
                    .map(|item| item.data_url.clone()),
            )
            .chain(
                task.file_attachments
                    .iter()
                    .map(|item| item.data_url.clone()),
            )
            .collect::<std::collections::HashSet<_>>();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_file() && !referenced.contains(&path.to_string_lossy().into_owned()) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn insert(&self, task: &Task) -> AppResult<()> {
        let screenshots_json = serde_json::to_string(&task.screenshots)?;
        let audio_attachments_json = serde_json::to_string(&task.audio_attachments)?;
        let links_json = serde_json::to_string(&task.links)?;
        let file_attachments_json = serde_json::to_string(&task.file_attachments)?;
        self.connect()?.execute(
            "INSERT INTO tasks (id, title, notes, status, source_type, source_url, source_label,
                source_author, source_excerpt, due_at, remind_at, screenshot_data_url,
                created_at, updated_at, completed_at, screenshots_json, audio_attachments_json,
                links_json, file_attachments_json, priority, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                task.id,
                task.title,
                task.notes,
                task.status,
                task.source_type,
                task.source_url,
                task.source_label,
                task.source_author,
                task.source_excerpt,
                task.due_at,
                task.remind_at,
                task.created_at,
                task.updated_at,
                task.completed_at,
                screenshots_json,
                audio_attachments_json,
                links_json,
                file_attachments_json,
                task.priority,
                task.deleted_at
            ],
        )?;
        Ok(())
    }

    fn replace(&self, task: &Task) -> AppResult<()> {
        let screenshots_json = serde_json::to_string(&task.screenshots)?;
        let audio_attachments_json = serde_json::to_string(&task.audio_attachments)?;
        let links_json = serde_json::to_string(&task.links)?;
        let file_attachments_json = serde_json::to_string(&task.file_attachments)?;
        self.connect()?.execute(
            "UPDATE tasks SET last_notified_for=CASE WHEN due_at IS NOT ?10 OR remind_at IS NOT ?11 THEN NULL ELSE last_notified_for END,
                title=?2, notes=?3, status=?4, source_type=?5, source_url=?6,
                source_label=?7, source_author=?8, source_excerpt=?9, due_at=?10, remind_at=?11,
                created_at=?12, updated_at=?13, completed_at=?14, screenshots_json=?15,
                audio_attachments_json=?16, links_json=?17, file_attachments_json=?18,
                priority=?19, deleted_at=?20 WHERE id=?1",
            params![
                task.id, task.title, task.notes, task.status, task.source_type, task.source_url,
                task.source_label, task.source_author, task.source_excerpt, task.due_at,
                task.remind_at, task.created_at, task.updated_at, task.completed_at,
                screenshots_json, audio_attachments_json, links_json, file_attachments_json,
                task.priority, task.deleted_at
            ],
        )?;
        Ok(())
    }
}

fn write_data_url(path: &Path, data_url: &str) -> AppResult<()> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, value)| value)
        .ok_or_else(|| AppError::InvalidInput("Invalid attachment data".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| AppError::InvalidInput(format!("Invalid attachment data: {error}")))?;
    fs::write(path, bytes)?;
    Ok(())
}

fn make_archive_media_path(path: &mut String) {
    let source = Path::new(path);
    if let (Some(task_id), Some(file_name)) = (
        source.parent().and_then(Path::file_name),
        source.file_name(),
    ) {
        *path = Path::new("media")
            .join(task_id)
            .join(file_name)
            .to_string_lossy()
            .into_owned();
    }
}

fn append_directory_to_zip(
    archive: &mut ZipWriter<fs::File>,
    root: &Path,
    directory: &Path,
    options: SimpleFileOptions,
) -> AppResult<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            append_directory_to_zip(archive, root, &path, options)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| AppError::InvalidInput(error.to_string()))?;
            archive.start_file(format!("media/{}", relative.to_string_lossy()), options)?;
            let mut source = fs::File::open(path)?;
            std::io::copy(&mut source, archive)?;
        }
    }
    Ok(())
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let legacy_screenshot = row.get::<_, Option<String>>(11)?;
    let mut screenshots: Vec<String> = parse_json_column(row, 15);
    if let Some(screenshot) = legacy_screenshot {
        if !screenshots.contains(&screenshot) {
            screenshots.insert(0, screenshot);
        }
    }
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        notes: row.get(2)?,
        status: row.get(3)?,
        priority: row.get(19)?,
        source_type: row.get(4)?,
        source_url: row.get(5)?,
        source_label: row.get(6)?,
        source_author: row.get(7)?,
        source_excerpt: row.get(8)?,
        due_at: row.get(9)?,
        remind_at: row.get(10)?,
        screenshots,
        audio_attachments: parse_json_column(row, 16),
        links: parse_json_column(row, 17),
        file_attachments: parse_json_column(row, 18),
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        completed_at: row.get(14)?,
        deleted_at: row.get(20)?,
    })
}

fn parse_json_column<T: DeserializeOwned + Default>(row: &Row<'_>, index: usize) -> T {
    row.get::<_, String>(index)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn set_json_array<T: DeserializeOwned>(
    object: &serde_json::Map<String, Value>,
    key: &str,
    target: &mut Vec<T>,
) -> AppResult<()> {
    if let Some(value) = object.get(key) {
        *target = serde_json::from_value(value.clone())
            .map_err(|error| AppError::InvalidInput(format!("Invalid {key} value: {error}")))?;
    }
    Ok(())
}

fn set_string(object: &serde_json::Map<String, Value>, key: &str, target: &mut String) {
    if let Some(value) = object.get(key).and_then(Value::as_str) {
        *target = value.into();
    }
}

fn set_nullable(object: &serde_json::Map<String, Value>, key: &str, target: &mut Option<String>) {
    if let Some(value) = object.get(key) {
        *target = value.as_str().map(Into::into);
    }
}

fn validate_status(status: &str) -> AppResult<()> {
    if ["inbox", "todo", "waiting", "done"].contains(&status) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Unknown task status: {status}"
        )))
    }
}

fn validate_priority(priority: &str) -> AppResult<()> {
    if ["high", "mid", "low"].contains(&priority) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Unknown task priority: {priority}"
        )))
    }
}

fn purge_expired_tasks(connection: &Connection, retention_days: u64) -> AppResult<Vec<String>> {
    let cutoff = (Utc::now() - chrono::Duration::days(retention_days as i64)).to_rfc3339();
    let mut statement = connection.prepare(
        "SELECT id FROM tasks WHERE (deleted_at IS NOT NULL AND deleted_at < ?1)
            OR (deleted_at IS NULL AND completed_at IS NOT NULL AND completed_at < ?1)",
    )?;
    let expired_ids = statement
        .query_map([&cutoff], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    drop(statement);
    connection.execute(
        "DELETE FROM tasks WHERE (deleted_at IS NOT NULL AND deleted_at < ?1)
            OR (deleted_at IS NULL AND completed_at IS NOT NULL AND completed_at < ?1)",
        [&cutoff],
    )?;
    Ok(expired_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DATABASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn test_database() -> Database {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let database = Database {
            path: std::env::temp_dir().join(format!(
                "threadbox-{}-{suffix}-{sequence}.sqlite3",
                std::process::id()
            )),
        };
        database.migrate().unwrap();
        database
    }

    #[test]
    fn creates_and_updates_a_task() {
        let database = test_database();
        let attachment = AudioAttachment {
            id: "recording-1".into(),
            data_url: "data:audio/wav;base64,UklGRg==".into(),
            duration_seconds: 1.5,
            created_at: Utc::now().to_rfc3339(),
        };
        let created = database
            .create_task(TaskInput {
                title: "Follow up".into(),
                notes: "".into(),
                status: "inbox".into(),
                priority: "mid".into(),
                source_type: "manual".into(),
                source_url: None,
                source_label: None,
                source_author: None,
                source_excerpt: None,
                due_at: None,
                remind_at: None,
                screenshot_data_url: None,
                screenshots: vec!["data:image/png;base64,cG5n".into()],
                audio_attachments: vec![attachment],
                links: vec!["https://example.com".into()],
                file_attachments: Vec::new(),
            })
            .unwrap();
        let updated = database
            .update_task(serde_json::json!({ "id": created.id, "status": "done" }))
            .unwrap();
        assert_eq!(updated.status, "done");
        assert!(updated.completed_at.is_some());
        assert_eq!(updated.screenshots.len(), 1);
        assert!(!updated.screenshots[0].starts_with("data:"));
        assert!(Path::new(&updated.screenshots[0]).is_file());
        assert_eq!(updated.audio_attachments.len(), 1);
        assert_eq!(updated.audio_attachments[0].id, "recording-1");
        assert!(!updated.audio_attachments[0].data_url.starts_with("data:"));
        assert!(Path::new(&updated.audio_attachments[0].data_url).is_file());
        assert_eq!(updated.links, vec!["https://example.com"]);
    }

    #[test]
    fn exports_media_files_and_portable_metadata_in_a_zip() {
        let database = test_database();
        let task = database
            .create_task(TaskInput {
                title: "Archive media".into(),
                notes: String::new(),
                status: "inbox".into(),
                priority: "mid".into(),
                source_type: "manual".into(),
                source_url: None,
                source_label: None,
                source_author: None,
                source_excerpt: None,
                due_at: None,
                remind_at: None,
                screenshot_data_url: None,
                screenshots: vec!["data:image/png;base64,cG5n".into()],
                audio_attachments: Vec::new(),
                links: Vec::new(),
                file_attachments: Vec::new(),
            })
            .unwrap();
        let backup_path = database.path.with_extension("zip");
        database.export(&backup_path).unwrap();

        let file = fs::File::open(&backup_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let backup: serde_json::Value =
            serde_json::from_reader(archive.by_name("backup.json").unwrap()).unwrap();
        let stored_path = backup["tasks"][0]["screenshots"][0].as_str().unwrap();
        assert!(stored_path.starts_with(&format!("media/{}/", task.id)));
        assert!(archive.by_name(stored_path).is_ok());
    }

    #[test]
    fn repeats_overdue_notifications_only_after_the_configured_interval() {
        let database = test_database();
        let created = database
            .create_task(TaskInput {
                title: "Overdue follow-up".into(),
                notes: "".into(),
                status: "todo".into(),
                priority: "high".into(),
                source_type: "manual".into(),
                source_url: None,
                source_label: None,
                source_author: None,
                source_excerpt: None,
                due_at: Some((Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
                remind_at: None,
                screenshot_data_url: None,
                screenshots: Vec::new(),
                audio_attachments: Vec::new(),
                links: Vec::new(),
                file_attachments: Vec::new(),
            })
            .unwrap();

        assert_eq!(database.due_notifications(15).unwrap().len(), 1);
        database.mark_notified(&created.id).unwrap();
        assert!(database.due_notifications(15).unwrap().is_empty());
    }

    #[test]
    fn soft_deletes_and_restores_a_task() {
        let database = test_database();
        let created = database
            .create_task(TaskInput {
                title: "Recoverable task".into(),
                notes: "".into(),
                status: "inbox".into(),
                priority: "low".into(),
                source_type: "manual".into(),
                source_url: None,
                source_label: None,
                source_author: None,
                source_excerpt: None,
                due_at: Some(Utc::now().to_rfc3339()),
                remind_at: Some(Utc::now().to_rfc3339()),
                screenshot_data_url: None,
                screenshots: Vec::new(),
                audio_attachments: Vec::new(),
                links: Vec::new(),
                file_attachments: Vec::new(),
            })
            .unwrap();

        database.delete_task(&created.id).unwrap();
        let deleted = database.get_task(&created.id).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(deleted.due_at.is_none());
        assert!(deleted.remind_at.is_none());

        let restored = database.restore_task(&created.id).unwrap();
        assert!(restored.deleted_at.is_none());
        assert_eq!(restored.status, "inbox");
        assert_eq!(restored.priority, "low");
    }

    #[test]
    fn purges_tasks_after_seven_days() {
        let database = test_database();
        let created = database
            .create_task(TaskInput {
                title: "Expired task".into(),
                notes: "".into(),
                status: "inbox".into(),
                priority: "mid".into(),
                source_type: "manual".into(),
                source_url: None,
                source_label: None,
                source_author: None,
                source_excerpt: None,
                due_at: None,
                remind_at: None,
                screenshot_data_url: None,
                screenshots: vec!["data:image/png;base64,aGVsbG8=".into()],
                audio_attachments: Vec::new(),
                links: Vec::new(),
                file_attachments: Vec::new(),
            })
            .unwrap();
        let media_directory = database.media_root().join(&created.id);
        assert!(media_directory.exists());
        database.delete_task(&created.id).unwrap();
        let expired_at = (Utc::now() - chrono::Duration::days(8)).to_rfc3339();
        database
            .connect()
            .unwrap()
            .execute(
                "UPDATE tasks SET deleted_at=?2 WHERE id=?1",
                params![created.id, expired_at],
            )
            .unwrap();

        assert_eq!(database.list_tasks_with_retention(30).unwrap().len(), 1);
        assert!(media_directory.exists());
        assert!(database.list_tasks_with_retention(7).unwrap().is_empty());
        assert!(database.get_task(&created.id).is_err());
        assert!(!media_directory.exists());
    }
}
