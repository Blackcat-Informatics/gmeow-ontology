// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::source_contracts::{authored_expected, authored_fanout_rules, declared_value_set};
use super::*;

#[test]
fn excluded_holds_exactly_the_one_terminal_bundle() {
    assert_eq!(EXCLUDED.len(), 1);
    assert!(EXCLUDED.contains(&"generated/dist/gmeow.gts"));
}

#[test]
fn archive_member_committed_path_restores_directory_for_basename_reps() {
    use crate::stages::carrier::committed_path_for_archive_member;
    // Basename-keyed reps get their directory prefix restored.
    assert_eq!(
        committed_path_for_archive_member("mappings-archive", "foaf.sssom.tsv").as_deref(),
        Some("generated/mappings/foaf.sssom.tsv")
    );
    assert_eq!(
        committed_path_for_archive_member("queries-archive", "bare.rq").as_deref(),
        Some("generated/queries/bare.rq")
    );
    assert_eq!(
        committed_path_for_archive_member("schemas-archive", "gmeow.schema.json").as_deref(),
        Some("generated/schemas/gmeow.schema.json")
    );
    // Repo-relative reps pass through unchanged.
    assert_eq!(
        committed_path_for_archive_member("generated-opaque-archive", "generated/n3/gmeow.n3")
            .as_deref(),
        Some("generated/n3/gmeow.n3")
    );
    assert_eq!(
        committed_path_for_archive_member("axioms-archive", "generated/owl/gmeow-dl.ttl")
            .as_deref(),
        Some("generated/owl/gmeow-dl.ttl")
    );
    // A non-generated rep resolves nothing.
    assert_eq!(
        committed_path_for_archive_member("cells-archive", "dsl/mappings/x.ttl"),
        None
    );
}

fn byte_decorated_rdf_paths_fall_through_to_blob_members() {
    let rules = authored_fanout_rules();
    for path in [
        "generated/logic/inferred-closure.rdf12.ttl",
        "generated/logic/reasoning-explanations.rdf12.ttl",
        "generated/logic/dl-el-crosscheck-report.ttl",
        "generated/logic/perf-ledger.ttl",
        "generated/metadata/void.ttl",
        "generated/metadata/dcat.ttl",
        // The statement layer's two: same reason, but they reconstruct from
        // REP_STATEMENTS rather than REP_GENERATED — a rep is the unit a dictionary
        // primes, and these are the claim corpus's byte frames.
        "generated/statements/gmeow-statements.owl.ttl",
        "generated/statements/gmeow.rdf12.ttl",
    ] {
        assert!(
            graph_rep_for_path(&rules, path).is_none(),
            "{path} has generated comments / section markers, so it cannot reconstruct \
                 from a canonical named-graph fold and must ride an archive member"
        );
    }
}

#[test]
fn reconstruct_graph_folds_turtle_without_the_graph_label() {
    use purrdf::RdfDatasetBuilder;

    const G: &str = "https://blackcatinformatics.ca/gmeow/graph/projections/sample.edoal";
    const S: &str = "https://blackcatinformatics.ca/gmeow/projections/sample";
    const P: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const O: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Alignment";

    let mut b = RdfDatasetBuilder::new();
    let g = b.intern_iri(G);
    let s = b.intern_iri(S);
    let p = b.intern_iri(P);
    let o = b.intern_iri(O);
    b.push_quad(s, p, o, Some(g));
    let dataset = b.freeze().expect("freeze");

    let turtle = reconstruct_graph(
        &dataset,
        &GraphRep {
            iri: G.to_string(),
            form: GraphForm::Turtle,
        },
    )
    .expect("turtle reconstruction");
    let turtle = String::from_utf8(turtle).expect("utf8");
    assert!(turtle.contains("align:Alignment") || turtle.contains(O));
    assert!(
        !turtle.contains(G),
        "turtle fold must not carry the graph label"
    );

    // A graph IRI with no quads yields no representative.
    assert!(
        reconstruct_graph(
            &dataset,
            &GraphRep {
                iri: "https://blackcatinformatics.ca/gmeow/graph/absent".to_string(),
                form: GraphForm::Turtle,
            },
        )
        .is_none()
    );
}

fn quality_assessment_nt_folds_as_ntriples_via_its_own_fanout_graph() {
    // The G2 quality-assessment `.nt` is a registered RDF-fanout class that folds as
    // plain N-Triples (default graph, no label) — the form the fanout writer emits and
    // the superset gate reconstructs, so `file == fold` holds by construction.
    const PATH: &str = "generated/quality/gmeow.quality-assessment.nt";
    assert!(source_contracts::authored_fanout_classes().contains(PATH));
    let rules = authored_fanout_rules();
    let rep =
        graph_rep_for_path(&rules, PATH).expect("quality-assessment path resolves a graph rep");
    assert_eq!(rep.form, GraphForm::NTriples);
    assert_eq!(
        rep.iri,
        "https://blackcatinformatics.ca/gmeow/graph/fanout/quality/gmeow.quality-assessment.nt"
    );
}

#[test]
fn edoal_graph_iri_convention_is_identity_in_both_directions() {
    assert_eq!(
        edoal_projection_graph_iri("generated/projections/foaf.edoal.ttl").as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/graph/projections/foaf.edoal")
    );
    // Non-EDOAL projections (template-emitted) are NOT named-graph carried yet.
    assert!(edoal_projection_graph_iri("generated/projections/core-prefixes.ttl").is_none());
    assert!(edoal_projection_graph_iri("generated/projections/functions.fno.ttl").is_none());
    assert!(edoal_projection_graph_iri("generated/mappings/foaf.sssom.tsv").is_none());
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn sweep_against_materialized_detects_missing_mismatch_and_orphan() {
    use std::io::Write;
    // Authority is the PROJECTION. Materialize a disk tree of three files under
    // generated/, then inject a projection whose keys diverge from it.
    // RAII: the materialized tree is removed when `tmp` drops, including on a
    // failed assertion below.
    let tmp = tempfile::tempdir().expect("create temp materialized root");
    let dir = tmp.path();
    let gen_dir = dir.join("generated/x");
    std::fs::create_dir_all(&gen_dir).unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let mut f = std::fs::File::create(gen_dir.join(name)).unwrap();
        f.write_all(bytes).unwrap();
    };
    write("match.ttl", b"SAME");
    write("drift.ttl", b"DISK-BYTES");
    write("orphan.ttl", b"UNDECLARED");

    // Projection keys: match.ttl agrees with disk; drift.ttl reconstructs to
    // different bytes than disk (mismatch); missing.ttl has NO disk file (missing).
    // orphan.ttl is on disk but is NOT a projection key (orphan).
    let mut files = BTreeMap::new();
    files.insert("generated/x/match.ttl".to_string(), b"SAME".to_vec());
    files.insert(
        "generated/x/drift.ttl".to_string(),
        b"BUNDLE-BYTES".to_vec(),
    );
    files.insert("generated/x/missing.ttl".to_string(), b"GONE".to_vec());
    let projection = BundleProjection { files };

    let report = sweep_against_materialized(&projection, dir).unwrap();

    assert_eq!(report.missing, vec!["generated/x/missing.ttl".to_string()]);
    assert_eq!(report.mismatch, vec!["generated/x/drift.ttl".to_string()]);
    assert_eq!(report.orphan, vec!["generated/x/orphan.ttl".to_string()]);
    assert!(!report.is_clean());
}

#[test]
fn superset_empty_materialized_tree_hard_fails() {
    // The vacuous-pass guard: with the projection as the authority, an EMPTY
    // (or absent) generated/ tree can never pass clean — every projection key is
    // flagged missing. Prove it for both an empty generated/ dir and a wholly
    // absent one.
    let mut files = BTreeMap::new();
    files.insert("generated/x/a.ttl".to_string(), b"A".to_vec());
    files.insert("generated/y/b.ttl".to_string(), b"B".to_vec());
    let projection = BundleProjection { files };

    // (a) An empty-but-present generated/ directory. RAII: removed when
    // `empty_tmp` drops, including on a failed assertion below.
    let empty_tmp = tempfile::tempdir().expect("create empty materialized root");
    let empty_dir = empty_tmp.path();
    std::fs::create_dir_all(empty_dir.join("generated")).unwrap();
    let report = sweep_against_materialized(&projection, empty_dir).unwrap();
    assert_eq!(
        report.missing,
        vec![
            "generated/x/a.ttl".to_string(),
            "generated/y/b.ttl".to_string()
        ]
    );
    assert!(report.orphan.is_empty());
    assert!(
        !report.is_clean(),
        "an empty generated/ tree must HARD-fail, never pass vacuously"
    );

    // (b) A wholly absent generated/ tree (fresh clone) is equally not clean.
    // The root exists but carries no `generated/` child at all.
    let absent_tmp = tempfile::tempdir().expect("create absent-generated root");
    let absent_dir = absent_tmp.path();
    let report = sweep_against_materialized(&projection, absent_dir).unwrap();
    assert_eq!(report.missing.len(), 2);
    assert!(!report.is_clean());
}

#[test]
fn fanout_rules_drive_reconstruction_and_bijection() {
    // The path↔representative map read as DATA (the promoted gmeow:fanoutExtracts rows):
    // parse a small row set, prove the form/family/graph-IRI resolution is data-driven,
    // and prove the bijection HARD-fail fires on an unmapped path AND on a stale row.
    let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:r2 gmeow:extractsPath "generated/logic/gmeow.correspondence.nt" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "ntriples" .
gmeow:r3 gmeow:extractsPath "generated/diagnostics/shacl.nq" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "nquads-diagnostics" .
gmeow:r4 gmeow:extractsPath "generated/profiles/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:r5 gmeow:extractsPath "generated/projections/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsSuffix ".edoal.ttl" ; gmeow:extractsGraphFamily "edoal" ; gmeow:extractsForm "turtle" .
"#;
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
    let rules = read_fanout_rules(&ds).unwrap();
    assert_eq!(rules.len(), 5);

    // Form + graph-IRI resolution is driven entirely by the data rows.
    let rep = graph_rep_for_path(&rules, "generated/evals/scores.ttl").unwrap();
    assert_eq!(rep.form, GraphForm::Turtle);
    assert_eq!(
        rep.iri,
        "https://blackcatinformatics.ca/gmeow/graph/fanout/evals/scores.ttl"
    );
    assert_eq!(
        graph_rep_for_path(&rules, "generated/logic/gmeow.correspondence.nt")
            .unwrap()
            .form,
        GraphForm::NTriples
    );
    assert_eq!(
        graph_rep_for_path(&rules, "generated/diagnostics/shacl.nq")
            .unwrap()
            .form,
        GraphForm::NQuads(GRAPH_DIAGNOSTICS_IRI)
    );
    // The EDOAL prefix+suffix rule resolves the edoal graph family.
    assert_eq!(
        graph_rep_for_path(&rules, "generated/projections/foaf.edoal.ttl")
            .unwrap()
            .iri,
        "https://blackcatinformatics.ca/gmeow/graph/projections/foaf.edoal"
    );
    // A profiles/ file rides the rdf-fanout prefix rule.
    assert_eq!(
        graph_rep_for_path(&rules, "generated/profiles/full.ttl")
            .unwrap()
            .iri,
        "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/full.ttl"
    );
    // A non-EDOAL projection under the same directory does NOT match the suffix-filtered
    // edoal rule (and no other rule claims it) — no representative.
    assert!(graph_rep_for_path(&rules, "generated/projections/core-prefixes.ttl").is_none());

    // Bijection holds for a path set each rule claims exactly.
    let paths: BTreeSet<String> = [
        "generated/evals/scores.ttl",
        "generated/logic/gmeow.correspondence.nt",
        "generated/diagnostics/shacl.nq",
        "generated/profiles/full.ttl",
        "generated/projections/foaf.edoal.ttl",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    check_fanout_bijection(&rules, &paths).expect("bijection holds over the covered paths");

    // Deliberately-missing row: a reconstructed path no rule matches HARD-fails.
    let mut unmapped = paths.clone();
    unmapped.insert("generated/quality/gmeow.quality-assessment.nt".to_string());
    let err = check_fanout_bijection(&rules, &unmapped).unwrap_err();
    assert_eq!(err.code(), crate::error::FanoutBijection::register());

    // Stale row: a rule matching no reconstructed path HARD-fails (drop r2's path).
    let mut stale = paths.clone();
    stale.remove("generated/logic/gmeow.correspondence.nt");
    let err2 = check_fanout_bijection(&rules, &stale).unwrap_err();
    assert_eq!(err2.code(), crate::error::FanoutBijection::register());
}

#[test]
fn opaque_rows_parse_and_bijection_checks_the_archive_members() {
    // The opaque family: exact/opaque/blob rows carrier-emitted per REP_GENERATED
    // member. Prove they parse, resolve NO named-graph rep (they ride the blob lane),
    // and that check_opaque_bijection HARD-fails on an undeclared member AND a stale row.
    let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:o1 gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
gmeow:o2 gmeow:extractsPath "generated/logic/inferred-closure.rdf12.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
"#;
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
    let rules = read_fanout_rules(&ds).unwrap();
    let (opaque, named): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
        .into_iter()
        .partition(|r| r.family == FanoutFamily::Opaque);
    assert_eq!(opaque.len(), 2);
    assert_eq!(named.len(), 1);

    // Opaque rows never resolve a named-graph rep — even the byte-decorated `.ttl` one.
    assert!(graph_rep_for_path(&opaque, "generated/n3/gmeow.n3").is_none());
    assert!(graph_rep_for_path(&opaque, "generated/logic/inferred-closure.rdf12.ttl").is_none());

    // Bijection holds when the member set equals the opaque-row path set exactly.
    let members: BTreeSet<String> = [
        "generated/n3/gmeow.n3",
        "generated/logic/inferred-closure.rdf12.ttl",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    check_opaque_bijection(&opaque, &members).expect("opaque bijection holds");

    // Undeclared member (a blob member with no opaque row) HARD-fails.
    let mut undeclared = members.clone();
    undeclared.insert("generated/cl/gmeow.clif".to_string());
    let err = check_opaque_bijection(&opaque, &undeclared).unwrap_err();
    assert_eq!(err.code(), crate::error::FanoutBijection::register());

    // Stale opaque row (a row claiming no archive member) HARD-fails.
    let mut stale = members.clone();
    stale.remove("generated/n3/gmeow.n3");
    let err2 = check_opaque_bijection(&opaque, &stale).unwrap_err();
    assert_eq!(err2.code(), crate::error::FanoutBijection::register());
}

#[test]
fn opaque_family_and_blob_form_are_mutually_required() {
    // A row that pairs opaque family with a non-blob form is malformed → HARD FAIL.
    let bad_form = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "turtle" .
"#;
    let ds = purrdf::parse_dataset(bad_form.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_fanout_rules(&ds).is_err());

    // A blob form paired with a non-opaque family is equally malformed.
    let bad_family = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "blob" .
"#;
    let ds2 = purrdf::parse_dataset(bad_family.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_fanout_rules(&ds2).is_err());

    // An opaque row using a prefix match is malformed (opaque members are exact).
    let bad_prefix = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
"#;
    let ds3 = purrdf::parse_dataset(bad_prefix.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_fanout_rules(&ds3).is_err());
}

#[test]
fn clean_report_requires_all_three_sweeps_empty() {
    let clean = SupersetReport {
        missing: vec![],
        mismatch: vec![],
        orphan: vec![],
    };
    assert!(clean.is_clean());
    let dirty = SupersetReport {
        missing: vec!["x".into()],
        mismatch: vec![],
        orphan: vec![],
    };
    assert!(!dirty.is_clean());
}

fn expected_output_inventory_round_trips_from_the_authored_module_ttl() {
    // The authored gmeow:expectsGeneratedOutput rows round-trip through the gate's OWN
    // reader: the complete non-terminal generated/ tree, deduplicated, every path under
    // generated/, and neither terminal bundle present.
    let expected = authored_expected();
    assert_eq!(
        expected.len(),
        417,
        "the authored inventory must hold every non-terminal generated/ path"
    );
    for p in &expected {
        assert!(
            p.starts_with("generated/"),
            "non-generated inventory path: {p}"
        );
        assert!(
            !EXCLUDED.contains(&p.as_str()),
            "terminal bundle leaked in: {p}"
        );
    }
    // The known runtime-consumed catalog files (crates/docs/src/model.rs) are present.
    assert!(expected.contains("generated/catalog/constraint-catalog.nq"));
    assert!(expected.contains("generated/catalog/term-content-manifest.nq"));
}

#[test]
fn read_expected_outputs_rejects_malformed_rows() {
    // Non-literal object.
    let bad_obj = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput gmeow:not-a-literal ."#;
    let ds = purrdf::parse_dataset(bad_obj.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_expected_outputs(&ds).is_err());
    // A path not under generated/.
    let outside = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "docs/x.md" ."#;
    let ds = purrdf::parse_dataset(outside.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_expected_outputs(&ds).is_err());
    // A terminal bundle listed.
    let terminal = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "generated/dist/gmeow.gts" ."#;
    let ds = purrdf::parse_dataset(terminal.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_expected_outputs(&ds).is_err());
    // The same path declared by two subjects (identical triples collapse under RDF set
    // semantics, so a genuine duplicate needs distinct subjects).
    let dup = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "generated/a.ttl" .
gmeow:other gmeow:expectsGeneratedOutput "generated/a.ttl" ."#;
    let ds = purrdf::parse_dataset(dup.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_expected_outputs(&ds).is_err());
    // No rows at all — the inventory did not reach the bundle.
    let empty = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build a gmeow:Pipeline ."#;
    let ds = purrdf::parse_dataset(empty.as_bytes(), "text/turtle", None).unwrap();
    assert!(read_expected_outputs(&ds).is_err());
}

fn completeness_hard_fails_naming_every_dropped_output() {
    // The completeness anchor: dropping ONE declared path from the produced set fires the
    // ExpectedOutputMissing HARD FAIL, and the message names the missing path — exactly the
    // deterministic-drop case the two-generation determinism gate is blind to.
    let expected = authored_expected();
    // A produced set that reconstructs every declared path passes.
    let full: BTreeMap<String, Vec<u8>> =
        expected.iter().map(|p| (p.clone(), Vec::new())).collect();
    check_expected_completeness(&full, &expected).expect("full production is complete");
    // Drop one declared output (simulate a carrier code change that stops emitting it).
    let dropped = "generated/catalog/constraint-catalog.nq";
    let mut partial = full.clone();
    partial.remove(dropped);
    let err = check_expected_completeness(&partial, &expected).unwrap_err();
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
    assert!(
        err.to_string().contains(dropped),
        "the HARD FAIL must name the dropped path, got: {err}"
    );
}

#[test]
fn project_bundle_hard_fails_when_a_declared_output_is_never_produced() {
    // NEVER-PRODUCED — the regression this task guards: a producing stage change stops
    // emitting a declared output. The bundle then carries NO representative for it, so the
    // bytes are IDENTICAL across two cold runs — the two-generation determinism gate is
    // blind. Only the completeness oracle catches it, and it must bite through the REAL
    // `project_bundle` path (not only the hand-built projection map
    // `completeness_hard_fails_naming_every_dropped_output` exercises), proving the
    // `check_expected_completeness` call at the top of `project_bundle` is wired.
    //
    // Build a minimal-but-valid gmeow.gts through the production terminal
    // (`emit_gmeow_gts`): the ontology header the importer requires, plus two authored
    // `gmeow:expectsGeneratedOutput` rows for paths the bundle does NOT produce — one
    // OPAQUE-family member (`generated/n3/gmeow.n3`, normally an inline archive member) and
    // one PREFIX-family member (`generated/profiles/full.ttl`, normally a named-graph fold).
    // With no fanout rows, no reconstruction graphs, and no opaque archive, the
    // reconstructed `files` set is empty, so BOTH declared paths are "never produced".
    use purrdf::gts_compose::SnapshotBuilder;

    const OPAQUE_MEMBER: &str = "generated/n3/gmeow.n3";
    const PREFIX_MEMBER: &str = "generated/profiles/full.ttl";
    let doc = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix dcterms: <http://purl.org/dc/terms/> .\n\
             <https://blackcatinformatics.ca/gmeow> a owl:Ontology ;\n\
                 dcterms:title \"GMEOW\" ;\n\
                 owl:versionInfo \"test\" .\n\
             gmeow:pipeline-build gmeow:expectsGeneratedOutput \"{OPAQUE_MEMBER}\" , \"{PREFIX_MEMBER}\" .\n"
    );
    let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).unwrap();
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(ds.as_ref()).expect("add_dataset");
    // gmeow-test-input: synthetic-only
    let gts = {
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            Vec::new(),
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("emit minimal expected-output bundle");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };

    // The reusable path performs the complete proof over one tied decode.
    let decoded = decode_projection_source(&gts).expect("decode projection source");
    let decoded_err = project_decoded_bundle(&decoded).expect_err(
        // gmeow-test-input: synthetic-only
        "decoded projection must HARD-fail: two declared outputs were never produced",
    );
    assert_eq!(
        decoded_err.code(),
        crate::error::ExpectedOutputMissing::register()
    );

    // The public one-shot path is exactly the decode + projection composition.
    let err =
            project_bundle(&gts) // gmeow-test-input: synthetic-only
                .expect_err(
                    "project_bundle must HARD-fail: two declared outputs were never produced",
                );
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
    let msg = err.to_string();
    assert_eq!(decoded_err.to_string(), msg);
    assert!(
        msg.contains(OPAQUE_MEMBER),
        "the HARD FAIL must name the never-produced opaque-family path, got: {msg}"
    );
    assert!(
        msg.contains(PREFIX_MEMBER),
        "the HARD FAIL must name the never-produced prefix-family path, got: {msg}"
    );
}
fn derivable_families_cross_check_catches_authored_derived_drift() {
    // The two DERIVED families (profiles, edoal): authored must EXACTLY equal the set the
    // carrier's reconstruction graphs yield. The real authored counts are pinned so a
    // silent family-count change trips the count-consistency guard.
    let expected = authored_expected();
    let profiles: BTreeSet<&str> = expected
        .iter()
        .filter(|p| p.starts_with("generated/profiles/"))
        .map(String::as_str)
        .collect();
    let edoal: BTreeSet<&str> = expected
        .iter()
        .filter(|p| p.starts_with("generated/projections/") && p.ends_with(".edoal.ttl"))
        .map(String::as_str)
        .collect();
    let dicts: BTreeSet<String> = expected
        .iter()
        .filter(|p| is_header_dict_path(p))
        .cloned()
        .collect();
    assert_eq!(profiles.len(), 8, "profiles family membership drifted");
    assert_eq!(edoal.len(), 47, "edoal family membership drifted");
    assert_eq!(dicts.len(), 6, "header-dict family membership drifted");

    // Equal authored/derived over the derivable families passes.
    let reconstructed: BTreeSet<String> = expected
        .iter()
        .filter(|p| {
            p.starts_with("generated/profiles/")
                || (p.starts_with("generated/projections/") && p.ends_with(".edoal.ttl"))
        })
        .cloned()
        .collect();
    check_derivable_families(&expected, &reconstructed, &dicts).expect("authored == derived");

    // A source individual added without its expected path (derived ⊋ authored) HARD-fails.
    let mut extra = reconstructed.clone();
    extra.insert("generated/profiles/newprofile.ttl".to_string());
    let err = check_derivable_families(&expected, &extra, &dicts).unwrap_err();
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());

    // A stale authored path (authored ⊋ derived) HARD-fails too.
    let mut short = reconstructed.clone();
    assert!(short.remove("generated/profiles/full.ttl"));
    let err = check_derivable_families(&expected, &short, &dicts).unwrap_err();
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());

    // The header-dict family is derived from the WIRE (the header's own "dct" map),
    // so both drift directions bite there too: a dictionary the medium axis pins but
    // the inventory never declared, and an inventory entry the header dropped.
    let mut extra_dict = dicts.clone();
    extra_dict.insert(header_dict_path("gmeow-invented-v1"));
    let err = check_derivable_families(&expected, &reconstructed, &extra_dict).unwrap_err();
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
    let mut short_dict = dicts.clone();
    assert!(short_dict.remove(&header_dict_path("gmeow-core-v1")));
    let err = check_derivable_families(&expected, &reconstructed, &short_dict).unwrap_err();
    assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
}

/// The header-dict family: rows parse, resolve NO named-graph rep, and the
/// family-scoped bijection HARD-fails on an undeclared pinned dictionary AND on a
/// stale row naming a dictionary the pack no longer carries.
#[test]
fn header_dict_rows_parse_and_bijection_checks_the_pinned_dictionaries() {
    let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:d1 gmeow:extractsPath "generated/medium/gmeow-core-v1.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" .
gmeow:d2 gmeow:extractsPath "generated/medium/gmeow-logic-v1.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:o1 gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
"#;
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
    let rules = read_fanout_rules(&ds).unwrap();
    let (header_dict, rest): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
        .into_iter()
        .partition(|r| r.family == FanoutFamily::HeaderDict);
    assert_eq!(header_dict.len(), 2);
    assert_eq!(rest.len(), 2);

    // A header-dict row never resolves a named-graph rep — it rides the header lane.
    for rule_set in [&header_dict, &rest] {
        assert!(
            graph_rep_for_path(rule_set, "generated/medium/gmeow-core-v1.zdict").is_none(),
            "a .zdict path must never resolve a phantom named-graph fold"
        );
    }

    let pinned: BTreeSet<String> = ["gmeow-core-v1", "gmeow-logic-v1"]
        .iter()
        .map(|id| header_dict_path(id))
        .collect();
    check_header_dict_bijection(&header_dict, &pinned).expect("header-dict bijection holds");

    // An undeclared pinned dictionary (the medium axis grew a third) HARD-fails.
    let mut undeclared = pinned.clone();
    undeclared.insert(header_dict_path("gmeow-unrowed-v1"));
    let err = check_header_dict_bijection(&header_dict, &undeclared).unwrap_err();
    assert_eq!(err.code(), crate::error::FanoutBijection::register());

    // A stale row (the pack stopped pinning that dictionary) HARD-fails too.
    let mut stale = pinned.clone();
    assert!(stale.remove(&header_dict_path("gmeow-core-v1")));
    let err = check_header_dict_bijection(&header_dict, &stale).unwrap_err();
    assert_eq!(err.code(), crate::error::FanoutBijection::register());
}

/// The `header-dict` family and form are mutually required, the match is always
/// `exact`, and the path must be a `generated/medium/<id>.zdict` one — otherwise the
/// row would name a dictionary the header cannot resolve.
#[test]
fn header_dict_family_form_match_and_path_shape_are_mutually_required() {
    let cases = [
        // family header-dict with a graph form.
        r#"gmeow:x gmeow:extractsPath "generated/medium/a.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "turtle" ."#,
        // form header-dict with a graph family.
        r#"gmeow:x gmeow:extractsPath "generated/medium/a.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "header-dict" ."#,
        // header-dict with a prefix match.
        r#"gmeow:x gmeow:extractsPath "generated/medium/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" ."#,
        // header-dict on a path outside the family.
        r#"gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" ."#,
    ];
    for case in cases {
        let ttl = format!("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{case}\n");
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        assert!(
            read_fanout_rules(&ds).is_err(),
            "malformed header-dict row accepted: {case}"
        );
    }
}

/// An unknown family / form string is still a HARD FAIL — the enums stay closed even
/// though a fourth family was added.
#[test]
fn an_unknown_fanout_family_or_form_still_hard_fails() {
    for case in [
        r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dictionary" ; gmeow:extractsForm "turtle" ."#,
        r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "zdict" ."#,
        r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "invented" ; gmeow:extractsForm "turtle" ."#,
    ] {
        let ttl = format!("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{case}\n");
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        assert!(
            read_fanout_rules(&ds).is_err(),
            "unknown family/form accepted: {case}"
        );
    }
}

/// The one enumerated value set, read from BOTH sides: the `skos:definition`s of
/// `gmeow:extractsGraphFamily` / `gmeow:extractsForm` name exactly the strings the
/// Rust match arms accept. Without this the ontology could keep enumerating three
/// families while the code accepted four — a Principle 4 second source of truth in
/// which the shipped definition contradicts the shipped behaviour.
fn the_rust_family_and_form_arms_equal_the_ontology_declared_value_sets() {
    // Every family/form string the Rust reader accepts, proved by round-tripping a
    // one-row document per value rather than by re-listing the arms (a second copy of
    // the match would drift exactly as the prose did).
    let accepted = |predicate: &str, value: &str| -> bool {
        let (family, form) = match predicate {
            "extractsGraphFamily" => (value, form_for_family(value)),
            _ => (family_for_form(value), value),
        };
        let path = match family {
            "header-dict" => "generated/medium/probe-v1.zdict",
            _ => "generated/probe.ttl",
        };
        let doc = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                 gmeow:probe gmeow:extractsPath {path:?} ; gmeow:extractsMatch \"exact\" ; \
                 gmeow:extractsGraphFamily {family:?} ; gmeow:extractsForm {form:?} .\n"
        );
        let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).unwrap();
        read_fanout_rules(&ds).is_ok()
    };

    for (predicate, rust_values) in [
        (
            "extractsGraphFamily",
            vec!["rdf-fanout", "edoal", "opaque", "header-dict"],
        ),
        (
            "extractsForm",
            vec![
                "turtle",
                "ntriples",
                "nquads-self",
                "nquads-diagnostics",
                "blob",
                "header-dict",
            ],
        ),
    ] {
        // The Rust side really does accept every value claimed, and nothing else it
        // was not told about.
        for value in &rust_values {
            assert!(
                accepted(predicate, value),
                "gmeow:{predicate} value {value:?} is claimed but the Rust reader rejects it"
            );
        }
        assert!(
            !accepted(predicate, "not-a-declared-value"),
            "gmeow:{predicate} accepts an undeclared value"
        );
        let declared = declared_value_set(predicate);
        assert_eq!(
            declared,
            rust_values.iter().map(|v| (*v).to_string()).collect(),
            "the gmeow:{predicate} skos:definition and the Rust match arms disagree"
        );
    }
}

/// The form a family is REQUIRED to pair with, for the round-trip probe above.
fn form_for_family(family: &str) -> &'static str {
    match family {
        "opaque" => "blob",
        "header-dict" => "header-dict",
        _ => "turtle",
    }
}

/// The family a form is REQUIRED to pair with, for the round-trip probe above.
fn family_for_form(form: &str) -> &'static str {
    match form {
        "blob" => "opaque",
        "header-dict" => "header-dict",
        _ => "rdf-fanout",
    }
}

fn authored_only_families_hold_their_count_consistency() {
    // The two prefix families whose producing individuals are NOT cleanly enumerable at
    // the gate — research-objects (a single research object with a mixed RDF / JSON / XML /
    // HTML sub-tree) and the heterogeneous lang projections (per-reading CoNLL-U, per-example
    // GMN1, per-sentence NIF/TEI/SEMAF, EBNF grammars) — are AUTHORED-ONLY. They cannot be
    // re-derived from reconstruction graphs (their non-RDF members ride opaque blobs), so a
    // count-consistency guard stands in for a derivation cross-check: a silent add/drop trips
    // the pinned membership. Both prefixes are also fully covered by the completeness ⊇ anchor.
    let expected = authored_expected();
    let research: Vec<&String> = expected
        .iter()
        .filter(|p| p.starts_with("generated/research-objects/"))
        .collect();
    let lang: Vec<&String> = expected
        .iter()
        .filter(|p| p.starts_with("generated/projections/lang/"))
        .collect();
    assert_eq!(
        research.len(),
        13,
        "research-objects family membership drifted"
    );
    assert_eq!(lang.len(), 36, "lang-projections family membership drifted");
    // These families are genuinely mixed (not all RDF), the reason they are authored-only.
    assert!(
        research.iter().any(|p| p.ends_with(".json"))
            && research.iter().any(|p| p.ends_with(".ttl")),
        "research-objects should carry both RDF and non-RDF members"
    );
    assert!(
        lang.iter().any(|p| p.ends_with(".conllu")) && lang.iter().any(|p| p.ends_with(".ttl")),
        "lang projections should carry both RDF and non-RDF members"
    );
}

/// Recursively collect the committed `generated/<file>` paths that shipped code names via
/// the repo-root read idiom `root.join("generated/…")` under one directory, skipping
/// integration-test trees (`…/tests/…`).
fn collect_root_join_generated(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            collect_root_join_generated(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            let needle = "root.join(\"generated/";
            let mut rest = text.as_str();
            while let Some(i) = rest.find(needle) {
                let after = &rest[i + "root.join(\"".len()..];
                if let Some(end) = after.find('"') {
                    let p = &after[..end];
                    // Only file references (a dotted final segment), never bare dirs.
                    if p.rsplit('/').next().is_some_and(|seg| seg.contains('.')) {
                        out.insert(p.to_string());
                    }
                    rest = &after[end..];
                } else {
                    break;
                }
            }
        }
    }
}

fn every_runtime_generated_read_is_in_the_authored_inventory() {
    // Downstream-read guarantee: every committed generated/ file shipped code reads at
    // runtime (via root.join) must be in the authored inventory, so a clean clone cannot
    // silently lose a consumed output. The terminal bundle is legitimately excluded.
    let inventory = authored_expected();
    let mut refs = BTreeSet::new();
    collect_root_join_generated(&repo_root().join("crates"), &mut refs);
    // Sanity: the flagged docs consumer read is actually discovered by the scan.
    assert!(
        refs.contains("generated/catalog/constraint-catalog.nq"),
        "scan failed to discover the crates/docs/src/model.rs catalog read"
    );
    for path in &refs {
        if EXCLUDED.contains(&path.as_str()) {
            continue;
        }
        assert!(
            inventory.contains(path),
            "runtime read {path} is absent from the authored expected-output inventory — a \
                 clean clone would silently lose a consumed file"
        );
    }
}
#[test]
fn authored_pipeline_source_contracts() {
    byte_decorated_rdf_paths_fall_through_to_blob_members();
    quality_assessment_nt_folds_as_ntriples_via_its_own_fanout_graph();
    expected_output_inventory_round_trips_from_the_authored_module_ttl();
    completeness_hard_fails_naming_every_dropped_output();
    derivable_families_cross_check_catches_authored_derived_drift();
    the_rust_family_and_form_arms_equal_the_ontology_declared_value_sets();
    authored_only_families_hold_their_count_consistency();
    every_runtime_generated_read_is_in_the_authored_inventory();
}
