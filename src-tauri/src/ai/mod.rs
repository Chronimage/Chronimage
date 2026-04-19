//! On-device AI inference layer.
//!
//! All heavy ONNX sessions are held in `OnceLock` globals so they are
//! initialised once and reused across Tauri command calls.

pub mod aesthetic;
pub mod budget;
pub mod download;
pub mod siglip;

pub use siglip::{dot_product, l2_normalise, SigLipSession, EMBED_DIM};
