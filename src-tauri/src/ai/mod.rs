//! AI inference modules — SigLIP embeddings, face detection, captions.
//!
//! Phase 1 scaffolding: only the SigLIP text encoder is wired; everything
//! else is placeholder. Phase 1b adds the real ort::Session load.

pub mod siglip;

pub use siglip::{dot_product, l2_normalise, SigLipSession, EMBED_DIM};
