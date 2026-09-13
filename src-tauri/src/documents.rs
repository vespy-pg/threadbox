//! Documents on a project: typed notes, links and files.
//!
//! They serve two purposes at once, and the second is why they are worth their own table: they are
//! reference material for the user, and they are the material a language model reads when it writes
//! notes for a meeting on that project. A project brief pasted as a note is the cheapest way to make
//! the first generated notes for a project useful. See `docs/domain-model.md`.

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

/// Free text typed by the user.
const KIND_NOTE: &str = "note";
/// An address elsewhere, stored as text rather than fetched.
const KIND_LINK: &str = "link";
/// Bytes held in the media store.
const KIND_FILE: &str = "file";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    /// Relative to the media root, never an absolute path.
    pub media_path: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocumentInput {
    pub project_id: String,
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
    /// The bytes of a file document, inline, on creation only.
    #[serde(default)]
    pub data_url: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_documents (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('note', 'link', 'file')),
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            url TEXT,
            media_path TEXT,
            mime_type TEXT,
            size_bytes INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_project_documents_project
            ON project_documents(project_id);",
    )?;
    Ok(())
}

impl Database {
    pub fn create_project_document(
        &self,
        input: ProjectDocumentInput,
    ) -> AppResult<ProjectDocument> {
        self.ensure_project_exists(Some(&input.project_id))?;
        validate_kind(&input.kind)?;
        let now = Utc::now().to_rfc3339();
        let mut document = ProjectDocument {
            id: Uuid::new_v4().to_string(),
            project_id: input.project_id,
            kind: input.kind,
            title: input.title.trim().to_string(),
            body: input.body,
            url: None,
            media_path: None,
            mime_type: input.mime_type,
            size_bytes: None,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        match document.kind.as_str() {
            KIND_NOTE => {
                if document.title.is_empty() && document.body.trim().is_empty() {
                    return Err(AppError::InvalidInput(
                        "A note needs a title or text".into(),
                    ));
                }
                if document.title.is_empty() {
                    document.title = first_line(&document.body);
                }
            }
            KIND_LINK => {
                let url = normalise_url(input.url.as_deref().unwrap_or_default())?;
                if document.title.is_empty() {
                    document.title = url.clone();
                }
                document.url = Some(url);
            }
            _ => {
                let data_url = input
                    .data_url
                    .ok_or_else(|| AppError::InvalidInput("A file document needs a file".into()))?;
                let name = input.file_name.unwrap_or_default();
                if document.title.is_empty() {
                    document.title = if name.trim().is_empty() {
                        "Attached file".into()
                    } else {
                        name.trim().to_string()
                    };
                }
                let extension = media::extension_of(&name, ".bin");
                let bytes = media::decode_data_url(&data_url)?;
                document.size_bytes = Some(bytes.len() as i64);
                document.media_path =
                    Some(media::store_bytes(&self.media_root(), &bytes, &extension)?);
            }
        }
        self.connect()?.execute(
            "INSERT INTO project_documents (id, project_id, kind, title, body, url, media_path,
                mime_type, size_bytes, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL)",
            params![
                document.id,
                document.project_id,
                document.kind,
                document.title,
                document.body,
                document.url,
                document.media_path,
                document.mime_type,
                document.size_bytes,
                document.created_at,
                document.updated_at
            ],
        )?;
        Ok(document)
    }

    pub fn list_project_documents(&self, project_id: &str) -> AppResult<Vec<ProjectDocument>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, kind, title, body, url, media_path, mime_type, size_bytes,
                    created_at, updated_at, deleted_at
             FROM project_documents
             WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY created_at DESC, id",
        )?;
        let rows = statement.query_map([project_id], document_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_project_document(&self, id: &str) -> AppResult<ProjectDocument> {
        self.connect()?
            .query_row(
                "SELECT id, project_id, kind, title, body, url, media_path, mime_type, size_bytes,
                        created_at, updated_at, deleted_at
                 FROM project_documents WHERE id = ?1",
                [id],
                document_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown document: {id}")))
    }

    /// Updates the text of a document. The kind and the stored bytes of a file are fixed: replacing
    /// a file means adding another document, so the earlier one stays citable.
    pub fn update_project_document(&self, patch: Value) -> AppResult<ProjectDocument> {
        let object = patch.as_object().ok_or_else(|| {
            AppError::InvalidInput("The document update must be an object".into())
        })?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("A document ID is required".into()))?;
        let mut document = self.get_project_document(id)?;
        let previous_project = document.project_id.clone();
        set_string(object, "projectId", &mut document.project_id);
        set_string(object, "title", &mut document.title);
        set_string(object, "body", &mut document.body);
        set_nullable(object, "url", &mut document.url);
        if document.project_id != previous_project {
            self.ensure_project_exists(Some(&document.project_id))?;
        }
        document.title = document.title.trim().to_string();
        if document.title.is_empty() {
            return Err(AppError::InvalidInput("A document needs a title".into()));
        }
        if document.kind == KIND_LINK {
            let url = normalise_url(document.url.as_deref().unwrap_or_default())?;
            document.url = Some(url);
        }
        document.updated_at = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE project_documents SET project_id = ?2, title = ?3, body = ?4, url = ?5,
                updated_at = ?6 WHERE id = ?1",
            params![
                document.id,
                document.project_id,
                document.title,
                document.body,
                document.url,
                document.updated_at
            ],
        )?;
        Ok(document)
    }

    /// Soft deletes the document, as organisations, projects and people are soft deleted. The row
    /// still refers to its file, so the bytes survive until the row is purged.
    pub fn delete_project_document(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE project_documents SET deleted_at = ?2, updated_at = ?2
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        Ok(())
    }
}

fn document_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectDocument> {
    Ok(ProjectDocument {
        id: row.get(0)?,
        project_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        url: row.get(5)?,
        media_path: row.get(6)?,
        mime_type: row.get(7)?,
        size_bytes: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted_at: row.get(11)?,
    })
}

fn validate_kind(kind: &str) -> AppResult<()> {
    if [KIND_NOTE, KIND_LINK, KIND_FILE].contains(&kind) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Unknown document kind: {kind}"
        )))
    }
}

/// Accepts what a user pastes, including an address without a scheme, and rejects what is not an
/// address at all.
fn normalise_url(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("A link needs an address".into()));
    }
    let candidate = if trimmed.contains("://") || trimmed.starts_with("mailto:") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    url::Url::parse(&candidate)
        .map(|parsed| parsed.to_string())
        .map_err(|error| AppError::InvalidInput(format!("That is not a usable address: {error}")))
}

fn first_line(body: &str) -> String {
    let line = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Note");
    let trimmed = line.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let shortened = trimmed.chars().take(79).collect::<String>();
    format!("{shortened}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{OrganizationInput, ProjectInput};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DATABASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    /// Each database gets a directory of its own, because the media root beside it holds blobs
    /// shared between rows, and collecting garbage in a shared root would delete another test's
    /// files.
    fn test_database() -> Database {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "threadbox-documents-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Database {
            path: directory.join("threadbox.sqlite3"),
        };
        database.migrate().unwrap();
        database
    }

    fn project(database: &Database) -> String {
        let organization = database
            .create_organization(OrganizationInput {
                name: "Aptvision".into(),
                notes: String::new(),
                context_sharing: "isolated".into(),
            })
            .unwrap();
        database
            .create_project(ProjectInput {
                organization_id: organization.id,
                parent_id: None,
                name: "Engage Hub".into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap()
            .id
    }

    fn input(project_id: &str, kind: &str) -> ProjectDocumentInput {
        ProjectDocumentInput {
            project_id: project_id.into(),
            kind: kind.into(),
            title: String::new(),
            body: String::new(),
            url: None,
            data_url: None,
            file_name: None,
            mime_type: None,
        }
    }

    #[test]
    fn titles_a_note_from_its_first_line() {
        let database = test_database();
        let project_id = project(&database);
        let mut note = input(&project_id, "note");
        note.body = "\n  Weekly sync runs on Tuesdays\nSecond line\n".into();
        let created = database.create_project_document(note).unwrap();
        assert_eq!(created.title, "Weekly sync runs on Tuesdays");
        assert_eq!(
            database.list_project_documents(&project_id).unwrap().len(),
            1
        );
    }

    #[test]
    fn refuses_an_empty_note() {
        let database = test_database();
        let project_id = project(&database);
        assert!(database
            .create_project_document(input(&project_id, "note"))
            .is_err());
    }

    #[test]
    fn completes_a_link_without_a_scheme() {
        let database = test_database();
        let project_id = project(&database);
        let mut link = input(&project_id, "link");
        link.url = Some("example.com/brief".into());
        let created = database.create_project_document(link).unwrap();
        assert_eq!(created.url.as_deref(), Some("https://example.com/brief"));

        let mut broken = input(&project_id, "link");
        broken.url = Some("   ".into());
        assert!(database.create_project_document(broken).is_err());
    }

    #[test]
    fn stores_a_file_document_in_the_blob_store() {
        let database = test_database();
        let project_id = project(&database);
        let mut file = input(&project_id, "file");
        file.data_url = Some("data:application/pdf;base64,YnJpZWY=".into());
        file.file_name = Some("brief.pdf".into());
        file.mime_type = Some("application/pdf".into());
        let created = database.create_project_document(file).unwrap();
        let reference = created.media_path.clone().unwrap();
        assert!(reference.starts_with("blobs/"));
        assert!(reference.ends_with(".pdf"));
        assert_eq!(created.title, "brief.pdf");
        assert_eq!(created.size_bytes, Some(5));
        assert_eq!(
            std::fs::read(media::resolve(&database.media_root(), &reference).unwrap()).unwrap(),
            b"brief"
        );

        // The bytes stay referenced while the row exists, so nothing collects them.
        database.collect_media_garbage().unwrap();
        assert!(media::resolve(&database.media_root(), &reference)
            .unwrap()
            .is_file());
    }

    #[test]
    fn refuses_a_document_on_an_unknown_project() {
        let database = test_database();
        let mut note = input("no-such-project", "note");
        note.body = "Orphan".into();
        assert!(database.create_project_document(note).is_err());
    }

    #[test]
    fn hides_a_deleted_document_and_moves_one_between_projects() {
        let database = test_database();
        let project_id = project(&database);
        let other_project_id = project(&database);
        let mut note = input(&project_id, "note");
        note.body = "Moved".into();
        let created = database.create_project_document(note).unwrap();

        let moved = database
            .update_project_document(
                serde_json::json!({ "id": created.id, "projectId": other_project_id }),
            )
            .unwrap();
        assert_eq!(moved.project_id, other_project_id);
        assert!(database
            .list_project_documents(&project_id)
            .unwrap()
            .is_empty());

        database.delete_project_document(&created.id).unwrap();
        assert!(database
            .list_project_documents(&other_project_id)
            .unwrap()
            .is_empty());
        assert!(database
            .get_project_document(&created.id)
            .unwrap()
            .deleted_at
            .is_some());
    }
}
