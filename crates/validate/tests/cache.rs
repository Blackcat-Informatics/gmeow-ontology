// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW validation verdict binding through the shared action cache.

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

fn implementation(label: &str) -> gmeow_action_cache::ProducerIdentity {
    gmeow_action_cache::ProducerIdentity::new(gmeow_action_cache::bytes_digest(label.as_bytes()))
}

fn cache_for(root: &Path) -> ValidationCache {
    ValidationCache::new(root, implementation("validation-fixture-v1")).unwrap()
}

#[test]
fn validation_keys_preserve_input_boundaries_and_selected_files() {
    assert_ne!(
        ValidationCache::cache_key(&[b"a\0b", b"c"]),
        ValidationCache::cache_key(&[b"a", b"b\0c"])
    );
    assert_eq!(ValidationCache::cache_key(&[b"hello"]).len(), 64);
    let (_dir, root) = temp_project_root();
    let file = write_file(&root, "input.ttl", "source");
    let cache = cache_for(&root);
    let before = cache.files_cache_key(std::slice::from_ref(&file)).unwrap();
    assert_eq!(
        before,
        cache
            .files_cache_key(&[file.canonicalize().unwrap()])
            .unwrap()
    );
    fs::write(&file, "changed").unwrap();
    assert_ne!(
        before,
        cache.files_cache_key(std::slice::from_ref(&file)).unwrap()
    );
    assert!(
        cache
            .files_cache_key(&[file, root.join("missing.ttl")])
            .is_err()
    );
}

#[test]
fn complete_findings_are_bound_to_implementation_kind_and_input_context() {
    let (_dir, root) = temp_project_root();
    let cache = cache_for(&root);
    let expected = cached(&[
        (Severity::Error, "shacl.policy", "policy violation"),
        (Severity::Warning, "shacl.context", "scoped evidence"),
    ]);
    cache
        .write_cached_result("dsl-shacl/mapping", "inputs-v1", &expected)
        .unwrap();
    assert_eq!(
        cache
            .read_cached_result("dsl-shacl/mapping", "inputs-v1")
            .unwrap(),
        Some(expected)
    );
    assert!(
        cache
            .read_cached_result("dsl-shacl-mapping", "inputs-v1")
            .unwrap()
            .is_none()
    );
    assert!(
        cache
            .read_cached_result("dsl-shacl/mapping", "inputs-v2")
            .unwrap()
            .is_none()
    );
    let changed = ValidationCache::new(&root, implementation("validation-fixture-v2")).unwrap();
    assert!(
        changed
            .read_cached_result("dsl-shacl/mapping", "inputs-v1")
            .unwrap()
            .is_none()
    );
}

#[test]
fn corrupt_cached_findings_fail_instead_of_becoming_an_empty_verdict() {
    let (_dir, root) = temp_project_root();
    let cache = cache_for(&root);
    cache
        .write_cached_result(
            "merged-shacl",
            "input",
            &cached(&[(Severity::Error, "shacl.error", "violation")]),
        )
        .unwrap();
    let blobs: Vec<_> = fs::read_dir(cache.cache_dir().join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(blobs.len(), 1);
    fs::write(&blobs[0], br#"{"findings":[]}"#).unwrap();
    assert!(cache.read_cached_result("merged-shacl", "input").is_err());
}

#[test]
fn dsl_verdict_reuse_preserves_first_source_and_typed_failure_attribution() {
    let (_dir, root) = temp_project_root();
    let source = "<https://example.org/claim> <https://example.org/needsEvidence> true .";
    let a = write_file(&root, "first.ttl", source);
    let b = write_file(&root, "second.ttl", source);
    let shapes = r#"
        @prefix sh: <http://www.w3.org/ns/shacl#> .
        @prefix ex: <https://example.org/> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        ex:EvidenceShape a sh:NodeShape ; sh:targetNode ex:claim ;
            gmeow:enforcesFailureClass ex:MissingEvidence ;
            sh:sparql [ sh:select "SELECT $this WHERE { $this ex:needsEvidence true . FILTER NOT EXISTS { $this ex:evidence ?e } }" ] .
    "#;
    let cache = cache_for(&root);
    let first = [a.clone(), b.clone()];
    let second = [b.clone(), a.clone()];
    let first_key = cache.files_cache_key(&first).unwrap();
    let second_key = cache.files_cache_key(&second).unwrap();
    assert_ne!(first_key, second_key);
    let findings = gmeow_validate::dsl_shacl::validate_dsl(&first, shapes, "statement").unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].failure_class.as_deref(),
        Some("https://example.org/MissingEvidence")
    );
    assert_eq!(findings[0].locations[0].path.as_deref(), a.to_str());
    cache
        .write_cached_result(
            "dsl-shacl/statement",
            &first_key,
            &CachedResult::from_findings(findings.clone()),
        )
        .unwrap();
    assert_eq!(
        cache
            .read_cached_result("dsl-shacl/statement", &first_key)
            .unwrap()
            .unwrap()
            .findings,
        findings
    );
    assert!(
        cache
            .read_cached_result("dsl-shacl/statement", &second_key)
            .unwrap()
            .is_none()
    );
    let reordered = gmeow_validate::dsl_shacl::validate_dsl(&second, shapes, "statement").unwrap();
    assert_eq!(reordered[0].locations[0].path.as_deref(), b.to_str());
    assert_eq!(reordered[0].failure_class, findings[0].failure_class);
}

#[test]
fn repository_cache_selection_requires_an_exact_implementation() {
    let (_dir, root) = temp_project_root();
    let source = write_file(
        &root,
        "input.ttl",
        "<https://example.org/a> <https://example.org/p> <https://example.org/b> .",
    );
    let options = ValidateOptions {
        project_root: Some(root.clone()),
        ..ValidateOptions::default()
    };
    let result = ValidationRun::run(
        &[source.display().to_string()],
        &mini_shapes_ttl(),
        "",
        "",
        &lint_config(),
        &options,
    );
    assert!(result.is_err());
    assert!(
        ValidationCache::new(root, gmeow_action_cache::ProducerIdentity::new("0.2.0")).is_err()
    );
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
        cache_implementation: Some(implementation("validation-fixture-v1")),
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
    let cache = cache_for(&root);
    let entries: Vec<_> = fs::read_dir(cache.cache_dir().join("receipts"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "shared action store must contain the validation receipt"
    );
}

#[test]
fn bundle_verdict_reuse_tracks_semantic_inputs_and_retains_failure_evidence() {
    let instance = build_gts_graph_with_triples(&[(
        "https://example.org/claim",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        "https://example.org/Claim",
    )]);
    let evidence = build_gts_graph_with_triples(&[(
        "https://example.org/claim",
        "https://example.org/evidence",
        "https://example.org/source",
    )]);
    let instance_bytes = write_gts_bundle(&instance, true);
    let evidence_bytes = write_gts_bundle(&evidence, true);
    let alternate_evidence = write_gts_bundle(&evidence, false);
    let original = [instance_bytes.as_slice(), evidence_bytes.as_slice()].concat();
    let reordered = [alternate_evidence.as_slice(), instance_bytes.as_slice()].concat();
    assert_ne!(original, reordered);
    // Exercise GMEOW's actual projection and finding attribution together. The
    // violated property shape is a generated blank node, distinct from its parent.
    use gmeow_logic_compile::ir::{
        ConstraintProvenance, PropertyConstraintIr, ShapeTarget, ValidationShapeIr,
    };
    let evidence_shape = ValidationShapeIr::new(
        "https://example.org/ClaimShape",
        ShapeTarget::Class("https://example.org/Claim".into()),
        vec![
            PropertyConstraintIr::new(
                "https://example.org/evidence",
                Some(1),
                None,
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
    .with_failure_class("https://example.org/MissingEvidence")
    .unwrap();
    let shapes = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n{}",
        gmeow_logic_compile::projections::shapes::project_validation_shape_shacl(&evidence_shape),
    );
    let (_dir, root) = temp_project_root();
    let mut options = ValidateOptions {
        timings: true,
        project_root: Some(root),
        cache_implementation: Some(implementation("validation-fixture-v1")),
        ..ValidateOptions::default()
    };
    let mut original_findings = None;
    for (bytes, cache_state) in [(original, "cache-miss"), (reordered, "cache-hit")] {
        options.gts_bytes = Some(bytes);
        let run = ValidationRun::run(&[], &shapes, "", "", &lint_config(), &options).unwrap();
        assert_eq!(
            run.timings
                .iter()
                .find(|t| t.phase == "merged-shacl")
                .unwrap()
                .metadata
                .as_deref(),
            Some(cache_state),
        );
        assert!(
            run.report
                .findings
                .iter()
                .all(|f| f.severity != Severity::Error)
        );
        if let Some(expected) = &original_findings {
            assert_eq!(&run.report.findings, expected);
        } else {
            original_findings = Some(run.report.findings);
        }
    }
    // Removing evidence changes the selected semantic input. The old conforming
    // verdict must not survive, and a subsequent hit must retain the typed failure.
    options.gts_bytes = Some(instance_bytes);
    let mut violation = None;
    for cache_state in ["cache-miss", "cache-hit"] {
        let run = ValidationRun::run(&[], &shapes, "", "", &lint_config(), &options).unwrap();
        assert_eq!(
            run.timings
                .iter()
                .find(|t| t.phase == "merged-shacl")
                .unwrap()
                .metadata
                .as_deref(),
            Some(cache_state),
        );
        let findings: Vec<_> = run
            .report
            .findings
            .iter()
            .filter(|f| f.failure_class.as_deref() == Some("https://example.org/MissingEvidence"))
            .cloned()
            .collect();
        assert_eq!(findings.len(), 1, "all findings: {:?}", run.report.findings);
        assert_eq!(findings[0].severity, Severity::Error);
        assert_eq!(
            findings[0].documented_terms,
            ["https://example.org/evidence"]
        );
        if let Some(expected) = &violation {
            assert_eq!(&findings, expected);
        } else {
            violation = Some(findings);
        }
    }
}
