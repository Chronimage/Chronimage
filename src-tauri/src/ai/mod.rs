//! On-device AI inference layer.
//!
//! All heavy ONNX sessions are held in `OnceLock` globals so they are
//! initialised once and reused across Tauri command calls.

pub mod aesthetic;
pub mod budget;
pub mod caption;
pub mod cluster;
pub mod cluster_persist;
pub mod download;
pub mod faces;
pub mod image_util;
pub mod providers;
pub mod siglip;

pub use siglip::{dot_product, l2_normalise, SigLipSession, EMBED_DIM};
