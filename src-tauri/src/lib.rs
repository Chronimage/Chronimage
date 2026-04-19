//! Chronimage library crate.
//!
//! The binary crate (`main.rs`) delegates almost all logic here so it can be
//! unit-tested and shared with the CLI bin. See `CLAUDE.md` for the module
//! layout.

// Non-test code must not use `unwrap/expect/panic/todo/unimplemented`.
// Enforced by `scripts/forbidden-patterns.cjs` in pre-commit and CI; clippy's
// equivalent lints fire inside test modules too, where those calls are fine.
#![allow(clippy::module_inception)]

pub mod ai;
pub mod catalog;
pub mod commands;
pub mod entitlements;
pub mod error;
pub mod import;
pub mod state;
pub mod telemetry;
pub mod util;

pub use error::{AppError, AppResult};

/// App identifier used for `dirs::data_local_dir`.
pub const APP_ID: &str = "app.chronimage.desktop";

/// Human-visible product name.
pub const APP_NAME: &str = "Chronimage";
