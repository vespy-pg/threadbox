//! Gmail adapter for bounded mailbox access and reviewed outbound messages.
//!
//! Message lists contain headers only. Bodies and attachment bytes are fetched explicitly and are
//! never mirrored as part of background synchronisation.

use base64::{engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{TimeZone, Utc};
use reqwest::{blocking::Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    database::Database,
    error::{AppError, AppResult},
    google,
    integrations::{
        CAPABILITY_MAIL_COMPOSE, CAPABILITY_MAIL_CONTENT_READ, CAPABILITY_MAIL_METADATA_READ,
        CAPABILITY_MAIL_SEND,
    },
};

const API_ROOT: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const CURSOR_KIND: &str = "gmail_history";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMailLabel {
    pub id: String,
    pub name: String,
    pub label_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMailAttachment {
    pub attachment_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMailMessage {
    pub id: String,
    pub thread_id: String,
    pub label_ids: Vec<String>,
    pub snippet: String,
    pub history_id: String,
    pub internal_date: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub message_id: String,
    pub references: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub attachments: Vec<ExternalMailAttachment>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailListInput {
    pub connection_id: String,
    pub label_id: Option<String>,
    pub query: Option<String>,
    pub max_results: u32,
    pub page_token: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailPage {
    pub messages: Vec<ExternalMailMessage>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailSyncResult {
    pub messages: Vec<ExternalMailMessage>,
    pub history_id: String,
    pub full_sync: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailDraftInput {
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub thread_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailActionResult {
    pub id: String,
    pub thread_id: String,
    pub draft_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMailItem {
    pub id: String,
    pub project_id: String,
    pub connection_id: String,
    pub external_id: String,
    pub thread_id: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub snippet: String,
    pub internal_date: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub attachments: Vec<ExternalMailAttachment>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailLabelList {
    #[serde(default)]
    labels: Vec<GmailLabel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailLabel {
    id: String,
    name: String,
    #[serde(rename = "type", default)]
    label_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageList {
    #[serde(default)]
    messages: Vec<GmailMessageReference>,
    next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageReference {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailHistoryList {
    #[serde(default)]
    history: Vec<GmailHistory>,
    #[serde(default)]
    history_id: String,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailHistory {
    #[serde(default)]
    messages_added: Vec<GmailMessageAdded>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageAdded {
    message: GmailMessageReference,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessage {
    id: String,
    thread_id: String,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    history_id: String,
    #[serde(default)]
    internal_date: String,
    #[serde(default)]
    payload: GmailPart,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailPart {
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    #[serde(default)]
    body: GmailPartBody,
    #[serde(default)]
    parts: Vec<GmailPart>,
}

#[derive(Clone, Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailPartBody {
    #[serde(default)]
    attachment_id: String,
    #[serde(default)]
    size: u64,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailProfile {
    history_id: String,
}

#[derive(Debug, Deserialize)]
struct GmailAttachmentBody {
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailActionResponse {
    id: String,
    #[serde(default)]
    thread_id: String,
    message: Option<Box<GmailActionResponse>>,
}

pub(crate) fn migrate_schema(connection: &rusqlite::Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_mail_items (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            connection_id TEXT NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
            external_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            recipients TEXT NOT NULL,
            cc TEXT NOT NULL,
            subject TEXT NOT NULL,
            snippet TEXT NOT NULL,
            internal_date TEXT NOT NULL,
            body_text TEXT,
            body_html TEXT,
            attachments_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(connection_id, external_id, project_id)
        );
        CREATE INDEX IF NOT EXISTS idx_project_mail_items_project
            ON project_mail_items(project_id, internal_date DESC);",
    )?;
    Ok(())
}

impl Database {
    pub fn google_mail_labels(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<Vec<ExternalMailLabel>> {
        self.require_google_mail_capability(connection_id, false)?;
        let response: GmailLabelList = gmail_client(connection_id, client_id)?
            .get(format!("{API_ROOT}/labels"))
            .send()?
            .error_for_status()?
            .json()?;
        Ok(response
            .labels
            .into_iter()
            .map(|label| ExternalMailLabel {
                id: label.id,
                name: label.name,
                label_type: label.label_type,
            })
            .collect())
    }

    pub fn google_mail_page(&self, client_id: &str, input: &MailListInput) -> AppResult<MailPage> {
        let has_content =
            self.has_integration_capability(&input.connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        self.require_google_mail_capability(&input.connection_id, false)?;
        let mut request = gmail_client(&input.connection_id, client_id)?
            .get(format!("{API_ROOT}/messages"))
            .query(&[("maxResults", input.max_results.clamp(1, 100).to_string())]);
        if let Some(label_id) = input.label_id.as_deref().filter(|value| !value.is_empty()) {
            request = request.query(&[("labelIds", label_id)]);
        }
        if let Some(page_token) = input
            .page_token
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            request = request.query(&[("pageToken", page_token)]);
        }
        if let Some(query) = input
            .query
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            if !has_content {
                return Err(AppError::InvalidInput(
                    "Gmail search needs the Read email content permission".into(),
                ));
            }
            request = request.query(&[("q", query.trim())]);
        }
        let page: GmailMessageList = request.send()?.error_for_status()?.json()?;
        let messages = page
            .messages
            .iter()
            .map(|item| {
                self.google_mail_message_metadata(&input.connection_id, client_id, &item.id)
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(MailPage {
            messages,
            next_page_token: page.next_page_token,
        })
    }

    pub fn sync_google_mail_headers(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<MailSyncResult> {
        self.require_google_mail_capability(connection_id, false)?;
        let Some(cursor) = self.integration_sync_cursor(connection_id, CURSOR_KIND)? else {
            return self.full_google_mail_header_sync(connection_id, client_id);
        };
        match self.google_mail_history(connection_id, client_id, &cursor) {
            Ok((ids, history_id)) => {
                let messages = ids
                    .iter()
                    .map(|id| self.google_mail_message_metadata(connection_id, client_id, id))
                    .collect::<AppResult<Vec<_>>>()?;
                self.set_integration_sync_cursor(connection_id, CURSOR_KIND, &history_id)?;
                Ok(MailSyncResult {
                    messages,
                    history_id,
                    full_sync: false,
                })
            }
            Err(HistoryError::Expired) => {
                self.full_google_mail_header_sync(connection_id, client_id)
            }
            Err(HistoryError::Other(error)) => Err(error),
        }
    }

    pub fn google_mail_message(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
    ) -> AppResult<ExternalMailMessage> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        self.fetch_google_mail_message(connection_id, client_id, message_id, "full")
    }

    pub fn google_mail_attachment(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
        attachment_id: &str,
        mime_type: &str,
    ) -> AppResult<String> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        let response: GmailAttachmentBody = gmail_client(connection_id, client_id)?
            .get(format!(
                "{API_ROOT}/messages/{}/attachments/{}",
                urlencoding::encode(message_id),
                urlencoding::encode(attachment_id)
            ))
            .send()?
            .error_for_status()?
            .json()?;
        let bytes = decode_base64url(&response.data)?;
        Ok(format!(
            "data:{};base64,{}",
            safe_mime_type(mime_type),
            STANDARD.encode(bytes)
        ))
    }

    pub fn create_google_mail_draft(
        &self,
        client_id: &str,
        input: &MailDraftInput,
    ) -> AppResult<MailActionResult> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_COMPOSE)?;
        let raw = build_raw_message(input)?;
        let mut message = json!({ "raw": raw });
        if let Some(thread_id) = input.thread_id.as_deref().filter(|value| !value.is_empty()) {
            message["threadId"] = json!(thread_id);
        }
        let response: GmailActionResponse = gmail_client(&input.connection_id, client_id)?
            .post(format!("{API_ROOT}/drafts"))
            .json(&json!({ "message": message }))
            .send()?
            .error_for_status()?
            .json()?;
        let message = response.message.as_deref();
        Ok(MailActionResult {
            id: message.map(|item| item.id.clone()).unwrap_or_default(),
            thread_id: message
                .map(|item| item.thread_id.clone())
                .unwrap_or_default(),
            draft_id: Some(response.id),
        })
    }

    pub fn send_google_mail(
        &self,
        client_id: &str,
        input: &MailDraftInput,
    ) -> AppResult<MailActionResult> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_SEND)?;
        let raw = build_raw_message(input)?;
        let mut body = json!({ "raw": raw });
        if let Some(thread_id) = input.thread_id.as_deref().filter(|value| !value.is_empty()) {
            body["threadId"] = json!(thread_id);
        }
        let response: GmailActionResponse = gmail_client(&input.connection_id, client_id)?
            .post(format!("{API_ROOT}/messages/send"))
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        Ok(MailActionResult {
            id: response.id,
            thread_id: response.thread_id,
            draft_id: None,
        })
    }

    pub fn import_google_mail_message(
        &self,
        connection_id: &str,
        project_id: &str,
        message: &ExternalMailMessage,
    ) -> AppResult<ProjectMailItem> {
        let snapshot = self.require_google_mail_capability(connection_id, false)?;
        let project = self.get_project(project_id)?;
        if project.organization_id != snapshot.connection.organization_id {
            return Err(AppError::InvalidInput(
                "The email and project must belong to the same organisation".into(),
            ));
        }
        if (message.body_text.is_some() || message.body_html.is_some())
            && !self.has_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?
        {
            return Err(AppError::InvalidInput(
                "Importing email content needs the Read email content permission".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        self.connect()?.execute(
            "INSERT INTO project_mail_items
                (id, project_id, connection_id, external_id, thread_id, sender, recipients, cc,
                 subject, snippet, internal_date, body_text, body_html, attachments_json,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
             ON CONFLICT(connection_id, external_id, project_id) DO UPDATE SET
                thread_id=excluded.thread_id, sender=excluded.sender, recipients=excluded.recipients,
                cc=excluded.cc, subject=excluded.subject, snippet=excluded.snippet,
                internal_date=excluded.internal_date,
                body_text=COALESCE(excluded.body_text, project_mail_items.body_text),
                body_html=COALESCE(excluded.body_html, project_mail_items.body_html),
                attachments_json=excluded.attachments_json, updated_at=excluded.updated_at",
            rusqlite::params![
                id,
                project_id,
                connection_id,
                message.id,
                message.thread_id,
                message.from,
                message.to,
                message.cc,
                message.subject,
                message.snippet,
                message.internal_date,
                message.body_text,
                message.body_html,
                serde_json::to_string(&message.attachments)?,
                now,
            ],
        )?;
        self.link_external_mail_message(
            connection_id,
            &message.id,
            &message.thread_id,
            project_id,
            &message.from,
        )?;
        self.project_mail_item(connection_id, project_id, &message.id)
    }

    pub fn project_mail_items(&self, project_id: &str) -> AppResult<Vec<ProjectMailItem>> {
        self.get_project(project_id)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, connection_id, external_id, thread_id, sender, recipients, cc,
                    subject, snippet, internal_date, body_text, body_html, attachments_json,
                    created_at, updated_at
             FROM project_mail_items WHERE project_id=?1 ORDER BY internal_date DESC, created_at DESC",
        )?;
        let rows = statement.query_map([project_id], project_mail_item_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn project_mail_item(
        &self,
        connection_id: &str,
        project_id: &str,
        external_id: &str,
    ) -> AppResult<ProjectMailItem> {
        Ok(self.connect()?.query_row(
            "SELECT id, project_id, connection_id, external_id, thread_id, sender, recipients, cc,
                    subject, snippet, internal_date, body_text, body_html, attachments_json,
                    created_at, updated_at
             FROM project_mail_items WHERE connection_id=?1 AND project_id=?2 AND external_id=?3",
            rusqlite::params![connection_id, project_id, external_id],
            project_mail_item_from_row,
        )?)
    }

    fn require_google_mail_capability(
        &self,
        connection_id: &str,
        content_only: bool,
    ) -> AppResult<crate::integrations::IntegrationSnapshot> {
        if content_only {
            return self
                .require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ);
        }
        if self.has_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)? {
            self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)
        } else {
            self.require_integration_capability(connection_id, CAPABILITY_MAIL_METADATA_READ)
        }
    }

    fn google_mail_message_metadata(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
    ) -> AppResult<ExternalMailMessage> {
        self.fetch_google_mail_message(connection_id, client_id, message_id, "metadata")
    }

    fn fetch_google_mail_message(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
        format: &str,
    ) -> AppResult<ExternalMailMessage> {
        let message: GmailMessage = gmail_client(connection_id, client_id)?
            .get(format!(
                "{API_ROOT}/messages/{}",
                urlencoding::encode(message_id)
            ))
            .query(&[("format", format)])
            .send()?
            .error_for_status()?
            .json()?;
        Ok(external_message(message))
    }

    fn full_google_mail_header_sync(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<MailSyncResult> {
        let input = MailListInput {
            connection_id: connection_id.into(),
            label_id: Some("INBOX".into()),
            query: None,
            max_results: 50,
            page_token: None,
        };
        let page = self.google_mail_page(client_id, &input)?;
        let profile: GmailProfile = gmail_client(connection_id, client_id)?
            .get(format!("{API_ROOT}/profile"))
            .send()?
            .error_for_status()?
            .json()?;
        self.set_integration_sync_cursor(connection_id, CURSOR_KIND, &profile.history_id)?;
        Ok(MailSyncResult {
            messages: page.messages,
            history_id: profile.history_id,
            full_sync: true,
        })
    }

    fn google_mail_history(
        &self,
        connection_id: &str,
        client_id: &str,
        cursor: &str,
    ) -> Result<(Vec<String>, String), HistoryError> {
        let client = gmail_client(connection_id, client_id).map_err(HistoryError::Other)?;
        let mut page_token: Option<String> = None;
        let mut ids = Vec::new();
        let mut latest = cursor.to_string();
        loop {
            let mut request = client
                .get(format!("{API_ROOT}/history"))
                .query(&[("startHistoryId", cursor), ("historyTypes", "messageAdded")]);
            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request
                .send()
                .map_err(|error| HistoryError::Other(error.into()))?;
            if response.status() == StatusCode::NOT_FOUND {
                return Err(HistoryError::Expired);
            }
            let page: GmailHistoryList = response
                .error_for_status()
                .map_err(|error| HistoryError::Other(error.into()))?
                .json()
                .map_err(|error| HistoryError::Other(error.into()))?;
            for history in page.history {
                ids.extend(
                    history
                        .messages_added
                        .into_iter()
                        .map(|item| item.message.id),
                );
            }
            if !page.history_id.is_empty() {
                latest = page.history_id;
            }
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        ids.sort();
        ids.dedup();
        Ok((ids, latest))
    }
}

enum HistoryError {
    Expired,
    Other(AppError),
}

fn gmail_client(connection_id: &str, client_id: &str) -> AppResult<GmailRequestClient> {
    Ok(GmailRequestClient {
        client: Client::new(),
        token: google::access_token(connection_id, client_id)?,
    })
}

struct GmailRequestClient {
    client: Client,
    token: String,
}

impl GmailRequestClient {
    fn get(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.client.get(url).bearer_auth(&self.token)
    }

    fn post(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.client.post(url).bearer_auth(&self.token)
    }
}

fn external_message(message: GmailMessage) -> ExternalMailMessage {
    let headers = &message.payload.headers;
    let mut body_text = None;
    let mut body_html = None;
    let mut attachments = Vec::new();
    collect_parts(
        &message.payload,
        &mut body_text,
        &mut body_html,
        &mut attachments,
    );
    ExternalMailMessage {
        id: message.id,
        thread_id: message.thread_id,
        label_ids: message.label_ids,
        snippet: message.snippet,
        history_id: message.history_id,
        internal_date: format_internal_date(&message.internal_date),
        from: header(headers, "From"),
        to: header(headers, "To"),
        cc: header(headers, "Cc"),
        subject: header(headers, "Subject"),
        message_id: header(headers, "Message-ID"),
        references: header(headers, "References"),
        body_text,
        body_html,
        attachments,
    }
}

fn collect_parts(
    part: &GmailPart,
    body_text: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<ExternalMailAttachment>,
) {
    if !part.filename.is_empty() && !part.body.attachment_id.is_empty() {
        attachments.push(ExternalMailAttachment {
            attachment_id: part.body.attachment_id.clone(),
            filename: part.filename.clone(),
            mime_type: part.mime_type.clone(),
            size_bytes: part.body.size,
        });
    } else if let Some(data) = part.body.data.as_deref() {
        if let Ok(bytes) = decode_base64url(data) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if part.mime_type == "text/plain" && body_text.is_none() {
                *body_text = Some(text);
            } else if part.mime_type == "text/html" && body_html.is_none() {
                *body_html = Some(text);
            }
        }
    }
    for child in &part.parts {
        collect_parts(child, body_text, body_html, attachments);
    }
}

fn header(headers: &[GmailHeader], name: &str) -> String {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
        .unwrap_or_default()
}

fn format_internal_date(value: &str) -> String {
    value
        .parse::<i64>()
        .ok()
        .and_then(|milliseconds| Utc.timestamp_millis_opt(milliseconds).single())
        .map(|date| date.to_rfc3339())
        .unwrap_or_else(|| value.to_string())
}

fn decode_base64url(value: &str) -> AppResult<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value.trim_end_matches('='))
        .map_err(|error| AppError::InvalidInput(format!("Invalid Gmail content encoding: {error}")))
}

fn build_raw_message(input: &MailDraftInput) -> AppResult<String> {
    if input.to.is_empty() {
        return Err(AppError::InvalidInput(
            "At least one email recipient is required".into(),
        ));
    }
    for value in input
        .to
        .iter()
        .chain(&input.cc)
        .chain(&input.bcc)
        .chain([&input.subject])
    {
        reject_header_line_breaks(value)?;
    }
    for recipient in input.to.iter().chain(&input.cc).chain(&input.bcc) {
        if !recipient.contains('@') {
            return Err(AppError::InvalidInput(format!(
                "Invalid email recipient: {recipient}"
            )));
        }
    }
    let mut lines = vec![
        format!("To: {}", input.to.join(", ")),
        format!("Subject: {}", encode_header(&input.subject)),
        "MIME-Version: 1.0".into(),
        "Content-Type: text/plain; charset=UTF-8".into(),
        "Content-Transfer-Encoding: base64".into(),
    ];
    if !input.cc.is_empty() {
        lines.insert(1, format!("Cc: {}", input.cc.join(", ")));
    }
    if !input.bcc.is_empty() {
        lines.insert(1, format!("Bcc: {}", input.bcc.join(", ")));
    }
    if let Some(message_id) = input
        .in_reply_to
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        reject_header_line_breaks(message_id)?;
        lines.push(format!("In-Reply-To: {message_id}"));
    }
    if let Some(references) = input
        .references
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        reject_header_line_breaks(references)?;
        lines.push(format!("References: {references}"));
    }
    lines.push(String::new());
    lines.push(STANDARD.encode(input.body.as_bytes()));
    Ok(URL_SAFE_NO_PAD.encode(lines.join("\r\n").as_bytes()))
}

fn reject_header_line_breaks(value: &str) -> AppResult<()> {
    if value.contains('\r') || value.contains('\n') {
        Err(AppError::InvalidInput(
            "Email headers cannot contain line breaks".into(),
        ))
    } else {
        Ok(())
    }
}

fn encode_header(value: &str) -> String {
    if value.is_ascii() {
        value.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(value.as_bytes()))
    }
}

fn safe_mime_type(value: &str) -> &str {
    if value.is_empty() || value.contains(['\r', '\n', ';']) {
        "application/octet-stream"
    } else {
        value
    }
}

fn project_mail_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectMailItem> {
    let attachments: String = row.get(13)?;
    Ok(ProjectMailItem {
        id: row.get(0)?,
        project_id: row.get(1)?,
        connection_id: row.get(2)?,
        external_id: row.get(3)?,
        thread_id: row.get(4)?,
        from: row.get(5)?,
        to: row.get(6)?,
        cc: row.get(7)?,
        subject: row.get(8)?,
        snippet: row.get(9)?,
        internal_date: row.get(10)?,
        body_text: row.get(11)?,
        body_html: row.get(12)?,
        attachments: serde_json::from_str(&attachments).unwrap_or_default(),
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        integrations::IntegrationConnectionInput,
        workspace::{OrganizationInput, ProjectInput},
    };

    #[test]
    fn builds_a_unicode_reply_as_rfc_message_and_base64url() {
        let raw = build_raw_message(&MailDraftInput {
            organization_id: "org".into(),
            project_id: Some("project".into()),
            connection_id: "connection".into(),
            to: vec!["person@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Odpowiedź".into(),
            body: "Dzień dobry\nTreść".into(),
            thread_id: Some("thread".into()),
            in_reply_to: Some("<message@example.com>".into()),
            references: Some("<parent@example.com>".into()),
        })
        .unwrap();
        let decoded = String::from_utf8(URL_SAFE_NO_PAD.decode(&raw).unwrap()).unwrap();
        assert!(decoded.contains("Subject: =?UTF-8?B?"));
        assert!(decoded.contains("In-Reply-To: <message@example.com>"));
        assert!(!raw.contains('='));
    }

    #[test]
    fn extracts_bodies_and_keeps_attachments_lazy() {
        let message = external_message(GmailMessage {
            id: "message".into(),
            thread_id: "thread".into(),
            label_ids: vec!["INBOX".into()],
            snippet: "Hello".into(),
            history_id: "12".into(),
            internal_date: "0".into(),
            payload: GmailPart {
                headers: vec![GmailHeader {
                    name: "Subject".into(),
                    value: "Update".into(),
                }],
                parts: vec![
                    GmailPart {
                        mime_type: "text/plain".into(),
                        body: GmailPartBody {
                            data: Some(URL_SAFE_NO_PAD.encode("Body")),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    GmailPart {
                        mime_type: "application/pdf".into(),
                        filename: "brief.pdf".into(),
                        body: GmailPartBody {
                            attachment_id: "attachment".into(),
                            size: 42,
                            data: None,
                        },
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        });
        assert_eq!(message.body_text.as_deref(), Some("Body"));
        assert_eq!(message.attachments[0].attachment_id, "attachment");
        assert_eq!(message.attachments[0].size_bytes, 42);
    }

    #[test]
    fn project_import_respects_content_access_and_links_the_thread() {
        let root = std::env::temp_dir().join(format!("threadbox-gmail-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let database = Database {
            path: root.join("threadbox.db"),
        };
        database.migrate().unwrap();
        let organization = database
            .create_organization(OrganizationInput {
                name: "Vespy".into(),
                notes: String::new(),
                context_sharing: "isolated".into(),
            })
            .unwrap();
        let project = database
            .create_project(ProjectInput {
                organization_id: organization.id.clone(),
                parent_id: None,
                name: "DINPanel".into(),
                description: String::new(),
                context_sharing: "inherit".into(),
                language: None,
                terminology_language: None,
            })
            .unwrap();
        let connection = database
            .upsert_integration_connection(IntegrationConnectionInput {
                organization_id: organization.id,
                provider: "google".into(),
                account_identifier: "owner@example.com".into(),
                display_name: "Owner".into(),
            })
            .unwrap();
        database
            .set_integration_capability(
                &connection.connection.id,
                CAPABILITY_MAIL_METADATA_READ,
                google::SCOPE_GMAIL_METADATA,
                true,
            )
            .unwrap();
        let mut message = ExternalMailMessage {
            id: "message-1".into(),
            thread_id: "thread-1".into(),
            label_ids: vec!["INBOX".into()],
            snippet: "Project update".into(),
            history_id: "3".into(),
            internal_date: Utc::now().to_rfc3339(),
            from: "Person <person@example.com>".into(),
            to: "owner@example.com".into(),
            cc: String::new(),
            subject: "Update".into(),
            message_id: "<message-1@example.com>".into(),
            references: String::new(),
            body_text: None,
            body_html: None,
            attachments: Vec::new(),
        };
        database
            .import_google_mail_message(&connection.connection.id, &project.id, &message)
            .unwrap();
        message.body_text = Some("Full message".into());
        assert!(database
            .import_google_mail_message(&connection.connection.id, &project.id, &message)
            .is_err());
        database
            .set_integration_capability(
                &connection.connection.id,
                CAPABILITY_MAIL_CONTENT_READ,
                google::SCOPE_GMAIL_READONLY,
                true,
            )
            .unwrap();
        let imported = database
            .import_google_mail_message(&connection.connection.id, &project.id, &message)
            .unwrap();
        assert_eq!(imported.body_text.as_deref(), Some("Full message"));
        let linked: i64 = database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM external_objects WHERE connection_id=?1
                 AND object_kind IN ('mail_message', 'mail_thread')",
                [&connection.connection.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked, 2);
    }
}
