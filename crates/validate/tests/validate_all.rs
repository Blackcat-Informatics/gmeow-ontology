// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the Rust-native validation orchestration (#634).

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use gmeow_validate::lint::LintConfig;
use gmeow_validate::store::parse_file;
use gmeow_validate::validate_all::{
    scoped_overlay_insert, scoped_overlay_remove, ValidateOptions, ValidationRun,
};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn write_tmp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{}_{}", name, std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path
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

/// A minimal SHACL shape that requires every `ex:Thing` to have an `ex:label`.
fn mini_shapes_ttl() -> String {
    "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:ThingShape a sh:NodeShape ;\n\
       sh:targetClass ex:Thing ;\n\
       sh:property [ sh:path ex:label ; sh:minCount 1 ] ."
        .to_owned()
}

#[test]
fn store_is_reused_across_phases() {
    // One source file with two terms: a well-formed class and one missing
    // skos:definition. The same store is used by structural_lint,
    // term_naming_lint, declared_terms, and reasoning_invariants.
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix gufo: <http://purl.org/nemo/gufo#> .\n\
         gmeow:Documented a owl:Class , gufo:Kind ;\n\
           rdfs:label \"Documented\" ;\n\
           skos:definition \"A well-formed term.\" ;\n\
           rdfs:isDefinedBy <{NS}> .\n\
         gmeow:Undocumented a owl:Class , gufo:Kind ;\n\
           rdfs:label \"Undocumented\" ;\n\
           rdfs:isDefinedBy <{NS}> .\n"
    );
    let path = write_tmp("gmeow_validate_all_reuse.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let run = ValidationRun::run(
        &[path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &ValidateOptions::default(),
    )
    .expect("orchestration must complete");

    std::fs::remove_file(&path).ok();

    // The missing-definition error proves structural_lint ran over the store.
    assert!(
        run.errors().iter().any(|e| e.contains("skos:definition")),
        "structural lint must flag missing definition: {:?}",
        run.errors()
    );

    // Declared terms include both classes.
    assert!(
        run.declared_terms.contains(&format!("{NS}Documented")),
        "declared_terms must be populated from the same store"
    );
    assert!(
        run.declared_terms.contains(&format!("{NS}Undocumented")),
        "declared_terms must include all typed terms"
    );

    // The shared store still contains both classes after all phases (9 triples
    // total: 5 for Documented, 4 for Undocumented).
    assert_eq!(
        run.store.len().unwrap(),
        9,
        "base store size must be unchanged after phases"
    );
}

#[test]
fn scoped_overlay_does_not_leak_example_quads() {
    // Base graph with one triple.
    let base_path = write_tmp(
        "gmeow_validate_all_base.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let store = build_store(&[base_path.to_string_lossy().to_string()]);
    std::fs::remove_file(&base_path).ok();

    assert_eq!(store.len().unwrap(), 1, "base store starts with one triple");

    // Example graph adds a distinct triple.
    let example_path = write_tmp(
        "gmeow_validate_all_example.ttl",
        "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
    );
    let quads = parse_file(&example_path).expect("example must parse");
    std::fs::remove_file(&example_path).ok();

    let inserted = scoped_overlay_insert(&store, quads.iter());
    assert_eq!(inserted.len(), 1, "example-only quad must be inserted");
    assert_eq!(
        store.len().unwrap(),
        2,
        "overlay quad must be visible during validation"
    );

    scoped_overlay_remove(&store, &inserted);
    assert_eq!(
        store.len().unwrap(),
        1,
        "base store must be restored after overlay removal"
    );
}

#[test]
fn scoped_overlay_skips_quads_already_in_base() {
    // Base graph already contains the example triple.
    let base_path = write_tmp(
        "gmeow_validate_all_base_dup.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let store = build_store(&[base_path.to_string_lossy().to_string()]);
    std::fs::remove_file(&base_path).ok();

    let example_path = write_tmp(
        "gmeow_validate_all_example_dup.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\nex:c ex:p ex:d .\n",
    );
    let quads = parse_file(&example_path).expect("example must parse");
    std::fs::remove_file(&example_path).ok();

    let inserted = scoped_overlay_insert(&store, quads.iter());
    assert_eq!(
        inserted.len(),
        1,
        "only the quad not already in base must be tracked"
    );
    assert_eq!(store.len().unwrap(), 2);

    scoped_overlay_remove(&store, &inserted);
    assert_eq!(store.len().unwrap(), 1, "base triple must survive removal");
}

#[test]
fn timings_are_populated_when_requested() {
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
    let path = write_tmp("gmeow_validate_all_timings.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let options = ValidateOptions {
        timings: true,
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &options,
    )
    .expect("orchestration must complete");

    std::fs::remove_file(&path).ok();

    assert!(
        !run.timings.is_empty(),
        "timings must be recorded when requested"
    );
    assert!(
        run.timings.iter().any(|t| t.phase == "build-store"),
        "build-store timing must be present"
    );
    assert!(
        run.timings.iter().any(|t| t.phase == "structural-lint"),
        "structural-lint timing must be present"
    );
    // All recorded elapsed times should be finite (u128 non-negative).
    assert!(run.timings.iter().all(|t| t.elapsed_ms < u128::MAX));
}

#[test]
fn test_dsl_shacl_runs_in_orchestration() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix gufo: <http://purl.org/nemo/gufo#> .\n\
         gmeow:Thing a owl:Class , gufo:Kind ;\n\
           rdfs:label \"Thing\" ;\n\
           skos:definition \"A thing.\" ;\n\
           rdfs:isDefinedBy <{NS}> ;\n\
           gmeow:graphBoxRole gmeow:boxTBox .\n"
    );
    let source_path = write_tmp("gmeow_validate_all_test_dsl_source.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let test_dsl_shapes = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                           @prefix ex: <https://example.org/> .\n\
                           ex:TestShape a sh:NodeShape ;\n\
                             sh:targetClass ex:Test ;\n\
                             sh:property [ sh:path ex:name ; sh:minCount 1 ] ."
        .to_owned();

    let vocab_dir = std::env::temp_dir().join(format!(
        "gmeow_validate_all_test_dsl_vocab_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&vocab_dir).unwrap();
    std::fs::write(
        vocab_dir.join("vocabulary.ttl"),
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix ex: <https://example.org/> .\n\
         ex:Test a owl:Class ;\n\
           rdfs:label \"Test\" ;\n\
           skos:definition \"A test.\" ;\n\
           rdfs:isDefinedBy <https://example.org/> .\n",
    )
    .unwrap();

    let slices_dir = std::env::temp_dir().join(format!(
        "gmeow_validate_all_test_dsl_slices_{}",
        std::process::id()
    ));
    let slice_dir = slices_dir.join("core").join("demo");
    std::fs::create_dir_all(slice_dir.join("tests")).unwrap();
    std::fs::write(slice_dir.join("manifest.ttl"), "# manifest\n").unwrap();
    std::fs::write(
        slice_dir.join("tests").join("demo.ttl"),
        "@prefix ex: <https://example.org/> .\n\
         ex:badTest a ex:Test .\n",
    )
    .unwrap();

    let options = ValidateOptions {
        slices_dir: Some(slices_dir.to_string_lossy().to_string()),
        test_dsl_dir: Some(vocab_dir.to_string_lossy().to_string()),
        test_dsl_shapes_ttl: Some(test_dsl_shapes),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[source_path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &options,
    )
    .expect("orchestration must complete");

    std::fs::remove_file(&source_path).ok();
    std::fs::remove_dir_all(&vocab_dir).ok();
    std::fs::remove_dir_all(&slices_dir).ok();

    assert!(
        run.errors()
            .iter()
            .any(|e| e == "SHACL constraint violated"),
        "test DSL SHACL phase must flag the missing ex:name violation: {:?}",
        run.errors()
    );
}

/// Helper: build a store from a list of Turtle file paths.
fn build_store(paths: &[String]) -> oxigraph::store::Store {
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    gmeow_validate::store::build_store(&path_bufs).expect("store must build")
}
