//! One-shot loopback HTTP listener for the OneDrive OAuth2 redirect.
//!
//! Microsoft's MSAL loopback flow is structurally identical to Google's PKCE
//! loopback: the browser lands on `http://127.0.0.1:<port>/callback?code=…&state=…`.
//! We re-use the same listener implementation from the google_photos module
//! rather than duplicating ~300 LOC — the parsing logic, HTML response pages,
//! and TCP-accept pattern are provider-agnostic.

pub use crate::sources::google_photos::loopback::{spawn_listener, RedirectPayload};
