// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Annotation-completeness twins migrated from tests/test_label_completeness.py
//! (whole file; the Python file is deleted, retiring the last `_graph_nt`
//! consumer — the `structural_lint` shim).
//!
//! Every GMEOW-namespaced header, class, property, annotation property, datatype,
//! and individual must carry `rdfs:label`, `skos:definition`, and
//! `rdfs:isDefinedBy`. These run the native `structural_lint_dataset` directly:
//!   - `test_merged_ontology_has_no_missing_annotations` →
//!     `merged_ontology_has_no_missing_annotations` (real-corpus sweep over
//!     `slices/*/*/module.ttl`, the native twin of `load_merged_graph`).
//!   - `test_mapping_dsl_vocabulary_has_no_missing_annotations` →
//!     `mapping_dsl_vocabulary_has_no_missing_annotations`.
//!   - `test_statement_dsl_vocabulary_has_no_missing_annotations` →
//!     `statement_dsl_vocabulary_has_no_missing_annotations`.
//!   - `test_structural_lint_flags_missing_label_definition_and_isdefinedby` →
//!     `flags_missing_label_definition_and_isdefinedby`.
//!   - `test_structural_lint_covers_individuals` → `covers_individuals`.
//!   - `test_structural_lint_covers_annotation_properties` →
//!     `covers_annotation_properties`.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use gmeow_validate::store::dataset_from_paths;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root two levels above crates/validate")
        .to_path_buf()
}

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

/// Run `structural_lint_dataset` over the given source files and return its errors.
fn lint_errors(paths: &[PathBuf]) -> Vec<String> {
    let dataset = dataset_from_paths(paths).expect("sources must parse into a dataset");
    structural_lint_dataset(&dataset, &lint_config()).errors()
}

/// Write inline Turtle to a unique temp file and lint it. The `NamedTempFile`
/// is RAII-cleaned on drop, so the file is removed even if the lint panics.
fn lint_inline(name: &str, ttl: &str) -> Vec<String> {
    use std::io::Write;

    let mut file = tempfile::Builder::new()
        .prefix(&format!("{name}_"))
        .suffix(".ttl")
        .tempfile()
        .expect("create temp .ttl file");
    file.write_all(ttl.as_bytes())
        .expect("write inline Turtle to temp file");
    file.flush().expect("flush inline Turtle to temp file");
    lint_errors(std::slice::from_ref(&file.path().to_path_buf()))
}

/// Every slice's `module.ttl` (`slices/*/*/module.ttl`) — the native twin of the
/// Python `load_merged_graph` source set.
fn slice_module_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let slices = repo_root().join("slices");
    for tier in std::fs::read_dir(&slices).expect("slices dir") {
        let tier = tier.expect("tier entry").path();
        if !tier.is_dir() {
            continue;
        }
        for slice in std::fs::read_dir(&tier).expect("tier subdir") {
            let module = slice.expect("slice entry").path().join("module.ttl");
            if module.is_file() {
                paths.push(module);
            }
        }
    }
    paths.sort();
    assert!(!paths.is_empty(), "expected at least one slice module.ttl");
    paths
}

// ── Real-corpus sweeps ────────────────────────────────────────────────────────

#[test]
fn merged_ontology_has_no_missing_annotations() {
    let errors = lint_errors(&slice_module_paths());
    assert!(
        errors.is_empty(),
        "merged ontology has missing annotations:\n{}",
        errors.join("\n")
    );
}

#[test]
fn mapping_dsl_vocabulary_has_no_missing_annotations() {
    let root = repo_root();
    let paths = vec![
        root.join("slices/core/kernel/module.ttl"),
        // The gmeow:GraphBoxRole value type and its five role individuals are
        // description-logic knowledge-base partitioning — a logical grounding
        // concept — so they live in `logic:`, not in `kernel`. Without it the
        // sweep sees every gmeow:graphBoxRole value as an untyped IRI.
        root.join("slices/grounding/logic/module.ttl"),
        root.join("dsl/mappings/vocabulary.ttl"),
    ];
    let errors = lint_errors(&paths);
    assert!(
        errors.is_empty(),
        "mapping DSL vocabulary has missing annotations:\n{}",
        errors.join("\n")
    );
}

#[test]
fn statement_dsl_vocabulary_has_no_missing_annotations() {
    let root = repo_root();
    let paths = vec![
        root.join("slices/core/kernel/module.ttl"),
        // See mapping_dsl_vocabulary_has_no_missing_annotations: the
        // gmeow:GraphBoxRole cluster is owned by the `logic:` grounding slice.
        root.join("slices/grounding/logic/module.ttl"),
        root.join("dsl/statements/vocabulary.ttl"),
    ];
    let errors = lint_errors(&paths);
    assert!(
        errors.is_empty(),
        "statement DSL vocabulary has missing annotations:\n{}",
        errors.join("\n")
    );
}

// ── Synthetic guards ──────────────────────────────────────────────────────────

#[test]
fn flags_missing_label_definition_and_isdefinedby() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:Undocumented a owl:Class .\n"
    );
    let errors = lint_inline("label_completeness_missing", &ttl);
    assert!(!errors.is_empty(), "undocumented class must be flagged");
    let blob = errors.join("\n");
    assert!(
        blob.contains("rdfs:label"),
        "missing rdfs:label not reported: {blob}"
    );
    assert!(
        blob.contains("skos:definition"),
        "missing skos:definition not reported: {blob}"
    );
    assert!(
        blob.contains("rdfs:isDefinedBy"),
        "missing rdfs:isDefinedBy not reported: {blob}"
    );
}

#[test]
fn covers_individuals() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:SampleIndividual a gmeow:SomeClass ;\n\
           rdfs:label \"Sample\" ;\n\
           skos:definition \"A sample individual.\" ;\n\
           gmeow:graphBoxRole <https://example.org/boxTBox> .\n\
         <https://example.org/boxTBox> a gmeow:GraphBoxRole .\n"
    );
    let errors = lint_inline("label_completeness_individual", &ttl);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("rdfs:isDefinedBy") && e.contains("individual")),
        "individual missing rdfs:isDefinedBy must be flagged: {errors:?}"
    );
}

#[test]
fn covers_annotation_properties() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:sampleAnnotationProperty a owl:AnnotationProperty ;\n\
           rdfs:label \"Sample\" ;\n\
           skos:definition \"A sample annotation property.\" ;\n\
           rdfs:isDefinedBy <{ns_iri}> ;\n\
           gmeow:graphBoxRole <https://example.org/boxTBox> .\n\
         <https://example.org/boxTBox> a gmeow:GraphBoxRole .\n",
        ns_iri = NS.trim_end_matches('/'),
    );
    let errors = lint_inline("label_completeness_annprop", &ttl);
    assert!(
        errors.is_empty(),
        "fully-annotated annotation property should pass: {errors:?}"
    );
}
