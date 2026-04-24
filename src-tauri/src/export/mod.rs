//! Phase-2 export engine. Drives the `export_jobs` + `export_job_items` tables,
//! decodes source photos, resizes + re-encodes to the user-chosen format, and
//! emits `chronimage.export.progress` events per item.
//!
//! Scope of this pass:
//! - Formats: JPEG, TIFF via the `image` crate. HEIC + MozJPEG deferred
//!   (requires libheif / mozjpeg system deps; out-of-scope for the Phase 2
//!   merge).
//! - No cloud upload. `upload_targets` is always `[]` for now — the Google
//!   Photos / OneDrive adapters are a follow-up PR.
//! - Color profile handling mirrors the Phase 1 `raw/color.rs` helpers: we
//!   strip embedded profiles and write sRGB bytes when the user picks sRGB
//!   (the common case). P3 / AdobeRGB are stubbed and log a warning.

pub mod engine;
pub mod preset;

pub use engine::{enqueue_job, list_jobs, run_next_item, ExportJob, ExportJobItem, JobStatus};
pub use preset::{ExportPreset, Format, StripMeta};
