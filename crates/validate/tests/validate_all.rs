// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the Rust-native validation orchestration.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use gmeow_validate::lint::LintConfig;
use gmeow_validate::store::{dataset_from_paths, parse_file_dataset};
use gmeow_validate::validate_all::{ValidateOptions, ValidationRun};
use purrdf::{DatasetView, GraphMatch, RdfDatasetBuilder};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on drop,
/// including on panic and early return. Bind it to a named `_tmp` (never a bare
/// `_`, which would drop it immediately) so it outlives the path. The file *name*
/// is preserved because the orchestration dispatches on the `.ttl` extension and
/// the syntax-error assertion matches on the file stem.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
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
    let (_tmp, path) = write_tmp("gmeow_validate_all_reuse.ttl", &ttl);
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

    // The shared dataset still contains both classes after all phases (9 triples
    // total: 5 for Documented, 4 for Undocumented).
    assert_eq!(
        run.dataset.quad_count(),
        9,
        "base dataset size must be unchanged after phases"
    );
}

#[test]
fn example_merge_unions_base_and_example_without_leaking() {
    // Base graph with one triple.
    let (_base_tmp, base_path) = write_tmp(
        "gmeow_validate_all_base.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let base = build_store(&[base_path.to_string_lossy().to_string()]);
    assert_eq!(base.quad_count(), 1, "base dataset starts with one triple");

    // Example carrying one duplicate of the base triple plus one new triple.
    let (_example_tmp, example_path) = write_tmp(
        "gmeow_validate_all_example.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\nex:c ex:p ex:d .\n",
    );
    let example = parse_file_dataset(&example_path).expect("example must parse");

    // The per-example merge re-interns the base quads + the example quads into a fresh
    // dataset; duplicates dedup and the base is untouched (no shared mutable store).
    let mut builder = RdfDatasetBuilder::new();
    for quad in base.owned_quads() {
        builder.push_owned_quad(&quad);
    }
    builder.push_dataset(&example);
    let merged = builder.freeze().expect("merge must freeze");
    assert_eq!(merged.quad_count(), 2, "the duplicate base triple dedups");
    assert_eq!(base.quad_count(), 1, "the base dataset is untouched");
    assert_eq!(
        merged
            .quads_for_pattern(None, None, None, GraphMatch::Default)
            .count(),
        2
    );
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
    let (_tmp, path) = write_tmp("gmeow_validate_all_timings.ttl", &ttl);
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
    let (_source_tmp, source_path) = write_tmp("gmeow_validate_all_test_dsl_source.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let test_dsl_shapes = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                           @prefix ex: <https://example.org/> .\n\
                           ex:TestShape a sh:NodeShape ;\n\
                             sh:targetClass ex:Test ;\n\
                             sh:property [ sh:path ex:name ; sh:minCount 1 ] ."
        .to_owned();

    // ONE RAII root holds both the vocab dir and the `slices/` tree; it is
    // removed when `dsl_tmp` drops at end of scope, including on panic.
    let dsl_tmp = tempfile::tempdir().expect("create temp dir");
    let vocab_dir = dsl_tmp.path().join("vocab");
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

    let slices_dir = dsl_tmp.path().join("slices");
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

    assert!(
        run.errors()
            .iter()
            .any(|e| e == "SHACL constraint violated"),
        "test DSL SHACL phase must flag the missing ex:name violation: {:?}",
        run.errors()
    );
}

#[test]
fn syntax_error_is_caught_before_structural_phases() {
    let (_bad_tmp, bad_path) = write_tmp(
        "gmeow_validate_all_syntax_bad.ttl",
        "this is not turtle @@@ <<<",
    );
    let shapes_ttl = mini_shapes_ttl();

    let result = ValidationRun::run(
        &[bad_path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &ValidateOptions::default(),
    );

    let msg = match result {
        Err(e) => e.message().to_string(),
        Ok(_) => panic!("orchestration must fail on syntax error"),
    };
    assert!(
        msg.contains("syntax error in") && msg.contains("gmeow_validate_all_syntax_bad"),
        "syntax error must be reported; got: {msg}"
    );
}

#[test]
fn validate_all_short_circuits_when_sameas_ban_fails() {
    let (_sameas_tmp, sameas_path) = write_tmp(
        "gmeow_validate_all_sameas_bad.ttl",
        "@prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         ex:a owl:sameAs ex:b .\n",
    );
    let shapes_ttl = mini_shapes_ttl();

    let run = ValidationRun::run(
        &[sameas_path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &ValidateOptions::default(),
    )
    .expect("orchestration must complete (reporting sameAs ban)");

    assert!(
        run.errors().iter().any(|e| {
            e.contains("banned owl:sameAs to external entity")
                && e.contains("https://example.org/a")
        }),
        "sameAs ban must flag external entity: {:?}",
        run.errors()
    );
    // Store-based phases are skipped once the sameAs ban reports errors.
    assert!(
        run.declared_terms.is_empty(),
        "declared terms must be empty after sameAs failure"
    );
}

#[test]
fn sameas_ban_allows_internal_sameas() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:A owl:sameAs gmeow:B .\n"
    );
    let (_tmp, path) = write_tmp("gmeow_validate_all_sameas_internal.ttl", &ttl);
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

    assert!(
        !run.errors().iter().any(|e| e.contains("banned owl:sameAs")),
        "internal sameAs must be allowed: {:?}",
        run.errors()
    );
}

#[test]
fn sameas_ban_respects_allowlist() {
    let ttl = "@prefix ex: <https://example.org/> .\n\
               @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
               ex:a owl:sameAs ex:b .\n"
        .to_owned();
    let (_tmp, path) = write_tmp("gmeow_validate_all_sameas_allowed.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let options = ValidateOptions {
        sameas_allowlist: vec![(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned(),
        )],
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

    assert!(
        run.errors().is_empty(),
        "allowlisted sameAs must pass: {:?}",
        run.errors()
    );
}

#[test]
fn empty_source_paths_rejected() {
    let shapes_ttl = mini_shapes_ttl();
    let result = ValidationRun::run(
        &[],
        &shapes_ttl,
        "",
        "",
        &lint_config(),
        &ValidateOptions::default(),
    );
    assert!(result.is_err(), "empty source paths must fail fast");
    let msg = match result {
        Err(e) => e.message().to_string(),
        Ok(_) => panic!("expected an error"),
    };
    assert!(
        msg.contains("source_paths must not be empty"),
        "error should mention empty source_paths; got: {msg}"
    );
}

#[test]
fn structural_lint_flags_missing_annotations_in_orchestration() {
    // A GMEOW class with label and isDefinedBy but no skos:definition.
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:Undocumented a owl:Class ;\n\
           rdfs:label \"x\" ;\n\
           rdfs:isDefinedBy <{NS}> ;\n\
           rdfs:subClassOf owl:Thing .\n"
    );
    let (_tmp, path) = write_tmp("gmeow_validate_all_structural.ttl", &ttl);
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

    assert!(
        run.errors().iter().any(|e| e.contains("skos:definition")),
        "structural lint must flag missing definition: {:?}",
        run.errors()
    );
}

#[test]
fn mapping_dsl_shacl_runs_in_orchestration() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:Thing a owl:Class ;\n\
           rdfs:label \"Thing\" ;\n\
           skos:definition \"A thing.\" ;\n\
           rdfs:isDefinedBy <{NS}> .\n"
    );
    let (_source_tmp, source_path) = write_tmp("gmeow_validate_all_mapping_source.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let mapping_dsl_shapes = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                              @prefix ex: <https://example.org/> .\n\
                              ex:MappingShape a sh:NodeShape ;\n\
                                sh:targetClass ex:Mapping ;\n\
                                sh:property [ sh:path ex:source ; sh:minCount 1 ] ."
        .to_owned();

    // RAII: the vocab dir is removed when `vocab_tmp` drops at end of scope,
    // including on panic.
    let vocab_tmp = tempfile::tempdir().expect("create temp dir");
    let vocab_dir = vocab_tmp.path().join("mapping-dsl-vocab");
    std::fs::create_dir_all(&vocab_dir).unwrap();
    std::fs::write(
        vocab_dir.join("vocabulary.ttl"),
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix ex: <https://example.org/> .\n\
         ex:Mapping a owl:Class ;\n\
           rdfs:label \"Mapping\" ;\n\
           skos:definition \"A mapping.\" ;\n\
           rdfs:isDefinedBy <https://example.org/> .\n",
    )
    .unwrap();
    std::fs::write(
        vocab_dir.join("bad.ttl"),
        "@prefix ex: <https://example.org/> .\n\
         ex:badMapping a ex:Mapping .\n",
    )
    .unwrap();

    let options = ValidateOptions {
        mapping_shapes_ttl: Some(mapping_dsl_shapes),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[source_path.to_string_lossy().to_string()],
        &shapes_ttl,
        &vocab_dir.to_string_lossy(),
        "",
        &lint_config(),
        &options,
    )
    .expect("orchestration must complete");

    assert!(
        run.errors()
            .iter()
            .any(|e| e == "SHACL constraint violated"),
        "mapping DSL SHACL phase must flag the missing ex:source violation: {:?}",
        run.errors()
    );
}

/// Build a single minimal-but-otherwise-complete `gmeow:DeclinedCorrespondence` Turtle
/// record, varying the two fields whose guards this test isolates: the
/// `gmeow:declinedTarget` label and whether `gmeow:probeVerdict` is present.
///
/// Every other field required by `gmeow:DeclinedCorrespondenceShape`
/// (`shapes/mapping-dsl-shapes.ttl`) is populated so a violation can be attributed
/// unambiguously to the field under test.
fn declined_correspondence_fixture(target: &str, include_probe_verdict: bool) -> String {
    let probe_verdict_triple = if include_probe_verdict {
        "gmeow:probeVerdict \"2026-01-01: fixture probe finding.\" ;\n"
    } else {
        ""
    };
    format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix skos:  <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\
         gmeow:testDeclinedFixture a gmeow:DeclinedCorrespondence ;\n\
           rdfs:label \"test decline fixture\" ;\n\
           gmeow:declinedTarget \"{target}\" ;\n\
           gmeow:intendedRelation skos:closeMatch ;\n\
           gmeow:declineRationale \"Fixture rationale for the SHACL teeth test.\" ;\n\
           gmeow:candidateNamespace \"https://example.org/fixture-probe\" ;\n\
           {probe_verdict_triple}\
           gmeow:revisitCondition \"Bridge iff a fixture condition holds.\" ;\n\
           gmeow:contentCarriedBy gmeow:AffectVectorObservation ;\n\
           logic:preservationKind logic:Unsupported .\n"
    )
}

/// Extract the verbatim `gmeow:DeclinedCorrespondenceShape` node-shape block out of the
/// full `shapes/mapping-dsl-shapes.ttl` text, prefixed with the file's own `@prefix`
/// header.
///
/// The full file also carries `gmeow:MappingDslVocabularyTermShape`, whose
/// `sh:target`/`sh:select` `SPARQLTarget` matches *any* `owl:Ontology`/`owl:Class`/etc.
/// subject in the gmeow namespace (by `STRSTARTS(STR(?this), ".../gmeow/")`) — including
/// the affect slice's OWN ontology header in `declined-bridges.ttl` — and demands an
/// `rdfs:isDefinedBy <.../mapping-dsl>` that only real `dsl/mappings/vocabulary.ttl`
/// terms carry. That is a pre-existing scoping property of a DIFFERENT, unrelated shape
/// (it only causes no harm in production because the real mapping-DSL SHACL phase only
/// ever loads `dsl/mappings/**/*.ttl`, never slice-local mapping files together with it).
/// Extracting just the shape under test keeps this test exercising the REAL, unmodified
/// `gmeow:DeclinedCorrespondenceShape` text without also tripping that unrelated shape.
fn extract_declined_correspondence_shape_ttl(shapes_ttl: &str) -> String {
    let prefixes: String = shapes_ttl
        .lines()
        .take_while(|line| line.starts_with("@prefix") || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let start = shapes_ttl.find("gmeow:DeclinedCorrespondenceShape").expect(
        "gmeow:DeclinedCorrespondenceShape must be present in shapes/mapping-dsl-shapes.ttl",
    );
    let rest = &shapes_ttl[start..];
    let end = rest.find("gmeow:ProjectionMappingShape").expect(
        "gmeow:ProjectionMappingShape must still follow DeclinedCorrespondenceShape in \
         shapes/mapping-dsl-shapes.ttl — update this extraction if the shapes file is \
         reordered",
    );
    let shape_block = rest[..end].trim_end();

    format!("{prefixes}\n\n{shape_block}\n")
}

/// Exercises the REAL `gmeow:DeclinedCorrespondenceShape` (`shapes/mapping-dsl-shapes.ttl`)
/// directly through the native SHACL engine — not the `ex:MappingShape` placeholder that
/// `mapping_dsl_shacl_runs_in_orchestration` uses.
///
/// POSITIVE: the real `slices/core/affect/mappings/declined-bridges.ttl` ledger must
/// conform. NEGATIVE: a bare-acronym `"EFO"` target, a URI-shaped target, and a record
/// missing `gmeow:probeVerdict` must each trip a violation — proving the shape's guards
/// have teeth on data, independent of whether any production gate wires the shape in.
#[test]
fn declined_correspondence_shape_has_teeth() {
    use gmeow_validate::store::{parse_file_dataset, shacl_validate_dataset};

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("crates/validate/../.. must resolve to the repo root");

    let shapes_path = repo_root.join("shapes/mapping-dsl-shapes.ttl");
    let full_shapes_ttl = std::fs::read_to_string(&shapes_path)
        .expect("shapes/mapping-dsl-shapes.ttl must be readable");
    let shapes_ttl = extract_declined_correspondence_shape_ttl(&full_shapes_ttl);
    // `parse_shapes` (not `shapes::from_dataset`) recovers the document's `@prefix` map,
    // the same entrypoint `crates/validate/src/dsl_shacl.rs::validate_dsl` uses for the
    // mapping-DSL SHACL phase (phase 11) in the real orchestration.
    let shapes = purrdf::shapes::engine::parse_shapes(&shapes_ttl, None)
        .expect("the extracted DeclinedCorrespondenceShape text must parse as SHACL shapes");

    // POSITIVE: the real declined-bridge ledger conforms to the real shape.
    let ledger_path = repo_root.join("slices/core/affect/mappings/declined-bridges.ttl");
    let ledger_dataset = parse_file_dataset(&ledger_path).expect("declined-bridges.ttl must parse");
    let ledger_report = shacl_validate_dataset(&ledger_dataset, &shapes);
    assert!(
        ledger_report.conforms,
        "the real declined-bridges.ttl ledger must conform to the real \
         DeclinedCorrespondenceShape: {:?}",
        ledger_report
            .results
            .iter()
            .map(|r| r.message.clone())
            .collect::<Vec<_>>()
    );

    // NEGATIVE (a): bare acronym "EFO" — must collide with the \bEFO\b guard.
    let efo_ttl = declined_correspondence_fixture("EFO", true);
    let (_efo_tmp, efo_path) = write_tmp("gmeow_validate_declined_efo.ttl", &efo_ttl);
    let efo_dataset = parse_file_dataset(&efo_path).expect("EFO fixture must parse");
    let efo_report = shacl_validate_dataset(&efo_dataset, &shapes);
    assert!(
        !efo_report.conforms,
        "a declinedTarget of bare \"EFO\" must violate DeclinedCorrespondenceShape"
    );

    // NEGATIVE (b): a URI-shaped target — must collide with the ^https?:// guard.
    let uri_ttl = declined_correspondence_fixture("http://dead.example/term", true);
    let (_uri_tmp, uri_path) = write_tmp("gmeow_validate_declined_uri.ttl", &uri_ttl);
    let uri_dataset = parse_file_dataset(&uri_path).expect("URI fixture must parse");
    let uri_report = shacl_validate_dataset(&uri_dataset, &shapes);
    assert!(
        !uri_report.conforms,
        "a declinedTarget shaped as a URI must violate DeclinedCorrespondenceShape"
    );

    // NEGATIVE (c): omit gmeow:probeVerdict — must collide with the minCount 1 guard.
    let missing_probe_ttl =
        declined_correspondence_fixture("Otherwise Complete Fixture Target", false);
    let (_missing_probe_tmp, missing_probe_path) = write_tmp(
        "gmeow_validate_declined_missing_probe.ttl",
        &missing_probe_ttl,
    );
    let missing_probe_dataset =
        parse_file_dataset(&missing_probe_path).expect("missing-probeVerdict fixture must parse");
    let missing_probe_report = shacl_validate_dataset(&missing_probe_dataset, &shapes);
    assert!(
        !missing_probe_report.conforms,
        "a DeclinedCorrespondence missing gmeow:probeVerdict must violate \
         DeclinedCorrespondenceShape"
    );
}

#[test]
fn statement_dsl_shacl_runs_in_orchestration() {
    let ttl = format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:Thing a owl:Class ;\n\
           rdfs:label \"Thing\" ;\n\
           skos:definition \"A thing.\" ;\n\
           rdfs:isDefinedBy <{NS}> .\n"
    );
    let (_source_tmp, source_path) = write_tmp("gmeow_validate_all_statement_source.ttl", &ttl);
    let shapes_ttl = mini_shapes_ttl();

    let statement_dsl_shapes = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                                @prefix ex: <https://example.org/> .\n\
                                ex:StatementShape a sh:NodeShape ;\n\
                                  sh:targetClass ex:Statement ;\n\
                                  sh:property [ sh:path ex:subject ; sh:minCount 1 ] ."
        .to_owned();

    // RAII: the vocab dir is removed when `vocab_tmp` drops at end of scope,
    // including on panic.
    let vocab_tmp = tempfile::tempdir().expect("create temp dir");
    let vocab_dir = vocab_tmp.path().join("statement-dsl-vocab");
    std::fs::create_dir_all(&vocab_dir).unwrap();
    std::fs::write(
        vocab_dir.join("vocabulary.ttl"),
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix ex: <https://example.org/> .\n\
         ex:Statement a owl:Class ;\n\
           rdfs:label \"Statement\" ;\n\
           skos:definition \"A statement.\" ;\n\
           rdfs:isDefinedBy <https://example.org/> .\n",
    )
    .unwrap();
    std::fs::write(
        vocab_dir.join("bad.ttl"),
        "@prefix ex: <https://example.org/> .\n\
         ex:badStatement a ex:Statement .\n",
    )
    .unwrap();

    let options = ValidateOptions {
        statement_shapes_ttl: Some(statement_dsl_shapes),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[source_path.to_string_lossy().to_string()],
        &shapes_ttl,
        "",
        &vocab_dir.to_string_lossy(),
        &lint_config(),
        &options,
    )
    .expect("orchestration must complete");

    assert!(
        run.errors()
            .iter()
            .any(|e| e == "SHACL constraint violated"),
        "statement DSL SHACL phase must flag the missing ex:subject violation: {:?}",
        run.errors()
    );
}

/// Helper: build a frozen native dataset from a list of Turtle file paths.
fn build_store(paths: &[String]) -> std::sync::Arc<purrdf::RdfDataset> {
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    dataset_from_paths(&path_bufs).expect("dataset must build")
}
