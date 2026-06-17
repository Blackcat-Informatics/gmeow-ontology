// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the `.cache/validate` content-addressed cache (#634).

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::PathBuf;

use gmeow_validate::cache::{CachedResult, ValidationCache};
use gmeow_validate::lint::LintConfig;
use gmeow_validate::validate_all::{ValidateOptions, ValidationRun};

fn temp_project_root() -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gmeow_validate_cache_test_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(dir: &PathBuf, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn cache_key_matches_python_vector() {
    // Python `_cache_key(["a", "b"])` joins parts with NUL and truncates to
    // 16 hex chars.
    assert_eq!(
        ValidationCache::cache_key(&[b"a", b"b"]),
        "8fb20ef63ced4145"
    );
}

#[test]
fn cache_key_is_stable_and_hex_truncated() {
    let key1 = ValidationCache::cache_key(&[b"hello", b"world"]);
    let key2 = ValidationCache::cache_key(&[b"hello", b"world"]);
    assert_eq!(key1, key2);
    assert_eq!(key1.len(), 16);
    assert!(key1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn cache_key_changes_with_parts() {
    let key1 = ValidationCache::cache_key(&[b"a", b"b"]);
    let key2 = ValidationCache::cache_key(&[b"a", b"c"]);
    assert_ne!(key1, key2);
}

#[test]
fn files_cache_key_matches_python_source_hash() {
    // Python `generator.source_hash` for a single file named "hello.txt" of
    // size 5 containing "hello" under the project root.
    let root = temp_project_root();
    let path = write_file(&root, "hello.txt", "hello");
    let cache = ValidationCache::new(&root);
    assert_eq!(cache.files_cache_key(&[path]).unwrap(), "9e34842845368e92");
}

#[test]
fn files_cache_key_is_content_addressed() {
    let root = temp_project_root();
    let path = write_file(
        &root,
        "input.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let cache = ValidationCache::new(&root);
    let key1 = cache.files_cache_key(&[path.clone()]).unwrap();
    let key2 = cache.files_cache_key(&[path.clone()]).unwrap();
    assert_eq!(key1, key2);

    fs::write(
        &path,
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:c .\n",
    )
    .unwrap();
    let key3 = cache.files_cache_key(&[path]).unwrap();
    assert_ne!(key1, key3);
}

#[test]
fn files_cache_key_uses_relative_path_when_possible() {
    // A file inside the project root and the same file reached via an absolute
    // path must hash to the same key (relative path normalization).
    let root = temp_project_root();
    let rel = write_file(&root, "nested/file.ttl", "<a> <b> <c> .\n");
    let cache = ValidationCache::new(&root);
    let key_rel = cache.files_cache_key(&[rel.clone()]).unwrap();
    let key_abs = cache
        .files_cache_key(&[rel.canonicalize().unwrap()])
        .unwrap();
    assert_eq!(key_rel, key_abs);
}

#[test]
fn read_write_roundtrip() {
    let root = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = CachedResult {
        errors: vec!["error one".to_owned(), "error two".to_owned()],
        warnings: vec!["warning one".to_owned()],
    };

    cache
        .write_cached_result("merged-shacl", "abc123", &result)
        .unwrap();
    let read = cache
        .read_cached_result("merged-shacl", "abc123")
        .expect("cached result must be readable");
    assert_eq!(read, result);
}

#[test]
fn atomic_write_leaves_no_temp_file() {
    let root = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = CachedResult {
        errors: vec!["e".to_owned()],
        warnings: vec![],
    };

    cache
        .write_cached_result("example-shacl", "deadbeef", &result)
        .unwrap();

    let cache_dir = cache.cache_dir().join("example-shacl");
    let mut entries: Vec<String> = fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(entries, vec!["deadbeef.json"]);
}

#[test]
fn corrupted_cache_file_is_ignored() {
    let root = temp_project_root();
    let cache = ValidationCache::new(&root);
    let kind_dir = cache.cache_dir().join("merged-shacl");
    fs::create_dir_all(&kind_dir).unwrap();
    fs::write(kind_dir.join("bad.json"), "not json").unwrap();
    assert!(cache.read_cached_result("merged-shacl", "bad").is_none());
}

#[test]
fn cache_hit_skips_computation() {
    let root = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = CachedResult {
        errors: vec!["cached error".to_owned()],
        warnings: vec![],
    };
    cache
        .write_cached_result("dsl-shacl/mapping", "hitkey", &result)
        .unwrap();

    // A fresh read returns the cached value without any compute function.
    let hit = cache
        .read_cached_result("dsl-shacl/mapping", "hitkey")
        .expect("cache hit must return the stored result");
    assert_eq!(hit, result);
}

#[test]
fn toolchain_salt_is_stable() {
    let salt1 = ValidationCache::toolchain_salt();
    let salt2 = ValidationCache::toolchain_salt();
    assert_eq!(salt1, salt2);
    assert_eq!(salt1.len(), 16);
}

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn lint_config() -> LintConfig {
    LintConfig {
        namespace: NS.to_owned(),
        ontology_iri: NS.trim_end_matches('/').to_owned(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: [
            "http://www.w3.org/2000/01/rdf-schema#label",
            "http://www.w3.org/2004/02/skos/core#definition",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

fn mini_shapes_ttl() -> String {
    "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:ThingShape a sh:NodeShape ;\n\
       sh:targetClass ex:Thing ;\n\
       sh:property [ sh:path ex:label ; sh:minCount 1 ] ."
        .to_owned()
}

#[test]
fn validate_all_uses_cache_when_configured() {
    let root = temp_project_root();
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix gufo: <http://purl.org/nemo/gufo#> .\n\
         gmeow:Thing a owl:Class , gufo:Kind ;\n\
           rdfs:label \"Thing\" ;\n\
           skos:definition \"A thing.\" ;\n\
           rdfs:isDefinedBy <{NS}> .\n"
    );
    let path = write_file(&root, "ontology.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let options = ValidateOptions {
        timings: true,
        project_root: Some(root.clone()),
        ..ValidateOptions::default()
    };

    let run1 = ValidationRun::run(
        &[path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &options,
    )
    .expect("first run must complete");
    let merged_meta1 = run1
        .timings
        .iter()
        .find(|t| t.phase == "merged-shacl")
        .expect("merged-shacl timing must exist")
        .metadata
        .as_deref();
    assert_eq!(merged_meta1, Some("cache-miss"));

    let run2 = ValidationRun::run(
        &[path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &options,
    )
    .expect("second run must complete");
    let merged_meta2 = run2
        .timings
        .iter()
        .find(|t| t.phase == "merged-shacl")
        .expect("merged-shacl timing must exist")
        .metadata
        .as_deref();
    assert_eq!(merged_meta2, Some("cache-hit"));

    // The cache directory must contain the merged-shacl entry.
    let cache = ValidationCache::new(&root);
    let entries: Vec<_> = fs::read_dir(cache.cache_dir().join("merged-shacl"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "merged-shacl cache directory must contain entries"
    );
}
