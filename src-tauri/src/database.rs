use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

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
    media,
    settings::AppSettings,
};

#[derive(Clone)]
pub struct Database {
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub status: String,
    pub priority: String,
    pub project_id: Option<String>,
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
    #[serde(default)]
    pub project_id: Option<String>,
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

    pub(crate) fn connect(&self) -> AppResult<Connection> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(connection)
    }

    pub(crate) fn migrate(&self) -> AppResult<()> {
        let connection = self.connect()?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            Self::migrate_to_v1(&connection)?;
            connection.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            Self::migrate_to_v2(&connection)?;
            connection.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            self.migrate_to_v3(&connection)?;
            connection.pragma_update(None, "user_version", 3)?;
        }
        if version < 4 {
            crate::workspace::migrate_language_columns(&connection)?;
            connection.pragma_update(None, "user_version", 4)?;
        }
        Ok(())
    }

    /// Schema as it shipped before migrations were versioned. Existing installations report version
    /// zero while already carrying this schema, so every statement here stays idempotent.
    fn migrate_to_v1(connection: &Connection) -> AppResult<()> {
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

    /// Organisations, projects and people, and the project a task belongs to.
    fn migrate_to_v2(connection: &Connection) -> AppResult<()> {
        crate::workspace::migrate_schema(connection)?;
        let columns = connection
            .prepare("PRAGMA table_info(tasks)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == "project_id") {
            connection.execute("ALTER TABLE tasks ADD COLUMN project_id TEXT", [])?;
        }
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);",
        )?;
        Ok(())
    }

    /// Project documents, and the move to content-addressed media referenced relative to the media
    /// root.
    ///
    /// Installations before this version recorded absolute paths such as
    /// `/home/…/media/<task id>/screenshot-1-<uuid>.png`, which break as soon as the data directory
    /// moves. Each such file is hashed into the blob store and the row is rewritten to the relative
    /// reference. A path whose file is already gone is dropped rather than kept as a broken
    /// reference, and the emptied per-task directories are removed.
    fn migrate_to_v3(&self, connection: &Connection) -> AppResult<()> {
        crate::documents::migrate_schema(connection)?;
        let root = self.media_root();
        let mut rows = Vec::new();
        {
            let mut statement = connection.prepare(
                "SELECT id, screenshots_json, audio_attachments_json, file_attachments_json,
                        screenshot_data_url
                 FROM tasks",
            )?;
            let mapped = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            for entry in mapped {
                rows.push(entry?);
            }
        }
        for (id, screenshots_json, audio_json, files_json, legacy_screenshot) in rows {
            let mut screenshots: Vec<String> =
                serde_json::from_str(&screenshots_json).unwrap_or_default();
            if let Some(legacy) = legacy_screenshot {
                if !legacy.is_empty() && !screenshots.contains(&legacy) {
                    screenshots.insert(0, legacy);
                }
            }
            let mut audio: Vec<AudioAttachment> =
                serde_json::from_str(&audio_json).unwrap_or_default();
            let mut files: Vec<FileAttachment> =
                serde_json::from_str(&files_json).unwrap_or_default();
            screenshots.retain_mut(|value| adopt_stored_file(&root, value, ".png"));
            audio.retain_mut(|attachment| {
                adopt_stored_file(&root, &mut attachment.data_url, ".wav")
            });
            files.retain_mut(|attachment| {
                let extension = media::extension_of(&attachment.name, ".bin");
                adopt_stored_file(&root, &mut attachment.data_url, &extension)
            });
            connection.execute(
                "UPDATE tasks SET screenshots_json = ?2, audio_attachments_json = ?3,
                    file_attachments_json = ?4, screenshot_data_url = NULL WHERE id = ?1",
                params![
                    id,
                    serde_json::to_string(&screenshots)?,
                    serde_json::to_string(&audio)?,
                    serde_json::to_string(&files)?
                ],
            )?;
        }
        remove_legacy_media_directories(&root)?;
        Ok(())
    }

    pub fn list_tasks(&self) -> AppResult<Vec<Task>> {
        let retention_days = AppSettings::load().unwrap_or_default().task_retention_days;
        self.list_tasks_with_retention(retention_days)
    }

    fn list_tasks_with_retention(&self, retention_days: u64) -> AppResult<Vec<Task>> {
        let connection = self.connect()?;
        let expired_ids = purge_expired_tasks(&connection, retention_days)?;
        let purged = !expired_ids.is_empty();
        let mut statement = connection.prepare(
            "SELECT id, title, notes, status, source_type, source_url, source_label,
                    source_author, source_excerpt, due_at, remind_at, screenshot_data_url,
                    created_at, updated_at, completed_at, screenshots_json, audio_attachments_json,
                    links_json, file_attachments_json, priority, deleted_at, project_id
             FROM tasks ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'mid' THEN 1 ELSE 2 END,
                    created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], task_from_row)?;
        let mut tasks = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        if purged {
            self.collect_media_garbage()?;
        }
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
                    links_json, file_attachments_json, priority, deleted_at, project_id
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
            project_id: input.project_id,
            screenshots,
            audio_attachments: input.audio_attachments,
            links: input.links,
            file_attachments: input.file_attachments,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
            deleted_at: None,
        };
        self.ensure_project_exists(task.project_id.as_deref())?;
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
        let previous_media = task_media_references(&task);
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
        set_nullable(object, "projectId", &mut task.project_id);
        set_json_array(object, "screenshots", &mut task.screenshots)?;
        set_json_array(object, "audioAttachments", &mut task.audio_attachments)?;
        set_json_array(object, "links", &mut task.links)?;
        set_json_array(object, "fileAttachments", &mut task.file_attachments)?;
        if task.title.trim().is_empty() {
            return Err(AppError::InvalidInput("A task title is required".into()));
        }
        validate_status(&task.status)?;
        validate_priority(&task.priority)?;
        self.ensure_project_exists(task.project_id.as_deref())?;
        task.updated_at = Utc::now().to_rfc3339();
        task.completed_at = if task.status == "done" {
            task.completed_at.or_else(|| Some(task.updated_at.clone()))
        } else {
            None
        };
        self.persist_media(&mut task)?;
        self.replace(&task)?;
        let current_media = task_media_references(&task);
        if previous_media
            .iter()
            .any(|value| !current_media.contains(value))
        {
            self.collect_media_garbage()?;
        }
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
            self.collect_media_garbage()?;
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
                    links_json, file_attachments_json, priority, deleted_at, project_id,
                    last_notified_for
             FROM tasks
             WHERE status != 'done' AND deleted_at IS NULL AND COALESCE(remind_at, due_at) IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((task_from_row(row)?, row.get::<_, Option<String>>(22)?))
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

    pub fn open_media(&self, reference: &str) -> AppResult<()> {
        open::that(media::resolve(&self.media_root(), reference)?).map_err(AppError::Io)
    }

    pub fn read_media(&self, reference: &str) -> AppResult<Vec<u8>> {
        Ok(fs::read(media::resolve(&self.media_root(), reference)?)?)
    }

    /// The absolute directory holding stored media, which the interface needs to display a file
    /// that the database refers to relatively.
    pub fn media_root(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("media")
    }

    /// Moves every inline `data:` URL on the task into the blob store, leaving the task holding
    /// references. Returns whether anything was rewritten.
    fn persist_media(&self, task: &mut Task) -> AppResult<bool> {
        let root = self.media_root();
        let mut changed = false;
        for screenshot in task.screenshots.iter_mut() {
            changed |= self.store_data_url(&root, screenshot, ".png")?;
        }
        for attachment in &mut task.audio_attachments {
            changed |= self.store_data_url(&root, &mut attachment.data_url, ".wav")?;
        }
        for attachment in &mut task.file_attachments {
            let extension = media::extension_of(&attachment.name, ".bin");
            changed |= self.store_data_url(&root, &mut attachment.data_url, &extension)?;
        }
        Ok(changed)
    }

    fn store_data_url(&self, root: &Path, value: &mut String, extension: &str) -> AppResult<bool> {
        if !media::is_data_url(value) {
            return Ok(false);
        }
        let bytes = media::decode_data_url(value)?;
        *value = media::store_bytes(root, &bytes, extension)?;
        Ok(true)
    }

    /// Deletes every stored file that no row refers to any more.
    ///
    /// Blobs are shared between rows, so a file can only be freed by looking at the whole database.
    /// Soft-deleted rows still count as references: only a permanent delete releases the bytes.
    pub(crate) fn collect_media_garbage(&self) -> AppResult<usize> {
        let referenced = self.media_references()?;
        media::collect_garbage(&self.media_root(), &referenced)
    }

    fn media_references(&self) -> AppResult<HashSet<String>> {
        let connection = self.connect()?;
        let mut referenced = HashSet::new();
        let mut statement = connection.prepare(
            "SELECT screenshots_json, audio_attachments_json, file_attachments_json FROM tasks",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (screenshots_json, audio_json, files_json) = row?;
            referenced
                .extend(serde_json::from_str::<Vec<String>>(&screenshots_json).unwrap_or_default());
            referenced.extend(
                serde_json::from_str::<Vec<AudioAttachment>>(&audio_json)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|attachment| attachment.data_url),
            );
            referenced.extend(
                serde_json::from_str::<Vec<FileAttachment>>(&files_json)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|attachment| attachment.data_url),
            );
        }
        drop(statement);
        let mut statement = connection
            .prepare("SELECT media_path FROM project_documents WHERE media_path IS NOT NULL")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            referenced.insert(row?);
        }
        referenced.retain(|value| !media::is_data_url(value));
        Ok(referenced)
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
                links_json, file_attachments_json, priority, deleted_at, project_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
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
                task.deleted_at,
                task.project_id
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
                priority=?19, deleted_at=?20, project_id=?21 WHERE id=?1",
            params![
                task.id, task.title, task.notes, task.status, task.source_type, task.source_url,
                task.source_label, task.source_author, task.source_excerpt, task.due_at,
                task.remind_at, task.created_at, task.updated_at, task.completed_at,
                screenshots_json, audio_attachments_json, links_json, file_attachments_json,
                task.priority, task.deleted_at, task.project_id
            ],
        )?;
        Ok(())
    }
}

/// Rewrites an absolute pre-v3 media path into a reference, moving the file into the blob store.
/// Returns false when the file has already disappeared, so the caller can drop the dead reference.
fn adopt_stored_file(root: &Path, value: &mut String, extension: &str) -> bool {
    if media::is_data_url(value) {
        return true;
    }
    let path = Path::new(value.as_str());
    if path.is_relative() {
        return true;
    }
    match media::store_file(root, path, extension) {
        Ok(reference) => {
            *value = reference;
            true
        }
        Err(error) => {
            eprintln!("Dropping a media reference that no longer resolves: {value} ({error})");
            false
        }
    }
}

/// Removes the per-task directories that held media before it became content-addressed.
fn remove_legacy_media_directories(root: &Path) -> AppResult<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir()
            && path
                .file_name()
                .is_some_and(|name| name != media::BLOB_DIRECTORY)
        {
            let _ = fs::remove_dir_all(path);
        }
    }
    Ok(())
}

/// Media in a backup archive sits under `media/`, mirroring the media root, so a stored reference
/// only needs that prefix.
fn make_archive_media_path(path: &mut String) {
    if !media::is_data_url(path) && !path.starts_with("media/") {
        *path = format!("media/{path}");
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
    // Column 11, `screenshot_data_url`, held a single screenshot before the list existed. The v3
    // migration folded it into `screenshots_json` and cleared it, and nothing writes it again.
    let screenshots: Vec<String> = parse_json_column(row, 15);
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
        project_id: row.get(21)?,
    })
}

fn task_media_references(task: &Task) -> HashSet<String> {
    task.screenshots
        .iter()
        .cloned()
        .chain(
            task.audio_attachments
                .iter()
                .map(|attachment| attachment.data_url.clone()),
        )
        .chain(
            task.file_attachments
                .iter()
                .map(|attachment| attachment.data_url.clone()),
        )
        .filter(|value| !media::is_data_url(value))
        .collect()
}

fn parse_json_column<T: DeserializeOwned + Default>(row: &Row<'_>, index: usize) -> T {
    row.get::<_, String>(index)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub(crate) fn set_json_array<T: DeserializeOwned>(
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

pub(crate) fn set_string(object: &serde_json::Map<String, Value>, key: &str, target: &mut String) {
    if let Some(value) = object.get(key).and_then(Value::as_str) {
        *target = value.into();
    }
}

pub(crate) fn set_nullable(
    object: &serde_json::Map<String, Value>,
    key: &str,
    target: &mut Option<String>,
) {
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

    /// A database in a directory of its own. The media root sits beside the database file and its
    /// blobs are shared between rows, so two tests sharing one root would collect each other's
    /// files as garbage.
    fn test_directory() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "threadbox-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn test_database() -> Database {
        let database = Database {
            path: test_directory().join("threadbox.sqlite3"),
        };
        database.migrate().unwrap();
        database
    }

    fn task_input(title: &str) -> TaskInput {
        TaskInput {
            title: title.into(),
            notes: String::new(),
            status: "inbox".into(),
            priority: "mid".into(),
            project_id: None,
            source_type: "manual".into(),
            source_url: None,
            source_label: None,
            source_author: None,
            source_excerpt: None,
            due_at: None,
            remind_at: None,
            screenshot_data_url: None,
            screenshots: Vec::new(),
            audio_attachments: Vec::new(),
            links: Vec::new(),
            file_attachments: Vec::new(),
        }
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
                project_id: None,
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
        assert!(Path::new(&updated.screenshots[0]).is_relative());
        assert!(updated.screenshots[0].starts_with("blobs/"));
        assert!(
            media::resolve(&database.media_root(), &updated.screenshots[0])
                .unwrap()
                .is_file()
        );
        assert_eq!(updated.audio_attachments.len(), 1);
        assert_eq!(updated.audio_attachments[0].id, "recording-1");
        assert!(Path::new(&updated.audio_attachments[0].data_url).is_relative());
        assert!(media::resolve(
            &database.media_root(),
            &updated.audio_attachments[0].data_url
        )
        .unwrap()
        .is_file());
        assert_eq!(updated.links, vec!["https://example.com"]);
    }

    #[test]
    fn exports_media_files_and_portable_metadata_in_a_zip() {
        let database = test_database();
        let _task = database
            .create_task(TaskInput {
                title: "Archive media".into(),
                notes: String::new(),
                status: "inbox".into(),
                priority: "mid".into(),
                project_id: None,
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
        assert!(stored_path.starts_with("media/blobs/"));
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
                project_id: None,
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
                project_id: None,
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
                project_id: None,
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
        let stored = media::resolve(&database.media_root(), &created.screenshots[0]).unwrap();
        assert!(stored.is_file());
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
        assert!(stored.is_file());
        assert!(database.list_tasks_with_retention(7).unwrap().is_empty());
        assert!(database.get_task(&created.id).is_err());
        assert!(!stored.is_file());
    }

    #[test]
    fn stores_identical_attachments_once_and_frees_them_with_the_last_task() {
        let database = test_database();
        let screenshot = "data:image/png;base64,c2hhcmVk";
        let mut first_input = task_input("First");
        first_input.screenshots = vec![screenshot.into()];
        let mut second_input = task_input("Second");
        second_input.screenshots = vec![screenshot.into()];
        let first = database.create_task(first_input).unwrap();
        let second = database.create_task(second_input).unwrap();
        assert_eq!(first.screenshots, second.screenshots);
        let stored = media::resolve(&database.media_root(), &first.screenshots[0]).unwrap();
        assert!(stored.is_file());

        database
            .delete_tasks(std::slice::from_ref(&first.id), true)
            .unwrap();
        assert!(
            stored.is_file(),
            "the second task still refers to those bytes"
        );
        database.delete_tasks(&[second.id], true).unwrap();
        assert!(!stored.is_file());
    }

    #[test]
    fn frees_an_attachment_removed_from_a_task() {
        let database = test_database();
        let mut input = task_input("Has a screenshot");
        input.screenshots = vec!["data:image/png;base64,cmVtb3ZlZA==".into()];
        let created = database.create_task(input).unwrap();
        let stored = media::resolve(&database.media_root(), &created.screenshots[0]).unwrap();
        assert!(stored.is_file());
        database
            .update_task(serde_json::json!({ "id": created.id, "screenshots": [] }))
            .unwrap();
        assert!(!stored.is_file());
    }

    #[test]
    fn rewrites_absolute_media_paths_and_the_legacy_screenshot_column() {
        let directory = test_directory();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        {
            let connection = database.connect().unwrap();
            Database::migrate_to_v1(&connection).unwrap();
            Database::migrate_to_v2(&connection).unwrap();
            connection.pragma_update(None, "user_version", 2).unwrap();
            let task_media = database.media_root().join("task-1");
            fs::create_dir_all(&task_media).unwrap();
            let screenshot = task_media.join("screenshot-1-old.png");
            let recording = task_media.join("recording-1.wav");
            fs::write(&screenshot, b"an old screenshot").unwrap();
            fs::write(&recording, b"an old recording").unwrap();
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "INSERT INTO tasks (id, title, notes, status, source_type, created_at,
                        updated_at, screenshots_json, audio_attachments_json, priority,
                        screenshot_data_url)
                     VALUES ('task-1', 'Old task', '', 'inbox', 'manual', ?1, ?1, '[]', ?2, 'mid', ?3)",
                    params![
                        now,
                        serde_json::json!([{
                            "id": "recording-1",
                            "dataUrl": recording.to_string_lossy(),
                            "durationSeconds": 2.0,
                            "createdAt": now,
                        }])
                        .to_string(),
                        screenshot.to_string_lossy()
                    ],
                )
                .unwrap();
        }

        database.migrate().unwrap();

        let task = database.get_task("task-1").unwrap();
        assert_eq!(task.screenshots.len(), 1);
        assert!(task.screenshots[0].starts_with("blobs/"));
        assert_eq!(
            fs::read(media::resolve(&database.media_root(), &task.screenshots[0]).unwrap())
                .unwrap(),
            b"an old screenshot"
        );
        assert!(task.audio_attachments[0].data_url.starts_with("blobs/"));
        assert!(!database.media_root().join("task-1").exists());

        // Reading twice must not resurrect the legacy column as a second screenshot.
        let listed = database.list_tasks_with_retention(3650).unwrap();
        assert_eq!(listed[0].screenshots.len(), 1);
        let listed = database.list_tasks_with_retention(3650).unwrap();
        assert_eq!(listed[0].screenshots.len(), 1);
    }

    #[test]
    fn drops_an_absolute_path_whose_file_is_gone() {
        let directory = test_directory();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        {
            let connection = database.connect().unwrap();
            Database::migrate_to_v1(&connection).unwrap();
            Database::migrate_to_v2(&connection).unwrap();
            connection.pragma_update(None, "user_version", 2).unwrap();
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "INSERT INTO tasks (id, title, notes, status, source_type, created_at,
                        updated_at, screenshots_json, priority)
                     VALUES ('task-2', 'Broken media', '', 'inbox', 'manual', ?1, ?1, ?2, 'mid')",
                    params![
                        now,
                        serde_json::json!([directory
                            .join("media/task-2/vanished.png")
                            .to_string_lossy()])
                        .to_string()
                    ],
                )
                .unwrap();
        }

        database.migrate().unwrap();

        assert!(database.get_task("task-2").unwrap().screenshots.is_empty());
    }
}
