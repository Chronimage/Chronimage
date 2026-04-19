//! Import pipeline.
//!
//! Phase 1: filesystem scanner, streaming hasher, RAW+JPG pair detector, and
//! the full tokio/rayon pipeline that ties them together.

pub mod exif;
pub mod google_takeout;
pub mod hash;
pub mod icloud;
pub mod iphone_usb;
pub mod pair;
pub mod pipeline;
pub mod scanner;

pub use hash::sha256_file;
pub use pair::{detect_pairs, is_raw_extension, ImagePair, RawExt};
pub use pipeline::{run_pipeline, ImportProgress, ImportResult};
pub use scanner::{scan_dir, ScanEntry, ScanOptions};
