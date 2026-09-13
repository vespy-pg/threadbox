//! Provider-neutral records for external accounts and actions.
//!
//! Provider adapters use these contracts instead of writing provider-specific state into projects.
//! Credentials never enter this module or SQLite; only the operating-system keyring stores them.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppResult;

pub const CAPABILITY_MAIL_METADATA_READ: &str = "mail_metadata_read";
pub const CAPABILITY_MAIL_CONTENT_READ: &str = "mail_content_read";
pub const CAPABILITY_MAIL_SEND: &str = "mail_send";
pub const CAPABILITY_CALENDAR_READ: &str = "calendar_read";
pub const CAPABILITY_CALENDAR_WRITE: &str = "calendar_write";
pub const CAPABILITY_CALENDAR_FREE_BUSY: &str = "calendar_free_busy";
pub const CAPABILITY_SPEECH_CLOUD: &str = "speech_cloud";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConnection {
    pub id: String,
    pub organization_id: String,
    pub provider: String,
    pub account_identifier: String,
    pub display_name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConnectionInput {
    pub organization_id: String,
    pub provider: String,
    pub account_identifier: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationCapability {
    pub connection_id: String,
    pub capability: String,
    pub status: String,
    pub provider_scope: Option<String>,
    pub granted_at: Option<String>,
    pub revoked_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalActionInput {
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: String,
    pub capability: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAction {
    pub id: String,
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: String,
    pub capability: String,
    pub kind: String,
    pub status: String,
    pub payload: Value,
    pub approved_payload: Option<Value>,
    pub scheduled_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub(crate) fn migrate_schema(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS integration_connections (
            id TEXT PRIMARY KEY,
            organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            provider TEXT NOT NULL,
            account_identifier TEXT NOT NULL,
            display_name TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('connected', 'degraded', 'disconnected')),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            UNIQUE(organization_id, provider, account_identifier)
        );
        CREATE INDEX IF NOT EXISTS idx_integration_connections_organization
            ON integration_connections(organization_id, deleted_at);
        CREATE TABLE IF NOT EXISTS integration_capabilities (
            connection_id TEXT NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
            capability TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('granted', 'revoked')),
            provider_scope TEXT,
            granted_at TEXT,
            revoked_at TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(connection_id, capability)
        );
        CREATE TABLE IF NOT EXISTS external_objects (
            id TEXT PRIMARY KEY,
            connection_id TEXT NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
            object_kind TEXT NOT NULL,
            external_id TEXT NOT NULL,
            project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
            meeting_id TEXT REFERENCES meetings(id) ON DELETE SET NULL,
            person_id TEXT REFERENCES people(id) ON DELETE SET NULL,
            etag TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            UNIQUE(connection_id, object_kind, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_external_objects_project ON external_objects(project_id, object_kind);
        CREATE TABLE IF NOT EXISTS sync_cursors (
            connection_id TEXT NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
            resource_kind TEXT NOT NULL,
            cursor TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(connection_id, resource_kind)
        );
        CREATE TABLE IF NOT EXISTS external_actions (
            id TEXT PRIMARY KEY,
            organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
            connection_id TEXT NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
            capability TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN
                ('draft', 'awaiting_approval', 'approved', 'queued', 'sending', 'sent', 'failed', 'cancelled')),
            payload_json TEXT NOT NULL,
            approved_payload_json TEXT,
            scheduled_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_external_actions_project ON external_actions(project_id, status, created_at);
        CREATE INDEX IF NOT EXISTS idx_external_actions_queue ON external_actions(status, scheduled_at, created_at);
        CREATE TABLE IF NOT EXISTS execution_attempts (
            id TEXT PRIMARY KEY,
            action_id TEXT NOT NULL REFERENCES external_actions(id) ON DELETE CASCADE,
            attempt_number INTEGER NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('started', 'succeeded', 'failed')),
            provider_receipt TEXT,
            safe_error TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            UNIQUE(action_id, attempt_number)
        );
        CREATE TRIGGER IF NOT EXISTS external_actions_connection_guard
        BEFORE INSERT ON external_actions
        WHEN NOT EXISTS (
            SELECT 1 FROM integration_connections
            WHERE id=NEW.connection_id
              AND organization_id=NEW.organization_id
              AND status='connected'
              AND deleted_at IS NULL
        )
        BEGIN
            SELECT RAISE(ABORT, 'integration connection is not active for this organisation');
        END;
        CREATE TRIGGER IF NOT EXISTS external_actions_capability_guard
        BEFORE INSERT ON external_actions
        WHEN NOT EXISTS (
            SELECT 1 FROM integration_capabilities
            WHERE connection_id=NEW.connection_id
              AND capability=NEW.capability
              AND status='granted'
        )
        BEGIN
            SELECT RAISE(ABORT, 'integration capability is not granted');
        END;
        CREATE TRIGGER IF NOT EXISTS external_actions_project_guard
        BEFORE INSERT ON external_actions
        WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM projects
            WHERE id=NEW.project_id
              AND organization_id=NEW.organization_id
              AND deleted_at IS NULL
        )
        BEGIN
            SELECT RAISE(ABORT, 'project is not active for this organisation');
        END;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::Database, workspace::OrganizationInput};
    use rusqlite::params;
    use uuid::Uuid;

    fn database() -> Database {
        let root = std::env::temp_dir().join(format!("threadbox-integrations-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let database = Database {
            path: root.join("threadbox.db"),
        };
        database.migrate().unwrap();
        database
    }

    fn organization(database: &Database) -> String {
        database
            .create_organization(OrganizationInput {
                name: "Vespy".into(),
                notes: String::new(),
                context_sharing: "isolated".into(),
            })
            .unwrap()
            .id
    }

    fn integration(database: &Database, organization_id: &str) -> String {
        let id = Uuid::new_v4().to_string();
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO integration_connections
                 (id, organization_id, provider, account_identifier, display_name, status,
                  created_at, updated_at)
                 VALUES (?1, ?2, 'google', 'owner@example.com', 'Work Google', 'connected',
                         'now', 'now')",
                params![id, organization_id],
            )
            .unwrap();
        id
    }

    fn set_capability(database: &Database, connection_id: &str, capability: &str, status: &str) {
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO integration_capabilities
                 (connection_id, capability, status, provider_scope, granted_at, revoked_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'now', NULL, 'now')
                 ON CONFLICT(connection_id, capability) DO UPDATE SET status=excluded.status",
                params![connection_id, capability, status, capability],
            )
            .unwrap();
    }

    #[test]
    fn migration_creates_the_integration_foundation() {
        let database = database();
        let connection = database.connect().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 10);
        for table in [
            "integration_connections",
            "integration_capabilities",
            "external_objects",
            "sync_cursors",
            "external_actions",
            "execution_attempts",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing {table}");
        }
    }

    #[test]
    fn capabilities_are_granted_and_revoked_independently() {
        let database = database();
        let organization_id = organization(&database);
        let account_id = integration(&database, &organization_id);
        set_capability(
            &database,
            &account_id,
            CAPABILITY_MAIL_CONTENT_READ,
            "granted",
        );
        set_capability(&database, &account_id, CAPABILITY_MAIL_SEND, "granted");
        set_capability(
            &database,
            &account_id,
            CAPABILITY_MAIL_CONTENT_READ,
            "revoked",
        );
        let connection = database.connect().unwrap();
        let read_status: String = connection
            .query_row(
                "SELECT status FROM integration_capabilities
                 WHERE connection_id=?1 AND capability=?2",
                params![account_id, CAPABILITY_MAIL_CONTENT_READ],
                |row| row.get(0),
            )
            .unwrap();
        let send_status: String = connection
            .query_row(
                "SELECT status FROM integration_capabilities
                 WHERE connection_id=?1 AND capability=?2",
                params![account_id, CAPABILITY_MAIL_SEND],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(read_status, "revoked");
        assert_eq!(send_status, "granted");
    }

    #[test]
    fn an_external_action_requires_its_exact_capability() {
        let database = database();
        let organization_id = organization(&database);
        let account_id = integration(&database, &organization_id);
        let insert = || {
            database.connect().unwrap().execute(
                "INSERT INTO external_actions
                 (id, organization_id, connection_id, capability, kind, status, payload_json,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'mail.send', 'draft', '{}', 'now', 'now')",
                params![
                    Uuid::new_v4().to_string(),
                    organization_id,
                    account_id,
                    CAPABILITY_MAIL_SEND
                ],
            )
        };
        assert!(insert().is_err());
        set_capability(&database, &account_id, CAPABILITY_MAIL_SEND, "granted");
        assert_eq!(insert().unwrap(), 1);
    }
}
