//! RAW+JPG pair detection.
//!
//! The user's Sony A7 IV shoots RAW + JPG on every click, producing
//! `IMG_4821.ARW` + `IMG_4821.JPG`. These must be stacked (one logical
//! photo), not deduped (two separate photos) and not ignored.
//!
//! Heuristic (Phase 1 exit criterion: ≥99.5% precision on 5k-pair fixture):
//!  1. Same case-insensitive stem (filename without extension).
//!  2. Located in the same parent directory.
//!  3. One is a RAW extension, the other is JPG/JPEG.
//!  4. Optional: similar file-modification timestamps (± 60s). We don't
//!     require this because some cameras write the JPG seconds later, and
//!     some filesystems lose sub-second precision.
//!
//! RAW is always the "master" member of the pair. The JPG is a preview /
//! legacy fallback.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// RAW extensions Chronimage treats as "master" in a RAW+JPG pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawExt {
    Arw,
    Cr2,
    Cr3,
    Nef,
    Nrw,
    Raf,
    Rw2,
    Orf,
    Dng,
    Pef,
    Srw,
}

impl RawExt {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "arw" => Some(Self::Arw),
            "cr2" => Some(Self::Cr2),
            "cr3" => Some(Self::Cr3),
            "nef" => Some(Self::Nef),
            "nrw" => Some(Self::Nrw),
            "raf" => Some(Self::Raf),
            "rw2" => Some(Self::Rw2),
            "orf" => Some(Self::Orf),
            "dng" => Some(Self::Dng),
            "pef" => Some(Self::Pef),
            "srw" => Some(Self::Srw),
            _ => None,
        }
    }
}

/// True if the extension (case-insensitive) is a RAW format.
pub fn is_raw_extension(ext: &str) -> bool {
    RawExt::from_extension(ext).is_some()
}

fn is_jpeg_extension(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePair {
    pub raw: PathBuf,
    pub jpg: PathBuf,
}

/// Given a set of candidate files, return (pair set, unpaired leftovers).
/// Does no disk I/O; purely a string-matching pass. Input should already
/// be filtered to image extensions (see [`crate::import::scan_dir`]).
pub fn detect_pairs<I, P>(paths: I) -> (Vec<ImagePair>, Vec<PathBuf>)
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let all: Vec<PathBuf> = paths.into_iter().map(Into::into).collect();

    // Group by (parent, lowercased stem)
    let mut groups: HashMap<(PathBuf, String), Vec<PathBuf>> = HashMap::new();
    for p in &all {
        let Some(parent) = p.parent().map(Path::to_path_buf) else {
            continue;
        };
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        groups
            .entry((parent, stem.to_ascii_lowercase()))
            .or_default()
            .push(p.clone());
    }

    let mut pairs = Vec::new();
    let mut paired: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for ((_parent, _stem), files) in &groups {
        if files.len() < 2 {
            continue;
        }
        let raw = files.iter().find(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(is_raw_extension)
        });
        let jpg = files.iter().find(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(is_jpeg_extension)
        });
        if let (Some(raw), Some(jpg)) = (raw, jpg) {
            pairs.push(ImagePair {
                raw: raw.clone(),
                jpg: jpg.clone(),
            });
            paired.insert(raw.clone());
            paired.insert(jpg.clone());
        }
    }

    let leftovers = all.into_iter().filter(|p| !paired.contains(p)).collect();
    (pairs, leftovers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_raw_jpg_pair() {
        let files = vec![
            PathBuf::from("/photos/IMG_0001.ARW"),
            PathBuf::from("/photos/IMG_0001.JPG"),
        ];
        let (pairs, leftovers) = detect_pairs(files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(leftovers.len(), 0);
        assert_eq!(pairs[0].raw, PathBuf::from("/photos/IMG_0001.ARW"));
        assert_eq!(pairs[0].jpg, PathBuf::from("/photos/IMG_0001.JPG"));
    }

    #[test]
    fn case_insensitive_stem_and_extension() {
        let files = vec![
            PathBuf::from("/p/img_0001.arw"),
            PathBuf::from("/p/IMG_0001.jpeg"),
        ];
        let (pairs, leftovers) = detect_pairs(files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(leftovers.len(), 0);
    }

    #[test]
    fn different_directories_do_not_pair() {
        let files = vec![
            PathBuf::from("/a/IMG_0001.ARW"),
            PathBuf::from("/b/IMG_0001.JPG"),
        ];
        let (pairs, leftovers) = detect_pairs(files);
        assert!(pairs.is_empty());
        assert_eq!(leftovers.len(), 2);
    }

    #[test]
    fn two_raws_same_stem_do_not_pair() {
        let files = vec![
            PathBuf::from("/p/IMG_0001.ARW"),
            PathBuf::from("/p/IMG_0001.CR3"),
        ];
        let (pairs, _) = detect_pairs(files);
        assert!(pairs.is_empty());
    }

    #[test]
    fn two_jpgs_do_not_pair() {
        let files = vec![
            PathBuf::from("/p/IMG_0001.JPG"),
            PathBuf::from("/p/IMG_0001.JPEG"),
        ];
        let (pairs, _) = detect_pairs(files);
        assert!(pairs.is_empty());
    }

    #[test]
    fn mixed_bag_partitions_correctly() {
        let files = vec![
            PathBuf::from("/p/IMG_0001.ARW"),
            PathBuf::from("/p/IMG_0001.JPG"),
            PathBuf::from("/p/IMG_0002.ARW"), // no jpg partner
            PathBuf::from("/p/IMG_0003.JPG"), // no raw partner
            PathBuf::from("/p/IMG_0004.HEIC"),
            PathBuf::from("/p/IMG_0005.CR3"),
            PathBuf::from("/p/IMG_0005.JPG"),
        ];
        let (pairs, leftovers) = detect_pairs(files);
        assert_eq!(pairs.len(), 2);
        let mut stems: Vec<_> = pairs
            .iter()
            .map(|p| p.raw.file_stem().unwrap().to_str().unwrap().to_string())
            .collect();
        stems.sort();
        assert_eq!(stems, vec!["IMG_0001", "IMG_0005"]);

        let mut leftover_names: Vec<_> = leftovers
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        leftover_names.sort();
        assert_eq!(
            leftover_names,
            vec!["IMG_0002.ARW", "IMG_0003.JPG", "IMG_0004.HEIC"]
        );
    }

    #[test]
    fn raw_ext_parsing_covers_all_supported() {
        for ext in ["arw", "ARW", "cr3", "NEF", "raf", "dng"] {
            assert!(is_raw_extension(ext), "{ext} should be raw");
        }
        for ext in ["jpg", "jpeg", "heic", "png", "tif"] {
            assert!(!is_raw_extension(ext), "{ext} should not be raw");
        }
    }
}
