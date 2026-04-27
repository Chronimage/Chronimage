//! Phase 3 RAW Develop — edit history + CPU pipeline + preset library.
//!
//! Module layout:
//! - [`ai_edits`] — generated AI artifact tracking + stale detection.
//! - [`ops`] — the [`Operations`] value type: a flat struct of slider values
//!   + curve control points. Serialised as `edits.operations_json`.
//! - [`pipeline`] — applies an `Operations` to an RGB image (via `image` +
//!   `rayon`). CPU-only for the MVP; wgpu shader path lands in a follow-up.
//! - [`history`] — CRUD over the `edits` table: save / reset / load /
//!   copy-paste / undo graph walk.
//! - [`masks`] — persistent local-adjustment mask layer metadata and CRUD.
//! - [`presets`] — built-in preset definitions (Clean up face · Enhance sky ·
//!   Portrait relight · B&W film) + user-preset CRUD.

pub mod ai_edits;
pub mod history;
pub mod masks;
pub mod ops;
pub mod pipeline;
pub mod presets;

pub use masks::{DevelopMask, DevelopMaskCreateRequest, DevelopMaskUpdateRequest};
pub use ops::{Operations, PastedReceipt, RenderReceipt};
pub use presets::Preset;
