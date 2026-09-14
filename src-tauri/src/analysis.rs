//! Derived meeting notes and work, always traceable to the timestamped source transcript.

use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    database::{Database, TaskInput},
    error::{AppError, AppResult},
    integrations::PrivacyReceiptInput,
    providers::{self, LanguageModelSettings},
    transcriptions::{MeetingTranscript, ProcessingJob},
};

const JOB_KIND: &str = "meeting_analysis";
const PROMPT_VERSION: &str = "meeting-analysis-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingAnalysisItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingAnalysis {
    pub id: String,
    pub meeting_id: String,
    pub notes: String,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<MeetingAnalysisItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelAnalysis {
    notes: String,
    #[serde(default)]
    items: Vec<ModelAnalysisItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelAnalysisItem {
    kind: String,
    #[serde(default)]
    title: String,
    text: String,
    start_ms: i64,
    end_ms: i64,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meeting_analyses (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL UNIQUE REFERENCES meetings(id) ON DELETE CASCADE,
            notes TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            prompt_version TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS meeting_analysis_items (
            id TEXT PRIMARY KEY,
            analysis_id TEXT NOT NULL REFERENCES meeting_analyses(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('decision', 'action_item', 'addressed', 'term_explanation')),
            title TEXT NOT NULL,
            text TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            task_id TEXT REFERENCES tasks(id),
            sequence INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_meeting_analysis_items_order
            ON meeting_analysis_items(analysis_id, kind, start_ms, sequence);",
    )?;
    Ok(())
}

impl Database {
    pub fn meeting_analysis(&self, meeting_id: &str) -> AppResult<Option<MeetingAnalysis>> {
        self.get_meeting(meeting_id)?;
        let connection = self.connect()?;
        let analysis = connection
            .query_row(
                "SELECT id, meeting_id, notes, provider, model, prompt_version, created_at, updated_at
                 FROM meeting_analyses WHERE meeting_id = ?1",
                [meeting_id],
                analysis_from_row,
            )
            .optional()?;
        analysis
            .map(|item| load_items(&connection, item))
            .transpose()
    }

    pub fn enqueue_meeting_analysis(&self, meeting_id: &str) -> AppResult<ProcessingJob> {
        if self.meeting_transcript(meeting_id)?.is_none() {
            return Err(AppError::InvalidInput(
                "Transcribe the meeting before requesting analysis".into(),
            ));
        }
        let connection = self.connect()?;
        if let Some(job) = connection
            .query_row(
                "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                        completed_at, updated_at FROM processing_jobs
                 WHERE meeting_id = ?1 AND kind = ?2 AND status IN ('queued', 'running')
                 ORDER BY created_at DESC LIMIT 1",
                params![meeting_id, JOB_KIND],
                processing_job_from_row,
            )
            .optional()?
        {
            return Ok(job);
        }
        let now = Utc::now().to_rfc3339();
        let job = ProcessingJob {
            id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.into(),
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

    pub fn list_meeting_analysis_jobs(&self, meeting_id: &str) -> AppResult<Vec<ProcessingJob>> {
        self.get_meeting(meeting_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                    completed_at, updated_at FROM processing_jobs
             WHERE meeting_id = ?1 AND kind = ?2 ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![meeting_id, JOB_KIND], processing_job_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn process_analysis_job(
        &self,
        job_id: &str,
        settings: &LanguageModelSettings,
    ) -> AppResult<MeetingAnalysis> {
        let job = self.claim_analysis_job(job_id)?;
        let result = self.analyse_meeting(&job.meeting_id, settings);
        self.finish_analysis_job(
            job_id,
            result.as_ref().err().map(ToString::to_string).as_deref(),
        )?;
        result
    }

    pub fn resume_analysis_jobs(&self, settings: &LanguageModelSettings) -> AppResult<()> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id FROM processing_jobs WHERE kind = ?1 AND status = 'queued'
             ORDER BY created_at",
        )?;
        let ids = statement
            .query_map([JOB_KIND], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        for id in ids {
            if let Err(error) = self.process_analysis_job(&id, settings) {
                eprintln!("Could not resume analysis job {id}: {error}");
            }
        }
        Ok(())
    }

    fn claim_analysis_job(&self, job_id: &str) -> AppResult<ProcessingJob> {
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
                "That analysis job is not ready to run".into(),
            ));
        }
        connection
            .query_row(
                "SELECT id, meeting_id, status, attempts, error, created_at, started_at,
                        completed_at, updated_at FROM processing_jobs WHERE id = ?1",
                [job_id],
                processing_job_from_row,
            )
            .map_err(Into::into)
    }

    fn finish_analysis_job(&self, job_id: &str, error: Option<&str>) -> AppResult<()> {
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

    fn analyse_meeting(
        &self,
        meeting_id: &str,
        settings: &LanguageModelSettings,
    ) -> AppResult<MeetingAnalysis> {
        let meeting = self.get_meeting(meeting_id)?;
        let transcript = self.meeting_transcript(meeting_id)?.ok_or_else(|| {
            AppError::InvalidInput("That meeting does not have a transcript".into())
        })?;
        let self_name = self
            .self_person()?
            .map(|person| person.display_name)
            .unwrap_or_else(|| "the user".into());
        let glossary = self.analysis_glossary(meeting.project_id.as_deref())?;
        let system_prompt = analysis_system_prompt(&self_name);
        let user_prompt = analysis_user_prompt(&meeting.title, &transcript, &glossary);
        if settings.kind == providers::KIND_API {
            if let Some(project_id) = meeting.project_id.as_deref() {
                let project = self.get_project(project_id)?;
                self.record_privacy_receipt(&PrivacyReceiptInput {
                    organization_id: project.organization_id,
                    project_id: Some(project_id.to_string()),
                    connection_id: None,
                    provider: settings.api.provider.clone(),
                    operation: "meeting.analyse".into(),
                    reason: "User requested meeting analysis with a cloud language model".into(),
                    data_categories: vec![
                        "meeting transcript".into(),
                        "project vocabulary".into(),
                        "user display name".into(),
                    ],
                    destination: match settings.api.provider.as_str() {
                        "anthropic" => "api.anthropic.com".into(),
                        "openai" => "api.openai.com".into(),
                        "openrouter" => "openrouter.ai".into(),
                        _ => settings.api.base_url.clone(),
                    },
                    byte_count: Some((system_prompt.len() + user_prompt.len()) as i64),
                })?;
            }
        }
        let completion = providers::complete(settings, &system_prompt, &user_prompt)?;
        let parsed = parse_model_analysis(&completion.content)?;
        self.store_analysis(
            &meeting.id,
            meeting.project_id.as_deref(),
            &meeting.title,
            completion.provider,
            completion.model,
            parsed,
        )
    }

    fn store_analysis(
        &self,
        meeting_id: &str,
        project_id: Option<&str>,
        meeting_title: &str,
        provider: String,
        model: String,
        parsed: ModelAnalysis,
    ) -> AppResult<MeetingAnalysis> {
        let existing = self.meeting_analysis(meeting_id)?;
        let existing_tasks = existing
            .as_ref()
            .map(|analysis| {
                analysis
                    .items
                    .iter()
                    .filter_map(|item| {
                        item.task_id
                            .clone()
                            .map(|task| (action_key(&item.title, item.start_ms), task))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut action_tasks = existing_tasks;
        let analysis_id = existing
            .as_ref()
            .map(|analysis| analysis.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now().to_rfc3339();
        let created_at = existing
            .map(|analysis| analysis.created_at)
            .unwrap_or_else(|| now.clone());
        for item in &parsed.items {
            validate_analysis_kind(&item.kind)?;
        }
        let mut items = Vec::new();
        for source in parsed.items {
            let title = if source.title.trim().is_empty() {
                source.text.trim().chars().take(100).collect()
            } else {
                source.title.trim().to_string()
            };
            let text = source.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let task_id = if source.kind == "action_item" {
                let key = action_key(&title, source.start_ms);
                if let Some(id) = action_tasks.get(&key) {
                    Some(id.clone())
                } else {
                    let id = self
                        .create_task(TaskInput {
                            title: title.clone(),
                            notes: format!("From meeting: {meeting_title}"),
                            status: "todo".into(),
                            priority: "mid".into(),
                            project_id: project_id.map(str::to_string),
                            source_type: "meeting".into(),
                            source_url: None,
                            source_label: Some(meeting_title.into()),
                            source_author: None,
                            source_excerpt: Some(format!(
                                "{} at {}",
                                text,
                                format_timestamp(source.start_ms)
                            )),
                            due_at: None,
                            remind_at: None,
                            screenshot_data_url: None,
                            screenshots: Vec::new(),
                            audio_attachments: Vec::new(),
                            links: Vec::new(),
                            file_attachments: Vec::new(),
                        })?
                        .id;
                    action_tasks.insert(key, id.clone());
                    Some(id)
                }
            } else {
                None
            };
            items.push(MeetingAnalysisItem {
                id: Uuid::new_v4().to_string(),
                kind: source.kind,
                title,
                text,
                start_ms: source.start_ms.max(0),
                end_ms: source.end_ms.max(source.start_ms).max(0),
                task_id,
            });
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO meeting_analyses
                (id, meeting_id, notes, provider, model, prompt_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(meeting_id) DO UPDATE SET notes = excluded.notes,
                provider = excluded.provider, model = excluded.model,
                prompt_version = excluded.prompt_version, updated_at = excluded.updated_at",
            params![
                analysis_id,
                meeting_id,
                parsed.notes.trim(),
                provider,
                model,
                PROMPT_VERSION,
                created_at,
                now
            ],
        )?;
        transaction.execute(
            "DELETE FROM meeting_analysis_items WHERE analysis_id = ?1",
            [&analysis_id],
        )?;
        for (sequence, item) in items.iter().enumerate() {
            transaction.execute(
                "INSERT INTO meeting_analysis_items
                    (id, analysis_id, kind, title, text, start_ms, end_ms, task_id, sequence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    item.id,
                    analysis_id,
                    item.kind,
                    item.title,
                    item.text,
                    item.start_ms,
                    item.end_ms,
                    item.task_id,
                    sequence as i64
                ],
            )?;
        }
        transaction.commit()?;
        self.meeting_analysis(meeting_id)?.ok_or_else(|| {
            AppError::InvalidInput("The meeting analysis could not be stored".into())
        })
    }

    fn analysis_glossary(&self, project_id: Option<&str>) -> AppResult<String> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT DISTINCT t.canonical_form, t.expansion, t.definition
             FROM vocabulary_terms t JOIN vocabulary_sets s ON s.id = t.set_id
             LEFT JOIN project_vocabulary_sets pvs ON pvs.set_id = s.id
             WHERE t.deleted_at IS NULL AND s.deleted_at IS NULL
               AND (s.always_active = 1 OR pvs.project_id = ?1)
               AND (t.definition IS NOT NULL OR t.expansion IS NOT NULL)
             ORDER BY t.priority DESC, t.canonical_form COLLATE NOCASE LIMIT 200",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut lines = Vec::new();
        for row in rows {
            let (term, expansion, definition) = row?;
            lines.push(format!(
                "- {term}{}: {}",
                expansion
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default(),
                definition.unwrap_or_default()
            ));
        }
        Ok(lines.join("\n"))
    }
}

fn analysis_system_prompt(self_name: &str) -> String {
    format!(
        "You analyse a timestamped meeting transcript. The user is {self_name}. Return only one JSON object with keys notes and items. notes is concise Markdown. Each item has kind, title, text, startMs and endMs. Allowed kinds: decision, action_item, addressed, term_explanation. Only action_item entries that are explicitly the user's own commitments. addressed means a moment where someone asks or directly addresses the user. term_explanation explains a glossary term needed to understand that moment. Use exact transcript timestamps in milliseconds. Do not invent facts or work."
    )
}

fn analysis_user_prompt(title: &str, transcript: &MeetingTranscript, glossary: &str) -> String {
    let lines = transcript
        .segments
        .iter()
        .map(|segment| {
            format!(
                "[{}-{}] {}: {}",
                format_timestamp(segment.start_ms),
                format_timestamp(segment.end_ms),
                if segment.channel == "microphone" {
                    "USER"
                } else {
                    "OTHERS"
                },
                segment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Meeting: {title}\n\nProject glossary:\n{}\n\nTranscript:\n{lines}",
        if glossary.is_empty() {
            "(none)"
        } else {
            glossary
        }
    )
}

fn parse_model_analysis(content: &str) -> AppResult<ModelAnalysis> {
    let trimmed = content.trim();
    let json = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(json).map_err(|error| {
        AppError::InvalidInput(format!("The analysis model returned invalid JSON: {error}"))
    })
}

fn validate_analysis_kind(kind: &str) -> AppResult<()> {
    if matches!(
        kind,
        "decision" | "action_item" | "addressed" | "term_explanation"
    ) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "The analysis model returned an unknown item kind: {kind}"
        )))
    }
}

fn format_timestamp(milliseconds: i64) -> String {
    let seconds = milliseconds.max(0) / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn action_key(title: &str, start_ms: i64) -> String {
    let normalized = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    format!("{normalized}:{}", start_ms.max(0) / 5_000)
}

fn processing_job_from_row(row: &Row<'_>) -> rusqlite::Result<ProcessingJob> {
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

fn analysis_from_row(row: &Row<'_>) -> rusqlite::Result<MeetingAnalysis> {
    Ok(MeetingAnalysis {
        id: row.get(0)?,
        meeting_id: row.get(1)?,
        notes: row.get(2)?,
        provider: row.get(3)?,
        model: row.get(4)?,
        prompt_version: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        items: Vec::new(),
    })
}

fn load_items(
    connection: &Connection,
    mut analysis: MeetingAnalysis,
) -> AppResult<MeetingAnalysis> {
    let mut statement = connection.prepare(
        "SELECT id, kind, title, text, start_ms, end_ms, task_id
         FROM meeting_analysis_items WHERE analysis_id = ?1 ORDER BY sequence",
    )?;
    analysis.items = statement
        .query_map([&analysis.id], |row| {
            Ok(MeetingAnalysisItem {
                id: row.get(0)?,
                kind: row.get(1)?,
                title: row.get(2)?,
                text: row.get(3)?,
                start_ms: row.get(4)?,
                end_ms: row.get(5)?,
                task_id: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meetings::MeetingInput,
        workspace::{OrganizationInput, ProjectInput},
    };

    fn database_with_meeting() -> (Database, String, String) {
        let directory = std::env::temp_dir().join(format!("threadbox-analysis-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        database.migrate().unwrap();
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
                name: "Delivery".into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap();
        let meeting = database
            .create_meeting(MeetingInput {
                project_id: Some(project.id.clone()),
                title: "Planning".into(),
                scheduled_start: None,
            })
            .unwrap();
        (database, project.id, meeting.id)
    }

    #[test]
    fn parses_a_fenced_structured_analysis() {
        let parsed = parse_model_analysis("```json\n{\"notes\":\"Summary\",\"items\":[{\"kind\":\"decision\",\"title\":\"Ship\",\"text\":\"Ship Friday\",\"startMs\":100,\"endMs\":200}]}\n```").unwrap();
        assert_eq!(parsed.notes, "Summary");
        assert_eq!(parsed.items[0].kind, "decision");
    }

    #[test]
    fn refuses_an_invented_artifact_kind() {
        assert!(validate_analysis_kind("general_task").is_err());
    }

    #[test]
    fn creates_each_user_action_as_one_project_task_across_reanalysis() {
        let (database, project_id, meeting_id) = database_with_meeting();
        let make_analysis = || ModelAnalysis {
            notes: "Plan agreed".into(),
            items: vec![ModelAnalysisItem {
                kind: "action_item".into(),
                title: "Send the plan".into(),
                text: "The user will send the plan".into(),
                start_ms: 12_000,
                end_ms: 14_000,
            }],
        };
        let first = database
            .store_analysis(
                &meeting_id,
                Some(&project_id),
                "Planning",
                "test".into(),
                "model".into(),
                make_analysis(),
            )
            .unwrap();
        let second = database
            .store_analysis(
                &meeting_id,
                Some(&project_id),
                "Planning",
                "test".into(),
                "model".into(),
                make_analysis(),
            )
            .unwrap();
        assert_eq!(first.items[0].task_id, second.items[0].task_id);
        assert_eq!(database.list_tasks().unwrap().len(), 1);
        assert_eq!(
            database.list_tasks().unwrap()[0].project_id.as_deref(),
            Some(project_id.as_str())
        );
    }
}
