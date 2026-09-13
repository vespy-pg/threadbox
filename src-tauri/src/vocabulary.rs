//! Project vocabulary used to bias recognition and explain domain terms.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    database::Database,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularySet {
    pub id: String,
    pub name: String,
    pub always_active: bool,
    pub project_ids: Vec<String>,
    pub term_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularySetInput {
    pub name: String,
    #[serde(default)]
    pub always_active: bool,
    #[serde(default)]
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyTerm {
    pub id: String,
    pub set_id: String,
    pub canonical_form: String,
    pub expansion: Option<String>,
    pub definition: Option<String>,
    pub language: String,
    pub variants: Vec<String>,
    pub priority: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyTermInput {
    pub set_id: String,
    pub canonical_form: String,
    #[serde(default)]
    pub expansion: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyCandidate {
    pub text: String,
    pub occurrences: i64,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS vocabulary_candidate_decisions (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            normalized_text TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('dismissed', 'accepted')),
            updated_at TEXT NOT NULL,
            PRIMARY KEY(project_id, normalized_text)
        );",
    )?;
    Ok(())
}

impl Database {
    pub fn list_vocabulary_sets(&self, project_id: Option<&str>) -> AppResult<Vec<VocabularySet>> {
        self.ensure_project_exists(project_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT s.id, s.name, s.always_active, s.created_at, s.updated_at, s.deleted_at,
                    (SELECT COUNT(1) FROM vocabulary_terms t
                     WHERE t.set_id = s.id AND t.deleted_at IS NULL)
             FROM vocabulary_sets s
             WHERE s.deleted_at IS NULL AND (?1 IS NULL OR s.always_active = 1 OR EXISTS (
                SELECT 1 FROM project_vocabulary_sets pvs
                WHERE pvs.set_id = s.id AND pvs.project_id = ?1
             ))
             ORDER BY s.always_active DESC, s.name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([project_id], vocabulary_set_from_row)?;
        rows.map(|row| {
            let mut set = row?;
            set.project_ids = vocabulary_set_projects(&connection, &set.id)?;
            Ok(set)
        })
        .collect()
    }

    pub fn create_vocabulary_set(&self, input: VocabularySetInput) -> AppResult<VocabularySet> {
        let name = required(&input.name, "A vocabulary set name is required")?;
        for project_id in &input.project_ids {
            self.ensure_project_exists(Some(project_id))?;
        }
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO vocabulary_sets
                (id, name, always_active, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?4, NULL)",
            params![id, name, input.always_active, now],
        )?;
        for project_id in input.project_ids {
            transaction.execute(
                "INSERT INTO project_vocabulary_sets (project_id, set_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![project_id, id, now],
            )?;
        }
        transaction.commit()?;
        self.get_vocabulary_set(&id)
    }

    pub fn update_vocabulary_set(
        &self,
        id: &str,
        name: &str,
        always_active: bool,
        project_ids: Vec<String>,
    ) -> AppResult<VocabularySet> {
        self.get_vocabulary_set(id)?;
        let name = required(name, "A vocabulary set name is required")?;
        for project_id in &project_ids {
            self.ensure_project_exists(Some(project_id))?;
        }
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE vocabulary_sets SET name = ?2, always_active = ?3, updated_at = ?4
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, name, always_active, now],
        )?;
        transaction.execute(
            "DELETE FROM project_vocabulary_sets WHERE set_id = ?1",
            [id],
        )?;
        for project_id in project_ids {
            transaction.execute(
                "INSERT INTO project_vocabulary_sets (project_id, set_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![project_id, id, now],
            )?;
        }
        transaction.commit()?;
        self.get_vocabulary_set(id)
    }

    pub fn delete_vocabulary_set(&self, id: &str) -> AppResult<()> {
        self.get_vocabulary_set(id)?;
        self.connect()?.execute(
            "UPDATE vocabulary_sets SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_vocabulary_terms(&self, set_id: &str) -> AppResult<Vec<VocabularyTerm>> {
        self.get_vocabulary_set(set_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, set_id, canonical_form, expansion, definition, language, variants_json,
                    priority, created_at, updated_at, deleted_at
             FROM vocabulary_terms WHERE set_id = ?1 AND deleted_at IS NULL
             ORDER BY priority DESC, canonical_form COLLATE NOCASE",
        )?;
        let rows = statement.query_map([set_id], vocabulary_term_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_vocabulary_term(&self, input: VocabularyTermInput) -> AppResult<VocabularyTerm> {
        self.get_vocabulary_set(&input.set_id)?;
        let canonical = required(&input.canonical_form, "A canonical term is required")?;
        let language = required(&input.language, "A term language is required")?;
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        self.connect()?.execute(
            "INSERT INTO vocabulary_terms
                (id, set_id, canonical_form, expansion, definition, language, variants_json,
                 priority, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, NULL)",
            params![
                id,
                input.set_id,
                canonical,
                clean(input.expansion),
                clean(input.definition),
                language,
                serde_json::to_string(&unique_variants(input.variants, &canonical))?,
                input.priority.clamp(0, 100),
                now
            ],
        )?;
        self.get_vocabulary_term(&id)
    }

    pub fn update_vocabulary_term(
        &self,
        input: VocabularyTermInput,
        id: &str,
    ) -> AppResult<VocabularyTerm> {
        self.get_vocabulary_term(id)?;
        self.get_vocabulary_set(&input.set_id)?;
        let canonical = required(&input.canonical_form, "A canonical term is required")?;
        let language = required(&input.language, "A term language is required")?;
        self.connect()?.execute(
            "UPDATE vocabulary_terms SET set_id = ?2, canonical_form = ?3, expansion = ?4,
                definition = ?5, language = ?6, variants_json = ?7, priority = ?8,
                updated_at = ?9 WHERE id = ?1 AND deleted_at IS NULL",
            params![
                id,
                input.set_id,
                canonical,
                clean(input.expansion),
                clean(input.definition),
                language,
                serde_json::to_string(&unique_variants(input.variants, &canonical))?,
                input.priority.clamp(0, 100),
                Utc::now().to_rfc3339()
            ],
        )?;
        self.get_vocabulary_term(id)
    }

    pub fn delete_vocabulary_term(&self, id: &str) -> AppResult<()> {
        self.get_vocabulary_term(id)?;
        self.connect()?.execute(
            "UPDATE vocabulary_terms SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn vocabulary_candidates(&self, project_id: &str) -> AppResult<Vec<VocabularyCandidate>> {
        self.ensure_project_exists(Some(project_id))?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT ts.text FROM transcript_segments ts
             JOIN meeting_transcripts mt ON mt.id = ts.transcript_id
             JOIN meetings m ON m.id = mt.meeting_id
             WHERE m.project_id = ?1 AND m.deleted_at IS NULL",
        )?;
        let texts = statement
            .query_map([project_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let known = known_terms(&connection, project_id)?;
        let decided = candidate_decisions(&connection, project_id)?;
        let mut counts: HashMap<String, (String, i64)> = HashMap::new();
        for text in texts {
            for token in candidate_tokens(&text) {
                let normalized = normalize(&token);
                if known.contains(&normalized) || decided.contains(&normalized) {
                    continue;
                }
                let entry = counts.entry(normalized).or_insert((token, 0));
                entry.1 += 1;
            }
        }
        let mut candidates = counts
            .into_values()
            .filter(|(text, count)| *count >= 3 || looks_domain_specific(text))
            .map(|(text, occurrences)| VocabularyCandidate { text, occurrences })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .occurrences
                .cmp(&left.occurrences)
                .then_with(|| left.text.cmp(&right.text))
        });
        candidates.truncate(80);
        Ok(candidates)
    }

    pub fn dismiss_vocabulary_candidate(&self, project_id: &str, text: &str) -> AppResult<()> {
        self.ensure_project_exists(Some(project_id))?;
        let normalized = normalize(text);
        if normalized.is_empty() {
            return Err(AppError::InvalidInput(
                "A candidate term is required".into(),
            ));
        }
        self.connect()?.execute(
            "INSERT INTO vocabulary_candidate_decisions
                (project_id, normalized_text, status, updated_at)
             VALUES (?1, ?2, 'dismissed', ?3)
             ON CONFLICT(project_id, normalized_text) DO UPDATE SET
                status = 'dismissed', updated_at = excluded.updated_at",
            params![project_id, normalized, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn correct_transcript_segment(&self, id: &str, text: &str) -> AppResult<()> {
        let corrected = required(text, "Corrected transcript text is required")?;
        let connection = self.connect()?;
        let (original, project_id): (String, Option<String>) = connection
            .query_row(
                "SELECT ts.original_text, m.project_id FROM transcript_segments ts
                 JOIN meeting_transcripts mt ON mt.id = ts.transcript_id
                 JOIN meetings m ON m.id = mt.meeting_id WHERE ts.id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown transcript segment: {id}")))?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "UPDATE transcript_segments SET text = ?2 WHERE id = ?1",
            params![id, corrected],
        )?;
        connection.execute(
            "UPDATE meeting_transcripts SET updated_at = ?2 WHERE id = (
                SELECT transcript_id FROM transcript_segments WHERE id = ?1
             )",
            params![id, now],
        )?;
        if let Some(project_id) = project_id {
            learn_variants_from_correction(&connection, &project_id, &original, &corrected)?;
        }
        Ok(())
    }

    fn get_vocabulary_set(&self, id: &str) -> AppResult<VocabularySet> {
        let connection = self.connect()?;
        let mut set = connection
            .query_row(
                "SELECT s.id, s.name, s.always_active, s.created_at, s.updated_at, s.deleted_at,
                        (SELECT COUNT(1) FROM vocabulary_terms t
                         WHERE t.set_id = s.id AND t.deleted_at IS NULL)
                 FROM vocabulary_sets s WHERE s.id = ?1 AND s.deleted_at IS NULL",
                [id],
                vocabulary_set_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown vocabulary set: {id}")))?;
        set.project_ids = vocabulary_set_projects(&connection, id)?;
        Ok(set)
    }

    fn get_vocabulary_term(&self, id: &str) -> AppResult<VocabularyTerm> {
        self.connect()?
            .query_row(
                "SELECT id, set_id, canonical_form, expansion, definition, language, variants_json,
                        priority, created_at, updated_at, deleted_at
                 FROM vocabulary_terms WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                vocabulary_term_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown vocabulary term: {id}")))
    }
}

fn vocabulary_set_from_row(row: &Row<'_>) -> rusqlite::Result<VocabularySet> {
    Ok(VocabularySet {
        id: row.get(0)?,
        name: row.get(1)?,
        always_active: row.get(2)?,
        project_ids: Vec::new(),
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        deleted_at: row.get(5)?,
        term_count: row.get(6)?,
    })
}

fn vocabulary_term_from_row(row: &Row<'_>) -> rusqlite::Result<VocabularyTerm> {
    let variants_json: String = row.get(6)?;
    Ok(VocabularyTerm {
        id: row.get(0)?,
        set_id: row.get(1)?,
        canonical_form: row.get(2)?,
        expansion: row.get(3)?,
        definition: row.get(4)?,
        language: row.get(5)?,
        variants: serde_json::from_str(&variants_json).unwrap_or_default(),
        priority: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

fn vocabulary_set_projects(connection: &Connection, set_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT project_id FROM project_vocabulary_sets WHERE set_id = ?1 ORDER BY project_id",
    )?;
    let projects = statement.query_map([set_id], |row| row.get(0))?.collect();
    projects
}

fn known_terms(connection: &Connection, project_id: &str) -> AppResult<HashSet<String>> {
    let mut statement = connection.prepare(
        "SELECT t.canonical_form, t.variants_json FROM vocabulary_terms t
         JOIN vocabulary_sets s ON s.id = t.set_id
         LEFT JOIN project_vocabulary_sets pvs ON pvs.set_id = s.id
         WHERE t.deleted_at IS NULL AND s.deleted_at IS NULL
           AND (s.always_active = 1 OR pvs.project_id = ?1)",
    )?;
    let rows = statement.query_map([project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut known = HashSet::new();
    for row in rows {
        let (canonical, variants) = row?;
        known.insert(normalize(&canonical));
        for variant in serde_json::from_str::<Vec<String>>(&variants).unwrap_or_default() {
            known.insert(normalize(&variant));
        }
    }
    Ok(known)
}

fn candidate_decisions(connection: &Connection, project_id: &str) -> AppResult<HashSet<String>> {
    let mut statement = connection.prepare(
        "SELECT normalized_text FROM vocabulary_candidate_decisions WHERE project_id = ?1",
    )?;
    let decisions = statement
        .query_map([project_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(decisions)
}

fn candidate_tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace().filter_map(|raw| {
        let token = raw.trim_matches(|character: char| {
            !character.is_alphanumeric() && character != '-' && character != '_'
        });
        (token.chars().count() >= 3).then(|| token.to_string())
    })
}

fn looks_domain_specific(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_ascii_digit() || character == '_' || character == '-')
        || (text.len() >= 2
            && text
                .chars()
                .all(|character| !character.is_alphabetic() || character.is_uppercase()))
}

fn learn_variants_from_correction(
    connection: &Connection,
    project_id: &str,
    original: &str,
    corrected: &str,
) -> AppResult<()> {
    let original = candidate_tokens(original).collect::<Vec<_>>();
    let corrected = candidate_tokens(corrected).collect::<Vec<_>>();
    if original.len() != corrected.len() {
        return Ok(());
    }
    for (before, after) in original.iter().zip(corrected) {
        if normalize(before) == normalize(&after) {
            continue;
        }
        let row: Option<(String, String, String)> = connection
            .query_row(
                "SELECT t.id, t.canonical_form, t.variants_json FROM vocabulary_terms t
             JOIN vocabulary_sets s ON s.id = t.set_id
             LEFT JOIN project_vocabulary_sets pvs ON pvs.set_id = s.id
             WHERE t.deleted_at IS NULL AND s.deleted_at IS NULL
               AND (s.always_active = 1 OR pvs.project_id = ?1)
               AND lower(t.canonical_form) = lower(?2) LIMIT 1",
                params![project_id, after],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((id, canonical, variants_json)) = row {
            let mut variants =
                serde_json::from_str::<Vec<String>>(&variants_json).unwrap_or_default();
            variants.push(before.clone());
            connection.execute(
                "UPDATE vocabulary_terms SET variants_json = ?2, updated_at = ?3 WHERE id = ?1",
                params![
                    id,
                    serde_json::to_string(&unique_variants(variants, &canonical))?,
                    Utc::now().to_rfc3339()
                ],
            )?;
        }
    }
    Ok(())
}

fn unique_variants(values: Vec<String>, canonical: &str) -> Vec<String> {
    let canonical = normalize(canonical);
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter_map(|value| {
            let value = value.trim().to_string();
            let normalized = normalize(&value);
            (!value.is_empty() && normalized != canonical && seen.insert(normalized))
                .then_some(value)
        })
        .collect()
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn required(value: &str, message: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(AppError::InvalidInput(message.into()))
    } else {
        Ok(value.into())
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_language() -> String {
    "en".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{OrganizationInput, ProjectInput};

    fn database_and_project() -> (Database, String) {
        let directory =
            std::env::temp_dir().join(format!("threadbox-vocabulary-{}", Uuid::new_v4()));
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
                name: "Work".into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap();
        (database, project.id)
    }

    #[test]
    fn shares_a_set_and_deduplicates_variants() {
        let (database, project_id) = database_and_project();
        let set = database
            .create_vocabulary_set(VocabularySetInput {
                name: "Products".into(),
                always_active: false,
                project_ids: vec![project_id.clone()],
            })
            .unwrap();
        let term = database
            .create_vocabulary_term(VocabularyTermInput {
                set_id: set.id.clone(),
                canonical_form: "Threadbox".into(),
                expansion: None,
                definition: Some("Meeting assistant".into()),
                language: "en".into(),
                variants: vec!["thread box".into(), "thread box".into(), "Threadbox".into()],
                priority: 90,
            })
            .unwrap();
        assert_eq!(term.variants, vec!["thread box"]);
        assert_eq!(
            database.list_vocabulary_sets(Some(&project_id)).unwrap()[0].term_count,
            1
        );
    }

    #[test]
    fn harvests_candidates_and_learns_only_from_a_user_correction() {
        let (database, project_id) = database_and_project();
        let set = database
            .create_vocabulary_set(VocabularySetInput {
                name: "Products".into(),
                always_active: false,
                project_ids: vec![project_id.clone()],
            })
            .unwrap();
        database
            .create_vocabulary_term(VocabularyTermInput {
                set_id: set.id.clone(),
                canonical_form: "Threadbox".into(),
                expansion: None,
                definition: None,
                language: "en".into(),
                variants: Vec::new(),
                priority: 50,
            })
            .unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO meetings (id, project_id, title, status, created_at, updated_at)
             VALUES ('meeting', ?1, 'Review', 'recorded', 'old', 'old')",
                [&project_id],
            )
            .unwrap();
        connection.execute_batch(
            "INSERT INTO meeting_transcripts
                (id, meeting_id, microphone_language, system_language, model_id, prompt_version, created_at, updated_at)
             VALUES ('transcript', 'meeting', 'en', 'en', 'small', 'v1', 'old', 'old');
             INSERT INTO transcript_segments
                (id, transcript_id, channel, start_ms, end_ms, text, original_text, sequence)
             VALUES ('segment', 'transcript', 'microphone', 0, 1000,
                     'Tredbox RIS-42 RIS-42', 'Tredbox RIS-42 RIS-42', 0);",
        ).unwrap();
        drop(connection);

        let candidates = database.vocabulary_candidates(&project_id).unwrap();
        assert_eq!(candidates[0].text, "RIS-42");
        assert_eq!(candidates[0].occurrences, 2);
        assert_eq!(
            database.list_vocabulary_terms(&set.id).unwrap()[0].variants,
            Vec::<String>::new()
        );

        database
            .correct_transcript_segment("segment", "Threadbox RIS-42 RIS-42")
            .unwrap();
        let terms = database.list_vocabulary_terms(&set.id).unwrap();
        assert_eq!(terms[0].variants, vec!["Tredbox"]);
        let connection = database.connect().unwrap();
        let (text, original, updated_at): (String, String, String) = connection
            .query_row(
                "SELECT ts.text, ts.original_text, mt.updated_at FROM transcript_segments ts
             JOIN meeting_transcripts mt ON mt.id = ts.transcript_id WHERE ts.id = 'segment'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(text, "Threadbox RIS-42 RIS-42");
        assert_eq!(original, "Tredbox RIS-42 RIS-42");
        assert_ne!(updated_at, "old");
    }
}
