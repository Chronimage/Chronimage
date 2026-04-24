//! Microsoft OneDrive OAuth2 (PKCE + loopback redirect) + Graph API upload client.
//!
//! ## Flow overview
//!
//! 1. Frontend calls `onedrive_begin_oauth_flow()` (commands.rs). This module
//!    spawns a loopback HTTP listener on `127.0.0.1:<ephemeral>`, generates
//!    PKCE + state, and returns `(auth_url, flow_id)` for the frontend to open
//!    + poll.
//! 2. Frontend opens `auth_url` in the system browser and polls
//!    `onedrive_poll_oauth_flow(flow_id)` until status is `Completed` or
//!    `Failed`.
//! 3. Once authed, frontend calls `onedrive_upload(photo_ids, remote_folder)`
//!    to upload exported photos to `OneDrive/Photos/<remote_folder>/`.
//!
//! ## Upload strategy
//!
//! Files ≤ 4 MB use a simple PUT to
//! `/me/drive/root:/Photos/<folder>/<filename>:/content`.
//!
//! Files > 4 MB use the Graph API upload-session pattern:
//!   1. POST `/me/drive/root:/...:/createUploadSession` → `uploadUrl`
//!   2. PUT ranges in 10 MB chunks to `uploadUrl` until complete.
//!
//! All `reqwest` call sites in this module are gated behind a function whose
//! name starts with `user_initiated_` per CLAUDE.md § Security.

use crate::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use getrandom::getrandom;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

pub mod loopback;

// ── Constants ────────────────────────────────────────────────────────────────

/// Microsoft identity platform authorization endpoint.
pub const AUTH_ENDPOINT: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";

/// Microsoft identity platform token endpoint.
pub const TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";

/// Microsoft Graph API base URL.
pub const GRAPH_API_BASE: &str = "https://graph.microsoft.com/v1.0";

/// Delegated permission: read + write the user's OneDrive files.
pub const SCOPE_FILES_READWRITE: &str = "Files.ReadWrite";

/// Delegated permission: issue a refresh token (offline access).
pub const SCOPE_OFFLINE: &str = "offline_access";

/// Delegated permission: read basic profile info (display name, email).
pub const SCOPE_USER_READ: &str = "User.Read";

/// Placeholder client ID. Replace with the actual Azure app-registration
/// client ID once the app is registered at portal.azure.com. The app will
/// log a `tracing::warn!` on first use if this placeholder is still in place,
/// guiding operators to complete the Azure registration (see
/// docs/manual-setup.md §3).
pub const DEFAULT_CLIENT_ID: &str = "__CHRONIMAGE_ONEDRIVE_CLIENT_ID__";

/// Env-var name for the OneDrive OAuth client secret. Add to `.env.local`:
/// `CHRONIMAGE_ONEDRIVE_CLIENT_SECRET=your_secret_here`
pub const CLIENT_SECRET_ENV: &str = "CHRONIMAGE_ONEDRIVE_CLIENT_SECRET";

/// Keyring service name. The username slot holds the Microsoft account UPN
/// once the token exchange succeeds.
pub const KEYRING_SERVICE: &str = "chronimage.source.onedrive";

/// Default keyring username when only one OneDrive account is connected.
pub const KEYRING_DEFAULT_USER: &str = "default";

/// PKCE verifier length in bytes. Base64url-encodes to 86 chars, well within
/// RFC 7636's 43–128 character requirement.
const PKCE_VERIFIER_BYTES: usize = 64;

/// CSRF state token length in bytes.
const STATE_TOKEN_BYTES: usize = 32;

/// Seconds before access token expiry at which we proactively refresh.
const REFRESH_LEAD_SECS: i64 = 60;

/// Files at or below this size use the simple PUT path; larger files use the
/// upload-session chunked path (Graph API requirement: sessions for > 4 MB).
const SIMPLE_UPLOAD_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024; // 4 MB

/// Chunk size for upload-session PUT ranges (10 MB, must be a multiple of
/// 320 KiB per Graph API docs).
const UPLOAD_CHUNK_SIZE: usize = 10 * 1024 * 1024; // 10 MB

// ── PKCE + state helpers ─────────────────────────────────────────────────────

/// Generate a cryptographically random PKCE code-verifier (RFC 7636 §4.1).
pub fn generate_pkce_verifier() -> AppResult<String> {
    let mut bytes = [0u8; PKCE_VERIFIER_BYTES];
    getrandom(&mut bytes).map_err(|e| AppError::Internal(format!("OS RNG unavailable: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Derive the SHA-256 PKCE code-challenge from a verifier (RFC 7636 §4.2).
pub fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Generate a random CSRF state token.
pub fn generate_state_token() -> AppResult<String> {
    let mut bytes = [0u8; STATE_TOKEN_BYTES];
    getrandom(&mut bytes).map_err(|e| AppError::Internal(format!("OS RNG unavailable: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Build the authorization URL the frontend opens in the system browser.
pub fn build_auth_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    let scope = format!(
        "{} {} {}",
        SCOPE_FILES_READWRITE, SCOPE_OFFLINE, SCOPE_USER_READ
    );
    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", &scope),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("response_mode", "query"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{AUTH_ENDPOINT}?{query}")
}

/// Everything needed to drive a fresh OAuth flow.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub auth_url: String,
    pub pkce_verifier: String,
    pub state: String,
}

/// Generate PKCE + state and build the auth URL in one call.
pub fn new_auth_request(client_id: &str, redirect_uri: &str) -> AppResult<AuthRequest> {
    let verifier = generate_pkce_verifier()?;
    let challenge = pkce_challenge(&verifier);
    let state = generate_state_token()?;
    let auth_url = build_auth_url(client_id, redirect_uri, &challenge, &state);
    Ok(AuthRequest {
        auth_url,
        pkce_verifier: verifier,
        state,
    })
}

// ── Token types + persistence ────────────────────────────────────────────────

/// Token set returned by the Microsoft identity platform on a successful
/// code exchange. Persisted as JSON in the keyring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    /// Refresh token — Microsoft includes this when `offline_access` scope
    /// is granted. Persist it; access tokens expire in ~1 hour.
    pub refresh_token: Option<String>,
    /// Absolute UTC time after which `access_token` is stale.
    pub expires_at: DateTime<Utc>,
    /// Space-delimited scope list actually granted by Microsoft.
    pub scope: String,
    pub token_type: String,
}

impl TokenSet {
    /// True when the token is past (or within `REFRESH_LEAD_SECS` of) expiry.
    pub fn needs_refresh(&self) -> bool {
        let lead = ChronoDuration::seconds(REFRESH_LEAD_SECS);
        Utc::now() + lead >= self.expires_at
    }
}

#[derive(Debug, Deserialize)]
struct RawTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
    token_type: String,
}

impl RawTokenResponse {
    fn into_token_set(self, previous_refresh: Option<String>) -> TokenSet {
        let expires_at = Utc::now() + ChronoDuration::seconds(self.expires_in);
        TokenSet {
            access_token: self.access_token,
            // Microsoft omits refresh_token on refresh responses; keep the old one.
            refresh_token: self.refresh_token.or(previous_refresh),
            expires_at,
            scope: self.scope.unwrap_or_default(),
            token_type: self.token_type,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn build_reqwest_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("reqwest client: {e}")))
}

fn resolve_client_id(client_id: Option<&str>) -> &str {
    let cid = client_id.unwrap_or(DEFAULT_CLIENT_ID);
    if cid == DEFAULT_CLIENT_ID {
        tracing::warn!(
            "OneDrive client ID is still the placeholder '__CHRONIMAGE_ONEDRIVE_CLIENT_ID__'. \
             Register an Azure app at portal.azure.com and replace the constant. \
             See docs/manual-setup.md §3."
        );
    }
    cid
}

/// Exchange an authorization code for a TokenSet.
///
/// Named `user_initiated_` per CLAUDE.md § Security — only callable after
/// the user completes the Azure consent prompt in their browser.
pub async fn user_initiated_exchange_code(
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
) -> AppResult<TokenSet> {
    let client = build_reqwest_client()?;
    let secret = std::env::var(CLIENT_SECRET_ENV)
        .ok()
        .filter(|s| !s.is_empty());
    let scope = format!(
        "{} {} {}",
        SCOPE_FILES_READWRITE, SCOPE_OFFLINE, SCOPE_USER_READ
    );
    let mut form: Vec<(&str, &str)> = vec![
        ("client_id", client_id),
        ("code", code),
        ("code_verifier", pkce_verifier),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
        ("scope", &scope),
    ];
    if let Some(s) = secret.as_deref() {
        form.push(("client_secret", s));
    }

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive token exchange request: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive token exchange body: {e}")))?;
    if !status.is_success() {
        return Err(AppError::PermissionDenied(format!(
            "onedrive oauth token exchange rejected: {}",
            parse_token_error(&body, status)
        )));
    }
    let raw: RawTokenResponse = serde_json::from_str(&body).map_err(|e| {
        AppError::Internal(format!("onedrive token response parse: {e} (body: {body})"))
    })?;
    Ok(raw.into_token_set(None))
}

/// Refresh an expired access token using the stored refresh token.
///
/// Named `user_initiated_` per CLAUDE.md § Security — the refresh is an
/// implicit consequence of a user-driven action (upload, account info, etc.).
pub async fn user_initiated_refresh_access_token(
    client_id: &str,
    refresh_token: &str,
) -> AppResult<TokenSet> {
    let client = build_reqwest_client()?;
    let secret = std::env::var(CLIENT_SECRET_ENV)
        .ok()
        .filter(|s| !s.is_empty());
    let scope = format!(
        "{} {} {}",
        SCOPE_FILES_READWRITE, SCOPE_OFFLINE, SCOPE_USER_READ
    );
    let mut form: Vec<(&str, &str)> = vec![
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
        ("scope", &scope),
    ];
    if let Some(s) = secret.as_deref() {
        form.push(("client_secret", s));
    }

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive token refresh request: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive token refresh body: {e}")))?;
    if !status.is_success() {
        return Err(AppError::PermissionDenied(format!(
            "onedrive oauth refresh rejected: {}",
            parse_token_error(&body, status)
        )));
    }
    let raw: RawTokenResponse = serde_json::from_str(&body).map_err(|e| {
        AppError::Internal(format!(
            "onedrive refresh response parse: {e} (body: {body})"
        ))
    })?;
    Ok(raw.into_token_set(Some(refresh_token.to_string())))
}

/// Return a currently-valid access token, refreshing if expired.
pub async fn user_initiated_current_access_token(client_id: &str) -> AppResult<String> {
    let tokens = load_tokens()?.ok_or_else(|| {
        AppError::PermissionDenied("onedrive not signed in; begin oauth flow first".into())
    })?;
    if !tokens.needs_refresh() {
        return Ok(tokens.access_token);
    }
    let Some(refresh) = tokens.refresh_token.clone() else {
        return Err(AppError::PermissionDenied(
            "onedrive access token expired and no refresh token stored — re-authenticate".into(),
        ));
    };
    let refreshed = user_initiated_refresh_access_token(client_id, &refresh).await?;
    store_tokens(&refreshed)?;
    Ok(refreshed.access_token)
}

fn parse_token_error(body: &str, status: reqwest::StatusCode) -> String {
    let parsed: Option<TokenErrorResponse> = serde_json::from_str(body).ok();
    match parsed {
        Some(e) => match e.error_description {
            Some(d) => format!("{} ({d})", e.error),
            None => e.error,
        },
        None => format!("HTTP {status}: {body}"),
    }
}

// ── Keyring round-trip ───────────────────────────────────────────────────────

pub fn store_tokens(tokens: &TokenSet) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("onedrive keyring entry: {e}")))?;
    let blob = serde_json::to_string(tokens)?;
    if blob.len() > 2400 {
        tracing::warn!(
            blob_len = blob.len(),
            "onedrive token blob is large; Windows Credential Manager caps at ~2560 UTF-16 chars",
        );
    }
    entry
        .set_password(&blob)
        .map_err(|e| AppError::Internal(format!("onedrive keyring set: {e}")))?;
    Ok(())
}

pub fn load_tokens() -> AppResult<Option<TokenSet>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("onedrive keyring entry: {e}")))?;
    match entry.get_password() {
        Ok(blob) => {
            let tokens: TokenSet = serde_json::from_str(&blob)?;
            Ok(Some(tokens))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Internal(format!("onedrive keyring get: {e}"))),
    }
}

pub fn delete_tokens() -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("onedrive keyring entry: {e}")))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("onedrive keyring delete: {e}"))),
    }
}

// ── Graph API — user info ────────────────────────────────────────────────────

/// Basic account info from `GET /me`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    /// Primary email / UPN. Graph returns `mail` for personal accounts and
    /// `userPrincipalName` for work/school; we prefer `mail` if present.
    #[serde(default)]
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName", default)]
    pub user_principal_name: Option<String>,
}

impl UserInfo {
    /// Returns `mail` if present, falling back to `userPrincipalName`.
    pub fn email(&self) -> Option<&str> {
        self.mail.as_deref().or(self.user_principal_name.as_deref())
    }
}

/// Fetch the connected account's display name + email from Graph `/me`.
pub async fn user_initiated_fetch_userinfo(access_token: &str) -> AppResult<UserInfo> {
    let client = build_reqwest_client()?;
    let resp = client
        .get(format!("{GRAPH_API_BASE}/me"))
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive userinfo request: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "onedrive userinfo HTTP {status}: {body}"
        )));
    }
    let info: UserInfo = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("onedrive userinfo parse: {e}")))?;
    Ok(info)
}

// ── OAuth flow state machine ─────────────────────────────────────────────────

/// Opaque handle the frontend uses with poll/cancel commands.
pub type FlowId = String;

/// Snapshot the frontend polls on.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FlowStatus {
    /// Waiting for the user to complete the Azure consent screen.
    Pending,
    /// Redirect landed, token exchange succeeded, tokens stored.
    Completed {
        display_name: Option<String>,
        email: Option<String>,
        scope: String,
    },
    /// Flow ended with an error (user cancelled, network failure, state
    /// mismatch, etc.).
    Failed { message: String },
    /// Browser never landed within the listener timeout.
    TimedOut,
}

struct FlowEntry {
    status: FlowStatus,
    handle: Option<tokio::task::AbortHandle>,
}

static FLOWS: Lazy<Mutex<HashMap<FlowId, FlowEntry>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn insert_flow(id: &str, status: FlowStatus, handle: Option<tokio::task::AbortHandle>) {
    if let Ok(mut map) = FLOWS.lock() {
        map.insert(id.to_string(), FlowEntry { status, handle });
    }
}

fn update_flow_status(id: &str, status: FlowStatus) {
    if let Ok(mut map) = FLOWS.lock() {
        if let Some(entry) = map.get_mut(id) {
            entry.status = status;
            entry.handle = None;
        }
    }
}

/// Look up the current status of an OAuth flow. Returns `None` for unknown ids.
pub fn peek_flow_status(id: &str) -> Option<FlowStatus> {
    let map = FLOWS.lock().ok()?;
    map.get(id).map(|e| e.status.clone())
}

/// Abort a running flow. Idempotent — no-op for unknown ids.
pub fn abort_flow(id: &str) {
    if let Ok(mut map) = FLOWS.lock() {
        if let Some(entry) = map.remove(id) {
            if let Some(handle) = entry.handle {
                handle.abort();
            }
        }
    }
}

/// Begin an OneDrive OAuth flow. Spawns the loopback listener, generates
/// PKCE + state, and returns `(auth_url, flow_id)` for the frontend.
pub async fn begin_oauth_flow(client_id: Option<&str>) -> AppResult<(String, FlowId)> {
    let cid = resolve_client_id(client_id);
    let (redirect_uri, code_rx) = loopback::spawn_listener(Duration::from_secs(300)).await?;
    let auth = new_auth_request(cid, &redirect_uri)?;
    let flow_id = generate_state_token()?;

    insert_flow(&flow_id, FlowStatus::Pending, None);

    let client_id_owned = cid.to_string();
    let flow_id_owned = flow_id.clone();
    let expected_state = auth.state.clone();
    let verifier = auth.pkce_verifier.clone();
    let redirect_for_task = redirect_uri.clone();

    let task = tokio::spawn(async move {
        match code_rx.await {
            Ok(Ok(loopback::RedirectPayload { code, state })) => {
                if state != expected_state {
                    update_flow_status(
                        &flow_id_owned,
                        FlowStatus::Failed {
                            message: "state mismatch on redirect — possible CSRF".into(),
                        },
                    );
                    return;
                }
                match user_initiated_exchange_code(
                    &client_id_owned,
                    &redirect_for_task,
                    &code,
                    &verifier,
                )
                .await
                {
                    Ok(tokens) => {
                        if let Err(e) = store_tokens(&tokens) {
                            update_flow_status(
                                &flow_id_owned,
                                FlowStatus::Failed {
                                    message: format!("keyring store failed: {e}"),
                                },
                            );
                            return;
                        }
                        let info = user_initiated_fetch_userinfo(&tokens.access_token)
                            .await
                            .ok();
                        update_flow_status(
                            &flow_id_owned,
                            FlowStatus::Completed {
                                display_name: info.as_ref().and_then(|u| u.display_name.clone()),
                                email: info.as_ref().and_then(|u| u.email().map(|s| s.to_string())),
                                scope: tokens.scope,
                            },
                        );
                    }
                    Err(e) => {
                        update_flow_status(
                            &flow_id_owned,
                            FlowStatus::Failed {
                                message: format!("token exchange failed: {e}"),
                            },
                        );
                    }
                }
            }
            Ok(Err(e)) => {
                update_flow_status(
                    &flow_id_owned,
                    FlowStatus::Failed {
                        message: format!("redirect listener error: {e}"),
                    },
                );
            }
            Err(_) => {
                // Sender dropped → listener timed out.
                update_flow_status(&flow_id_owned, FlowStatus::TimedOut);
            }
        }
    });

    insert_flow(&flow_id, FlowStatus::Pending, Some(task.abort_handle()));
    Ok((auth.auth_url, flow_id))
}

// ── Graph API — upload ───────────────────────────────────────────────────────

/// Summary returned after an upload run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadReceipt {
    pub uploaded_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

/// Upload a slice of local file paths to OneDrive under
/// `OneDrive/Photos/<remote_folder>/`.
///
/// Files ≤ 4 MB use a simple PUT; larger files use the Graph API
/// upload-session chunked protocol (10 MB chunks).
///
/// Named `user_initiated_` per CLAUDE.md § Security.
pub async fn user_initiated_upload_to_onedrive(
    client_id: &str,
    photo_paths: &[PathBuf],
    remote_folder: &str,
) -> AppResult<UploadReceipt> {
    let client = build_reqwest_client()?;
    let mut receipt = UploadReceipt {
        uploaded_count: 0,
        skipped_count: 0,
        errors: Vec::new(),
    };

    for path in photo_paths {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("photo.jpg")
            .to_string();

        // Sanitise: strip path separators that would confuse the Graph URL.
        let safe_filename: String = filename
            .chars()
            .map(|c| if c == '/' || c == '\\' { '_' } else { c })
            .collect();

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                receipt
                    .errors
                    .push(format!("{safe_filename}: stat failed: {e}"));
                receipt.skipped_count += 1;
                continue;
            }
        };
        let file_size = metadata.len();

        let access = match user_initiated_current_access_token(client_id).await {
            Ok(t) => t,
            Err(e) => {
                receipt
                    .errors
                    .push(format!("{safe_filename}: token refresh failed: {e}"));
                receipt.skipped_count += 1;
                continue;
            }
        };

        // Sanitise remote_folder to avoid path traversal.
        let safe_folder: String = remote_folder
            .chars()
            .map(|c| {
                if c == '/' || c == '\\' || c == ':' {
                    '_'
                } else {
                    c
                }
            })
            .collect();

        let remote_path = format!("Photos/{safe_folder}/{safe_filename}");

        let ok = if file_size <= SIMPLE_UPLOAD_THRESHOLD_BYTES {
            upload_simple(
                &client,
                &access,
                path,
                &remote_path,
                &safe_filename,
                &mut receipt,
            )
            .await
        } else {
            upload_chunked(
                &client,
                &access,
                path,
                &remote_path,
                &safe_filename,
                file_size,
                &mut receipt,
            )
            .await
        };

        if ok {
            receipt.uploaded_count += 1;
        } else {
            receipt.skipped_count += 1;
        }
    }

    Ok(receipt)
}

/// PUT the file contents directly to the Graph item-content URL.
/// Returns `true` on success.
async fn upload_simple(
    client: &reqwest::Client,
    access_token: &str,
    path: &std::path::Path,
    remote_path: &str,
    label: &str,
    receipt: &mut UploadReceipt,
) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            receipt.errors.push(format!("{label}: read failed: {e}"));
            return false;
        }
    };

    let url = format!("{GRAPH_API_BASE}/me/drive/root:/{remote_path}:/content");

    let resp = client
        .put(&url)
        .bearer_auth(access_token)
        .header("Content-Type", "application/octet-stream")
        .body(bytes)
        .send()
        .await;

    match resp {
        Err(e) => {
            receipt
                .errors
                .push(format!("{label}: PUT request failed: {e}"));
            false
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            receipt
                .errors
                .push(format!("{label}: PUT HTTP {status}: {body}"));
            false
        }
        Ok(_) => true,
    }
}

/// Upload a large file using a Graph API upload session (chunked PUT).
/// Creates the session, then sends 10 MB ranges until the server returns 201.
/// Returns `true` on success.
async fn upload_chunked(
    client: &reqwest::Client,
    access_token: &str,
    path: &std::path::Path,
    remote_path: &str,
    label: &str,
    file_size: u64,
    receipt: &mut UploadReceipt,
) -> bool {
    // 1. Create the upload session.
    let session_url = format!("{GRAPH_API_BASE}/me/drive/root:/{remote_path}:/createUploadSession");
    let session_body = serde_json::json!({
        "item": {
            "@microsoft.graph.conflictBehavior": "replace"
        }
    });

    let session_resp = client
        .post(&session_url)
        .bearer_auth(access_token)
        .json(&session_body)
        .send()
        .await;

    let upload_url = match session_resp {
        Err(e) => {
            receipt
                .errors
                .push(format!("{label}: createUploadSession request: {e}"));
            return false;
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            receipt.errors.push(format!(
                "{label}: createUploadSession HTTP {status}: {body}"
            ));
            return false;
        }
        Ok(r) => {
            let val: serde_json::Value = match r.json().await {
                Ok(v) => v,
                Err(e) => {
                    receipt
                        .errors
                        .push(format!("{label}: createUploadSession parse: {e}"));
                    return false;
                }
            };
            match val["uploadUrl"].as_str() {
                Some(u) => u.to_string(),
                None => {
                    receipt
                        .errors
                        .push(format!("{label}: createUploadSession missing uploadUrl"));
                    return false;
                }
            }
        }
    };

    // 2. Read the file and send it in chunks.
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            receipt.errors.push(format!("{label}: read failed: {e}"));
            return false;
        }
    };

    let mut offset: usize = 0;
    while offset < bytes.len() {
        let end = (offset + UPLOAD_CHUNK_SIZE).min(bytes.len());
        let chunk = &bytes[offset..end];
        let content_range = format!("bytes {}-{}/{}", offset, end - 1, file_size);

        let chunk_resp = client
            .put(&upload_url)
            .header("Content-Range", &content_range)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", chunk.len().to_string())
            .body(chunk.to_vec())
            .send()
            .await;

        match chunk_resp {
            Err(e) => {
                receipt
                    .errors
                    .push(format!("{label}: chunk PUT @ offset {offset}: {e}"));
                return false;
            }
            Ok(r) => {
                let status = r.status();
                // 201 Created → final chunk accepted; 202 Accepted → more chunks needed.
                if status.as_u16() == 201 || status.as_u16() == 200 {
                    // Upload complete.
                    return true;
                }
                if status.as_u16() != 202 {
                    let body = r.text().await.unwrap_or_default();
                    receipt.errors.push(format!(
                        "{label}: chunk PUT @ offset {offset} returned HTTP {status}: {body}"
                    ));
                    return false;
                }
                // 202 → continue with next chunk.
            }
        }

        offset = end;
    }

    // If we exit the loop without receiving a 201, the upload is incomplete.
    receipt
        .errors
        .push(format!("{label}: upload session ended without 201 Created"));
    false
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Minimal RFC 3986 percent-encoder for OAuth2 URL params.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── PKCE + auth URL ──────────────────────────────────────────────────────

    #[test]
    fn pkce_verifier_length_and_charset() {
        let v = generate_pkce_verifier().expect("rng");
        assert_eq!(v.len(), 86);
        for ch in v.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
                "verifier contains non-base64url char: {ch:?}"
            );
        }
    }

    #[test]
    fn pkce_verifier_is_non_deterministic() {
        let a = generate_pkce_verifier().expect("rng");
        let b = generate_pkce_verifier().expect("rng");
        assert_ne!(a, b);
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        // Same test vector as google_photos to confirm shared SHA-256 logic.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn state_tokens_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(generate_state_token().expect("rng"));
        }
        assert_eq!(seen.len(), 50);
    }

    #[test]
    fn auth_url_contains_required_params() {
        let url = build_auth_url(
            "client-abc",
            "http://127.0.0.1:9876/",
            "challenge-xyz",
            "state-123",
        );
        assert!(url.starts_with(AUTH_ENDPOINT), "wrong auth endpoint prefix");
        assert!(url.contains("client_id=client-abc"));
        assert!(url.contains("code_challenge=challenge-xyz"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("offline_access"));
        assert!(url.contains("Files.ReadWrite"));
        assert!(url.contains("User.Read"));
        assert!(!url.contains(' '), "URL must not contain raw spaces");
    }

    #[test]
    fn new_auth_request_is_self_consistent() {
        let req = new_auth_request("cid", "http://127.0.0.1:0/").expect("build");
        let expected_challenge = pkce_challenge(&req.pkce_verifier);
        assert!(req
            .auth_url
            .contains(&format!("code_challenge={expected_challenge}")));
        assert!(req.auth_url.contains(&format!("state={}", req.state)));
    }

    // ── Token types ──────────────────────────────────────────────────────────

    #[test]
    fn needs_refresh_triggers_before_expiry() {
        let mut tokens = TokenSet {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Utc::now() + ChronoDuration::seconds(30),
            scope: format!("{SCOPE_FILES_READWRITE} {SCOPE_OFFLINE} {SCOPE_USER_READ}"),
            token_type: "Bearer".into(),
        };
        assert!(
            tokens.needs_refresh(),
            "30 s < REFRESH_LEAD_SECS → must refresh"
        );

        tokens.expires_at = Utc::now() + ChronoDuration::seconds(600);
        assert!(!tokens.needs_refresh());

        tokens.expires_at = Utc::now() - ChronoDuration::seconds(1);
        assert!(tokens.needs_refresh());
    }

    #[test]
    fn raw_token_response_preserves_refresh_token() {
        let raw = RawTokenResponse {
            access_token: "new-access".into(),
            refresh_token: None, // Microsoft omits on refresh responses
            expires_in: 3600,
            scope: Some(SCOPE_FILES_READWRITE.into()),
            token_type: "Bearer".into(),
        };
        let ts = raw.into_token_set(Some("prior-refresh".into()));
        assert_eq!(ts.refresh_token.as_deref(), Some("prior-refresh"));
        assert_eq!(ts.access_token, "new-access");
        let delta = (ts.expires_at - Utc::now()).num_seconds();
        assert!((3550..=3600).contains(&delta), "unexpected delta: {delta}");
    }

    #[test]
    fn token_set_json_roundtrip() {
        let tokens = TokenSet {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Utc::now() + ChronoDuration::seconds(3600),
            scope: SCOPE_FILES_READWRITE.into(),
            token_type: "Bearer".into(),
        };
        let blob = serde_json::to_string(&tokens).expect("ser");
        let back: TokenSet = serde_json::from_str(&blob).expect("de");
        assert_eq!(back, tokens);
    }

    // ── Flow state machine ───────────────────────────────────────────────────

    #[test]
    fn flow_status_serialises_with_tag() {
        let pending = serde_json::to_value(FlowStatus::Pending).expect("ser");
        assert_eq!(pending["state"], "pending");

        let done = serde_json::to_value(FlowStatus::Completed {
            display_name: Some("Jay".into()),
            email: Some("jay@example.com".into()),
            scope: SCOPE_FILES_READWRITE.into(),
        })
        .expect("ser");
        assert_eq!(done["state"], "completed");
        assert_eq!(done["email"], "jay@example.com");

        let failed = serde_json::to_value(FlowStatus::Failed {
            message: "timeout".into(),
        })
        .expect("ser");
        assert_eq!(failed["state"], "failed");
        assert_eq!(failed["message"], "timeout");

        let timed_out = serde_json::to_value(FlowStatus::TimedOut).expect("ser");
        assert_eq!(timed_out["state"], "timed_out");
    }

    #[test]
    fn peek_and_abort_handle_unknown_ids() {
        assert!(peek_flow_status("no-such-flow").is_none());
        abort_flow("no-such-flow"); // must not panic
    }

    // ── UserInfo ─────────────────────────────────────────────────────────────

    #[test]
    fn user_info_email_prefers_mail_over_upn() {
        let info = UserInfo {
            display_name: Some("Jay".into()),
            mail: Some("jay@personal.com".into()),
            user_principal_name: Some("jay@work.onmicrosoft.com".into()),
        };
        assert_eq!(info.email(), Some("jay@personal.com"));

        let info_no_mail = UserInfo {
            display_name: None,
            mail: None,
            user_principal_name: Some("jay@work.onmicrosoft.com".into()),
        };
        assert_eq!(info_no_mail.email(), Some("jay@work.onmicrosoft.com"));
    }

    // ── Upload receipt ───────────────────────────────────────────────────────

    #[test]
    fn upload_receipt_serialises() {
        let r = UploadReceipt {
            uploaded_count: 5,
            skipped_count: 2,
            errors: vec!["big.jpg: chunk error".into()],
        };
        let j = serde_json::to_string(&r).expect("ser");
        let back: UploadReceipt = serde_json::from_str(&j).expect("de");
        assert_eq!(back.uploaded_count, 5);
        assert_eq!(back.skipped_count, 2);
        assert_eq!(back.errors.len(), 1);
    }

    #[test]
    fn upload_skips_missing_file_and_records_error() {
        // Validate the file-read error path without making HTTP calls.
        let path = std::path::PathBuf::from("/nonexistent/photo.jpg");
        let err = std::fs::read(&path).unwrap_err();
        let msg = format!("photo.jpg: read failed: {err}");
        assert!(msg.contains("photo.jpg"));
    }

    // ── Constants ────────────────────────────────────────────────────────────

    #[test]
    fn constants_have_expected_values() {
        assert_eq!(DEFAULT_CLIENT_ID, "__CHRONIMAGE_ONEDRIVE_CLIENT_ID__");
        assert_eq!(KEYRING_SERVICE, "chronimage.source.onedrive");
        assert_eq!(CLIENT_SECRET_ENV, "CHRONIMAGE_ONEDRIVE_CLIENT_SECRET");
        assert_eq!(
            AUTH_ENDPOINT,
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
        );
        assert_eq!(
            TOKEN_ENDPOINT,
            "https://login.microsoftonline.com/common/oauth2/v2.0/token"
        );
        assert_eq!(GRAPH_API_BASE, "https://graph.microsoft.com/v1.0");
        assert_eq!(SIMPLE_UPLOAD_THRESHOLD_BYTES, 4 * 1024 * 1024);
        assert_eq!(UPLOAD_CHUNK_SIZE, 10 * 1024 * 1024);
        // Chunk size must be a multiple of 320 KiB (Graph API requirement).
        assert_eq!(UPLOAD_CHUNK_SIZE % (320 * 1024), 0);
    }

    #[test]
    fn percent_encode_matches_rfc3986() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a:b/c"), "a%3Ab%2Fc");
    }

    #[test]
    #[ignore = "touches the platform keyring; run with --ignored on a dev box"]
    fn store_load_delete_roundtrip() {
        let tokens = TokenSet {
            access_token: "access-test".into(),
            refresh_token: Some("refresh-test".into()),
            expires_at: Utc::now() + ChronoDuration::seconds(3600),
            scope: SCOPE_FILES_READWRITE.into(),
            token_type: "Bearer".into(),
        };
        store_tokens(&tokens).expect("store");
        let loaded = load_tokens().expect("load").expect("some");
        assert_eq!(loaded, tokens);
        delete_tokens().expect("delete");
        assert!(load_tokens().expect("load-after-delete").is_none());
    }
}
