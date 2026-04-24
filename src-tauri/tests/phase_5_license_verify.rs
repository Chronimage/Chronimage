//! Phase 5 §7 acceptance test: valid signed license loads; tampered +
//! expired are rejected; `community` plan works without a license file.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::license::{
    clear, import_from_json, load, verify_payload, LicenseError, LicensePayload,
};
use ed25519_dalek::{Signer, SigningKey};

fn sign(plan: &str, email: &str, issued: &str, expires: &str) -> (String, [u8; 32]) {
    let sk = SigningKey::from_bytes(&[13u8; 32]);
    let pubkey = sk.verifying_key().to_bytes();
    let msg = format!("{plan}|{email}|{issued}|{expires}");
    use base64::Engine;
    let sig = sk.sign(msg.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    let payload = LicensePayload {
        plan: plan.into(),
        email: email.into(),
        issued_at: issued.into(),
        expires_at: expires.into(),
        signature: sig_b64,
    };
    (serde_json::to_string(&payload).unwrap(), pubkey)
}

#[tokio::test]
async fn community_plan_is_default() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    let state = load(&pool).await.expect("load");
    assert_eq!(state.plan, "community");
    assert!(!state.is_valid_insider());
}

#[tokio::test]
async fn valid_insider_license_imports_and_loads() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    let (raw, pk) = sign(
        "insider",
        "jay@stackular.com",
        "2026-12-01T00:00:00Z",
        "2099-12-01T00:00:00Z",
    );
    let state = import_from_json(&pool, &raw, pk).await.expect("import");
    assert_eq!(state.plan, "insider");
    assert_eq!(state.email.as_deref(), Some("jay@stackular.com"));
    assert!(state.is_valid_insider());
}

#[tokio::test]
async fn tampered_signature_is_rejected() {
    let (raw, pk) = sign(
        "insider",
        "alice@example.com",
        "2026-12-01T00:00:00Z",
        "2099-12-01T00:00:00Z",
    );
    // Mutate the email after signing so the stored sig no longer
    // covers the joined message.
    let tampered = raw.replace("alice@example.com", "eve@example.com");
    let err = verify_payload(&tampered, pk).expect_err("should fail");
    matches!(err, LicenseError::SignatureMismatch);
}

#[tokio::test]
async fn expired_license_is_rejected() {
    let (raw, pk) = sign(
        "insider",
        "alice@example.com",
        "2000-01-01T00:00:00Z",
        "2000-01-02T00:00:00Z",
    );
    let err = verify_payload(&raw, pk).expect_err("should fail");
    matches!(err, LicenseError::Expired);
}

#[tokio::test]
async fn clear_reverts_to_community() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    let (raw, pk) = sign(
        "insider",
        "alice@example.com",
        "2026-12-01T00:00:00Z",
        "2099-12-01T00:00:00Z",
    );
    import_from_json(&pool, &raw, pk).await.expect("import");
    clear(&pool).await.expect("clear");
    let state = load(&pool).await.expect("load");
    assert_eq!(state.plan, "community");
}
