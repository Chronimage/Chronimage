//! Duplicate detection pipeline.
//!
//! Phase 1 implements two layers:
//! 1. `phash` — fast perceptual hash pre-filter (bit-distance ≤ 8).
//! 2. `confirm` — SigLIP cosine similarity for semantic grouping.
//!
//! The two-layer design keeps full O(n²) embedding comparisons fast enough
//! for typical hobbyist library sizes (< 10,000 photos). For larger catalogs
//! the `confirm` layer should be replaced with an ANN index (e.g. sqlite-vec).

pub mod confirm;
