//! Organisations, projects, people, and the context scope a language model may draw on.
//!
//! Sharing between projects is a question of context, not permission: at a single user everything
//! is readable, and what matters is which projects' material is assembled when a model summarises a
//! meeting or explains a term. See `docs/domain-model.md`.

use std::collections::{BTreeSet, HashMap};

use chrono::Utc;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    database::{set_json_array, set_nullable, set_string, Database},
    error::{AppError, AppResult},
    providers,
    settings::AppSettings,
};

const SHARING_INHERIT: &str = "inherit";
const SHARING_ISOLATED: &str = "isolated";
const SHARING_SHARED: &str = "shared";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub context_sharing: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationInput {
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_organization_sharing")]
    pub context_sharing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub organization_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: String,
    pub context_sharing: String,
    /// The language spoken in this project's meetings, `auto` for detection, or `None` to take the
    /// nearest ancestor's answer and finally the global setting.
    pub language: Option<String>,
    /// The language its terminology is written in, when that differs from the spoken language.
    pub terminology_language: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub organization_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_project_sharing")]
    pub context_sharing: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub terminology_language: Option<String>,
}

/// The languages recognition should use for a project, with every fallback already applied.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLanguage {
    pub language: String,
    pub terminology_language: String,
    /// The project the answer came from, or `None` when it came from the global setting. Shown so a
    /// surprising language is traceable to where it was configured.
    pub inherited_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub display_name: String,
    /// Spoken and written variants of the name, used to recognise the person in a transcript.
    pub aliases: Vec<String>,
    pub email: Option<String>,
    pub notes: String,
    /// Marks the single record representing the user of this installation.
    pub is_self: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonInput {
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMember {
    pub role: Option<String>,
    pub person: Person,
}

fn default_organization_sharing() -> String {
    SHARING_ISOLATED.into()
}

fn default_project_sharing() -> String {
    SHARING_INHERIT.into()
}

/// Per-project recognition language, added after organisations and projects already existed.
pub(crate) fn migrate_language_columns(connection: &Connection) -> AppResult<()> {
    let columns = connection
        .prepare("PRAGMA table_info(projects)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "language") {
        connection.execute("ALTER TABLE projects ADD COLUMN language TEXT", [])?;
    }
    if !columns.iter().any(|name| name == "terminology_language") {
        connection.execute(
            "ALTER TABLE projects ADD COLUMN terminology_language TEXT",
            [],
        )?;
    }
    Ok(())
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS organizations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            context_sharing TEXT NOT NULL DEFAULT 'isolated'
                CHECK(context_sharing IN ('isolated', 'shared')),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            parent_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            context_sharing TEXT NOT NULL DEFAULT 'inherit'
                CHECK(context_sharing IN ('inherit', 'isolated', 'shared')),
            language TEXT,
            terminology_language TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_projects_organization ON projects(organization_id);
        CREATE INDEX IF NOT EXISTS idx_projects_parent ON projects(parent_id);
        CREATE TABLE IF NOT EXISTS project_context_links (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            linked_project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY (project_id, linked_project_id)
        );
        CREATE TABLE IF NOT EXISTS people (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            aliases_json TEXT NOT NULL DEFAULT '[]',
            email TEXT,
            notes TEXT NOT NULL DEFAULT '',
            is_self INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        );
        CREATE TABLE IF NOT EXISTS project_people (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
            role TEXT,
            created_at TEXT NOT NULL,
            PRIMARY KEY (project_id, person_id)
        );",
    )?;
    Ok(())
}

impl Database {
    pub fn create_organization(&self, input: OrganizationInput) -> AppResult<Organization> {
        let name = require_text(&input.name, "An organisation name is required")?;
        validate_organization_sharing(&input.context_sharing)?;
        let now = Utc::now().to_rfc3339();
        let organization = Organization {
            id: Uuid::new_v4().to_string(),
            name,
            notes: input.notes,
            context_sharing: input.context_sharing,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.connect()?.execute(
            "INSERT INTO organizations
                (id, name, notes, context_sharing, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                organization.id,
                organization.name,
                organization.notes,
                organization.context_sharing,
                organization.created_at,
                organization.updated_at
            ],
        )?;
        Ok(organization)
    }

    pub fn list_organizations(&self) -> AppResult<Vec<Organization>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, name, notes, context_sharing, created_at, updated_at, deleted_at
             FROM organizations WHERE deleted_at IS NULL ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], organization_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_organization(&self, id: &str) -> AppResult<Organization> {
        self.connect()?
            .query_row(
                "SELECT id, name, notes, context_sharing, created_at, updated_at, deleted_at
                 FROM organizations WHERE id = ?1",
                [id],
                organization_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown organisation: {id}")))
    }

    pub fn update_organization(&self, patch: Value) -> AppResult<Organization> {
        let object = patch.as_object().ok_or_else(|| {
            AppError::InvalidInput("The organisation update must be an object".into())
        })?;
        let id = required_id(object, "An organisation ID is required")?;
        let mut organization = self.get_organization(id)?;
        set_string(object, "name", &mut organization.name);
        set_string(object, "notes", &mut organization.notes);
        set_string(object, "contextSharing", &mut organization.context_sharing);
        organization.name = require_text(&organization.name, "An organisation name is required")?;
        validate_organization_sharing(&organization.context_sharing)?;
        organization.updated_at = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE organizations SET name = ?2, notes = ?3, context_sharing = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                organization.id,
                organization.name,
                organization.notes,
                organization.context_sharing,
                organization.updated_at
            ],
        )?;
        Ok(organization)
    }

    /// Soft deletes the organisation together with every project inside it.
    pub fn delete_organization(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE organizations SET deleted_at = ?2, updated_at = ?2
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        transaction.execute(
            "UPDATE projects SET deleted_at = ?2, updated_at = ?2
             WHERE organization_id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn create_project(&self, input: ProjectInput) -> AppResult<Project> {
        let name = require_text(&input.name, "A project name is required")?;
        validate_project_sharing(&input.context_sharing)?;
        let organization = self.get_organization(&input.organization_id)?;
        if organization.deleted_at.is_some() {
            return Err(AppError::InvalidInput(
                "That organisation has been deleted".into(),
            ));
        }
        if let Some(parent_id) = input.parent_id.as_deref() {
            let parent = self.get_project(parent_id)?;
            if parent.organization_id != organization.id {
                return Err(AppError::InvalidInput(
                    "A parent project must belong to the same organisation".into(),
                ));
            }
        }
        let now = Utc::now().to_rfc3339();
        let project = Project {
            id: Uuid::new_v4().to_string(),
            organization_id: organization.id,
            parent_id: input.parent_id,
            name,
            description: input.description,
            context_sharing: input.context_sharing,
            language: normalise_language(input.language)?,
            terminology_language: normalise_language(input.terminology_language)?,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.connect()?.execute(
            "INSERT INTO projects (id, organization_id, parent_id, name, description,
                context_sharing, language, terminology_language, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
            params![
                project.id,
                project.organization_id,
                project.parent_id,
                project.name,
                project.description,
                project.context_sharing,
                project.language,
                project.terminology_language,
                project.created_at,
                project.updated_at
            ],
        )?;
        Ok(project)
    }

    pub fn list_projects(&self, organization_id: Option<String>) -> AppResult<Vec<Project>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, organization_id, parent_id, name, description, context_sharing,
                    language, terminology_language, created_at, updated_at, deleted_at
             FROM projects
             WHERE deleted_at IS NULL AND (?1 IS NULL OR organization_id = ?1)
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([organization_id], project_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_project(&self, id: &str) -> AppResult<Project> {
        self.connect()?
            .query_row(
                "SELECT id, organization_id, parent_id, name, description, context_sharing,
                        language, terminology_language, created_at, updated_at, deleted_at
                 FROM projects WHERE id = ?1",
                [id],
                project_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown project: {id}")))
    }

    pub fn update_project(&self, patch: Value) -> AppResult<Project> {
        let object = patch
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("The project update must be an object".into()))?;
        let id = required_id(object, "A project ID is required")?;
        let mut project = self.get_project(id)?;
        if let Some(organization_id) = object.get("organizationId").and_then(Value::as_str) {
            if organization_id != project.organization_id {
                return Err(AppError::InvalidInput(
                    "Moving a project between organisations is not supported yet".into(),
                ));
            }
        }
        set_string(object, "name", &mut project.name);
        set_string(object, "description", &mut project.description);
        set_string(object, "contextSharing", &mut project.context_sharing);
        set_nullable(object, "parentId", &mut project.parent_id);
        set_nullable(object, "language", &mut project.language);
        set_nullable(
            object,
            "terminologyLanguage",
            &mut project.terminology_language,
        );
        project.name = require_text(&project.name, "A project name is required")?;
        validate_project_sharing(&project.context_sharing)?;
        project.language = normalise_language(project.language)?;
        project.terminology_language = normalise_language(project.terminology_language)?;
        self.validate_parent(&project)?;
        project.updated_at = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE projects SET parent_id = ?2, name = ?3, description = ?4,
                context_sharing = ?5, language = ?6, terminology_language = ?7,
                updated_at = ?8 WHERE id = ?1",
            params![
                project.id,
                project.parent_id,
                project.name,
                project.description,
                project.context_sharing,
                project.language,
                project.terminology_language,
                project.updated_at
            ],
        )?;
        Ok(project)
    }

    /// Soft deletes the project together with every project nested inside it.
    pub fn delete_project(&self, id: &str) -> AppResult<()> {
        let project = self.get_project(id)?;
        let projects = self.projects_in_organization(&project.organization_id)?;
        let mut doomed = descendants_of(&project.id, &projects);
        doomed.insert(project.id.clone());
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        {
            let mut statement = transaction.prepare(
                "UPDATE projects SET deleted_at = ?2, updated_at = ?2
                 WHERE id = ?1 AND deleted_at IS NULL",
            )?;
            for project_id in &doomed {
                statement.execute(params![project_id, now])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn ensure_project_exists(&self, project_id: Option<&str>) -> AppResult<()> {
        let Some(id) = project_id else {
            return Ok(());
        };
        let known: i64 = self.connect()?.query_row(
            "SELECT COUNT(1) FROM projects WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| row.get(0),
        )?;
        if known == 0 {
            return Err(AppError::InvalidInput(format!("Unknown project: {id}")));
        }
        Ok(())
    }

    pub fn create_person(&self, input: PersonInput) -> AppResult<Person> {
        let display_name = require_text(&input.display_name, "A person needs a name")?;
        let now = Utc::now().to_rfc3339();
        let person = Person {
            id: Uuid::new_v4().to_string(),
            display_name,
            aliases: normalise_aliases(input.aliases),
            email: input.email,
            notes: input.notes,
            is_self: input.is_self,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO people (id, display_name, aliases_json, email, notes, is_self,
                created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
            params![
                person.id,
                person.display_name,
                serde_json::to_string(&person.aliases)?,
                person.email,
                person.notes,
                person.is_self,
                person.created_at,
                person.updated_at
            ],
        )?;
        if person.is_self {
            clear_other_self_flags(&connection, &person.id)?;
        }
        Ok(person)
    }

    pub fn list_people(&self) -> AppResult<Vec<Person>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, display_name, aliases_json, email, notes, is_self,
                    created_at, updated_at, deleted_at
             FROM people WHERE deleted_at IS NULL ORDER BY display_name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], person_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_person(&self, id: &str) -> AppResult<Person> {
        self.connect()?
            .query_row(
                "SELECT id, display_name, aliases_json, email, notes, is_self,
                        created_at, updated_at, deleted_at
                 FROM people WHERE id = ?1",
                [id],
                person_from_row,
            )
            .map_err(|_| AppError::InvalidInput(format!("Unknown person: {id}")))
    }

    /// The record representing the user, used later to recognise being addressed by name.
    pub fn self_person(&self) -> AppResult<Option<Person>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, display_name, aliases_json, email, notes, is_self,
                    created_at, updated_at, deleted_at
             FROM people WHERE is_self = 1 AND deleted_at IS NULL LIMIT 1",
        )?;
        let mut rows = statement.query_map([], person_from_row)?;
        match rows.next() {
            Some(person) => Ok(Some(person?)),
            None => Ok(None),
        }
    }

    pub fn update_person(&self, patch: Value) -> AppResult<Person> {
        let object = patch
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("The person update must be an object".into()))?;
        let id = required_id(object, "A person ID is required")?;
        let mut person = self.get_person(id)?;
        set_string(object, "displayName", &mut person.display_name);
        set_string(object, "notes", &mut person.notes);
        set_nullable(object, "email", &mut person.email);
        set_json_array(object, "aliases", &mut person.aliases)?;
        if let Some(is_self) = object.get("isSelf").and_then(Value::as_bool) {
            person.is_self = is_self;
        }
        person.display_name = require_text(&person.display_name, "A person needs a name")?;
        person.aliases = normalise_aliases(std::mem::take(&mut person.aliases));
        person.updated_at = Utc::now().to_rfc3339();
        let connection = self.connect()?;
        connection.execute(
            "UPDATE people SET display_name = ?2, aliases_json = ?3, email = ?4, notes = ?5,
                is_self = ?6, updated_at = ?7 WHERE id = ?1",
            params![
                person.id,
                person.display_name,
                serde_json::to_string(&person.aliases)?,
                person.email,
                person.notes,
                person.is_self,
                person.updated_at
            ],
        )?;
        if person.is_self {
            clear_other_self_flags(&connection, &person.id)?;
        }
        Ok(person)
    }

    pub fn delete_person(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "UPDATE people SET deleted_at = ?2, updated_at = ?2, is_self = 0
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn add_project_person(
        &self,
        project_id: &str,
        person_id: &str,
        role: Option<String>,
    ) -> AppResult<()> {
        self.get_project(project_id)?;
        self.get_person(person_id)?;
        self.connect()?.execute(
            "INSERT INTO project_people (project_id, person_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, person_id) DO UPDATE SET role = ?3",
            params![project_id, person_id, role, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn remove_project_person(&self, project_id: &str, person_id: &str) -> AppResult<()> {
        self.connect()?.execute(
            "DELETE FROM project_people WHERE project_id = ?1 AND person_id = ?2",
            params![project_id, person_id],
        )?;
        Ok(())
    }

    pub fn list_project_people(&self, project_id: &str) -> AppResult<Vec<ProjectMember>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT people.id, people.display_name, people.aliases_json, people.email,
                    people.notes, people.is_self, people.created_at, people.updated_at,
                    people.deleted_at, project_people.role
             FROM project_people
             JOIN people ON people.id = project_people.person_id
             WHERE project_people.project_id = ?1 AND people.deleted_at IS NULL
             ORDER BY people.display_name COLLATE NOCASE, people.id",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok(ProjectMember {
                person: person_from_row(row)?,
                role: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Links two projects so each may draw on the other's material, across organisations if needed.
    pub fn link_projects(&self, project_id: &str, linked_project_id: &str) -> AppResult<()> {
        if project_id == linked_project_id {
            return Err(AppError::InvalidInput(
                "A project cannot be linked to itself".into(),
            ));
        }
        self.get_project(project_id)?;
        self.get_project(linked_project_id)?;
        self.connect()?.execute(
            "INSERT OR IGNORE INTO project_context_links
                (project_id, linked_project_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![project_id, linked_project_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn unlink_projects(&self, project_id: &str, linked_project_id: &str) -> AppResult<()> {
        self.connect()?.execute(
            "DELETE FROM project_context_links
             WHERE (project_id = ?1 AND linked_project_id = ?2)
                OR (project_id = ?2 AND linked_project_id = ?1)",
            params![project_id, linked_project_id],
        )?;
        Ok(())
    }

    pub fn list_project_links(&self, project_id: &str) -> AppResult<Vec<Project>> {
        let ids = self.linked_project_ids(project_id)?;
        let mut linked = Vec::with_capacity(ids.len());
        for id in ids {
            linked.push(self.get_project(&id)?);
        }
        Ok(linked)
    }

    /// Every project whose material may be used as context for the given project.
    ///
    /// Always the project itself, its ancestors and its nested projects. Other projects of the same
    /// organisation only when sharing is effectively enabled on both sides. Projects in another
    /// organisation only through an explicit link.
    pub fn project_context_scope(&self, project_id: &str) -> AppResult<Vec<String>> {
        let project = self.get_project(project_id)?;
        let organization = self.get_organization(&project.organization_id)?;
        let projects = self.projects_in_organization(&project.organization_id)?;
        let mut scope = BTreeSet::new();
        scope.insert(project.id.clone());
        let mut ancestor = project.parent_id.clone();
        while let Some(id) = ancestor {
            match projects.get(&id) {
                Some(found) => {
                    scope.insert(found.id.clone());
                    ancestor = found.parent_id.clone();
                }
                None => break,
            }
        }
        scope.extend(descendants_of(&project.id, &projects));
        if effective_sharing(&project, &projects, &organization.context_sharing) == SHARING_SHARED {
            for candidate in projects.values() {
                if effective_sharing(candidate, &projects, &organization.context_sharing)
                    == SHARING_SHARED
                {
                    scope.insert(candidate.id.clone());
                }
            }
        }
        scope.extend(self.linked_project_ids(&project.id)?);
        Ok(scope.into_iter().collect())
    }

    fn linked_project_ids(&self, project_id: &str) -> AppResult<Vec<String>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT CASE WHEN links.project_id = ?1 THEN links.linked_project_id
                         ELSE links.project_id END AS other_id
             FROM project_context_links AS links
             JOIN projects ON projects.id = CASE WHEN links.project_id = ?1
                     THEN links.linked_project_id ELSE links.project_id END
             WHERE (links.project_id = ?1 OR links.linked_project_id = ?1)
                AND projects.deleted_at IS NULL",
        )?;
        let rows = statement.query_map([project_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn projects_in_organization(
        &self,
        organization_id: &str,
    ) -> AppResult<HashMap<String, Project>> {
        Ok(self
            .list_projects(Some(organization_id.to_string()))?
            .into_iter()
            .map(|project| (project.id.clone(), project))
            .collect())
    }

    /// Resolves which language recognition should use for a project: its own setting, else the
    /// nearest ancestor that states one, else the global setting. The terminology language falls back
    /// to the spoken language, because stating it separately is the exception.
    pub fn project_language(&self, project_id: &str) -> AppResult<ProjectLanguage> {
        let project = self.get_project(project_id)?;
        let projects = self.projects_in_organization(&project.organization_id)?;
        let mut current = Some(project);
        let mut guard = std::collections::HashSet::new();
        while let Some(candidate) = current {
            if !guard.insert(candidate.id.clone()) {
                break;
            }
            if let Some(language) = candidate.language.as_deref() {
                return Ok(ProjectLanguage {
                    language: language.to_string(),
                    terminology_language: candidate
                        .terminology_language
                        .clone()
                        .unwrap_or_else(|| language.to_string()),
                    inherited_from: Some(candidate.id),
                });
            }
            current = candidate
                .parent_id
                .as_deref()
                .and_then(|parent_id| projects.get(parent_id).cloned());
        }
        let speech = AppSettings::load().unwrap_or_default().speech;
        Ok(ProjectLanguage {
            terminology_language: speech
                .terminology_language
                .unwrap_or_else(|| speech.language.clone()),
            language: speech.language,
            inherited_from: None,
        })
    }

    fn validate_parent(&self, project: &Project) -> AppResult<()> {
        let Some(parent_id) = project.parent_id.as_deref() else {
            return Ok(());
        };
        if parent_id == project.id {
            return Err(AppError::InvalidInput(
                "A project cannot be its own parent".into(),
            ));
        }
        let parent = self.get_project(parent_id)?;
        if parent.organization_id != project.organization_id {
            return Err(AppError::InvalidInput(
                "A parent project must belong to the same organisation".into(),
            ));
        }
        let projects = self.projects_in_organization(&project.organization_id)?;
        if descendants_of(&project.id, &projects).contains(parent_id) {
            return Err(AppError::InvalidInput(
                "That parent is nested inside this project".into(),
            ));
        }
        Ok(())
    }
}

fn descendants_of(project_id: &str, projects: &HashMap<String, Project>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut frontier = vec![project_id.to_string()];
    while let Some(current) = frontier.pop() {
        for candidate in projects
            .values()
            .filter(|project| project.parent_id.as_deref() == Some(current.as_str()))
        {
            if found.insert(candidate.id.clone()) {
                frontier.push(candidate.id.clone());
            }
        }
    }
    found
}

/// Resolves `inherit` against the nearest ancestor that states a preference, then the organisation.
fn effective_sharing(
    project: &Project,
    projects: &HashMap<String, Project>,
    organization_sharing: &str,
) -> String {
    let mut current = Some(project);
    while let Some(found) = current {
        if found.context_sharing != SHARING_INHERIT {
            return found.context_sharing.clone();
        }
        current = found
            .parent_id
            .as_deref()
            .and_then(|parent_id| projects.get(parent_id));
    }
    organization_sharing.to_string()
}

fn clear_other_self_flags(connection: &Connection, person_id: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE people SET is_self = 0 WHERE id != ?1 AND is_self = 1",
        [person_id],
    )?;
    Ok(())
}

fn normalise_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    aliases
        .into_iter()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .filter(|alias| seen.insert(alias.to_lowercase()))
        .collect()
}

fn require_text(value: &str, message: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(message.into()));
    }
    Ok(trimmed.to_string())
}

fn required_id<'a>(
    object: &'a serde_json::Map<String, Value>,
    message: &str,
) -> AppResult<&'a str> {
    object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput(message.into()))
}

fn validate_organization_sharing(value: &str) -> AppResult<()> {
    if [SHARING_ISOLATED, SHARING_SHARED].contains(&value) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Unknown organisation context sharing: {value}"
        )))
    }
}

/// An empty string from a form means "not set" rather than an invalid language.
fn normalise_language(value: Option<String>) -> AppResult<Option<String>> {
    let Some(language) = value else {
        return Ok(None);
    };
    let language = language.trim().to_ascii_lowercase();
    if language.is_empty() {
        return Ok(None);
    }
    providers::validate_speech_language(&language)?;
    Ok(Some(language))
}

fn validate_project_sharing(value: &str) -> AppResult<()> {
    if [SHARING_INHERIT, SHARING_ISOLATED, SHARING_SHARED].contains(&value) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "Unknown project context sharing: {value}"
        )))
    }
}

fn organization_from_row(row: &Row<'_>) -> rusqlite::Result<Organization> {
    Ok(Organization {
        id: row.get(0)?,
        name: row.get(1)?,
        notes: row.get(2)?,
        context_sharing: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
    })
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        organization_id: row.get(1)?,
        parent_id: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        context_sharing: row.get(5)?,
        language: row.get(6)?,
        terminology_language: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

fn person_from_row(row: &Row<'_>) -> rusqlite::Result<Person> {
    let aliases = row
        .get::<_, String>(2)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default();
    Ok(Person {
        id: row.get(0)?,
        display_name: row.get(1)?,
        aliases,
        email: row.get(3)?,
        notes: row.get(4)?,
        is_self: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::TaskInput;
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
                "threadbox-workspace-{}-{suffix}-{sequence}.sqlite3",
                std::process::id()
            )),
        };
        database.migrate().unwrap();
        database
    }

    fn organization(database: &Database, name: &str, sharing: &str) -> Organization {
        database
            .create_organization(OrganizationInput {
                name: name.into(),
                notes: String::new(),
                context_sharing: sharing.into(),
            })
            .unwrap()
    }

    fn project(
        database: &Database,
        organization_id: &str,
        name: &str,
        parent_id: Option<&str>,
        sharing: &str,
    ) -> Project {
        database
            .create_project(ProjectInput {
                organization_id: organization_id.into(),
                parent_id: parent_id.map(Into::into),
                name: name.into(),
                description: String::new(),
                context_sharing: sharing.into(),
                language: None,
                terminology_language: None,
            })
            .unwrap()
    }

    fn task_input(title: &str, project_id: Option<&str>) -> TaskInput {
        TaskInput {
            title: title.into(),
            notes: String::new(),
            status: "inbox".into(),
            priority: "mid".into(),
            project_id: project_id.map(Into::into),
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
    fn nests_projects_within_one_organisation_only() {
        let database = test_database();
        let aptvision = organization(&database, "Aptvision", SHARING_ISOLATED);
        let other = organization(&database, "Other client", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &aptvision.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let workstream = project(
            &database,
            &aptvision.id,
            "Booking flow",
            Some(&engage_hub.id),
            SHARING_INHERIT,
        );
        assert_eq!(
            workstream.parent_id.as_deref(),
            Some(engage_hub.id.as_str())
        );

        let rejected = database.create_project(ProjectInput {
            organization_id: other.id,
            parent_id: Some(engage_hub.id),
            name: "Wrong organisation".into(),
            description: String::new(),
            context_sharing: SHARING_INHERIT.into(),
            language: None,
            terminology_language: None,
        });
        assert!(rejected.is_err());
    }

    #[test]
    fn rejects_a_parent_nested_inside_the_project() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_ISOLATED);
        let parent = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let child = project(
            &database,
            &organization.id,
            "Booking flow",
            Some(&parent.id),
            SHARING_INHERIT,
        );
        let rejected = database.update_project(serde_json::json!({
            "id": parent.id,
            "parentId": child.id,
        }));
        assert!(rejected.is_err());
    }

    #[test]
    fn an_isolated_organisation_keeps_context_inside_the_project_tree() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let workstream = project(
            &database,
            &organization.id,
            "Booking flow",
            Some(&engage_hub.id),
            SHARING_INHERIT,
        );
        let nhs = project(&database, &organization.id, "NHS", None, SHARING_INHERIT);

        let scope = database.project_context_scope(&engage_hub.id).unwrap();
        assert!(scope.contains(&engage_hub.id));
        assert!(scope.contains(&workstream.id));
        assert!(!scope.contains(&nhs.id));

        let nested_scope = database.project_context_scope(&workstream.id).unwrap();
        assert!(nested_scope.contains(&engage_hub.id));
        assert!(nested_scope.contains(&workstream.id));
    }

    #[test]
    fn a_shared_organisation_widens_context_and_an_isolated_project_stays_out() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_SHARED);
        let engage_hub = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let nhs = project(&database, &organization.id, "NHS", None, SHARING_INHERIT);
        let confidential = project(
            &database,
            &organization.id,
            "Acquisition",
            None,
            SHARING_ISOLATED,
        );

        let scope = database.project_context_scope(&engage_hub.id).unwrap();
        assert!(scope.contains(&nhs.id));
        assert!(!scope.contains(&confidential.id));

        let confidential_scope = database.project_context_scope(&confidential.id).unwrap();
        assert_eq!(confidential_scope, vec![confidential.id]);
    }

    #[test]
    fn an_explicit_link_shares_context_across_organisations() {
        let database = test_database();
        let aptvision = organization(&database, "Aptvision", SHARING_ISOLATED);
        let supplier = organization(&database, "Supplier", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &aptvision.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let integration = project(
            &database,
            &supplier.id,
            "Integration",
            None,
            SHARING_INHERIT,
        );

        assert!(!database
            .project_context_scope(&engage_hub.id)
            .unwrap()
            .contains(&integration.id));

        database
            .link_projects(&engage_hub.id, &integration.id)
            .unwrap();

        assert!(database
            .project_context_scope(&engage_hub.id)
            .unwrap()
            .contains(&integration.id));
        assert!(database
            .project_context_scope(&integration.id)
            .unwrap()
            .contains(&engage_hub.id));
        assert_eq!(
            database.list_project_links(&integration.id).unwrap()[0].id,
            engage_hub.id
        );

        database
            .unlink_projects(&integration.id, &engage_hub.id)
            .unwrap();
        assert!(!database
            .project_context_scope(&engage_hub.id)
            .unwrap()
            .contains(&integration.id));
    }

    #[test]
    fn deleting_a_project_removes_the_projects_nested_inside_it() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_ISOLATED);
        let parent = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let child = project(
            &database,
            &organization.id,
            "Booking flow",
            Some(&parent.id),
            SHARING_INHERIT,
        );
        let unrelated = project(&database, &organization.id, "NHS", None, SHARING_INHERIT);

        database.delete_project(&parent.id).unwrap();

        let remaining = database.list_projects(None).unwrap();
        let remaining_ids = remaining
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(remaining_ids, vec![unrelated.id.as_str()]);
        assert!(database
            .get_project(&child.id)
            .unwrap()
            .deleted_at
            .is_some());
    }

    #[test]
    fn only_one_person_is_marked_as_the_user() {
        let database = test_database();
        let first = database
            .create_person(PersonInput {
                display_name: "Pawel".into(),
                aliases: vec!["Paul".into(), " paul ".into(), String::new()],
                email: None,
                notes: String::new(),
                is_self: true,
            })
            .unwrap();
        assert_eq!(first.aliases, vec!["Paul".to_string()]);
        assert_eq!(database.self_person().unwrap().unwrap().id, first.id);

        let second = database
            .create_person(PersonInput {
                display_name: "Aidan".into(),
                aliases: Vec::new(),
                email: None,
                notes: String::new(),
                is_self: true,
            })
            .unwrap();
        assert_eq!(database.self_person().unwrap().unwrap().id, second.id);
        assert!(!database.get_person(&first.id).unwrap().is_self);
    }

    #[test]
    fn project_membership_carries_a_role() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let person = database
            .create_person(PersonInput {
                display_name: "Manju".into(),
                aliases: Vec::new(),
                email: None,
                notes: String::new(),
                is_self: false,
            })
            .unwrap();

        database
            .add_project_person(
                &engage_hub.id,
                &person.id,
                Some("Quality specialist".into()),
            )
            .unwrap();
        let members = database.list_project_people(&engage_hub.id).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role.as_deref(), Some("Quality specialist"));
        assert_eq!(members[0].person.display_name, "Manju");

        database
            .add_project_person(&engage_hub.id, &person.id, Some("Validation lead".into()))
            .unwrap();
        let members = database.list_project_people(&engage_hub.id).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role.as_deref(), Some("Validation lead"));

        database
            .remove_project_person(&engage_hub.id, &person.id)
            .unwrap();
        assert!(database
            .list_project_people(&engage_hub.id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_task_only_references_a_known_project() {
        let database = test_database();
        let organization = organization(&database, "Aptvision", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &organization.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );

        let filed = database
            .create_task(task_input("Send the validation plan", Some(&engage_hub.id)))
            .unwrap();
        assert_eq!(filed.project_id.as_deref(), Some(engage_hub.id.as_str()));
        assert_eq!(
            database.get_task(&filed.id).unwrap().project_id.as_deref(),
            Some(engage_hub.id.as_str())
        );

        let inbox = database.create_task(task_input("Unfiled", None)).unwrap();
        assert!(inbox.project_id.is_none());

        assert!(database
            .create_task(task_input("Bad project", Some("missing")))
            .is_err());

        database.delete_project(&engage_hub.id).unwrap();
        assert!(database
            .update_task(serde_json::json!({
                "id": inbox.id,
                "projectId": engage_hub.id,
            }))
            .is_err());
    }

    #[test]
    fn a_project_takes_its_language_from_the_nearest_ancestor_that_states_one() {
        let database = test_database();
        let aptvision = organization(&database, "Aptvision", SHARING_SHARED);
        let engage_hub = project(
            &database,
            &aptvision.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        let api = project(
            &database,
            &aptvision.id,
            "API",
            Some(&engage_hub.id),
            SHARING_INHERIT,
        );

        // Nothing stated anywhere: the global setting answers, and detection is its default.
        let resolved = database.project_language(&api.id).unwrap();
        assert_eq!(resolved.language, "auto");
        assert_eq!(resolved.terminology_language, "auto");
        assert!(resolved.inherited_from.is_none());

        database
            .update_project(serde_json::json!({
                "id": engage_hub.id,
                "language": "pl",
                "terminologyLanguage": "en",
            }))
            .unwrap();
        let resolved = database.project_language(&api.id).unwrap();
        assert_eq!(resolved.language, "pl");
        assert_eq!(
            resolved.terminology_language, "en",
            "Polish meetings about English terminology are the normal case here"
        );
        assert_eq!(
            resolved.inherited_from.as_deref(),
            Some(engage_hub.id.as_str())
        );

        // A project that states its own language stops the walk.
        database
            .update_project(serde_json::json!({ "id": api.id, "language": "en" }))
            .unwrap();
        let resolved = database.project_language(&api.id).unwrap();
        assert_eq!(resolved.language, "en");
        assert_eq!(
            resolved.terminology_language, "en",
            "stating only the spoken language means the terminology is in it too"
        );
        assert_eq!(resolved.inherited_from.as_deref(), Some(api.id.as_str()));
    }

    #[test]
    fn an_empty_language_clears_it_and_a_wrong_one_is_refused() {
        let database = test_database();
        let aptvision = organization(&database, "Aptvision", SHARING_ISOLATED);
        let engage_hub = project(
            &database,
            &aptvision.id,
            "Engage Hub",
            None,
            SHARING_INHERIT,
        );
        assert!(engage_hub.language.is_none());

        let stated = database
            .update_project(serde_json::json!({ "id": engage_hub.id, "language": "PL" }))
            .unwrap();
        assert_eq!(
            stated.language.as_deref(),
            Some("pl"),
            "a code typed in capitals is still that code"
        );

        let cleared = database
            .update_project(serde_json::json!({ "id": engage_hub.id, "language": "  " }))
            .unwrap();
        assert!(
            cleared.language.is_none(),
            "an empty field means unset, not an invalid language"
        );

        assert!(database
            .update_project(serde_json::json!({ "id": engage_hub.id, "language": "Polish" }))
            .is_err());
    }
}
