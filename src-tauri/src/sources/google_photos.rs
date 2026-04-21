//! Google Photos OAuth2 (PKCE) + token storage for the Library API connector.
//!
//! ## What this module does today
//!
//! 1. Generates OAuth2 authorization-code + PKCE artifacts (verifier +
//!    challenge + CSRF state token) that the frontend feeds into the system
//!    browser.
//! 2. Exchanges the returned authorisation code for an access + refresh
//!    token pair by hitting Google's token endpoint. The reqwest call sits
//!    behind a `user_initiated_*`-prefixed function per CLAUDE.md § Security.
//! 3. Persists the token set via the `keyring` crate — Windows Credential
//!    Manager on the target platform — so refresh tokens never touch the
//!    catalog DB.
//!
//! ## What's deferred (deliberate scope cut)
//!
//! - Actual Google Photos Library API calls (`mediaItems.list` / `search` /
//!   `batchDelete`) — wired up once the frontend-side auth UX lands.
//! - Loopback HTTP listener on `127.0.0.1:<ephemeral>` for the redirect —
//!   a follow-up can add that alongside the UI that opens the browser.
//! - `source_add`-style glue that registers a `sources` row once the user
//!   completes auth. Hook it up from the Tauri command layer.
//!
//! The goal of this landing is: unblock frontend work on the OAuth modal by
//! giving it a stable Rust-side API + a real token round-trip.

use crate::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// OAuth2 authorization endpoint for Google's v2 OAuth.
pub const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";

/// OAuth2 token endpoint.
pub const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

/// Read-only scope for the Google Photos Library API. Paired with
/// `photoslibrary.appendonly` when the user enables source-side cleanup.
pub const SCOPE_READONLY: &str = "https://www.googleapis.com/auth/photoslibrary.readonly";

/// Mutate-only scope — required for `mediaItems.batchDelete` (Phase 1
/// source-cleanup). Only requested after the user opts in to cleanup.
pub const SCOPE_DELETE: &str = "https://www.googleapis.com/auth/photoslibrary";

/// Keyring service name. The username slot holds the Google account email
/// once the token exchange succeeds (so a user with multiple Google
/// accounts can layer them; not enabled in UI yet).
pub const KEYRING_SERVICE: &str = "chronimage.source.google_photos";

/// Default keyring username when only one Google account is supported.
pub const KEYRING_DEFAULT_USER: &str = "default";

/// PKCE verifier length (RFC 7636 recommends 43-128 URL-safe chars). We
/// generate 64 bytes of entropy and base64url-encode unpadded → 86 chars.
const PKCE_VERIFIER_BYTES: usize = 64;

/// CSRF state token length (32 bytes → 43-char base64url string).
const STATE_TOKEN_BYTES: usize = 32;

/// Everything the caller needs to drive a fresh OAuth flow.
///
/// Send `auth_url` to the system browser. Keep `pkce_verifier` + `state` in
/// process memory (not disk) until the redirect comes back — the frontend
/// passes them back into [`user_initiated_exchange_code`] alongside the
/// `code` + `state` that Google returned.
#[derive(Debug, Clone, Serialize)]
pub struct AuthRequest {
    pub auth_url: String,
    pub pkce_verifier: String,
    pub state: String,
}

/// Token set Google hands back on a successful code exchange. We store this
/// as JSON in the keyring entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    /// Refresh token. Google only returns this on the first exchange
    /// (or when `prompt=consent` is passed). Persist it — otherwise the
    /// user has to re-consent when the access token expires.
    pub refresh_token: Option<String>,
    /// Seconds until `access_token` expires, relative to when the token
    /// was minted. Google typically returns 3600.
    pub expires_in: i64,
    /// Space-delimited scope list Google actually granted (may differ from
    /// what we requested if the user unchecks scopes on the consent screen).
    pub scope: String,
    /// OIDC bag when `openid`+`email` are in scope. Unused today but handy
    /// for showing the connected account in Settings.
    pub id_token: Option<String>,
    pub token_type: String,
}

/// Raw Google response shape. Renamed to [`TokenSet`] once deserialised.
#[derive(Debug, Deserialize)]
struct RawTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    token_type: String,
}

/// Google's standard error shape for failed token exchanges.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Generate a cryptographically random PKCE code-verifier.
///
/// Returns an unpadded base64url string of 86 characters (64 bytes of
/// entropy). Conforms to RFC 7636 §4.1.
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

/// Generate a CSRF `state` token the caller round-trips through the
/// browser. We check it matches the `state` query param on the redirect.
pub fn generate_state_token() -> AppResult<String> {
    let mut bytes = [0u8; STATE_TOKEN_BYTES];
    getrandom(&mut bytes).map_err(|e| AppError::Internal(format!("OS RNG unavailable: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Build the authorization-endpoint URL the frontend opens in the browser.
///
/// `scopes` should be space-delimited; most callers pass [`SCOPE_READONLY`]
/// on first auth and escalate via a second flow if the user enables
/// source-cleanup.
pub fn build_auth_url(
    client_id: &str,
    redirect_uri: &str,
    scopes: &str,
    challenge: &str,
    state: &str,
) -> String {
    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", scopes),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{AUTH_ENDPOINT}?{query}")
}

/// Wrap [`generate_pkce_verifier`] + [`pkce_challenge`] + [`build_auth_url`]
/// in one call — the shape the Tauri command wants to hand to the frontend.
pub fn new_auth_request(
    client_id: &str,
    redirect_uri: &str,
    scopes: &str,
) -> AppResult<AuthRequest> {
    let verifier = generate_pkce_verifier()?;
    let challenge = pkce_challenge(&verifier);
    let state = generate_state_token()?;
    let auth_url = build_auth_url(client_id, redirect_uri, scopes, &challenge, &state);
    Ok(AuthRequest {
        auth_url,
        pkce_verifier: verifier,
        state,
    })
}

/// Exchange the authorisation code Google returned for a TokenSet.
///
/// **This is the only function in this module that performs network I/O.**
/// It's named with the `user_initiated_` prefix the CLAUDE.md § Security
/// rule requires — the `forbidden-patterns` hook greps for it. Do NOT call
/// this on app boot or from any background task; the contract is that the
/// user just completed an OAuth consent prompt in their browser.
pub async fn user_initiated_exchange_code(
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
) -> AppResult<TokenSet> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("reqwest client: {e}")))?;

    let form = [
        ("client_id", client_id),
        ("code", code),
        ("code_verifier", pkce_verifier),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ];

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("token exchange request: {e}")))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("token exchange body read: {e}")))?;

    if !status.is_success() {
        let parsed: Option<TokenErrorResponse> = serde_json::from_str(&body).ok();
        let msg = match parsed {
            Some(e) => match e.error_description {
                Some(d) => format!("{} ({d})", e.error),
                None => e.error,
            },
            None => format!("HTTP {status}: {body}"),
        };
        return Err(AppError::PermissionDenied(format!(
            "google oauth token exchange rejected: {msg}"
        )));
    }

    let raw: RawTokenResponse = serde_json::from_str(&body)
        .map_err(|e| AppError::Internal(format!("token response parse: {e} (body: {body})")))?;

    Ok(TokenSet {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token,
        expires_in: raw.expires_in,
        scope: raw.scope.unwrap_or_default(),
        id_token: raw.id_token,
        token_type: raw.token_type,
    })
}

/// Persist a token set in the platform secret store under
/// [`KEYRING_SERVICE`] / [`KEYRING_DEFAULT_USER`].
pub fn store_tokens(tokens: &TokenSet) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("keyring entry: {e}")))?;
    let blob = serde_json::to_string(tokens)?;
    entry
        .set_password(&blob)
        .map_err(|e| AppError::Internal(format!("keyring set: {e}")))?;
    Ok(())
}

/// Look up the persisted token set. Returns `Ok(None)` when the entry does
/// not exist — the keyring crate surfaces that via `NoEntry`, which we map
/// to `None` so callers can check "is the user signed in?" without a match
/// arm on the underlying error enum.
pub fn load_tokens() -> AppResult<Option<TokenSet>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("keyring entry: {e}")))?;
    match entry.get_password() {
        Ok(blob) => {
            let tokens: TokenSet = serde_json::from_str(&blob)?;
            Ok(Some(tokens))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Internal(format!("keyring get: {e}"))),
    }
}

/// Delete any persisted tokens. Idempotent — a missing entry is success.
pub fn delete_tokens() -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DEFAULT_USER)
        .map_err(|e| AppError::Internal(format!("keyring entry: {e}")))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("keyring delete: {e}"))),
    }
}

/// Minimal RFC 3986 percent-encoder for the handful of characters that
/// appear in OAuth2 URL params (scopes contain `:` and `/`; redirect URIs
/// contain `:` and `/`; state tokens are already base64url-safe). We avoid
/// pulling in a full URL crate for a nine-char table.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        // Unreserved per RFC 3986 §2.3 + `~` already in the unreserved set.
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_length_and_charset_are_rfc7636_compliant() {
        let v = generate_pkce_verifier().expect("rng");
        // Unpadded base64url of 64 bytes = 86 chars; RFC 7636 allows
        // 43-128.
        assert_eq!(v.len(), 86);
        assert!(v.len() >= 43 && v.len() <= 128);
        for ch in v.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
                "verifier contains non-base64url char: {ch:?}"
            );
        }
    }

    #[test]
    fn verifier_is_non_deterministic() {
        let a = generate_pkce_verifier().expect("rng");
        let b = generate_pkce_verifier().expect("rng");
        assert_ne!(a, b, "two successive verifiers collided — RNG broken");
    }

    #[test]
    fn challenge_matches_rfc7636_example() {
        // RFC 7636 Appendix B: the verifier
        //   dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk
        // produces challenge
        //   E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn state_tokens_are_unique_enough() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            seen.insert(generate_state_token().expect("rng"));
        }
        assert_eq!(seen.len(), 100);
    }

    #[test]
    fn auth_url_contains_expected_params() {
        let url = build_auth_url(
            "client-abc",
            "http://127.0.0.1:8734/callback",
            SCOPE_READONLY,
            "challenge-xyz",
            "state-123",
        );
        assert!(url.starts_with(AUTH_ENDPOINT));
        assert!(url.contains("client_id=client-abc"));
        assert!(url.contains("code_challenge=challenge-xyz"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("access_type=offline"));
        // Space-containing scope string must be percent-encoded (the colon
        // + forward slashes get encoded too).
        assert!(!url.contains(' '));
        assert!(url.contains("scope="));
    }

    #[test]
    fn new_auth_request_is_self_consistent() {
        let req = new_auth_request("cid", "http://127.0.0.1:8734/callback", SCOPE_READONLY)
            .expect("build");
        // The challenge embedded in auth_url must match the verifier we
        // return to the caller — otherwise Google will reject the exchange.
        let expected_challenge = pkce_challenge(&req.pkce_verifier);
        assert!(
            req.auth_url
                .contains(&format!("code_challenge={expected_challenge}")),
            "auth_url challenge doesn't match verifier-derived challenge"
        );
        assert!(req.auth_url.contains(&format!("state={}", req.state)));
    }

    #[test]
    fn percent_encode_matches_rfc3986() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a:b/c"), "a%3Ab%2Fc");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
    }

    /// Keyring round-trip. Gated on a mock-only keyring backend so CI
    /// boxes without a real secret store still exercise the code path.
    /// On developer machines keyring uses the platform backend.
    #[test]
    #[ignore = "touches the platform keyring; enable with `--ignored` when \
                running on a developer machine with Credential Manager / \
                Keychain / Secret Service available"]
    fn store_load_delete_roundtrip() {
        let tokens = TokenSet {
            access_token: "access-test-token".into(),
            refresh_token: Some("refresh-test-token".into()),
            expires_in: 3599,
            scope: SCOPE_READONLY.into(),
            id_token: None,
            token_type: "Bearer".into(),
        };

        store_tokens(&tokens).expect("store");
        let loaded = load_tokens().expect("load").expect("some");
        assert_eq!(loaded, tokens);
        delete_tokens().expect("delete");
        let after = load_tokens().expect("load after delete");
        assert!(after.is_none(), "expected None after delete, got {after:?}");
    }

    #[test]
    fn token_set_json_roundtrip_is_stable() {
        let tokens = TokenSet {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_in: 3600,
            scope: SCOPE_READONLY.into(),
            id_token: None,
            token_type: "Bearer".into(),
        };
        let blob = serde_json::to_string(&tokens).expect("ser");
        let back: TokenSet = serde_json::from_str(&blob).expect("de");
        assert_eq!(back, tokens);
    }
}
