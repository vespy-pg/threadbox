//! Project meetings and their original two-channel recordings.
//!
//! The recording is one stereo WAV: the user's microphone is the left channel and the system
//! monitor is the right channel. Keeping both tracks in one content-addressed file makes later
//! per-channel transcription deterministic while a project reassignment only changes the meeting
//! row. The media never has to be copied or renamed.

use chrono::Utc;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    database::{set_nullable, set_string, Database},
    error::{AppError, AppResult},
    media,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub status: String,
    pub scheduled_start: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub recording_path: Option<String>,
    pub duration_seconds: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInput {
    #[serde(default)]
    pub project_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub scheduled_start: Option<String>,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            project_id TEXT REFERENCES projects(id),
            title TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('planned', 'recording', 'recorded')),
            scheduled_start TEXT,
            started_at TEXT,
            ended_at TEXT,
            recording_path TEXT,
            duration_seconds REAL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_meetings_project
            ON meetings(project_id, scheduled_start, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_meetings_status ON meetings(status);
        CREATE TABLE IF NOT EXISTS meeting_project_history (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            from_project_id TEXT,
            to_project_id TEXT,
            changed_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_meeting_project_history_meeting
            ON meeting_project_history(meeting_id, changed_at DESC);",
    )?;
    Ok(())
}

impl Database {
    pub fn create_meeting(&self, input: MeetingInput) -> AppResult<Meeting> {
        self.ensure_project_exists(input.project_id.as_deref())?;
        let title = required_title(&input.title)?;
        let now = Utc::now().to_rfc3339();
        let meeting = Meeting {
            id: Uuid::new_v4().to_string(),
            project_id: input.project_id,
            title,
            status: "planned".into(),
            scheduled_start: input.scheduled_start,
            started_at: None,
            ended_at: None,
            recording_path: None,
            duration_seconds: None,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.connect()?.execute(
            "INSERT INTO meetings (id, project_id, title, status, scheduled_start, started_at,
                ended_at, recording_path, duration_seconds, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, NULL, ?6, ?7, NULL)",
            params![
                meeting.id,
                meeting.project_id,
                meeting.title,
                meeting.status,
                meeting.scheduled_start,
                meeting.created_at,
                meeting.updated_at
            ],
        )?;
        Ok(meeting)
    }

    pub fn list_meetings(&self, project_id: Option<&str>) -> AppResult<Vec<Meeting>> {
        self.ensure_project_exists(project_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, title, status, scheduled_start, started_at, ended_at,
                    recording_path, duration_seconds, created_at, updated_at, deleted_at
             FROM meetings
             WHERE deleted_at IS NULL AND (?1 IS NULL OR project_id = ?1)
             ORDER BY COALESCE(scheduled_start, started_at, created_at) DESC, id",
        )?;
        let rows = statement.query_map([project_id], meeting_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_meeting(&self, id: &str) -> AppResult<Meeting> {
        self.connect()?
            .query_row(
                "SELECT id, project_id, title, status, scheduled_start, started_at, ended_at,
                        recording_path, duration_seconds, created_at, updated_at, deleted_at
                 FROM meetings WHERE id = ?1",
                [id],
                meeting_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown meeting: {id}")))
    }

    pub fn update_meeting(&self, patch: Value) -> AppResult<Meeting> {
        let object = patch
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("The meeting update must be an object".into()))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("A meeting ID is required".into()))?;
        let mut meeting = self.get_meeting(id)?;
        let previous_project_id = meeting.project_id.clone();
        set_string(object, "title", &mut meeting.title);
        set_nullable(object, "projectId", &mut meeting.project_id);
        set_nullable(object, "scheduledStart", &mut meeting.scheduled_start);
        meeting.title = required_title(&meeting.title)?;
        self.ensure_project_exists(meeting.project_id.as_deref())?;
        meeting.updated_at = Utc::now().to_rfc3339();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE meetings SET project_id = ?2, title = ?3, scheduled_start = ?4,
                updated_at = ?5 WHERE id = ?1 AND deleted_at IS NULL",
            params![
                meeting.id,
                meeting.project_id,
                meeting.title,
                meeting.scheduled_start,
                meeting.updated_at
            ],
        )?;
        if meeting.project_id != previous_project_id {
            transaction.execute(
                "INSERT INTO meeting_project_history
                    (id, meeting_id, from_project_id, to_project_id, changed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    meeting.id,
                    previous_project_id,
                    meeting.project_id,
                    meeting.updated_at
                ],
            )?;
        }
        transaction.commit()?;
        Ok(meeting)
    }

    pub fn mark_meeting_recording(&self, id: &str) -> AppResult<Meeting> {
        let meeting = self.get_meeting(id)?;
        if meeting.deleted_at.is_some() {
            return Err(AppError::InvalidInput("That meeting was deleted".into()));
        }
        if meeting.status == "recorded" {
            return Err(AppError::InvalidInput(
                "A recorded meeting cannot be recorded again".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE meetings SET status = 'recording', started_at = COALESCE(started_at, ?2),
                updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        self.get_meeting(id)
    }

    pub fn store_meeting_recording(
        &self,
        id: &str,
        wav: &[u8],
        duration_seconds: f64,
    ) -> AppResult<Meeting> {
        let meeting = self.get_meeting(id)?;
        if meeting.status != "recording" {
            return Err(AppError::InvalidInput(
                "That meeting is not recording".into(),
            ));
        }
        let recording_path = media::store_bytes(&self.media_root(), wav, ".wav")?;
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE meetings SET status = 'recorded', ended_at = ?2, recording_path = ?3,
                duration_seconds = ?4, updated_at = ?2 WHERE id = ?1",
            params![id, now, recording_path, duration_seconds],
        )?;
        self.get_meeting(id)
    }

    pub fn reset_meeting_after_failed_recording(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE meetings SET status = 'planned', started_at = NULL, updated_at = ?2
             WHERE id = ?1 AND status = 'recording' AND recording_path IS NULL",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: &str) -> AppResult<()> {
        let meeting = self.get_meeting(id)?;
        if meeting.status == "recording" {
            return Err(AppError::InvalidInput(
                "Stop the recording before deleting the meeting".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE meetings SET deleted_at = ?2, updated_at = ?2
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        Ok(())
    }
}

fn meeting_from_row(row: &Row<'_>) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        status: row.get(3)?,
        scheduled_start: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        recording_path: row.get(7)?,
        duration_seconds: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted_at: row.get(11)?,
    })
}

fn required_title(value: &str) -> AppResult<String> {
    let title = value.trim();
    if title.is_empty() {
        Err(AppError::InvalidInput("A meeting needs a title".into()))
    } else {
        Ok(title.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{OrganizationInput, ProjectInput};

    fn test_database() -> Database {
        let directory = std::env::temp_dir().join(format!("threadbox-meetings-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        database.migrate().unwrap();
        database
    }

    fn project(database: &Database, name: &str) -> String {
        let organization = database
            .create_organization(OrganizationInput {
                name: format!("{name} organisation"),
                notes: String::new(),
                context_sharing: "isolated".into(),
            })
            .unwrap();
        database
            .create_project(ProjectInput {
                organization_id: organization.id,
                parent_id: None,
                name: name.into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap()
            .id
    }

    #[test]
    fn creates_records_and_moves_a_meeting_with_its_recording() {
        let database = test_database();
        let first = project(&database, "First");
        let second = project(&database, "Second");
        let created = database
            .create_meeting(MeetingInput {
                project_id: Some(first.clone()),
                title: "Weekly sync".into(),
                scheduled_start: None,
            })
            .unwrap();
        database.mark_meeting_recording(&created.id).unwrap();
        let recorded = database
            .store_meeting_recording(&created.id, b"stereo wav", 12.5)
            .unwrap();
        let recording = recorded.recording_path.clone().unwrap();

        let moved = database
            .update_meeting(serde_json::json!({ "id": created.id, "projectId": second }))
            .unwrap();
        assert_eq!(moved.recording_path.as_deref(), Some(recording.as_str()));
        assert!(database.list_meetings(Some(&first)).unwrap().is_empty());
        assert_eq!(database.list_meetings(Some(&second)).unwrap().len(), 1);
        let moves: i64 = database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(1) FROM meeting_project_history WHERE meeting_id = ?1",
                [created.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(moves, 1);
    }

    #[test]
    fn supports_assignment_before_during_and_after_recording() {
        let database = test_database();
        let first = project(&database, "First");
        let second = project(&database, "Second");
        let meeting = database
            .create_meeting(MeetingInput {
                project_id: None,
                title: "Unfiled call".into(),
                scheduled_start: None,
            })
            .unwrap();
        let assigned = database
            .update_meeting(serde_json::json!({ "id": meeting.id, "projectId": first }))
            .unwrap();
        database.mark_meeting_recording(&assigned.id).unwrap();
        let moved = database
            .update_meeting(serde_json::json!({ "id": assigned.id, "projectId": second }))
            .unwrap();
        assert_eq!(moved.status, "recording");
        database
            .store_meeting_recording(&moved.id, b"wav", 2.0)
            .unwrap();
        let unassigned = database
            .update_meeting(serde_json::json!({ "id": moved.id, "projectId": null }))
            .unwrap();
        assert_eq!(unassigned.project_id, None);
        assert!(unassigned.recording_path.is_some());
    }

    #[test]
    fn rejects_unknown_projects_and_empty_titles() {
        let database = test_database();
        assert!(database
            .create_meeting(MeetingInput {
                project_id: Some("missing".into()),
                title: "Meeting".into(),
                scheduled_start: None,
            })
            .is_err());
        assert!(database
            .create_meeting(MeetingInput {
                project_id: None,
                title: "  ".into(),
                scheduled_start: None,
            })
            .is_err());
    }
}
