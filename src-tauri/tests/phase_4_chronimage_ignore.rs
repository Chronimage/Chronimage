//! Phase 4 §8 acceptance test: scan_dir honours `.chronimage-ignore`
//! and the built-in defaults (`Thumbs.db`, `.DS_Store`, `@eaDir`,
//! `.thumbnails`, `cache/`).

use chronimage::import::scanner::{scan_dir, ScanOptions};
use std::fs;
use tempfile::TempDir;

fn touch(dir: &std::path::Path, rel: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(&p, b"stub").expect("write");
}

#[test]
fn custom_ignore_file_excludes_matching_paths() {
    let tmp = TempDir::new().expect("tmp");
    let root = tmp.path();

    touch(root, "keep_me.jpg");
    touch(root, "draft.jpg.tmp");
    touch(root, "backup.jpg.bak");
    touch(root, "subdir/also_keep.jpg");
    touch(root, "subdir/ignore_me.jpg.tmp");

    // `.chronimage-ignore` at root — drops `.tmp` + `.bak`.
    fs::write(root.join(".chronimage-ignore"), "*.tmp\n*.bak\n").expect("write ignore");

    let entries = scan_dir(&ScanOptions::new(root)).expect("scan");
    let names: Vec<String> = entries
        .iter()
        .map(|e| {
            e.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert!(names.contains(&"keep_me.jpg".to_string()));
    assert!(names.contains(&"also_keep.jpg".to_string()));
    assert!(
        !names.iter().any(|n| n.ends_with(".tmp")),
        ".tmp files excluded; got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.ends_with(".bak")),
        ".bak files excluded; got {names:?}"
    );
}

#[test]
fn default_ignores_drop_thumbsdb_and_ds_store() {
    let tmp = TempDir::new().expect("tmp");
    let root = tmp.path();
    touch(root, "photo.jpg");
    touch(root, "Thumbs.db");
    touch(root, ".DS_Store");
    touch(root, ".thumbnails/preview.jpg");
    touch(root, "cache/old.jpg");
    touch(root, "@eaDir/nas-thumb.jpg");

    let entries = scan_dir(&ScanOptions::new(root)).expect("scan");
    let names: Vec<String> = entries
        .iter()
        .map(|e| {
            e.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(names, vec!["photo.jpg".to_string()]);
}

#[test]
fn nested_ignore_file_applies_to_its_subtree_only() {
    let tmp = TempDir::new().expect("tmp");
    let root = tmp.path();
    touch(root, "top.jpg");
    touch(root, "draft.jpg");
    touch(root, "hidden/draft.jpg");
    touch(root, "hidden/keep.jpg");

    // Nested ignore only fires inside `hidden/`.
    fs::write(root.join("hidden/.chronimage-ignore"), "draft.jpg\n").expect("write");

    let entries = scan_dir(&ScanOptions::new(root)).expect("scan");
    let rels: Vec<String> = entries
        .iter()
        .map(|e| {
            e.path
                .strip_prefix(root)
                .unwrap_or(&e.path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    assert!(rels.contains(&"top.jpg".to_string()));
    assert!(
        rels.contains(&"draft.jpg".to_string()),
        "top-level draft stays"
    );
    assert!(rels.contains(&"hidden/keep.jpg".to_string()));
    assert!(
        !rels.contains(&"hidden/draft.jpg".to_string()),
        "nested draft excluded; got {rels:?}"
    );
}
