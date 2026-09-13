//! Persisted meeting transcription jobs and timestamped source transcripts.
//!
//! A job is committed before Whisper starts, so closing Threadbox cannot lose the request. Running
//! jobs return to the queue during migration on the next launch. The original stereo recording and
//! timestamped channel transcripts remain the source of truth for every later derived artefact.

use std::fs;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    database::Database,
    error::{AppError, AppResult},
    media,
    speech::{self, SpeechTranscript},
};

const JOB_KIND: &str = "meeting_transcription";
const PROMPT_VERSION: &str = "meeting-vocabulary-v1";
const PROMPT_CHARACTER_LIMIT: usize = 800;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingJob {
    pub id: String,
    pub meeting_id: String,
    pub status: String,
    pub attempts: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: String,
    pub channel: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub original_text: String,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingTranscript {
    pub id: String,
    pub meeting_id: String,
    pub microphone_language: String,
    pub system_language: String,
    pub model_id: String,
    pub prompt_version: String,
    pub created_at: String,
    pub updated_at: String,
    pub segments: Vec<TranscriptSegment>,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS processing_jobs (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'completed', 'failed')),
            attempts INTEGER NOT NULL DEFAULT 0,
            error TEXT,
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_processing_jobs_next
            ON processing_jobs(kind, status, created_at);
        CREATE TABLE IF NOT EXISTS meeting_transcripts (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL UNIQUE REFERENCES meetings(id) ON DELETE CASCADE,
            microphone_language TEXT NOT NULL,
            system_language TEXT NOT NULL,
            model_id TEXT NOT NULL,
            prompt_version TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS transcript_segments (
            id TEXT PRIMARY KEY,
            transcript_id TEXT NOT NULL REFERENCES meeting_transcripts(id) ON DELETE CASCADE,
            channel TEXT NOT NULL CHECK(channel IN ('microphone', 'system')),
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            text TEXT NOT NULL,
            original_text TEXT NOT NULL,
            sequence INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_transcript_segments_order
            ON transcript_segments(transcript_id, start_ms, channel, sequence);
        CREATE TABLE IF NOT EXISTS vocabulary_sets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            always_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE TABLE IF NOT EXISTS vocabulary_terms (
            id TEXT PRIMARY KEY,
            set_id TEXT NOT NULL REFERENCES vocabulary_sets(id) ON DELETE CASCADE,
            canonical_form TEXT NOT NULL,
            expansion TEXT,
            definition TEXT,
            language TEXT NOT NULL,
            variants_json TEXT NOT NULL DEFAULT '[]',
            priority INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE TABLE IF NOT EXISTS project_vocabulary_sets (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            set_id TEXT NOT NULL REFERENCES vocabulary_sets(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY(project_id, set_id)
        );",
    )?;
    Ok(())
}

pub(crate) fn recover_interrupted_jobs(connection: &Connection) -> AppResult<()> {
    connection.execute(
        "UPDATE processing_jobs SET status = 'queued', started_at = NULL,
            error = 'Threadbox restarted while this job was running', updated_at = ?1
         WHERE status = 'running'",
        [Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

impl Database {
    pub fn enqueue_meeting_transcription(&self, meeting_id: &str) -> AppResult<ProcessingJob> {
        let meeting = self.get_meeting(meeting_id)?;
        if meeting.recording_path.is_none() {
            return Err(AppError::InvalidInput(
                "Record the meeting before requesting a transcript".into(),
            ));
        }
        let connection = self.connect()?;
        if let Some(job) = connection
            .query_row(
                "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                        completed_at, updated_at
                 FROM processing_jobs
                 WHERE meeting_id = ?1 AND kind = ?2 AND status IN ('queued', 'running')
                 ORDER BY created_at DESC LIMIT 1",
                params![meeting_id, JOB_KIND],
                job_from_row,
            )
            .optional()?
        {
            return Ok(job);
        }
        let now = Utc::now().to_rfc3339();
        let job = ProcessingJob {
            id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            status: "queued".into(),
            attempts: 0,
            error: None,
            created_at: now.clone(),
            started_at: None,
            completed_at: None,
            updated_at: now,
        };
        connection.execute(
            "INSERT INTO processing_jobs
                (id, meeting_id, kind, status, attempts, error, created_at, started_at,
                 completed_at, updated_at)
             VALUES (?1, ?2, ?3, 'queued', 0, NULL, ?4, NULL, NULL, ?5)",
            params![
                job.id,
                job.meeting_id,
                JOB_KIND,
                job.created_at,
                job.updated_at
            ],
        )?;
        Ok(job)
    }

    pub fn list_meeting_jobs(&self, meeting_id: &str) -> AppResult<Vec<ProcessingJob>> {
        self.get_meeting(meeting_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                    completed_at, updated_at
             FROM processing_jobs WHERE meeting_id = ?1 AND kind = ?2
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![meeting_id, JOB_KIND], job_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn meeting_transcript(&self, meeting_id: &str) -> AppResult<Option<MeetingTranscript>> {
        self.get_meeting(meeting_id)?;
        let connection = self.connect()?;
        let transcript = connection
            .query_row(
                "SELECT id, meeting_id, microphone_language, system_language, model_id,
                        prompt_version, created_at, updated_at
                 FROM meeting_transcripts WHERE meeting_id = ?1",
                [meeting_id],
                transcript_from_row,
            )
            .optional()?;
        transcript
            .map(|item| load_segments(&connection, item))
            .transpose()
    }

    pub fn process_transcription_job(
        &self,
        job_id: &str,
        model_id: &str,
        language: &str,
    ) -> AppResult<MeetingTranscript> {
        let job = self.claim_job(job_id)?;
        let result = self.transcribe_meeting(&job.meeting_id, model_id, language);
        match result {
            Ok(transcript) => {
                self.finish_job(job_id, None)?;
                Ok(transcript)
            }
            Err(error) => {
                self.finish_job(job_id, Some(&error.to_string()))?;
                Err(error)
            }
        }
    }

    pub fn resume_transcription_jobs(&self, model_id: &str, language: &str) -> AppResult<()> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id FROM processing_jobs
             WHERE kind = ?1 AND status = 'queued' ORDER BY created_at",
        )?;
        let ids = statement
            .query_map([JOB_KIND], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        for id in ids {
            if let Err(error) = self.process_transcription_job(&id, model_id, language) {
                eprintln!("Could not resume transcription job {id}: {error}");
            }
        }
        Ok(())
    }

    fn claim_job(&self, job_id: &str) -> AppResult<ProcessingJob> {
        let now = Utc::now().to_rfc3339();
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE processing_jobs SET status = 'running', attempts = attempts + 1,
                error = NULL, started_at = ?2, completed_at = NULL, updated_at = ?2
             WHERE id = ?1 AND kind = ?3 AND status IN ('queued', 'failed')",
            params![job_id, now, JOB_KIND],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "That transcription job is not ready to run".into(),
            ));
        }
        connection
            .query_row(
                "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                    completed_at, updated_at FROM processing_jobs WHERE id = ?1",
                [job_id],
                job_from_row,
            )
            .map_err(Into::into)
    }

    fn finish_job(&self, job_id: &str, error: Option<&str>) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let (status, completed_at) = if error.is_some() {
            ("failed", None)
        } else {
            ("completed", Some(now.as_str()))
        };
        self.connect()?.execute(
            "UPDATE processing_jobs SET status = ?2, error = ?3, completed_at = ?4,
                updated_at = ?5 WHERE id = ?1",
            params![job_id, status, error, completed_at, now],
        )?;
        Ok(())
    }

    fn transcribe_meeting(
        &self,
        meeting_id: &str,
        model_id: &str,
        language: &str,
    ) -> AppResult<MeetingTranscript> {
        let meeting = self.get_meeting(meeting_id)?;
        let reference = meeting.recording_path.ok_or_else(|| {
            AppError::InvalidInput("That meeting does not have a recording".into())
        })?;
        let language = meeting
            .project_id
            .as_deref()
            .map(|project_id| {
                self.project_language(project_id)
                    .map(|value| value.language)
            })
            .transpose()?
            .unwrap_or_else(|| language.to_string());
        let bytes = fs::read(media::resolve(&self.media_root(), &reference)?)?;
        let prompt = self.meeting_vocabulary_prompt(meeting.project_id.as_deref())?;
        let microphone =
            speech::transcribe_wav_bytes(&bytes, model_id, &language, Some(0), prompt.as_deref())?;
        let system =
            speech::transcribe_wav_bytes(&bytes, model_id, &language, Some(1), prompt.as_deref())?;
        self.replace_transcript(meeting_id, model_id, microphone, system)
    }

    fn replace_transcript(
        &self,
        meeting_id: &str,
        model_id: &str,
        microphone: SpeechTranscript,
        system: SpeechTranscript,
    ) -> AppResult<MeetingTranscript> {
        let now = Utc::now().to_rfc3339();
        let existing = self.meeting_transcript(meeting_id)?;
        let transcript_id = existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = existing
            .map(|item| item.created_at)
            .unwrap_or_else(|| now.clone());
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO meeting_transcripts
                (id, meeting_id, microphone_language, system_language, model_id, prompt_version,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(meeting_id) DO UPDATE SET microphone_language = excluded.microphone_language,
                system_language = excluded.system_language, model_id = excluded.model_id,
                prompt_version = excluded.prompt_version, updated_at = excluded.updated_at",
            params![
                transcript_id,
                meeting_id,
                microphone.language,
                system.language,
                model_id,
                PROMPT_VERSION,
                created_at,
                now
            ],
        )?;
        transaction.execute(
            "DELETE FROM transcript_segments WHERE transcript_id = ?1",
            [&transcript_id],
        )?;
        for (channel, source) in [("microphone", microphone), ("system", system)] {
            for (sequence, segment) in source.segments.into_iter().enumerate() {
                transaction.execute(
                    "INSERT INTO transcript_segments
                        (id, transcript_id, channel, start_ms, end_ms, text, original_text, sequence)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
                    params![
                        Uuid::new_v4().to_string(),
                        transcript_id,
                        channel,
                        segment.start_ms,
                        segment.end_ms,
                        segment.text,
                        sequence as i64
                    ],
                )?;
            }
        }
        transaction.commit()?;
        self.meeting_transcript(meeting_id)?
            .ok_or_else(|| AppError::InvalidInput("The transcript could not be stored".into()))
    }

    fn meeting_vocabulary_prompt(&self, project_id: Option<&str>) -> AppResult<Option<String>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT DISTINCT t.canonical_form, t.expansion, t.priority
             FROM vocabulary_terms t
             JOIN vocabulary_sets s ON s.id = t.set_id
             LEFT JOIN project_vocabulary_sets pvs ON pvs.set_id = s.id
             WHERE t.deleted_at IS NULL AND s.deleted_at IS NULL
               AND (s.always_active = 1 OR pvs.project_id = ?1)
             ORDER BY t.priority DESC, t.canonical_form COLLATE NOCASE",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut prompt = String::new();
        for row in rows {
            let (term, expansion) = row?;
            let entry = expansion
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("{term} ({value})"))
                .unwrap_or(term);
            let separator = if prompt.is_empty() { "" } else { ", " };
            if prompt.chars().count() + separator.len() + entry.chars().count()
                > PROMPT_CHARACTER_LIMIT
            {
                break;
            }
            prompt.push_str(separator);
            prompt.push_str(&entry);
        }
        Ok((!prompt.is_empty()).then_some(prompt))
    }
}

fn job_from_row(row: &Row<'_>) -> rusqlite::Result<ProcessingJob> {
    Ok(ProcessingJob {
        id: row.get(0)?,
        meeting_id: row.get(1)?,
        status: row.get(2)?,
        attempts: row.get(3)?,
        error: row.get(4)?,
        created_at: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn transcript_from_row(row: &Row<'_>) -> rusqlite::Result<MeetingTranscript> {
    Ok(MeetingTranscript {
        id: row.get(0)?,
        meeting_id: row.get(1)?,
        microphone_language: row.get(2)?,
        system_language: row.get(3)?,
        model_id: row.get(4)?,
        prompt_version: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        segments: Vec::new(),
    })
}

fn load_segments(
    connection: &Connection,
    mut transcript: MeetingTranscript,
) -> AppResult<MeetingTranscript> {
    let mut statement = connection.prepare(
        "SELECT id, channel, start_ms, end_ms, text, original_text, sequence
         FROM transcript_segments WHERE transcript_id = ?1
         ORDER BY start_ms, channel, sequence",
    )?;
    let rows = statement.query_map([&transcript.id], |row| {
        Ok(TranscriptSegment {
            id: row.get(0)?,
            channel: row.get(1)?,
            start_ms: row.get(2)?,
            end_ms: row.get(3)?,
            text: row.get(4)?,
            original_text: row.get(5)?,
            sequence: row.get(6)?,
        })
    })?;
    transcript.segments = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meetings::MeetingInput,
        speech::SpeechSegment,
        workspace::{OrganizationInput, ProjectInput},
    };

    fn database() -> Database {
        let directory =
            std::env::temp_dir().join(format!("threadbox-transcripts-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        database.migrate().unwrap();
        database
    }

    fn recorded_meeting(database: &Database) -> String {
        let organization = database
            .create_organization(OrganizationInput {
                name: "Example".into(),
                notes: String::new(),
                context_sharing: "isolated".into(),
            })
            .unwrap();
        let project = database
            .create_project(ProjectInput {
                organization_id: organization.id,
                parent_id: None,
                name: "Work".into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap();
        let meeting = database
            .create_meeting(MeetingInput {
                project_id: Some(project.id),
                title: "Review".into(),
                scheduled_start: None,
            })
            .unwrap();
        database.mark_meeting_recording(&meeting.id).unwrap();
        database
            .store_meeting_recording(&meeting.id, b"wav", 1.0)
            .unwrap();
        meeting.id
    }

    #[test]
    fn persists_and_deduplicates_queued_jobs() {
        let database = database();
        let meeting_id = recorded_meeting(&database);
        let first = database.enqueue_meeting_transcription(&meeting_id).unwrap();
        let second = database.enqueue_meeting_transcription(&meeting_id).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(database.list_meeting_jobs(&meeting_id).unwrap().len(), 1);
    }

    #[test]
    fn stores_timestamped_channels_as_the_source_transcript() {
        let database = database();
        let meeting_id = recorded_meeting(&database);
        let microphone = SpeechTranscript {
            language: "pl".into(),
            segments: vec![SpeechSegment {
                start_ms: 100,
                end_ms: 800,
                text: "I will do it".into(),
            }],
        };
        let system = SpeechTranscript {
            language: "en".into(),
            segments: vec![SpeechSegment {
                start_ms: 50,
                end_ms: 900,
                text: "Please send it".into(),
            }],
        };
        let transcript = database
            .replace_transcript(&meeting_id, "small", microphone, system)
            .unwrap();
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[0].channel, "system");
        assert_eq!(transcript.segments[1].original_text, "I will do it");
        assert_eq!(transcript.prompt_version, PROMPT_VERSION);
    }

    #[test]
    fn returns_interrupted_jobs_to_the_queue_on_launch() {
        let database = database();
        let meeting_id = recorded_meeting(&database);
        let job = database.enqueue_meeting_transcription(&meeting_id).unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                "UPDATE processing_jobs SET status = 'running' WHERE id = ?1",
                [&job.id],
            )
            .unwrap();
        recover_interrupted_jobs(&database.connect().unwrap()).unwrap();
        assert_eq!(
            database.list_meeting_jobs(&meeting_id).unwrap()[0].status,
            "queued"
        );
    }
}
