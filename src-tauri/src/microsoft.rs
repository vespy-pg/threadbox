//! Microsoft identity desktop OAuth with PKCE and operating-system credential storage.

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    secrets,
};

const AUTH_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const PROFILE_URL: &str =
    "https://graph.microsoft.com/v1.0/me?$select=displayName,mail,userPrincipalName";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

pub const SCOPE_IDENTITY: [&str; 4] = ["openid", "profile", "email", "offline_access"];
pub const SCOPE_MAIL_METADATA: &str = "https://graph.microsoft.com/Mail.ReadBasic";
pub const SCOPE_MAIL_READ: &str = "https://graph.microsoft.com/Mail.Read";
pub const SCOPE_MAIL_COMPOSE: &str = "https://graph.microsoft.com/Mail.ReadWrite";
pub const SCOPE_MAIL_SEND: &str = "https://graph.microsoft.com/Mail.Send";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MicrosoftToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    scope: String,
}

impl MicrosoftToken {
    pub fn grants(&self, scopes: &[&str]) -> bool {
        let granted = self
            .scope
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        scopes
            .iter()
            .all(|scope| granted.contains(&scope.to_ascii_lowercase()))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftProfile {
    #[serde(default)]
    pub display_name: String,
    pub mail: Option<String>,
    pub user_principal_name: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

pub fn authorize(
    client_id: &str,
    scopes: &[&str],
    login_hint: Option<&str>,
) -> AppResult<(MicrosoftToken, MicrosoftProfile)> {
    if client_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Add the Threadbox Microsoft desktop OAuth client ID before connecting an account"
                .into(),
        ));
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let redirect_uri = format!("http://localhost:{}", listener.local_addr()?.port());
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = Uuid::new_v4().simple().to_string();
    let mut url = Url::parse(AUTH_URL)
        .map_err(|error| AppError::InvalidInput(format!("Invalid Microsoft OAuth URL: {error}")))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", client_id.trim())
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("response_mode", "query")
            .append_pair("scope", &scopes.join(" "))
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("prompt", "select_account");
        if let Some(login_hint) = login_hint {
            query.append_pair("login_hint", login_hint);
        }
    }
    open::that(url.as_str())?;
    let callback = wait_for_callback(&listener)?;
    if callback
        .query_pairs()
        .find(|(key, _)| key == "state")
        .is_none_or(|(_, value)| value != state)
    {
        return Err(AppError::InvalidInput(
            "Microsoft authorization returned an invalid state value".into(),
        ));
    }
    if let Some((_, error)) = callback.query_pairs().find(|(key, _)| key == "error") {
        return Err(AppError::InvalidInput(format!(
            "Microsoft authorization was not completed: {error}"
        )));
    }
    let code = callback
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| AppError::InvalidInput("Microsoft returned no authorization code".into()))?;
    let scope_text = scopes.join(" ");
    let response: TokenResponse = Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.trim()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", scope_text.as_str()),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    let token = MicrosoftToken {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: Utc::now().timestamp() + response.expires_in,
        scope: response.scope,
    };
    let profile = profile(&token.access_token)?;
    Ok((token, profile))
}

pub fn store_authorized_token(connection_id: &str, mut token: MicrosoftToken) -> AppResult<()> {
    let account = secrets::integration_account("microsoft", connection_id);
    if token.refresh_token.is_empty() {
        if let Some(encoded) = secrets::read(&account)? {
            token.refresh_token = serde_json::from_str::<MicrosoftToken>(&encoded)?.refresh_token;
        }
    }
    secrets::store(&account, &serde_json::to_string(&token)?)
}

pub fn delete_token(connection_id: &str) -> AppResult<()> {
    secrets::delete(&secrets::integration_account("microsoft", connection_id))
}

pub fn access_token(connection_id: &str, client_id: &str) -> AppResult<String> {
    let account = secrets::integration_account("microsoft", connection_id);
    let encoded = secrets::read(&account)?.ok_or_else(|| {
        AppError::InvalidInput("The Microsoft connection has no stored credential".into())
    })?;
    let mut token: MicrosoftToken = serde_json::from_str(&encoded)?;
    if token_is_fresh(token.expires_at, Utc::now().timestamp()) {
        return Ok(token.access_token);
    }
    let scopes = token.scope.clone();
    let response: TokenResponse = Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.trim()),
            ("refresh_token", token.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
            ("scope", scopes.as_str()),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    token.access_token = response.access_token;
    token.expires_at = Utc::now().timestamp() + response.expires_in;
    if !response.refresh_token.is_empty() {
        token.refresh_token = response.refresh_token;
    }
    if !response.scope.is_empty() {
        token.scope = response.scope;
    }
    secrets::store(&account, &serde_json::to_string(&token)?)?;
    Ok(token.access_token)
}

fn token_is_fresh(expires_at: i64, now: i64) -> bool {
    expires_at > now + 60
}

fn profile(access_token: &str) -> AppResult<MicrosoftProfile> {
    Ok(Client::new()
        .get(PROFILE_URL)
        .bearer_auth(access_token)
        .send()?
        .error_for_status()?
        .json()?)
}

fn wait_for_callback(listener: &TcpListener) -> AppResult<Url> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let mut buffer = [0_u8; 8192];
                let count = stream.read(&mut buffer)?;
                let request = String::from_utf8_lossy(&buffer[..count]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| {
                        AppError::InvalidInput("Could not read the OAuth callback".into())
                    })?;
                let callback =
                    Url::parse(&format!("http://localhost{target}")).map_err(|error| {
                        AppError::InvalidInput(format!("Invalid OAuth callback: {error}"))
                    })?;
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>Threadbox connected</title><p>You can close this window and return to Threadbox.</p>")?;
                return Ok(callback);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::InvalidInput(
        "Microsoft authorization timed out after five minutes".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_checks_are_case_insensitive() {
        let token = MicrosoftToken {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            scope: "openid Mail.Send".into(),
        };
        assert!(token.grants(&["OPENID", "mail.send"]));
        assert!(!token.grants(&["Mail.Read"]));
    }

    #[test]
    fn refreshes_before_a_microsoft_token_expires() {
        assert!(token_is_fresh(1_061, 1_000));
        assert!(!token_is_fresh(1_060, 1_000));
        assert!(!token_is_fresh(999, 1_000));
    }
}
