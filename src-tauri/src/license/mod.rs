//! Phase 5 §7 — Insider license verification.
//!
//! Chronimage's v1 entitlement flow is offline and auditable: the user
//! drops a signed `license.json` into `{app_data}/license.json`, the
//! app verifies the Ed25519 signature against the embedded public key,
//! and — if valid — writes the relevant fields into the single-row
//! `license_state` table. No activation server, no phone-home.
//!
//! License payload (JSON):
//!
//! ```json
//! {
//!   "plan": "insider",
//!   "email": "alice@example.com",
//!   "issued_at": "2026-12-01T00:00:00Z",
//!   "expires_at": "2027-12-01T00:00:00Z",
//!   "signature": "<base64-encoded Ed25519 sig over the other fields joined by '|'>"
//! }
//! ```
//!
//! The signed message is the deterministic join
//! `{plan}|{email}|{issued_at}|{expires_at}` so any tool (including a
//! curl + minisign script) can issue licenses without needing this
//! crate's serialisation semantics.

use crate::{AppError, AppResult};
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Ed25519 public key for Insider licence signatures. Replaced at
/// release time; the placeholder `0x00` key makes every `insider`
/// license fail verification until a real key is wired in via the
/// CI/CD secret rotation step.
///
/// See `docs/prds/phase-5.md` §2 for the rotation policy.
pub const INSIDER_PUBKEY_BYTES: [u8; 32] = [0u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LicenseState {
    pub id: i64,
    pub plan: String,
    pub email: Option<String>,
    pub issued_at: Option<String>,
    pub expires_at: Option<String>,
    pub signature: Option<String>,
    pub verified_at: Option<String>,
    pub last_checked_at: Option<String>,
}

impl LicenseState {
    pub fn community() -> Self {
        Self {
            id: 1,
            plan: "community".into(),
            email: None,
            issued_at: None,
            expires_at: None,
            signature: None,
            verified_at: None,
            last_checked_at: None,
        }
    }

    /// True when the plan is `insider` and the stored `expires_at` is
    /// in the future. Community licences are always "valid" in the
    /// sense that the app functions; they're just not Insider.
    pub fn is_valid_insider(&self) -> bool {
        if self.plan != "insider" {
            return false;
        }
        let Some(exp) = self.expires_at.as_deref() else {
            return false;
        };
        match DateTime::parse_from_rfc3339(exp) {
            Ok(t) => t.with_timezone(&Utc) > Utc::now(),
            Err(_) => false,
        }
    }
}

/// Raw payload parsed out of a user-supplied `license.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub plan: String,
    pub email: String,
    pub issued_at: String,
    pub expires_at: String,
    pub signature: String,
}

impl LicensePayload {
    /// Canonical signable message: the four fields joined by `|`.
    /// Keep this deterministic and boring — `serde_json` re-orders map
    /// keys under some flags and we don't want a signature to depend
    /// on that.
    fn signed_bytes(&self) -> Vec<u8> {
        format!(
            "{}|{}|{}|{}",
            self.plan, self.email, self.issued_at, self.expires_at
        )
        .into_bytes()
    }
}

/// Errors distinguishable from the generic `AppError::InvalidInput`
/// so tests (and the UI) can branch precisely on which check failed.
#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("license file is not valid JSON: {0}")]
    Parse(String),
    #[error("license plan must be `community` or `insider`; got {0}")]
    UnknownPlan(String),
    #[error("signature is not valid base64")]
    BadSignatureEncoding,
    #[error("signature length must be 64 bytes")]
    BadSignatureLength,
    #[error("signature does not verify against the embedded public key")]
    SignatureMismatch,
    #[error("license has expired")]
    Expired,
    #[error("license issued_at / expires_at is not an RFC 3339 timestamp")]
    BadTimestamp,
}

impl From<LicenseError> for AppError {
    fn from(e: LicenseError) -> Self {
        AppError::InvalidInput(e.to_string())
    }
}

/// Parse + verify a user-supplied license payload.
///
/// The public key defaults to the compile-time `INSIDER_PUBKEY_BYTES`;
/// tests supply their own key so they can cover the verification
/// happy-path without round-tripping through CI-issued licenses.
pub fn verify_payload(raw: &str, pubkey: [u8; 32]) -> Result<LicensePayload, LicenseError> {
    let payload: LicensePayload =
        serde_json::from_str(raw).map_err(|e| LicenseError::Parse(e.to_string()))?;

    if payload.plan != "community" && payload.plan != "insider" {
        return Err(LicenseError::UnknownPlan(payload.plan.clone()));
    }

    DateTime::parse_from_rfc3339(&payload.issued_at).map_err(|_| LicenseError::BadTimestamp)?;
    let expires = DateTime::parse_from_rfc3339(&payload.expires_at)
        .map_err(|_| LicenseError::BadTimestamp)?;
    if expires.with_timezone(&Utc) <= Utc::now() {
        return Err(LicenseError::Expired);
    }

    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.signature.as_bytes())
        .map_err(|_| LicenseError::BadSignatureEncoding)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| LicenseError::BadSignatureLength)?;
    let sig = Signature::from_bytes(&sig_arr);

    let verifying =
        VerifyingKey::from_bytes(&pubkey).map_err(|_| LicenseError::SignatureMismatch)?;
    verifying
        .verify(&payload.signed_bytes(), &sig)
        .map_err(|_| LicenseError::SignatureMismatch)?;

    Ok(payload)
}

// ── Catalog persistence ──────────────────────────────────────────────────

pub async fn load(pool: &SqlitePool) -> AppResult<LicenseState> {
    let row: Option<LicenseState> = sqlx::query_as::<_, LicenseState>(
        "SELECT id, plan, email, issued_at, expires_at, signature, verified_at, last_checked_at \
         FROM license_state WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or_else(LicenseState::community))
}

pub async fn clear(pool: &SqlitePool) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE license_state SET plan = 'community', email = NULL, issued_at = NULL, \
         expires_at = NULL, signature = NULL, verified_at = NULL, last_checked_at = ?1 WHERE id = 1",
    )
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Import from a user-supplied `license.json`. Verifies against the
/// embedded public key and, on success, persists the fields. Failures
/// bubble a typed [`LicenseError`] so callers can map to actionable UI.
pub async fn import_from_json(
    pool: &SqlitePool,
    raw_json: &str,
    pubkey: [u8; 32],
) -> AppResult<LicenseState> {
    let payload = verify_payload(raw_json, pubkey)?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE license_state SET plan = ?1, email = ?2, issued_at = ?3, expires_at = ?4, \
         signature = ?5, verified_at = ?6, last_checked_at = ?6 WHERE id = 1",
    )
    .bind(&payload.plan)
    .bind(&payload.email)
    .bind(&payload.issued_at)
    .bind(&payload.expires_at)
    .bind(&payload.signature)
    .bind(&now)
    .execute(pool)
    .await?;
    load(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use ed25519_dalek::{Signer, SigningKey};

    fn sign_fixture(
        plan: &str,
        email: &str,
        issued: &str,
        expires: &str,
    ) -> (LicensePayload, [u8; 32]) {
        // Deterministic key so tests don't depend on entropy.
        let sk_bytes: [u8; 32] = [7u8; 32];
        let signing = SigningKey::from_bytes(&sk_bytes);
        let pubkey = signing.verifying_key().to_bytes();
        let msg = format!("{plan}|{email}|{issued}|{expires}");
        let sig = signing.sign(msg.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let payload = LicensePayload {
            plan: plan.into(),
            email: email.into(),
            issued_at: issued.into(),
            expires_at: expires.into(),
            signature: sig_b64,
        };
        (payload, pubkey)
    }

    #[test]
    fn valid_signed_payload_verifies() {
        let (p, pk) = sign_fixture(
            "insider",
            "alice@example.com",
            "2026-01-01T00:00:00Z",
            "2099-01-01T00:00:00Z",
        );
        let raw = serde_json::to_string(&p).unwrap();
        let parsed = verify_payload(&raw, pk).expect("verify");
        assert_eq!(parsed.plan, "insider");
        assert_eq!(parsed.email, "alice@example.com");
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let (mut p, pk) = sign_fixture(
            "insider",
            "alice@example.com",
            "2026-01-01T00:00:00Z",
            "2099-01-01T00:00:00Z",
        );
        // Flip the email after signing so the stored signature no longer
        // covers the current payload.
        p.email = "eve@example.com".into();
        let raw = serde_json::to_string(&p).unwrap();
        let err = verify_payload(&raw, pk).expect_err("should fail");
        matches!(err, LicenseError::SignatureMismatch);
    }

    #[test]
    fn expired_license_is_rejected() {
        let (p, pk) = sign_fixture(
            "insider",
            "alice@example.com",
            "2000-01-01T00:00:00Z",
            "2000-01-02T00:00:00Z",
        );
        let raw = serde_json::to_string(&p).unwrap();
        let err = verify_payload(&raw, pk).expect_err("should fail");
        matches!(err, LicenseError::Expired);
    }

    #[test]
    fn unknown_plan_is_rejected() {
        let (p, pk) = sign_fixture(
            "pro",
            "alice@example.com",
            "2026-01-01T00:00:00Z",
            "2099-01-01T00:00:00Z",
        );
        let raw = serde_json::to_string(&p).unwrap();
        let err = verify_payload(&raw, pk).expect_err("should fail");
        matches!(err, LicenseError::UnknownPlan(_));
    }

    #[tokio::test]
    async fn fresh_catalog_reports_community() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        let state = load(&pool).await.unwrap();
        assert_eq!(state.plan, "community");
        assert!(!state.is_valid_insider());
    }

    #[tokio::test]
    async fn import_then_load_round_trips() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        let (p, pk) = sign_fixture(
            "insider",
            "alice@example.com",
            "2026-01-01T00:00:00Z",
            "2099-01-01T00:00:00Z",
        );
        let raw = serde_json::to_string(&p).unwrap();
        let state = import_from_json(&pool, &raw, pk).await.unwrap();
        assert_eq!(state.plan, "insider");
        assert!(state.is_valid_insider());
        // Round-trip via load().
        let reloaded = load(&pool).await.unwrap();
        assert_eq!(reloaded.email.as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn clear_drops_back_to_community() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        let (p, pk) = sign_fixture(
            "insider",
            "alice@example.com",
            "2026-01-01T00:00:00Z",
            "2099-01-01T00:00:00Z",
        );
        import_from_json(&pool, &serde_json::to_string(&p).unwrap(), pk)
            .await
            .unwrap();
        clear(&pool).await.unwrap();
        let state = load(&pool).await.unwrap();
        assert_eq!(state.plan, "community");
        assert!(state.email.is_none());
    }
}
