//! Filesystem scanner. Walks a root directory, yields per-file metadata
//! (path, size, modified time, extension). Designed to be driven by an async
//! task; backpressure is handled by the caller via a channel.

use crate::{AppError, AppResult};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};
use walkdir::WalkDir;

const DEFAULT_EXTENSIONS: &[&str] = &[
    // raster
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff", // heic/apple
    "heic", "heif", "avif", // raw
    "arw", "cr2", "cr3", "nef", "nrw", "raf", "rw2", "orf", "dng", "pef", "srw",
];

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub extensions: Vec<String>,
    pub follow_links: bool,
    pub max_depth: Option<usize>,
}

impl ScanOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            extensions: DEFAULT_EXTENSIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            follow_links: false,
            max_depth: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    pub path: PathBuf,
    pub ext_lowercase: String,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
}

/// Walk `opts.root` and return every file whose lowercased extension is in
/// `opts.extensions`. Errors on individual files are logged and skipped
/// rather than propagated — a single unreadable file must not abort a
/// 200k-photo scan.
pub fn scan_dir(opts: &ScanOptions) -> AppResult<Vec<ScanEntry>> {
    if !opts.root.exists() {
        return Err(AppError::NotFound(format!(
            "scan root does not exist: {}",
            opts.root.display()
        )));
    }
    if !opts.root.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "scan root is not a directory: {}",
            opts.root.display()
        )));
    }

    let mut walker = WalkDir::new(&opts.root).follow_links(opts.follow_links);
    if let Some(d) = opts.max_depth {
        walker = walker.max_depth(d);
    }

    let ext_set: std::collections::HashSet<&str> =
        opts.extensions.iter().map(String::as_str).collect();

    let mut entries = Vec::new();
    for res in walker.into_iter() {
        let entry = match res {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(error = %err, "scan: skipping unreadable entry");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if !ext_set.contains(ext_lower.as_str()) {
            continue;
        }
        let md = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(path = %entry.path().display(), error = %err, "scan: metadata failed");
                continue;
            }
        };
        entries.push(ScanEntry {
            path: entry.path().to_path_buf(),
            ext_lowercase: ext_lower,
            size_bytes: md.len(),
            modified: md.modified().ok(),
        });
    }

    Ok(entries)
}

fn _forbid_unused(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(dir: &Path, rel: &str) -> PathBuf {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&p, b"stub").expect("write");
        p
    }

    #[test]
    fn scan_picks_up_photo_extensions_only() {
        let tmp = TempDir::new().expect("tempdir");
        touch(tmp.path(), "a.jpg");
        touch(tmp.path(), "nested/b.ARW");
        touch(tmp.path(), "nested/c.heic");
        touch(tmp.path(), "ignored.txt");
        touch(tmp.path(), "no-ext");

        let entries = scan_dir(&ScanOptions::new(tmp.path())).expect("scan");
        let mut exts: Vec<_> = entries.iter().map(|e| e.ext_lowercase.as_str()).collect();
        exts.sort();
        assert_eq!(exts, vec!["arw", "heic", "jpg"]);
    }

    #[test]
    fn scan_errors_on_missing_root() {
        let err = scan_dir(&ScanOptions::new("/no/such/path/for/chronimage-test-123")).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn scan_respects_max_depth() {
        let tmp = TempDir::new().expect("tempdir");
        touch(tmp.path(), "root.jpg");
        touch(tmp.path(), "one/nested.jpg");
        touch(tmp.path(), "one/two/deeper.jpg");
        let opts = ScanOptions {
            root: tmp.path().to_path_buf(),
            extensions: vec!["jpg".into()],
            follow_links: false,
            max_depth: Some(2),
        };
        let entries = scan_dir(&opts).expect("scan");
        // Expect root.jpg + one/nested.jpg; `two/deeper.jpg` is filtered out.
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn scan_entries_capture_size() {
        let tmp = TempDir::new().expect("tempdir");
        let p = tmp.path().join("x.jpg");
        fs::write(&p, b"abcdef").expect("write");
        let entries = scan_dir(&ScanOptions::new(tmp.path())).expect("scan");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size_bytes, 6);
    }
}
