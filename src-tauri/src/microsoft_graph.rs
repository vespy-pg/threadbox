//! Microsoft Graph mail adapter using the same project-mail contracts as Gmail.

use reqwest::blocking::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    database::Database,
    error::{AppError, AppResult},
    gmail::{
        ExternalMailAttachment, ExternalMailLabel, ExternalMailMessage, MailActionResult,
        MailDraftInput, MailListInput, MailPage, MailSyncResult,
    },
    integrations::{
        CAPABILITY_MAIL_COMPOSE, CAPABILITY_MAIL_CONTENT_READ, CAPABILITY_MAIL_METADATA_READ,
        CAPABILITY_MAIL_SEND,
    },
    microsoft,
};

const API_ROOT: &str = "https://graph.microsoft.com/v1.0/me";
const CURSOR_KIND: &str = "microsoft_mail_delta";

#[derive(Debug, Deserialize)]
struct GraphPage<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    delta_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphFolder {
    id: String,
    display_name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphAddress {
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphRecipient {
    #[serde(default)]
    email_address: GraphAddress,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphBody {
    #[serde(default)]
    content: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphMessage {
    id: String,
    #[serde(default)]
    conversation_id: String,
    #[serde(default)]
    internet_message_id: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    from: GraphRecipient,
    #[serde(default)]
    to_recipients: Vec<GraphRecipient>,
    #[serde(default)]
    cc_recipients: Vec<GraphRecipient>,
    #[serde(default)]
    received_date_time: String,
    #[serde(default)]
    body_preview: String,
    body: Option<GraphBody>,
    #[serde(default)]
    has_attachments: bool,
    #[serde(rename = "@removed")]
    removed: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphAttachment {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    is_inline: bool,
    content_bytes: Option<String>,
}

impl Database {
    pub fn microsoft_mail_labels(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<Vec<ExternalMailLabel>> {
        self.require_microsoft_mail_read(connection_id)?;
        let response: GraphPage<GraphFolder> = graph(connection_id, client_id)?
            .get(format!("{API_ROOT}/mailFolders"))
            .query(&[("$top", "100"), ("$select", "id,displayName")])
            .send()?
            .error_for_status()?
            .json()?;
        Ok(response
            .value
            .into_iter()
            .map(|folder| ExternalMailLabel {
                id: folder.id,
                name: folder.display_name,
                label_type: "folder".into(),
            })
            .collect())
    }

    pub fn microsoft_mail_page(
        &self,
        client_id: &str,
        input: &MailListInput,
    ) -> AppResult<MailPage> {
        let content =
            self.has_integration_capability(&input.connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        self.require_microsoft_mail_read(&input.connection_id)?;
        let folder = input.label_id.as_deref().unwrap_or("inbox");
        let client = graph(&input.connection_id, client_id)?;
        if let Some(next_link) = input
            .page_token
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let response: GraphPage<GraphMessage> = client
                .get(next_link.into())
                .send()?
                .error_for_status()?
                .json()?;
            return Ok(mail_page(response));
        }
        let url = format!(
            "{API_ROOT}/mailFolders/{}/messages",
            urlencoding::encode(folder)
        );
        let select = if content {
            "id,conversationId,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,bodyPreview,hasAttachments"
        } else {
            "id,conversationId,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,hasAttachments"
        };
        let mut request = client.get(url).query(&[
            ("$top", input.max_results.clamp(1, 100).to_string()),
            ("$select", select.to_string()),
        ]);
        if let Some(query) = input
            .query
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            if !content {
                return Err(AppError::InvalidInput(
                    "Microsoft mail search needs the Read email content permission".into(),
                ));
            }
            request = request.query(&[("$search", format!("\"{}\"", query.trim()))]);
        } else {
            request = request.query(&[("$orderby", "receivedDateTime desc")]);
        }
        let response: GraphPage<GraphMessage> = request.send()?.error_for_status()?.json()?;
        Ok(mail_page(response))
    }

    pub fn sync_microsoft_mail_headers(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<MailSyncResult> {
        self.require_microsoft_mail_read(connection_id)?;
        let initial = format!("{API_ROOT}/mailFolders/inbox/messages/delta?$top=30&$select=id,conversationId,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,hasAttachments");
        let mut url = self
            .integration_sync_cursor(connection_id, CURSOR_KIND)?
            .unwrap_or(initial);
        let full_sync = self
            .integration_sync_cursor(connection_id, CURSOR_KIND)?
            .is_none();
        let mut messages = Vec::new();
        let delta_link = loop {
            let page: GraphPage<GraphMessage> = graph(connection_id, client_id)?
                .get(url)
                .send()?
                .error_for_status()?
                .json()?;
            messages.extend(
                page.value
                    .into_iter()
                    .filter(|message| message.removed.is_none())
                    .map(|message| external_message(message, Vec::new())),
            );
            if let Some(next) = page.next_link {
                url = next;
            } else {
                break page.delta_link.ok_or_else(|| {
                    AppError::InvalidInput("Microsoft Graph returned no delta cursor".into())
                })?;
            }
        };
        self.set_integration_sync_cursor(connection_id, CURSOR_KIND, &delta_link)?;
        Ok(MailSyncResult {
            messages,
            history_id: delta_link,
            full_sync,
        })
    }

    pub fn microsoft_mail_message(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
    ) -> AppResult<ExternalMailMessage> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        let client = graph(connection_id, client_id)?;
        let message: GraphMessage = client
            .get(format!(
                "{API_ROOT}/messages/{}",
                urlencoding::encode(message_id)
            ))
            .header("Prefer", "outlook.body-content-type=\"text\"")
            .query(&[("$select", "id,conversationId,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,bodyPreview,body,hasAttachments")])
            .send()?
            .error_for_status()?
            .json()?;
        let attachments = if message.has_attachments {
            let page: GraphPage<GraphAttachment> = client
                .get(format!(
                    "{API_ROOT}/messages/{}/attachments",
                    urlencoding::encode(message_id)
                ))
                .query(&[("$select", "id,name,contentType,size,isInline")])
                .send()?
                .error_for_status()?
                .json()?;
            page.value
                .into_iter()
                .filter(|item| !item.is_inline)
                .map(|item| ExternalMailAttachment {
                    attachment_id: item.id,
                    filename: item.name,
                    mime_type: item.content_type,
                    size_bytes: item.size,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(external_message(message, attachments))
    }

    pub fn microsoft_mail_attachment(
        &self,
        connection_id: &str,
        client_id: &str,
        message_id: &str,
        attachment_id: &str,
        mime_type: &str,
    ) -> AppResult<String> {
        self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)?;
        let attachment: GraphAttachment = graph(connection_id, client_id)?
            .get(format!(
                "{API_ROOT}/messages/{}/attachments/{}",
                urlencoding::encode(message_id),
                urlencoding::encode(attachment_id)
            ))
            .send()?
            .error_for_status()?
            .json()?;
        let content = attachment.content_bytes.ok_or_else(|| {
            AppError::InvalidInput("This Microsoft attachment is not a downloadable file".into())
        })?;
        let safe_type = if mime_type.contains(['\r', '\n', ';']) {
            "application/octet-stream"
        } else {
            mime_type
        };
        Ok(format!("data:{safe_type};base64,{content}"))
    }

    pub fn create_microsoft_mail_draft(
        &self,
        client_id: &str,
        input: &MailDraftInput,
    ) -> AppResult<MailActionResult> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_COMPOSE)?;
        validate_mail_input(input)?;
        let client = graph(&input.connection_id, client_id)?;
        let request = if let Some(source_id) = input.reply_to_external_id.as_deref() {
            client
                .post(format!(
                    "{API_ROOT}/messages/{}/createReply",
                    urlencoding::encode(source_id)
                ))
                .json(&json!({ "comment": input.body }))
        } else {
            client
                .post(format!("{API_ROOT}/messages"))
                .json(&graph_message_payload(input))
        };
        let response: GraphMessage = request.send()?.error_for_status()?.json()?;
        Ok(MailActionResult {
            id: response.id,
            thread_id: response.conversation_id,
            draft_id: None,
        })
    }

    pub fn send_microsoft_mail(
        &self,
        client_id: &str,
        input: &MailDraftInput,
    ) -> AppResult<MailActionResult> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_MAIL_SEND)?;
        validate_mail_input(input)?;
        let client = graph(&input.connection_id, client_id)?;
        let request = if let Some(source_id) = input.reply_to_external_id.as_deref() {
            client
                .post(format!(
                    "{API_ROOT}/messages/{}/reply",
                    urlencoding::encode(source_id)
                ))
                .json(&json!({ "comment": input.body }))
        } else {
            client
                .post(format!("{API_ROOT}/sendMail"))
                .json(&json!({ "message": graph_message_payload(input), "saveToSentItems": true }))
        };
        request.send()?.error_for_status()?;
        Ok(MailActionResult {
            id: "accepted".into(),
            thread_id: input.thread_id.clone().unwrap_or_default(),
            draft_id: None,
        })
    }

    fn require_microsoft_mail_read(
        &self,
        connection_id: &str,
    ) -> AppResult<crate::integrations::IntegrationSnapshot> {
        if self.has_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)? {
            self.require_integration_capability(connection_id, CAPABILITY_MAIL_CONTENT_READ)
        } else {
            self.require_integration_capability(connection_id, CAPABILITY_MAIL_METADATA_READ)
        }
    }
}

fn mail_page(response: GraphPage<GraphMessage>) -> MailPage {
    MailPage {
        messages: response
            .value
            .into_iter()
            .filter(|message| message.removed.is_none())
            .map(|message| external_message(message, Vec::new()))
            .collect(),
        next_page_token: response.next_link,
    }
}

struct GraphClient {
    client: Client,
    token: String,
}

impl GraphClient {
    fn get(&self, url: String) -> RequestBuilder {
        self.client.get(url).bearer_auth(&self.token)
    }

    fn post(&self, url: String) -> RequestBuilder {
        self.client.post(url).bearer_auth(&self.token)
    }
}

fn graph(connection_id: &str, client_id: &str) -> AppResult<GraphClient> {
    Ok(GraphClient {
        client: Client::new(),
        token: microsoft::access_token(connection_id, client_id)?,
    })
}

fn external_message(
    message: GraphMessage,
    attachments: Vec<ExternalMailAttachment>,
) -> ExternalMailMessage {
    ExternalMailMessage {
        id: message.id,
        thread_id: message.conversation_id,
        label_ids: Vec::new(),
        snippet: message.body_preview,
        history_id: String::new(),
        internal_date: message.received_date_time,
        from: format_address(&message.from.email_address),
        to: format_recipients(&message.to_recipients),
        cc: format_recipients(&message.cc_recipients),
        subject: message.subject,
        message_id: message.internet_message_id,
        references: String::new(),
        body_text: message.body.map(|body| body.content),
        body_html: None,
        attachments,
    }
}

fn format_address(address: &GraphAddress) -> String {
    if address.name.is_empty() {
        address.address.clone()
    } else {
        format!("{} <{}>", address.name, address.address)
    }
}

fn format_recipients(recipients: &[GraphRecipient]) -> String {
    recipients
        .iter()
        .map(|recipient| format_address(&recipient.email_address))
        .collect::<Vec<_>>()
        .join(", ")
}

fn graph_message_payload(input: &MailDraftInput) -> Value {
    let recipient = |address: &String| json!({ "emailAddress": { "address": address } });
    json!({
        "subject": input.subject,
        "body": { "contentType": "Text", "content": input.body },
        "toRecipients": input.to.iter().map(recipient).collect::<Vec<_>>(),
        "ccRecipients": input.cc.iter().map(recipient).collect::<Vec<_>>(),
        "bccRecipients": input.bcc.iter().map(recipient).collect::<Vec<_>>()
    })
}

fn validate_mail_input(input: &MailDraftInput) -> AppResult<()> {
    if input.to.is_empty() {
        return Err(AppError::InvalidInput(
            "At least one email recipient is required".into(),
        ));
    }
    if input.subject.contains(['\r', '\n']) {
        return Err(AppError::InvalidInput(
            "Email headers cannot contain line breaks".into(),
        ));
    }
    for address in input.to.iter().chain(&input.cc).chain(&input.bcc) {
        if !address.contains('@') || address.contains(['\r', '\n']) {
            return Err(AppError::InvalidInput(format!(
                "Invalid email recipient: {address}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_payload_keeps_recipients_and_plain_text_body() {
        let input = MailDraftInput {
            organization_id: "org".into(),
            project_id: Some("project".into()),
            connection_id: "connection".into(),
            to: vec!["person@example.com".into()],
            cc: vec!["copy@example.com".into()],
            bcc: Vec::new(),
            subject: "Update".into(),
            body: "Exact body".into(),
            thread_id: None,
            in_reply_to: None,
            references: None,
            reply_to_external_id: None,
        };
        validate_mail_input(&input).unwrap();
        let payload = graph_message_payload(&input);
        assert_eq!(payload["body"]["content"], "Exact body");
        assert_eq!(
            payload["toRecipients"][0]["emailAddress"]["address"],
            "person@example.com"
        );
    }
}
