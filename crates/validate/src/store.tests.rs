// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on
/// drop, including on panic and early return. Bind it to a named `_tmp`
/// (never a bare `_`, which would drop it immediately) so it outlives the
/// path. The file *name* is preserved because the parser dispatches on the
/// `.ttl` extension and the parse-error assertions match on the file name.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

use purrdf::{DatasetView, GraphMatch};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

// ── result-set collapse ────────────────────────────────────────────────────

fn sparql_result(focus: &str) -> purrdf::shapes::report::ValidationResult {
    use purrdf::shapes::report::Severity as ShaclSeverity;
    use purrdf::shapes::term::{NamedNode, Term};
    purrdf::shapes::report::ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(focus)),
        result_path: None,
        path_structure: None,
        value: Some(Term::NamedNode(NamedNode::new_unchecked(focus))),
        source_constraint_component: NamedNode::new_unchecked(
            "http://www.w3.org/ns/shacl#SPARQLConstraintComponent",
        ),
        source_shape: Term::NamedNode(NamedNode::new_unchecked("https://ex/ContiguityShape")),
        severity: ShaclSeverity::Violation,
        message: Some("slot indexes must be contiguous".to_owned()),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: Vec::new(),
        attributions: Vec::new(),
    }
}

fn report_of(
    results: Vec<purrdf::shapes::report::ValidationResult>,
) -> purrdf::shapes::report::ValidationReport {
    purrdf::shapes::report::ValidationReport {
        conforms: results.is_empty(),
        results,
    }
}

#[test]
fn indistinguishable_results_collapse_to_one() {
    // The defect this exists for: a projected `SELECT $this` whose WHERE clause
    // binds further variables yields ONE result per binding combination, all
    // projecting the same `$this` — four byte-identical findings for one violation,
    // which a consumer counting findings reads as four defects.
    let mut report = report_of(vec![
        sparql_result("https://ex/badBinder"),
        sparql_result("https://ex/badBinder"),
        sparql_result("https://ex/badBinder"),
        sparql_result("https://ex/badBinder"),
    ]);
    assert_eq!(duplicate_validation_results(&report).len(), 1);
    assert_eq!(duplicate_validation_results(&report)[0].1, 4);
    dedupe_validation_results(&mut report);
    assert_eq!(report.results.len(), 1, "one violation is one result");
}

#[test]
fn distinguishable_results_all_survive_the_collapse() {
    // The collapse may only ever drop results NO consumer can tell apart. Two
    // violations of the same law at different focus nodes are two violations, and a
    // second component over the same focus node is a second observation — both must
    // survive, or the collapse would be hiding real defects.
    let other_focus = sparql_result("https://ex/otherBinder");
    let mut other_component = sparql_result("https://ex/badBinder");
    other_component.source_constraint_component = purrdf::shapes::term::NamedNode::new_unchecked(
        "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
    );
    let mut other_shape = sparql_result("https://ex/badBinder");
    other_shape.source_shape = purrdf::shapes::term::Term::NamedNode(
        purrdf::shapes::term::NamedNode::new_unchecked("https://ex/UniquenessShape"),
    );
    let mut other_message = sparql_result("https://ex/badBinder");
    other_message.message = Some("a different law speaking".to_owned());
    let mut report = report_of(vec![
        sparql_result("https://ex/badBinder"),
        other_focus,
        other_component,
        other_shape,
        other_message,
    ]);
    assert!(duplicate_validation_results(&report).is_empty());
    dedupe_validation_results(&mut report);
    assert_eq!(report.results.len(), 5);
}

#[test]
fn parse_file_dataset_rejects_bad_turtle() {
    let (_tmp, path) = write_tmp("gmeow_validate_store_bad.ttl", "this is not turtle <<< @@@");
    let result = parse_file_dataset(&path);
    assert!(result.is_err(), "malformed Turtle must parse-error");
}

#[test]
fn parse_file_dataset_accepts_good_turtle() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_store_good.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let result = parse_file_dataset(&path);
    let ds = result.expect("well-formed Turtle must parse");
    assert_eq!(ds.quad_count(), 1);
}

#[test]
fn dataset_from_paths_loads_multiple_files() {
    let (_tmp_a, a) = write_tmp(
        "gmeow_validate_store_multi_a.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let (_tmp_b, b) = write_tmp(
        "gmeow_validate_store_multi_b.ttl",
        "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
    );
    let ds = dataset_from_paths(&[a.clone(), b.clone()]).expect("both files must load");
    assert_eq!(ds.quad_count(), 2);
}

#[test]
fn dataset_from_paths_propagates_parse_error() {
    let (_tmp_good, good) = write_tmp(
        "gmeow_validate_parsed_err_good.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let (_tmp_bad, bad) = write_tmp(
        "gmeow_validate_parsed_err_bad.ttl",
        "this is not turtle @@@ <<<",
    );
    let result = dataset_from_paths(&[good.clone(), bad.clone()]);
    assert!(result.is_err(), "a malformed file must propagate");
    let err = result.err().unwrap();
    let msg = err.message();
    assert!(
        msg.contains("syntax error in") && msg.contains("gmeow_validate_parsed_err_bad.ttl"),
        "error must use 'syntax error in' format naming the bad file; got: {msg}"
    );
}

#[test]
fn sameas_flags_external_object() {
    let ds = parse_dataset(
        "@prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a owl:sameAs ex:b .\n"
            .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    let violations = sameas_violations(&ds, NS, &[]);
    assert_eq!(
        violations,
        vec![(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned()
        )]
    );
}

#[test]
fn sameas_dedups_dual_spelling_identity() {
    // Thread r3832952678: one external identity carried in BOTH the authored
    // logic:sameAs and its projected owl:sameAs spelling is ONE violation, not two.
    let ds = parse_dataset(
        "@prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:a logic:sameAs ex:b .\n\
             ex:a owl:sameAs ex:b .\n"
            .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    assert_eq!(
        sameas_violations(&ds, NS, &[]),
        vec![(
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned()
        )],
        "a dual-spelling identity must dedup to one violation"
    );
}

#[test]
fn sameas_skips_internal_and_allowlisted() {
    let ds = parse_dataset(
        format!(
            "@prefix gmeow: <{NS}> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                 gmeow:A owl:sameAs gmeow:B .\n\
                 ex:a owl:sameAs ex:b .\n"
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    let allowlist = vec![(
        "https://example.org/a".to_owned(),
        "https://example.org/b".to_owned(),
    )];
    assert!(sameas_violations(&ds, NS, &allowlist).is_empty());
}

#[test]
fn dataset_from_nt_loads_and_rejects_malformed() {
    let nt = "<https://example.org/a> <https://example.org/p> <https://example.org/b> .\n";
    let ds = dataset_from_nt(nt).expect("valid N-Triples must load");
    assert_eq!(ds.quad_count(), 1);
    assert!(dataset_from_nt("this is not n-triples @@@").is_err());
}

#[test]
fn dataset_from_gts_loads_single_triple_in_default_graph() {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    let mut graph = purrdf::gts::model::Graph::default();
    for iri in [
        "https://example.org/s",
        "https://example.org/p",
        "https://example.org/o",
    ] {
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(iri.to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }
    // Named graph slot to verify the flatten.
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://blackcatinformatics.ca/gmeow/graph/metadata".to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.quads.push((0, 1, 2, Some(3)));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    let ds = dataset_from_gts(&writer.to_bytes()).expect("GTS bytes must fold into dataset");
    assert_eq!(ds.quad_count(), 1);
    // The named-graph quad is flattened to the default graph.
    assert_eq!(
        ds.quads_for_pattern(None, None, None, GraphMatch::Default)
            .count(),
        1
    );
}

#[test]
fn dataset_from_gts_accepts_private_lang_tag_and_flattens_named_graph() {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    // A literal with a private-use `@x-gmeow-*` tag (BCP-47 subtag > 8 chars)
    // in a NAMED graph. The lenient native fold must accept the tag and collapse
    // the named graph into the default graph.
    let mut graph = purrdf::gts::model::Graph::default();
    for value in ["https://example.org/s", "https://example.org/p"] {
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }
    graph.terms.push(Term {
        kind: TermKind::Literal,
        value: Some("hallo".to_string()),
        datatype: None,
        lang: Some("x-gmeow-afrikaans".to_string()),
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://blackcatinformatics.ca/gmeow/graph/metadata".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    // Object (term 2) carries the private lang tag; quad lives in named graph (term 3).
    graph.quads.push((0, 1, 2, Some(3)));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    let ds = dataset_from_gts(&writer.to_bytes())
        .expect("private lang tag in a named graph must load leniently");

    assert_eq!(ds.quad_count(), 1);
    // Flattened: the triple is in the default graph and the lang tag survives.
    let q = ds
        .quads_for_pattern(None, None, None, GraphMatch::Default)
        .next()
        .expect("one default-graph quad");
    match ds.resolve(q.o) {
        purrdf::TermRef::Literal { language, .. } => {
            assert_eq!(language, Some("x-gmeow-afrikaans"), "lang tag preserved");
        }
        other => panic!("object must be a literal, got {other:?}"),
    }
}

#[test]
fn core_browser_bundle_keeps_default_drops_named_graphs() {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    // A bundle with one quad in the DEFAULT graph (object-level) and one quad in
    // a heavy named graph (`graph/documentation`). The core browser projection
    // must keep the default-graph quad and DROP the named-graph quad.
    let mut graph = purrdf::gts::model::Graph::default();
    for value in [
        "https://blackcatinformatics.ca/gmeow/Cat", // 0: default s
        "http://www.w3.org/2000/01/rdf-schema#label", // 1: default p
        "https://blackcatinformatics.ca/gmeow/DocNode", // 2: named s
        "https://blackcatinformatics.ca/gmeow/docTitle", // 3: named p
        "https://blackcatinformatics.ca/gmeow/graph/documentation", // 4: named graph
    ] {
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }
    for lit in ["Cat", "A documentation node"] {
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some(lit.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }
    // default-graph quad: Cat rdfs:label "Cat" .
    graph.quads.push((0, 1, 5, None));
    // named-graph quad: DocNode docTitle "A documentation node" <graph/documentation>
    graph.quads.push((2, 3, 6, Some(4)));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    let nq = core_browser_bundle_nquads(&writer.to_bytes(), &[]) // gmeow-test-input: synthetic-only
        .expect("core browser bundle must serialize");
    assert!(
        nq.contains("https://blackcatinformatics.ca/gmeow/Cat"),
        "core keeps the default-graph object-level quad:\n{nq}"
    );
    assert!(
        !nq.contains("graph/documentation") && !nq.contains("DocNode"),
        "core drops the heavy named graph and its quads:\n{nq}"
    );
}

#[test]
fn dataset_nquads_from_gts_preserves_named_graph() {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    // The same one-quad-in-a-named-graph bundle, but read through the
    // graph-PRESERVING browser primitive: the emitted N-Quads MUST carry the
    // named-graph IRI as the fourth term (a flatten would drop it to the default
    // graph and the assertion would fail).
    let mut graph = purrdf::gts::model::Graph::default();
    for value in [
        "https://example.org/s",
        "https://example.org/p",
        "https://example.org/o",
        "https://blackcatinformatics.ca/gmeow/graph/metadata",
    ] {
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        });
    }
    graph.quads.push((0, 1, 2, Some(3)));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    let nquads = dataset_nquads_from_gts(&writer.to_bytes())
        .expect("graph-preserving bundle N-Quads must serialize");
    assert!(
        nquads.contains("https://blackcatinformatics.ca/gmeow/graph/metadata"),
        "bundle N-Quads must retain the named-graph component (graph-preserving), got:\n{nquads}"
    );
    assert!(
        nquads.contains("https://example.org/s") && nquads.contains("https://example.org/o"),
        "bundle N-Quads must carry the quad's subject and object:\n{nquads}"
    );
}

#[test]
fn dataset_from_gts_rejects_malformed_bytes() {
    // Clearly non-GTS bytes must trigger a fold diagnostic and return Err, not a
    // silent empty dataset. This exercises the fail-fast contract.
    let result = dataset_from_gts(b"this is not a valid gts file");
    assert!(
        result.is_err(),
        "malformed GTS bytes must return Err, not a silent empty dataset"
    );
    // Also verify raw garbage bytes (not even ASCII text).
    let result2 = dataset_from_gts(&[0u8, 1, 2, 3, 0xFF, 0xFE]);
    assert!(
        result2.is_err(),
        "binary garbage bytes must return Err, not a silent empty dataset"
    );
}

#[test]
fn read_gts_graph_rejects_malformed_bytes() {
    let result = read_gts_graph(b"not a gts bundle");
    assert!(
        result.is_err(),
        "malformed bytes must be rejected by read_gts_graph"
    );
    let err = result.err().unwrap();
    let msg = err.message();
    assert!(
        msg.contains("magic")
            || msg.contains("header")
            || msg.contains("parse")
            || msg.contains("diagnostic"),
        "error message should mention magic, header, parse, or diagnostics; got: {msg}"
    );
}

#[test]
fn read_gts_graph_populates_segment_heads() {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    let mut graph = purrdf::gts::model::Graph::default();
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/s".to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/p".to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/o".to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.quads.push((0, 1, 2, None));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    let graph = read_gts_graph(&writer.to_bytes()).expect("valid GTS bytes must parse");

    assert!(
        !graph.segment_heads.is_empty(),
        "read_gts_graph must populate segment_heads"
    );
}
