//! Standards-based IMAP and SMTP mail connection.
//!
//! Incoming and outgoing credentials are independent and live only in the OS keyring. IMAP always
//! uses TLS, while SMTP uses implicit TLS or mandatory STARTTLS - plaintext credential transport is
//! deliberately unsupported.

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use lettre::{
    message::Mailbox,
    transport::smtp::{authentication::Credentials, client::Tls},
    Message, SmtpTransport, Transport,
};
use mailparse::{parse_headers, parse_mail, MailHeaderMap, ParsedMail};
use native_tls::TlsConnector;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    database::Database,
    error::{AppError, AppResult},
    gmail::{
        ExternalMailAttachment, ExternalMailLabel, ExternalMailMessage, MailActionResult,
        MailDraftInput, MailListInput, MailPage, MailSyncResult,
    },
    integrations::{
        CAPABILITY_MAIL_CONTENT_READ, CAPABILITY_MAIL_METADATA_READ, CAPABILITY_MAIL_SEND,
    },
    secrets,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardMailConnectionInput {
    pub organization_id: String,
    pub email: String,
    pub display_name: String,
    pub username: String,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub imap_password: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<String>,
    pub smtp_password: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardMailConfig {
    pub connection_id: String,
    pub username: String,
    pub email: String,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailDiagnostic {
    pub incoming_ok: Option<bool>,
    pub incoming_message: String,
    pub outgoing_ok: Option<bool>,
    pub outgoing_message: String,
}

pub(crate) fn migrate_schema(connection: &rusqlite::Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS standard_mail_configs (
            connection_id TEXT PRIMARY KEY REFERENCES integration_connections(id) ON DELETE CASCADE,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            imap_host TEXT,
            imap_port INTEGER,
            smtp_host TEXT,
            smtp_port INTEGER,
            smtp_security TEXT CHECK(smtp_security IS NULL OR smtp_security IN ('tls', 'starttls')),
            updated_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

impl Database {
    pub fn upsert_standard_mail_connection(
        &self,
        input: &StandardMailConnectionInput,
    ) -> AppResult<crate::integrations::IntegrationSnapshot> {
        validate_config(input)?;
        let snapshot =
            self.upsert_integration_connection(crate::integrations::IntegrationConnectionInput {
                organization_id: input.organization_id.clone(),
                provider: "standard_mail".into(),
                account_identifier: input.email.trim().into(),
                display_name: if input.display_name.trim().is_empty() {
                    input.email.trim().into()
                } else {
                    input.display_name.trim().into()
                },
            })?;
        self.connect()?.execute(
            "INSERT INTO standard_mail_configs
                (connection_id, username, email, imap_host, imap_port, smtp_host, smtp_port,
                 smtp_security, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(connection_id) DO UPDATE SET username=excluded.username,
                email=excluded.email, imap_host=excluded.imap_host, imap_port=excluded.imap_port,
                smtp_host=excluded.smtp_host, smtp_port=excluded.smtp_port,
                smtp_security=excluded.smtp_security, updated_at=excluded.updated_at",
            params![
                snapshot.connection.id,
                input.username.trim(),
                input.email.trim(),
                input.imap_host.as_deref().map(str::trim),
                input.imap_port,
                input.smtp_host.as_deref().map(str::trim),
                input.smtp_port,
                input.smtp_security,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        if let Some(password) = input
            .imap_password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            secrets::store(&imap_secret(&snapshot.connection.id), password)?;
        }
        if let Some(password) = input
            .smtp_password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            secrets::store(&smtp_secret(&snapshot.connection.id), password)?;
        }
        if input.imap_host.is_some() {
            self.set_integration_capability(
                &snapshot.connection.id,
                CAPABILITY_MAIL_METADATA_READ,
                "imap",
                true,
            )?;
            self.set_integration_capability(
                &snapshot.connection.id,
                CAPABILITY_MAIL_CONTENT_READ,
                "imap",
                true,
            )?;
        }
        if input.smtp_host.is_some() {
            self.set_integration_capability(
                &snapshot.connection.id,
                CAPABILITY_MAIL_SEND,
                "smtp",
                true,
            )?;
        }
        self.integration_snapshot(&snapshot.connection.id)
    }

    pub fn standard_mail_config(&self, connection_id: &str) -> AppResult<StandardMailConfig> {
        self.connect()?
            .query_row(
                "SELECT connection_id, username, email, imap_host, imap_port, smtp_host,
                        smtp_port, smtp_security FROM standard_mail_configs WHERE connection_id=?1",
                [connection_id],
                |row| {
                    Ok(StandardMailConfig {
                        connection_id: row.get(0)?,
                        username: row.get(1)?,
                        email: row.get(2)?,
                        imap_host: row.get(3)?,
                        imap_port: row.get(4)?,
                        smtp_host: row.get(5)?,
                        smtp_port: row.get(6)?,
                        smtp_security: row.get(7)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("Standard mail configuration not found".into()))
    }

    pub fn diagnose_standard_mail(&self, connection_id: &str) -> AppResult<MailDiagnostic> {
        let config = self.standard_mail_config(connection_id)?;
        let incoming = if config.imap_host.is_some() {
            Some(self.with_imap(&config, |session| {
                session.noop().map_err(imap_error)?;
                Ok(())
            }))
        } else {
            None
        };
        let outgoing =
            if config.smtp_host.is_some() {
                Some(self.smtp_transport(&config).and_then(|transport| {
                    transport.test_connection().map(|_| ()).map_err(mail_error)
                }))
            } else {
                None
            };
        Ok(MailDiagnostic {
            incoming_ok: incoming.as_ref().map(Result::is_ok),
            incoming_message: diagnostic_message(incoming, "Incoming mail is not configured"),
            outgoing_ok: outgoing.as_ref().map(Result::is_ok),
            outgoing_message: diagnostic_message(outgoing, "Outgoing mail is not configured"),
        })
    }

    pub fn standard_mail_labels(&self, connection_id: &str) -> AppResult<Vec<ExternalMailLabel>> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_METADATA_READ)?;
        let config = self.standard_mail_config(connection_id)?;
        self.with_imap(&config, |session| {
            Ok(session
                .list(None, Some("*"))
                .map_err(imap_error)?
                .iter()
                .map(|name| ExternalMailLabel {
                    id: name.name().into(),
                    name: name.name().into(),
                    label_type: "folder".into(),
                })
                .collect())
        })
    }

    pub fn standard_mail_page(&self, input: &MailListInput) -> AppResult<MailPage> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_METADATA_READ)?;
        let config = self.standard_mail_config(&input.connection_id)?;
        self.with_imap(&config, |session| {
            let folder = input.label_id.as_deref().unwrap_or("INBOX");
            session.select(folder).map_err(imap_error)?;
            let query = input
                .query
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("TEXT \"{}\"", value.replace(['\"', '\r', '\n'], "")))
                .unwrap_or_else(|| "ALL".into());
            let mut uids = session
                .uid_search(query)
                .map_err(imap_error)?
                .into_iter()
                .collect::<Vec<_>>();
            uids.sort_unstable_by(|left, right| right.cmp(left));
            uids.truncate(input.max_results.clamp(1, 100) as usize);
            let mut messages = Vec::new();
            for uid in uids {
                let fetches = session
                    .uid_fetch(uid.to_string(), "RFC822.HEADER")
                    .map_err(imap_error)?;
                if let Some(header) = fetches.iter().next().and_then(|fetch| fetch.header()) {
                    messages.push(message_from_headers(folder, uid, header)?);
                }
            }
            Ok(MailPage {
                messages,
                next_page_token: None,
            })
        })
    }

    pub fn sync_standard_mail_headers(&self, connection_id: &str) -> AppResult<MailSyncResult> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_METADATA_READ)?;
        let config = self.standard_mail_config(connection_id)?;
        self.with_imap(&config, |session| {
            let mailbox = session.select("INBOX").map_err(imap_error)?;
            let uid_validity = mailbox.uid_validity.unwrap_or_default();
            let previous = self
                .integration_sync_cursor(connection_id, "imap_inbox_uid")?
                .and_then(|value| parse_imap_cursor(&value));
            let full_sync = previous
                .as_ref()
                .is_none_or(|(validity, _)| *validity != uid_validity);
            let previous_uid = previous
                .filter(|(validity, _)| *validity == uid_validity)
                .map(|(_, uid)| uid)
                .unwrap_or_default();
            let mut uids = session
                .uid_search("ALL")
                .map_err(imap_error)?
                .into_iter()
                .filter(|uid| full_sync || *uid > previous_uid)
                .collect::<Vec<_>>();
            uids.sort_unstable_by(|left, right| right.cmp(left));
            uids.truncate(30);
            let newest_uid = uids.iter().copied().max().unwrap_or(previous_uid);
            let mut messages = Vec::new();
            for uid in uids {
                let fetches = session
                    .uid_fetch(uid.to_string(), "RFC822.HEADER")
                    .map_err(imap_error)?;
                if let Some(header) = fetches.iter().next().and_then(|fetch| fetch.header()) {
                    messages.push(message_from_headers("INBOX", uid, header)?);
                }
            }
            let cursor = format!("{uid_validity}:{newest_uid}");
            self.set_integration_sync_cursor(connection_id, "imap_inbox_uid", &cursor)?;
            Ok(MailSyncResult {
                messages,
                history_id: cursor,
                full_sync,
            })
        })
    }

    pub fn standard_mail_message(
        &self,
        connection_id: &str,
        message_id: &str,
    ) -> AppResult<ExternalMailMessage> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        let config = self.standard_mail_config(connection_id)?;
        let (folder, uid) = decode_message_id(message_id)?;
        self.with_imap(&config, |session| {
            session.select(&folder).map_err(imap_error)?;
            let fetches = session
                .uid_fetch(uid.to_string(), "RFC822")
                .map_err(imap_error)?;
            let raw = fetches
                .iter()
                .next()
                .and_then(|fetch| fetch.body())
                .ok_or_else(|| AppError::InvalidInput("IMAP returned no message body".into()))?;
            parsed_message(&folder, uid, raw)
        })
    }

    pub fn standard_mail_attachment(
        &self,
        connection_id: &str,
        message_id: &str,
        attachment_id: &str,
        mime_type: &str,
    ) -> AppResult<String> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        let config = self.standard_mail_config(connection_id)?;
        let (folder, uid) = decode_message_id(message_id)?;
        self.with_imap(&config, |session| {
            session.select(&folder).map_err(imap_error)?;
            let fetches = session
                .uid_fetch(uid.to_string(), "RFC822")
                .map_err(imap_error)?;
            let raw = fetches
                .iter()
                .next()
                .and_then(|fetch| fetch.body())
                .ok_or_else(|| AppError::InvalidInput("IMAP returned no message body".into()))?;
            let parsed = parse_mail(raw).map_err(mail_error)?;
            let part = find_part(&parsed, "1", attachment_id)
                .ok_or_else(|| AppError::InvalidInput("Attachment part not found".into()))?;
            let bytes = part.get_body_raw().map_err(mail_error)?;
            let safe_type = if mime_type.contains(['\r', '\n', ';']) {
                "application/octet-stream"
            } else {
                mime_type
            };
            Ok(format!(
                "data:{safe_type};base64,{}",
                STANDARD.encode(bytes)
            ))
        })
    }

    pub fn send_standard_mail(&self, input: &MailDraftInput) -> AppResult<MailActionResult> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_SEND)?;
        let config = self.standard_mail_config(&input.connection_id)?;
        let mut builder = Message::builder()
            .from(config.email.parse::<Mailbox>().map_err(mail_error)?)
            .subject(&input.subject);
        for address in &input.to {
            builder = builder.to(address.parse::<Mailbox>().map_err(mail_error)?);
        }
        for address in &input.cc {
            builder = builder.cc(address.parse::<Mailbox>().map_err(mail_error)?);
        }
        for address in &input.bcc {
            builder = builder.bcc(address.parse::<Mailbox>().map_err(mail_error)?);
        }
        let message = builder.body(input.body.clone()).map_err(mail_error)?;
        let response = self
            .smtp_transport(&config)?
            .send(&message)
            .map_err(mail_error)?;
        Ok(MailActionResult {
            id: response.message().collect::<Vec<_>>().join(" "),
            thread_id: input.thread_id.clone().unwrap_or_default(),
            draft_id: None,
        })
    }

    fn with_imap<T>(
        &self,
        config: &StandardMailConfig,
        action: impl FnOnce(
            &mut imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
        ) -> AppResult<T>,
    ) -> AppResult<T> {
        let host = config
            .imap_host
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput("Incoming mail is not configured".into()))?;
        let password = secrets::read(&imap_secret(&config.connection_id))?.ok_or_else(|| {
            AppError::InvalidInput("The incoming mail password is missing".into())
        })?;
        let tls = TlsConnector::builder().build().map_err(mail_error)?;
        let client = imap::connect((host, config.imap_port.unwrap_or(993)), host, &tls)
            .map_err(imap_error)?;
        let mut session = client
            .login(&config.username, password)
            .map_err(|(error, _)| imap_error(error))?;
        let result = action(&mut session);
        let _ = session.logout();
        result
    }

    fn smtp_transport(&self, config: &StandardMailConfig) -> AppResult<SmtpTransport> {
        let host = config
            .smtp_host
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput("Outgoing mail is not configured".into()))?;
        let password = secrets::read(&smtp_secret(&config.connection_id))?.ok_or_else(|| {
            AppError::InvalidInput("The outgoing mail password is missing".into())
        })?;
        let tls =
            lettre::transport::smtp::client::TlsParameters::new(host.into()).map_err(mail_error)?;
        let security = config.smtp_security.as_deref().unwrap_or("starttls");
        let mode = if security == "tls" {
            Tls::Wrapper(tls)
        } else {
            Tls::Required(tls)
        };
        Ok(SmtpTransport::builder_dangerous(host)
            .port(
                config
                    .smtp_port
                    .unwrap_or(if security == "tls" { 465 } else { 587 }),
            )
            .tls(mode)
            .credentials(Credentials::new(config.username.clone(), password))
            .build())
    }
}

fn validate_config(input: &StandardMailConnectionInput) -> AppResult<()> {
    if !input.email.contains('@') || input.username.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Email and username are required".into(),
        ));
    }
    if input.imap_host.is_none() && input.smtp_host.is_none() {
        return Err(AppError::InvalidInput(
            "Configure incoming mail, outgoing mail, or both".into(),
        ));
    }
    if input.imap_host.is_some() && input.imap_password.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::InvalidInput(
            "An incoming mail password or app password is required".into(),
        ));
    }
    if input.smtp_host.is_some() && input.smtp_password.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::InvalidInput(
            "An outgoing mail password or app password is required".into(),
        ));
    }
    if input
        .smtp_security
        .as_deref()
        .is_some_and(|value| !["tls", "starttls"].contains(&value))
    {
        return Err(AppError::InvalidInput(
            "SMTP security must be TLS or required STARTTLS".into(),
        ));
    }
    Ok(())
}

fn message_from_headers(folder: &str, uid: u32, raw: &[u8]) -> AppResult<ExternalMailMessage> {
    let (headers, _) = parse_headers(raw).map_err(mail_error)?;
    Ok(ExternalMailMessage {
        id: encode_message_id(folder, uid),
        thread_id: headers
            .get_first_value("References")
            .unwrap_or_else(|| headers.get_first_value("Message-ID").unwrap_or_default()),
        label_ids: vec![folder.into()],
        snippet: String::new(),
        history_id: uid.to_string(),
        internal_date: headers.get_first_value("Date").unwrap_or_default(),
        from: headers.get_first_value("From").unwrap_or_default(),
        to: headers.get_first_value("To").unwrap_or_default(),
        cc: headers.get_first_value("Cc").unwrap_or_default(),
        subject: headers.get_first_value("Subject").unwrap_or_default(),
        message_id: headers.get_first_value("Message-ID").unwrap_or_default(),
        references: headers.get_first_value("References").unwrap_or_default(),
        body_text: None,
        body_html: None,
        attachments: Vec::new(),
    })
}

fn parsed_message(folder: &str, uid: u32, raw: &[u8]) -> AppResult<ExternalMailMessage> {
    let parsed = parse_mail(raw).map_err(mail_error)?;
    let mut message = message_from_headers(folder, uid, raw)?;
    collect_parts(&parsed, "1", &mut message)?;
    Ok(message)
}

fn encode_message_id(folder: &str, uid: u32) -> String {
    format!("{}:{uid}", URL_SAFE_NO_PAD.encode(folder.as_bytes()))
}

fn decode_message_id(value: &str) -> AppResult<(String, u32)> {
    let (encoded_folder, uid) = value
        .rsplit_once(':')
        .ok_or_else(|| AppError::InvalidInput("Invalid IMAP message identifier".into()))?;
    let folder = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_folder).map_err(mail_error)?)
        .map_err(mail_error)?;
    let uid = uid.parse::<u32>().map_err(mail_error)?;
    Ok((folder, uid))
}

fn parse_imap_cursor(value: &str) -> Option<(u32, u32)> {
    let (validity, uid) = value.split_once(':')?;
    Some((validity.parse().ok()?, uid.parse().ok()?))
}

fn collect_parts(
    part: &ParsedMail<'_>,
    path: &str,
    message: &mut ExternalMailMessage,
) -> AppResult<()> {
    if part.subparts.is_empty() {
        let disposition = part.get_content_disposition();
        let filename = disposition
            .params
            .get("filename")
            .cloned()
            .unwrap_or_default();
        if !filename.is_empty() {
            message.attachments.push(ExternalMailAttachment {
                attachment_id: path.into(),
                filename,
                mime_type: part.ctype.mimetype.clone(),
                size_bytes: part.raw_bytes.len() as u64,
            });
        } else if part.ctype.mimetype == "text/plain" && message.body_text.is_none() {
            message.body_text = Some(part.get_body().map_err(mail_error)?);
        } else if part.ctype.mimetype == "text/html" && message.body_html.is_none() {
            message.body_html = Some(part.get_body().map_err(mail_error)?);
        }
    }
    for (index, child) in part.subparts.iter().enumerate() {
        collect_parts(child, &format!("{path}.{}", index + 1), message)?;
    }
    Ok(())
}

fn find_part<'a>(part: &'a ParsedMail<'a>, path: &str, wanted: &str) -> Option<&'a ParsedMail<'a>> {
    if path == wanted {
        return Some(part);
    }
    part.subparts
        .iter()
        .enumerate()
        .find_map(|(index, child)| find_part(child, &format!("{path}.{}", index + 1), wanted))
}

fn imap_secret(id: &str) -> String {
    secrets::integration_account("imap", id)
}
fn smtp_secret(id: &str) -> String {
    secrets::integration_account("smtp", id)
}
fn imap_error(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(format!("Incoming mail: {error}"))
}
fn mail_error(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(error.to_string())
}
fn diagnostic_message(result: Option<AppResult<()>>, absent: &str) -> String {
    match result {
        Some(Ok(())) => "Connection successful".into(),
        Some(Err(error)) => error.to_string(),
        None => absent.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_plaintext_or_missing_credentials() {
        let mut input = StandardMailConnectionInput {
            organization_id: "org".into(),
            email: "me@example.com".into(),
            display_name: String::new(),
            username: "me@example.com".into(),
            imap_host: None,
            imap_port: None,
            imap_password: None,
            smtp_host: Some("smtp.example.com".into()),
            smtp_port: Some(587),
            smtp_security: Some("plain".into()),
            smtp_password: Some("secret".into()),
        };
        assert!(validate_config(&input).is_err());
        input.smtp_security = Some("starttls".into());
        input.smtp_password = None;
        assert!(validate_config(&input).is_err());
    }

    #[test]
    fn message_identifier_preserves_folder_and_uid() {
        let encoded = encode_message_id("Archive/Clients", 42);
        assert_eq!(
            decode_message_id(&encoded).unwrap(),
            ("Archive/Clients".into(), 42)
        );
    }

    #[test]
    fn imap_cursor_preserves_mailbox_generation_and_uid() {
        assert_eq!(parse_imap_cursor("12:345"), Some((12, 345)));
        assert_eq!(parse_imap_cursor("broken"), None);
    }
}
