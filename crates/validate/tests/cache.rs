// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the `.cache/validate` content-addressed cache.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Severity};
use gmeow_validate::cache::{CachedResult, ValidationCache};
use gmeow_validate::lint::LintConfig;
use gmeow_validate::store;
use gmeow_validate::validate_all::{ValidateOptions, ValidationRun};

/// Build a `CachedResult` from `(severity, code, message)` triples for tests.
fn cached(findings: &[(Severity, &str, &str)]) -> CachedResult {
    CachedResult::from_findings(
        findings
            .iter()
            .map(|(severity, code, message)| Finding::new(*severity, *code, *message))
            .collect(),
    )
}

/// A fresh, isolated project root for one test.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on drop,
/// including on panic and early return. Bind it to a named `_root` (never a bare
/// `_`, which would drop it immediately) so it outlives the returned path.
fn temp_project_root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp project root");
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
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
    let (_root, root) = temp_project_root();
    let path = write_file(&root, "hello.txt", "hello");
    let cache = ValidationCache::new(&root);
    assert_eq!(cache.files_cache_key(&[path]).unwrap(), "9e34842845368e92");
}

#[test]
fn files_cache_key_is_content_addressed() {
    let (_root, root) = temp_project_root();
    let path = write_file(
        &root,
        "input.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let cache = ValidationCache::new(&root);
    let key1 = cache.files_cache_key(std::slice::from_ref(&path)).unwrap();
    let key2 = cache.files_cache_key(std::slice::from_ref(&path)).unwrap();
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
    let (_root, root) = temp_project_root();
    let rel = write_file(&root, "nested/file.ttl", "<a> <b> <c> .\n");
    let cache = ValidationCache::new(&root);
    let key_rel = cache.files_cache_key(std::slice::from_ref(&rel)).unwrap();
    let key_abs = cache
        .files_cache_key(&[rel.canonicalize().unwrap()])
        .unwrap();
    assert_eq!(key_rel, key_abs);
}

#[test]
fn read_write_roundtrip() {
    let (_root, root) = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = cached(&[
        (Severity::Error, "shacl.x", "error one"),
        (Severity::Error, "shacl.y", "error two"),
        (Severity::Warning, "shacl.z", "warning one"),
    ]);

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
    let (_root, root) = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = cached(&[(Severity::Error, "shacl.e", "e")]);

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
    let (_root, root) = temp_project_root();
    let cache = ValidationCache::new(&root);
    let kind_dir = cache.cache_dir().join("merged-shacl");
    fs::create_dir_all(&kind_dir).unwrap();
    fs::write(kind_dir.join("bad.json"), "not json").unwrap();
    assert!(cache.read_cached_result("merged-shacl", "bad").is_none());
}

#[test]
fn cache_hit_skips_computation() {
    let (_root, root) = temp_project_root();
    let cache = ValidationCache::new(&root);
    let result = cached(&[(Severity::Error, "shacl.cached", "cached error")]);
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

fn build_gts_graph_with_triples(triples: &[(&str, &str, &str)]) -> purrdf::gts::model::Graph {
    use purrdf::gts::model::{Term, TermKind};

    let mut graph = purrdf::gts::model::Graph::default();
    let mut iri_to_id: HashMap<String, usize> = HashMap::new();

    for (s, p, o) in triples {
        let s_id = *iri_to_id.entry(s.to_string()).or_insert_with(|| {
            let id = graph.terms.len();
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(s.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
                triple: None,
            });
            id
        });
        let p_id = *iri_to_id.entry(p.to_string()).or_insert_with(|| {
            let id = graph.terms.len();
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(p.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
                triple: None,
            });
            id
        });
        let o_id = *iri_to_id.entry(o.to_string()).or_insert_with(|| {
            let id = graph.terms.len();
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(o.to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
                triple: None,
            });
            id
        });
        graph.quads.push((s_id, p_id, o_id, None));
    }

    graph
}

fn write_gts_bundle(graph: &purrdf::gts::model::Graph, deterministic: bool) -> Vec<u8> {
    if deterministic {
        purrdf::gts::writer::Writer::deterministic(graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed")
            .to_bytes()
    } else {
        // Non-deterministic serialization of the same semantic graph: fold the
        // deterministic bundle back into a Graph so the logical frame order is
        // identical, then emit it again with the optional CBOR self-describe
        // tag omitted. The wire bytes differ, but the content-addressed
        // segment head stays stable.
        let canonical_bytes =
            purrdf::gts::writer::Writer::deterministic(graph, "gmeow-validate-test")
                .expect("deterministic GTS writer must succeed")
                .to_bytes();
        let canonical_graph =
            store::read_gts_graph(&canonical_bytes).expect("canonical bundle must parse");
        let mut writer = purrdf::gts::writer::Writer::with_options(
            "gmeow-validate-test",
            purrdf::gts::writer::WriterOptions {
                magic_tag: false,
                ..Default::default()
            },
        )
        .expect("non-deterministic writer options must be valid");
        writer.add_terms(&canonical_graph.terms);
        writer.add_quads(&canonical_graph.quads);
        writer.to_bytes()
    }
}

#[test]
fn validate_all_uses_cache_when_configured() {
    let (_root, root) = temp_project_root();
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

#[test]
fn gts_cache_key_is_stable_across_serializations() {
    let graph = build_gts_graph_with_triples(&[(
        "https://example.org/a",
        "https://example.org/p",
        "https://example.org/b",
    )]);

    let deterministic_bytes = write_gts_bundle(&graph, true);
    let non_deterministic_bytes = write_gts_bundle(&graph, false);

    assert_ne!(
        deterministic_bytes, non_deterministic_bytes,
        "deterministic and non-deterministic serializations must differ on the wire"
    );

    let graph1 =
        store::read_gts_graph(&deterministic_bytes).expect("deterministic bundle must parse");
    let graph2 = store::read_gts_graph(&non_deterministic_bytes)
        .expect("non-deterministic bundle must parse");

    assert!(
        !graph1.segment_heads.is_empty(),
        "parsed GTS graph must expose wire segment_heads"
    );
    assert!(
        !graph2.segment_heads.is_empty(),
        "parsed GTS graph must expose wire segment_heads"
    );

    let key1 = ValidationCache::cache_key(
        &graph1
            .segment_heads
            .iter()
            .map(|h| h.as_slice())
            .collect::<Vec<_>>(),
    );
    let key2 = ValidationCache::cache_key(
        &graph2
            .segment_heads
            .iter()
            .map(|h| h.as_slice())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        key1, key2,
        "different GTS serializations of the same graph must yield the same cache key"
    );
}

#[test]
fn gts_cache_key_changes_with_content() {
    let g1 = build_gts_graph_with_triples(&[(
        "https://example.org/a",
        "https://example.org/p",
        "https://example.org/b",
    )]);
    let g2 = build_gts_graph_with_triples(&[(
        "https://example.org/a",
        "https://example.org/p",
        "https://example.org/c",
    )]);

    let b1 = write_gts_bundle(&g1, true);
    let b2 = write_gts_bundle(&g2, true);

    let graph1 = gmeow_validate::store::read_gts_graph(&b1).expect("first bundle must parse");
    let graph2 = gmeow_validate::store::read_gts_graph(&b2).expect("second bundle must parse");

    let k1 = ValidationCache::cache_key(
        &graph1
            .segment_heads
            .iter()
            .map(|h| h.as_slice())
            .collect::<Vec<_>>(),
    );
    let k2 = ValidationCache::cache_key(
        &graph2
            .segment_heads
            .iter()
            .map(|h| h.as_slice())
            .collect::<Vec<_>>(),
    );
    assert_ne!(
        k1, k2,
        "different GTS content must yield different segment-head-based keys"
    );
}

#[test]
fn gts_cache_key_is_stable_across_segment_orders() {
    // Build two distinct single-segment graphs and serialize each
    // deterministically so their wire bytes are stable.
    let alpha = build_gts_graph_with_triples(&[(
        "https://example.org/a",
        "https://example.org/p",
        "https://example.org/b",
    )]);
    let beta = build_gts_graph_with_triples(&[(
        "https://example.org/c",
        "https://example.org/q",
        "https://example.org/d",
    )]);

    let alpha_bytes = write_gts_bundle(&alpha, true);
    let beta_bytes = write_gts_bundle(&beta, true);

    // Concatenate the same two segments in opposite orders.  The resulting
    // multi-segment bundles have identical semantic content but different
    // on-the-wire segment order.
    let mut original = alpha_bytes.clone();
    original.extend_from_slice(&beta_bytes);
    let mut reversed = beta_bytes.clone();
    reversed.extend_from_slice(&alpha_bytes);

    assert_ne!(
        original, reversed,
        "multi-segment bundles with reordered segments must differ on the wire"
    );

    let graph_original =
        store::read_gts_graph(&original).expect("original multi-segment bundle must parse");
    let graph_reversed =
        store::read_gts_graph(&reversed).expect("reversed multi-segment bundle must parse");

    assert_eq!(
        graph_original.segment_heads.len(),
        2,
        "original bundle must expose two segment heads"
    );
    assert_eq!(
        graph_reversed.segment_heads.len(),
        2,
        "reversed bundle must expose two segment heads"
    );

    // The merged-shacl source key sorts segment heads before hashing so that
    // segment order on the wire does not affect cache identity.
    let mut heads_original: Vec<&[u8]> = graph_original
        .segment_heads
        .iter()
        .map(|h| h.as_slice())
        .collect();
    heads_original.sort();
    let key_original = ValidationCache::cache_key(&heads_original);

    let mut heads_reversed: Vec<&[u8]> = graph_reversed
        .segment_heads
        .iter()
        .map(|h| h.as_slice())
        .collect();
    heads_reversed.sort();
    let key_reversed = ValidationCache::cache_key(&heads_reversed);

    assert_eq!(
        key_original, key_reversed,
        "multi-segment bundles with the same segments in different order must yield the same cache key"
    );
}

#[test]
fn gts_validate_uses_cache_when_configured() {
    use purrdf::gts::model::{Term, TermKind};

    // Build a minimal GTS graph that mirrors the ontology used in
    // `validate_all_uses_cache_when_configured`, with the required annotations
    // for the structural lint.
    let mut graph = purrdf::gts::model::Graph::default();
    let ns = "https://blackcatinformatics.ca/gmeow/";
    let thing_iri = format!("{ns}Thing");

    let iris = [
        thing_iri.clone(),
        "http://www.w3.org/2002/07/owl#Class".to_string(),
        "http://purl.org/nemo/gufo#Kind".to_string(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
        "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
        "http://www.w3.org/2004/02/skos/core#definition".to_string(),
        "http://www.w3.org/2000/01/rdf-schema#isDefinedBy".to_string(),
        ns.to_string(),
    ];
    for iri in &iris {
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(iri.clone()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }

    fn literal(graph: &mut purrdf::gts::model::Graph, value: &str) -> usize {
        let id = graph.terms.len();
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
        id
    }

    let thing = 0;
    let owl_class = 1;
    let gufo_kind = 2;
    let rdf_type = 3;
    let rdfs_label = 4;
    let skos_def = 5;
    let rdfs_defined_by = 6;
    let ns_term = 7;
    let label = literal(&mut graph, "Thing");
    let definition = literal(&mut graph, "A thing.");

    graph.quads.push((thing, rdfs_label, label, None));
    graph.quads.push((thing, skos_def, definition, None));
    graph.quads.push((thing, rdfs_defined_by, ns_term, None));
    graph.quads.push((thing, rdf_type, owl_class, None));
    graph.quads.push((thing, rdf_type, gufo_kind, None));

    let bytes = write_gts_bundle(&graph, true);
    let (_root, root) = temp_project_root();
    let options = ValidateOptions {
        timings: true,
        project_root: Some(root.clone()),
        gts_bytes: Some(bytes.clone()),
        ..ValidateOptions::default()
    };

    let run1 = ValidationRun::run(&[], &mini_shapes_ttl(), "", "", &lint_config(), &options)
        .expect("first run must complete");
    let merged_meta1 = run1
        .timings
        .iter()
        .find(|t| t.phase == "merged-shacl")
        .expect("merged-shacl timing must exist")
        .metadata
        .as_deref();
    assert_eq!(merged_meta1, Some("cache-miss"));

    let run2 = ValidationRun::run(&[], &mini_shapes_ttl(), "", "", &lint_config(), &options)
        .expect("second run must complete");
    let merged_meta2 = run2
        .timings
        .iter()
        .find(|t| t.phase == "merged-shacl")
        .expect("merged-shacl timing must exist")
        .metadata
        .as_deref();
    assert_eq!(merged_meta2, Some("cache-hit"));

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
