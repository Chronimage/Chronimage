//! Import pipeline.
//!
//! Phase 1 scaffolding: filesystem scanner, streaming hasher, and the
//! RAW+JPG pair detector. The full tokio/rayon pipeline lands in the next
//! Phase 1 session; these modules are individually unit-tested so the next
//! pass can assemble them without fear.

pub mod hash;
pub mod pair;
pub mod scanner;

pub use hash::sha256_file;
pub use pair::{detect_pairs, is_raw_extension, ImagePair, RawExt};
pub use scanner::{scan_dir, ScanEntry, ScanOptions};
