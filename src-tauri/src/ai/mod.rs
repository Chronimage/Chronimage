//! On-device AI inference layer.
//!
//! All heavy ONNX sessions are held in `OnceLock` globals so they are
//! initialised once and reused across Tauri command calls.

pub mod aesthetic;
pub mod budget;
pub mod siglip;
