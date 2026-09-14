//! Google desktop OAuth and Calendar API primitives.
//!
//! Authorization uses the system browser, PKCE and a random loopback port. Tokens are serialized
//! only into the operating system credential store. Installed Google clients do not support
//! incremental authorization, so callers request the complete union of locally enabled scopes when
//! adding a capability.

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

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

pub const SCOPE_IDENTITY: [&str; 3] = ["openid", "email", "profile"];
pub const SCOPE_CALENDAR_READ: &str = "https://www.googleapis.com/auth/calendar.readonly";
pub const SCOPE_CALENDAR_WRITE: &str = "https://www.googleapis.com/auth/calendar.events";
pub const SCOPE_CALENDAR_FREE_BUSY: &str = "https://www.googleapis.com/auth/calendar.freebusy";
pub const SCOPE_GMAIL_METADATA: &str = "https://www.googleapis.com/auth/gmail.metadata";
pub const SCOPE_GMAIL_READONLY: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const SCOPE_GMAIL_COMPOSE: &str = "https://www.googleapis.com/auth/gmail.compose";
pub const SCOPE_GMAIL_SEND: &str = "https://www.googleapis.com/auth/gmail.send";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GoogleToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    scope: String,
}

impl GoogleToken {
    pub fn grants(&self, scopes: &[&str]) -> bool {
        let granted = self.scope.split_whitespace().collect::<Vec<_>>();
        scopes.iter().all(|scope| granted.contains(scope))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct GoogleProfile {
    pub email: String,
    #[serde(default)]
    pub name: String,
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

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

pub fn authorize(
    client_id: &str,
    scopes: &[&str],
    login_hint: Option<&str>,
) -> AppResult<(GoogleToken, GoogleProfile)> {
    if client_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Add the Threadbox Google desktop OAuth client ID before connecting an account".into(),
        ));
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let redirect_uri = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = Uuid::new_v4().simple().to_string();
    let mut url = Url::parse(AUTH_URL)
        .map_err(|error| AppError::InvalidInput(format!("Invalid Google OAuth URL: {error}")))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", client_id.trim())
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &scopes.join(" "))
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");
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
            "Google authorization returned an invalid state value".into(),
        ));
    }
    if let Some((_, error)) = callback.query_pairs().find(|(key, _)| key == "error") {
        return Err(AppError::InvalidInput(format!(
            "Google authorization was not completed: {error}"
        )));
    }
    let code = callback
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| AppError::InvalidInput("Google returned no authorization code".into()))?;
    let response: TokenResponse = Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.trim()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    let token = GoogleToken {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: Utc::now().timestamp() + response.expires_in,
        scope: response.scope,
    };
    let profile = profile(&token.access_token)?;
    Ok((token, profile))
}

pub fn store_token(connection_id: &str, token: &GoogleToken) -> AppResult<()> {
    secrets::store(
        &secrets::integration_account("google", connection_id),
        &serde_json::to_string(token)?,
    )
}

pub fn store_authorized_token(connection_id: &str, mut token: GoogleToken) -> AppResult<()> {
    if token.refresh_token.is_empty() {
        let account = secrets::integration_account("google", connection_id);
        if let Some(encoded) = secrets::read(&account)? {
            let existing: GoogleToken = serde_json::from_str(&encoded)?;
            token.refresh_token = existing.refresh_token;
        }
    }
    store_token(connection_id, &token)
}

pub fn delete_token(connection_id: &str) -> AppResult<()> {
    secrets::delete(&secrets::integration_account("google", connection_id))
}

pub fn access_token(connection_id: &str, client_id: &str) -> AppResult<String> {
    let account = secrets::integration_account("google", connection_id);
    let encoded = secrets::read(&account)?.ok_or_else(|| {
        AppError::InvalidInput("The Google connection has no stored credential".into())
    })?;
    let mut token: GoogleToken = serde_json::from_str(&encoded)?;
    if token_is_fresh(token.expires_at, Utc::now().timestamp()) {
        return Ok(token.access_token);
    }
    if token.refresh_token.is_empty() {
        return Err(AppError::InvalidInput(
            "The Google connection must be authorized again".into(),
        ));
    }
    let response: RefreshResponse = Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.trim()),
            ("refresh_token", token.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    token.access_token = response.access_token;
    token.expires_at = Utc::now().timestamp() + response.expires_in;
    if !response.scope.is_empty() {
        token.scope = response.scope;
    }
    store_token(connection_id, &token)?;
    Ok(token.access_token)
}

fn token_is_fresh(expires_at: i64, now: i64) -> bool {
    expires_at > now + 60
}

fn profile(access_token: &str) -> AppResult<GoogleProfile> {
    Ok(Client::new()
        .get(USERINFO_URL)
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
                        AppError::InvalidInput("Could not read the Google OAuth callback".into())
                    })?;
                let callback =
                    Url::parse(&format!("http://127.0.0.1{target}")).map_err(|error| {
                        AppError::InvalidInput(format!("Invalid OAuth callback: {error}"))
                    })?;
                let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>Threadbox connected</title><p>You can close this window and return to Threadbox.</p>";
                stream.write_all(response.as_bytes())?;
                return Ok(callback);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::InvalidInput(
        "Google authorization timed out after five minutes".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_shape_is_accepted_by_the_google_contract() {
        let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        assert!((43..=128).contains(&verifier.len()));
        assert!(verifier.chars().all(|value| value.is_ascii_alphanumeric()));
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert!(!challenge.contains('='));
    }

    #[test]
    fn capability_is_not_recorded_when_google_omits_a_requested_scope() {
        let token = GoogleToken {
            access_token: "token".into(),
            refresh_token: "refresh".into(),
            expires_at: 0,
            scope: format!("openid email profile {SCOPE_GMAIL_SEND}"),
        };
        assert!(token.grants(&["openid", SCOPE_GMAIL_SEND]));
        assert!(!token.grants(&["openid", SCOPE_GMAIL_READONLY]));
    }

    #[test]
    fn refreshes_before_a_google_token_expires() {
        assert!(token_is_fresh(1_061, 1_000));
        assert!(!token_is_fresh(1_060, 1_000));
        assert!(!token_is_fresh(999, 1_000));
    }
}
