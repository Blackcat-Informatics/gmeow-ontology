// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn cached_result_write_replaces_cleanly() {
    // RAII: the directory is removed when `tmp` drops at end of scope,
    // including on panic or early return.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let cache = ValidationCache::new(tmp.path());
    let key = "abc123";
    let kind = "test-phase";

    let old = CachedResult::from_findings(vec![Finding::new(
        gmeow_errors::Severity::Error,
        "old",
        "old error",
    )]);
    let new = CachedResult::from_findings(vec![Finding::new(
        gmeow_errors::Severity::Warning,
        "new",
        "new warning",
    )]);

    cache.write_cached_result(kind, key, &old).unwrap();
    cache.write_cached_result(kind, key, &new).unwrap();

    let read = cache
        .read_cached_result(kind, key)
        .expect("cached result must exist");
    assert_eq!(read.findings.len(), 1);
    assert_eq!(read.findings[0].code, "new");

    // No stray temp files left behind.
    let tmp_files: Vec<_> = cache
        .cache_dir()
        .read_dir()
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!(".{key}.json."))
        })
        .collect();
    assert!(tmp_files.is_empty(), "temp cache files must be cleaned up");
}

#[test]
fn cached_result_ignores_non_object_payload() {
    // RAII: the directory is removed when `tmp` drops at end of scope,
    // including on panic or early return.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let cache = ValidationCache::new(tmp.path());
    let path = cache.cache_path("test-phase", "abc123");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"[]").unwrap();

    assert!(cache.read_cached_result("test-phase", "abc123").is_none());
}
