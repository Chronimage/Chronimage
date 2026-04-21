//! Cloud source connectors.
//!
//! Each submodule implements the OAuth + API-surface glue for a specific
//! cloud photo provider. Local / external / NAS sources don't live here —
//! they're plain filesystem walks and stay under [`crate::import`].
//!
//! Token storage is uniform: `keyring` (Windows Credential Manager on the
//! target platform) keyed by a `CHRONIMAGE_SOURCE_*` service name. OAuth
//! refresh tokens never touch the catalog DB.

pub mod google_photos;
