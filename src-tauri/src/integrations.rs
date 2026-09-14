//! Provider-neutral records for external accounts and actions.
//!
//! Provider adapters use these contracts instead of writing provider-specific state into projects.
//! Credentials never enter this module or SQLite; only the operating-system keyring stores them.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    database::Database,
    error::{AppError, AppResult},
};

pub const CAPABILITY_MAIL_METADATA_READ: &str = "mail_metadata_read";
pub const CAPABILITY_MAIL_CONTENT_READ: &str = "mail_content_read";
pub const CAPABILITY_MAIL_COMPOSE: &str = "mail_compose";
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSnapshot {
    pub connection: IntegrationConnection,
    pub capabilities: Vec<IntegrationCapability>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyReceiptInput {
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: Option<String>,
    pub provider: String,
    pub operation: String,
    pub reason: String,
    pub data_categories: Vec<String>,
    pub destination: String,
    pub byte_count: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyReceipt {
    pub id: String,
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: Option<String>,
    pub provider: String,
    pub operation: String,
    pub reason: String,
    pub data_categories: Vec<String>,
    pub destination: String,
    pub byte_count: Option<i64>,
    pub created_at: String,
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
        CREATE TABLE IF NOT EXISTS privacy_receipts (
            id TEXT PRIMARY KEY,
            organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
            connection_id TEXT REFERENCES integration_connections(id) ON DELETE SET NULL,
            provider TEXT NOT NULL,
            operation TEXT NOT NULL,
            reason TEXT NOT NULL,
            data_categories_json TEXT NOT NULL,
            destination TEXT NOT NULL,
            byte_count INTEGER,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_privacy_receipts_organization
            ON privacy_receipts(organization_id, created_at DESC);
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

impl Database {
    pub fn integration_snapshots(
        &self,
        organization_id: &str,
    ) -> AppResult<Vec<IntegrationSnapshot>> {
        self.get_organization(organization_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, organization_id, provider, account_identifier, display_name, status,
                    created_at, updated_at, deleted_at
             FROM integration_connections
             WHERE organization_id = ?1 AND deleted_at IS NULL ORDER BY display_name",
        )?;
        let connections = statement
            .query_map([organization_id], integration_connection_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        connections
            .into_iter()
            .map(|item| {
                Ok(IntegrationSnapshot {
                    capabilities: integration_capabilities(&connection, &item.id)?,
                    connection: item,
                })
            })
            .collect()
    }

    pub fn upsert_integration_connection(
        &self,
        input: IntegrationConnectionInput,
    ) -> AppResult<IntegrationSnapshot> {
        self.get_organization(&input.organization_id)?;
        if input.provider.trim().is_empty() || input.account_identifier.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "An integration provider and account identifier are required".into(),
            ));
        }
        let connection = self.connect()?;
        let now = Utc::now().to_rfc3339();
        let existing_id = connection
            .query_row(
                "SELECT id FROM integration_connections
                 WHERE organization_id = ?1 AND provider = ?2 AND account_identifier = ?3",
                params![
                    input.organization_id,
                    input.provider,
                    input.account_identifier
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        connection.execute(
            "INSERT INTO integration_connections
                (id, organization_id, provider, account_identifier, display_name, status,
                 created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'connected', ?6, ?6, NULL)
             ON CONFLICT(organization_id, provider, account_identifier) DO UPDATE SET
                display_name = excluded.display_name, status = 'connected',
                updated_at = excluded.updated_at, deleted_at = NULL",
            params![
                id,
                input.organization_id,
                input.provider,
                input.account_identifier,
                input.display_name,
                now
            ],
        )?;
        drop(connection);
        self.integration_snapshot(&id)
    }

    pub fn set_integration_capability(
        &self,
        connection_id: &str,
        capability: &str,
        provider_scope: &str,
        granted: bool,
    ) -> AppResult<IntegrationSnapshot> {
        let now = Utc::now().to_rfc3339();
        let connection = self.connect()?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM integration_connections
             WHERE id = ?1 AND status = 'connected' AND deleted_at IS NULL)",
            [connection_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::InvalidInput(
                "That integration connection is not active".into(),
            ));
        }
        let (status, granted_at, revoked_at) = if granted {
            ("granted", Some(now.as_str()), None)
        } else {
            ("revoked", None, Some(now.as_str()))
        };
        connection.execute(
            "INSERT INTO integration_capabilities
                (connection_id, capability, status, provider_scope, granted_at, revoked_at,
                 updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(connection_id, capability) DO UPDATE SET status = excluded.status,
                provider_scope = excluded.provider_scope,
                granted_at = CASE WHEN excluded.status = 'granted' THEN excluded.granted_at
                                  ELSE integration_capabilities.granted_at END,
                revoked_at = excluded.revoked_at, updated_at = excluded.updated_at",
            params![
                connection_id,
                capability,
                status,
                provider_scope,
                granted_at,
                revoked_at,
                now
            ],
        )?;
        drop(connection);
        self.integration_snapshot(connection_id)
    }

    pub fn disconnect_integration(&self, connection_id: &str) -> AppResult<()> {
        let changed = self.connect()?.execute(
            "UPDATE integration_connections SET status = 'disconnected', deleted_at = ?2,
                updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![connection_id, Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "That integration connection does not exist".into(),
            ));
        }
        Ok(())
    }

    pub fn integration_snapshot(&self, connection_id: &str) -> AppResult<IntegrationSnapshot> {
        let connection = self.connect()?;
        let item = connection
            .query_row(
                "SELECT id, organization_id, provider, account_identifier, display_name, status,
                        created_at, updated_at, deleted_at
                 FROM integration_connections WHERE id = ?1 AND deleted_at IS NULL",
                [connection_id],
                integration_connection_from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("Integration connection not found".into()))?;
        Ok(IntegrationSnapshot {
            capabilities: integration_capabilities(&connection, connection_id)?,
            connection: item,
        })
    }

    pub fn require_integration_capability(
        &self,
        connection_id: &str,
        capability: &str,
    ) -> AppResult<IntegrationSnapshot> {
        let snapshot = self.integration_snapshot(connection_id)?;
        if snapshot.connection.status != "connected"
            || !snapshot
                .capabilities
                .iter()
                .any(|item| item.capability == capability && item.status == "granted")
        {
            return Err(AppError::InvalidInput(format!(
                "The integration has not granted {capability}"
            )));
        }
        Ok(snapshot)
    }

    pub fn has_integration_capability(
        &self,
        connection_id: &str,
        capability: &str,
    ) -> AppResult<bool> {
        let snapshot = self.integration_snapshot(connection_id)?;
        Ok(snapshot.connection.status == "connected"
            && snapshot
                .capabilities
                .iter()
                .any(|item| item.capability == capability && item.status == "granted"))
    }

    pub fn integration_sync_cursor(
        &self,
        connection_id: &str,
        resource_kind: &str,
    ) -> AppResult<Option<String>> {
        Ok(self
            .connect()?
            .query_row(
                "SELECT cursor FROM sync_cursors WHERE connection_id=?1 AND resource_kind=?2",
                params![connection_id, resource_kind],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_integration_sync_cursor(
        &self,
        connection_id: &str,
        resource_kind: &str,
        cursor: &str,
    ) -> AppResult<()> {
        self.integration_snapshot(connection_id)?;
        self.connect()?.execute(
            "INSERT INTO sync_cursors (connection_id, resource_kind, cursor, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(connection_id, resource_kind) DO UPDATE SET
                cursor=excluded.cursor, updated_at=excluded.updated_at",
            params![
                connection_id,
                resource_kind,
                cursor,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn create_approved_external_action(
        &self,
        input: &ExternalActionInput,
    ) -> AppResult<ExternalAction> {
        let now = Utc::now().to_rfc3339();
        let action = ExternalAction {
            id: Uuid::new_v4().to_string(),
            organization_id: input.organization_id.clone(),
            project_id: input.project_id.clone(),
            connection_id: input.connection_id.clone(),
            capability: input.capability.clone(),
            kind: input.kind.clone(),
            status: "queued".into(),
            payload: input.payload.clone(),
            approved_payload: Some(input.payload.clone()),
            scheduled_at: None,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
        };
        self.connect()?.execute(
            "INSERT INTO external_actions
                (id, organization_id, project_id, connection_id, capability, kind, status,
                 payload_json, approved_payload_json, scheduled_at, created_at, updated_at,
                 completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?7, NULL, ?8, ?8, NULL)",
            params![
                action.id,
                action.organization_id,
                action.project_id,
                action.connection_id,
                action.capability,
                action.kind,
                serde_json::to_string(&action.payload)?,
                action.created_at,
            ],
        )?;
        Ok(action)
    }

    pub fn start_external_action_attempt(&self, action_id: &str) -> AppResult<i64> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let attempt: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM execution_attempts
             WHERE action_id = ?1",
            [action_id],
            |row| row.get(0),
        )?;
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE external_actions SET status = 'sending', updated_at = ?2 WHERE id = ?1",
            params![action_id, now],
        )?;
        transaction.execute(
            "INSERT INTO execution_attempts
                (id, action_id, attempt_number, status, started_at)
             VALUES (?1, ?2, ?3, 'started', ?4)",
            params![Uuid::new_v4().to_string(), action_id, attempt, now],
        )?;
        let (organization_id, project_id, connection_id, provider, destination, operation, payload):
            (String, Option<String>, String, String, String, String, String) = transaction
            .query_row(
                "SELECT a.organization_id, a.project_id, a.connection_id, c.provider,
                        c.account_identifier, a.kind, a.approved_payload_json
                 FROM external_actions a
                 JOIN integration_connections c ON c.id = a.connection_id
                 WHERE a.id = ?1",
                [action_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )?;
        let data_category = if operation.starts_with("mail.") {
            "reviewed email content"
        } else if operation.starts_with("calendar.") {
            "reviewed calendar event"
        } else {
            "reviewed external action"
        };
        transaction.execute(
            "INSERT INTO privacy_receipts
                (id, organization_id, project_id, connection_id, provider, operation, reason,
                 data_categories_json, destination, byte_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'User approved this external action', ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                organization_id,
                project_id,
                connection_id,
                provider,
                operation,
                serde_json::to_string(&vec![data_category])?,
                destination,
                payload.len() as i64,
                now,
            ],
        )?;
        transaction.commit()?;
        Ok(attempt)
    }

    pub fn record_privacy_receipt(&self, input: &PrivacyReceiptInput) -> AppResult<PrivacyReceipt> {
        self.get_organization(&input.organization_id)?;
        let receipt = PrivacyReceipt {
            id: Uuid::new_v4().to_string(),
            organization_id: input.organization_id.clone(),
            project_id: input.project_id.clone(),
            connection_id: input.connection_id.clone(),
            provider: input.provider.trim().to_string(),
            operation: input.operation.trim().to_string(),
            reason: input.reason.trim().to_string(),
            data_categories: input.data_categories.clone(),
            destination: input.destination.trim().to_string(),
            byte_count: input.byte_count,
            created_at: Utc::now().to_rfc3339(),
        };
        if receipt.provider.is_empty() || receipt.operation.is_empty() || receipt.reason.is_empty()
        {
            return Err(AppError::InvalidInput(
                "A privacy receipt needs a provider, operation and reason".into(),
            ));
        }
        self.connect()?.execute(
            "INSERT INTO privacy_receipts
                (id, organization_id, project_id, connection_id, provider, operation, reason,
                 data_categories_json, destination, byte_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                receipt.id,
                receipt.organization_id,
                receipt.project_id,
                receipt.connection_id,
                receipt.provider,
                receipt.operation,
                receipt.reason,
                serde_json::to_string(&receipt.data_categories)?,
                receipt.destination,
                receipt.byte_count,
                receipt.created_at
            ],
        )?;
        Ok(receipt)
    }

    pub fn privacy_receipts(
        &self,
        organization_id: &str,
        limit: i64,
    ) -> AppResult<Vec<PrivacyReceipt>> {
        self.get_organization(organization_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, organization_id, project_id, connection_id, provider, operation, reason,
                    data_categories_json, destination, byte_count, created_at
             FROM privacy_receipts WHERE organization_id = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![organization_id, limit.clamp(1, 200)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(
                    id,
                    organization_id,
                    project_id,
                    connection_id,
                    provider,
                    operation,
                    reason,
                    data_categories_json,
                    destination,
                    byte_count,
                    created_at,
                )| {
                    Ok(PrivacyReceipt {
                        id,
                        organization_id,
                        project_id,
                        connection_id,
                        provider,
                        operation,
                        reason,
                        data_categories: serde_json::from_str(&data_categories_json)?,
                        destination,
                        byte_count,
                        created_at,
                    })
                },
            )
            .collect()
    }

    pub fn finish_external_action_attempt(
        &self,
        action_id: &str,
        attempt: i64,
        receipt: Option<&str>,
        error: Option<&str>,
    ) -> AppResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        let attempt_status = if error.is_some() {
            "failed"
        } else {
            "succeeded"
        };
        let action_status = if error.is_some() { "failed" } else { "sent" };
        transaction.execute(
            "UPDATE execution_attempts SET status = ?3, provider_receipt = ?4,
                safe_error = ?5, finished_at = ?6
             WHERE action_id = ?1 AND attempt_number = ?2",
            params![action_id, attempt, attempt_status, receipt, error, now],
        )?;
        transaction.execute(
            "UPDATE external_actions SET status = ?2, updated_at = ?3, completed_at = ?3
             WHERE id = ?1",
            params![action_id, action_status, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn link_external_calendar_event(
        &self,
        connection_id: &str,
        external_id: &str,
        project_id: &str,
        meeting_id: &str,
        etag: Option<&str>,
    ) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute(
            "INSERT INTO external_objects
                (id, connection_id, object_kind, external_id, project_id, meeting_id, etag,
                 created_at, updated_at)
             VALUES (?1, ?2, 'calendar_event', ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(connection_id, object_kind, external_id) DO UPDATE SET
                project_id = excluded.project_id, meeting_id = excluded.meeting_id,
                etag = excluded.etag, updated_at = excluded.updated_at, deleted_at = NULL",
            params![
                Uuid::new_v4().to_string(),
                connection_id,
                external_id,
                project_id,
                meeting_id,
                etag,
                now
            ],
        )?;
        Ok(())
    }

    pub fn external_calendar_meeting_id(
        &self,
        connection_id: &str,
        external_id: &str,
    ) -> AppResult<Option<String>> {
        Ok(self
            .connect()?
            .query_row(
                "SELECT meeting_id FROM external_objects
                 WHERE connection_id = ?1 AND object_kind = 'calendar_event'
                   AND external_id = ?2 AND deleted_at IS NULL",
                params![connection_id, external_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn link_external_mail_message(
        &self,
        connection_id: &str,
        external_id: &str,
        thread_id: &str,
        project_id: &str,
        sender: &str,
    ) -> AppResult<()> {
        let snapshot = self.integration_snapshot(connection_id)?;
        let person_id = self
            .connect()?
            .query_row(
                "SELECT people.id FROM people
                 JOIN organization_people ON organization_people.person_id=people.id
                 WHERE organization_people.organization_id=?1 AND people.email IS NOT NULL
                   AND instr(lower(?2), lower(people.email)) > 0
                 LIMIT 1",
                params![snapshot.connection.organization_id, sender],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO external_objects
                (id, connection_id, object_kind, external_id, project_id, person_id,
                 created_at, updated_at)
             VALUES (?1, ?2, 'mail_message', ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(connection_id, object_kind, external_id) DO UPDATE SET
                project_id=excluded.project_id, person_id=excluded.person_id,
                updated_at=excluded.updated_at, deleted_at=NULL",
            params![
                Uuid::new_v4().to_string(),
                connection_id,
                external_id,
                project_id,
                person_id,
                now
            ],
        )?;
        transaction.execute(
            "INSERT INTO external_objects
                (id, connection_id, object_kind, external_id, project_id,
                 created_at, updated_at)
             VALUES (?1, ?2, 'mail_thread', ?3, ?4, ?5, ?5)
             ON CONFLICT(connection_id, object_kind, external_id) DO UPDATE SET
                project_id=excluded.project_id, updated_at=excluded.updated_at, deleted_at=NULL",
            params![
                Uuid::new_v4().to_string(),
                connection_id,
                thread_id,
                project_id,
                now
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn integration_connection_from_row(row: &Row<'_>) -> rusqlite::Result<IntegrationConnection> {
    Ok(IntegrationConnection {
        id: row.get(0)?,
        organization_id: row.get(1)?,
        provider: row.get(2)?,
        account_identifier: row.get(3)?,
        display_name: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

fn integration_capabilities(
    connection: &Connection,
    connection_id: &str,
) -> AppResult<Vec<IntegrationCapability>> {
    let mut statement = connection.prepare(
        "SELECT connection_id, capability, status, provider_scope, granted_at, revoked_at,
                updated_at FROM integration_capabilities
         WHERE connection_id = ?1 ORDER BY capability",
    )?;
    let rows = statement.query_map([connection_id], |row| {
        Ok(IntegrationCapability {
            connection_id: row.get(0)?,
            capability: row.get(1)?,
            status: row.get(2)?,
            provider_scope: row.get(3)?,
            granted_at: row.get(4)?,
            revoked_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
        assert_eq!(version, 13);
        for table in [
            "integration_connections",
            "integration_capabilities",
            "external_objects",
            "sync_cursors",
            "external_actions",
            "execution_attempts",
            "privacy_receipts",
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

    #[test]
    fn an_approved_action_keeps_its_payload_and_immutable_attempt() {
        let database = database();
        let organization_id = organization(&database);
        let account_id = integration(&database, &organization_id);
        set_capability(&database, &account_id, CAPABILITY_CALENDAR_WRITE, "granted");
        let action = database
            .create_approved_external_action(&ExternalActionInput {
                organization_id,
                project_id: None,
                connection_id: account_id,
                capability: CAPABILITY_CALENDAR_WRITE.into(),
                kind: "calendar.event.create".into(),
                payload: serde_json::json!({"summary": "Review"}),
            })
            .unwrap();
        assert_eq!(action.approved_payload, Some(action.payload.clone()));
        let attempt = database.start_external_action_attempt(&action.id).unwrap();
        database
            .finish_external_action_attempt(&action.id, attempt, Some("google-event-1"), None)
            .unwrap();
        let connection = database.connect().unwrap();
        let action_status: String = connection
            .query_row(
                "SELECT status FROM external_actions WHERE id = ?1",
                [&action.id],
                |row| row.get(0),
            )
            .unwrap();
        let (attempt_status, receipt): (String, String) = connection
            .query_row(
                "SELECT status, provider_receipt FROM execution_attempts
                 WHERE action_id = ?1 AND attempt_number = 1",
                [&action.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(action_status, "sent");
        assert_eq!(attempt_status, "succeeded");
        assert_eq!(receipt, "google-event-1");
        let privacy_receipts = database
            .privacy_receipts(&action.organization_id, 10)
            .unwrap();
        assert_eq!(privacy_receipts.len(), 1);
        assert_eq!(privacy_receipts[0].provider, "google");
        assert_eq!(privacy_receipts[0].operation, "calendar.event.create");
        assert_eq!(
            privacy_receipts[0].data_categories,
            vec!["reviewed calendar event"]
        );
        assert!(privacy_receipts[0].byte_count.unwrap() > 0);
    }
}
