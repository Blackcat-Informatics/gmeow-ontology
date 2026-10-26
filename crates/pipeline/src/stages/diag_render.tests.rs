// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::{Finding, FindingCategory, Location, Severity, StageId};

#[test]
fn production_forward_fold_retains_distinct_named_source_graphs() {
    let mut report = Report::new("synthetic");
    for graph in ["urn:world:a", "urn:world:b"] {
        let mut finding = Finding::new(Severity::Error, "test.graph.forward", "same observation")
            .with_category(FindingCategory::PermittedEpistemicConflict)
            .with_standpoint(gmeow_errors::Standpoint::Advisory);
        finding.add_location(
            Location::new(
                Some("same.ttl".into()),
                Some(17),
                Some(9),
                Some("urn:focus".into()),
            )
            .with_gts_quad(0),
        );
        finding.source_graph = Some(purrdf::iri::BaseIri::parse(graph).unwrap());
        report.add_finding(finding);
    }
    let nodes = finding_nodes(&report, "synthetic");
    assert_eq!(nodes.len(), 2);
    for node in &nodes {
        assert!(
            node.frames.is_empty(),
            "a report finding must not acquire an invented native cause"
        );
        assert_eq!(node.source_ctx.location.line, Some(17));
        assert_eq!(node.source_ctx.location.column, Some(9));
        assert_eq!(
            node.source_ctx.location.logical.as_deref(),
            Some("urn:focus")
        );
        assert_eq!(node.source_ctx.location.gts_quad_index, Some(0));
        assert_eq!(
            node.grade.category,
            FindingCategory::PermittedEpistemicConflict
        );
        assert_eq!(node.grade.standpoint, gmeow_errors::Standpoint::Advisory);
    }
    let mut ledger = DiagLedger::new();
    ledger.replay(nodes.clone());
    assert_ne!(ledger.verdict(), gmeow_errors::GateVerdict::Fatal);
    let graphs: std::collections::BTreeSet<_> = nodes
        .iter()
        .map(|node| node.source_ctx.graph.as_ref().unwrap().as_str())
        .collect();
    assert_eq!(
        graphs,
        std::collections::BTreeSet::from(["urn:world:a", "urn:world:b"])
    );
}

#[test]
fn native_diagnostics_preserve_evidence_grades_and_exact_location_values() {
    use gmeow_errors::Standpoint;
    use gmeow_errors::model::RelatedLabel;
    use purrdf::{RdfLiteral, RdfQuad, RdfTerm};

    let namespace = "https://blackcatinformatics.ca/gmeow/";
    let subject = "https://example.org/finding";
    let root = "https://example.org/root";
    let graph = crate::stages::carrier::GRAPH_DIAGNOSTICS;
    let message = "retain \"quotation\", newline\nand \\ path";
    let location = Location::new(Some("module.ttl".into()), Some(17), Some(9), None)
        .with_gts_term(u64::MAX)
        .with_gts_quad(7)
        .with_gts_reifier(8)
        .with_gts_frame(9)
        .with_gts_segment(10);
    let mut finding = Finding::new(Severity::Error, "logic.example", message)
        .with_category(FindingCategory::ModelingDisciplineViolation)
        .with_standpoint(Standpoint::Binding);
    finding.finding_iri = Some(subject.into());
    finding.source_graph = Some(purrdf::iri::BaseIri::parse("urn:world:source").unwrap());
    finding.antecedents.push(root.into());
    finding.locations.push(location.clone());
    finding.related_labels.push(RelatedLabel {
        location,
        message: "retained premise".into(),
    });
    finding
        .derived_from_quads
        .push("https://example.org/quad-proof".into());
    let mut report = Report::new("logic");
    report.add_finding(finding);
    let mut cause = Finding::new(Severity::Note, "logic.premise", "source evidence");
    cause.finding_iri = Some(root.into());
    report.add_finding(cause);
    let rendered = render_diagnostics_artifacts(
        "synthetic",
        report.clone(),
        &DiagnosticsPaths {
            json: "report.json",
            sarif: "report.sarif",
            html: "report.html",
            rdf: "report.nq",
        },
        None,
        None,
        None,
    )
    .unwrap();
    let quads: Vec<_> = rendered.dataset.owned_quads().collect();
    let has = |s: &str, p: &str, o: RdfTerm| {
        assert!(
            quads.contains(&RdfQuad::new(RdfTerm::iri(s), p, o).in_graph(RdfTerm::iri(graph))),
            "missing {s} {p}; actual rows: {:?}",
            quads
                .iter()
                .filter(|quad| quad.predicate == p)
                .collect::<Vec<_>>()
        );
    };
    has(
        subject,
        &format!("{namespace}findingMessage"),
        RdfTerm::literal(RdfLiteral::simple(message)),
    );
    // The owned view omits the redundant xsd:string annotation. Check the
    // native datatype as well as the exact lexical value, language and direction.
    let message_quad = rendered
        .dataset
        .quads()
        .find(|quad| {
            rendered.dataset.resolve(quad.s) == purrdf::TermRef::Iri(subject)
                && rendered.dataset.resolve(quad.p)
                    == purrdf::TermRef::Iri("https://blackcatinformatics.ca/gmeow/findingMessage")
        })
        .unwrap();
    match rendered.dataset.resolve(message_quad.o) {
        purrdf::TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => {
            assert_eq!(lexical, message);
            assert_eq!(
                rendered.dataset.resolve(datatype),
                purrdf::TermRef::Iri("http://www.w3.org/2001/XMLSchema#string")
            );
            assert_eq!(language, None);
            assert_eq!(direction, None);
        }
        other => panic!("finding message is not a literal: {other:?}"),
    }
    has(
        subject,
        &format!("{namespace}findingAntecedent"),
        RdfTerm::iri(root),
    );
    has(
        subject,
        &format!("{namespace}findingSourceGraph"),
        RdfTerm::iri("urn:world:source"),
    );
    has(
        root,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        RdfTerm::iri(format!("{namespace}Finding")),
    );
    has(
        subject,
        &format!("{namespace}findingDerivedFromQuad"),
        RdfTerm::iri("https://example.org/quad-proof"),
    );
    has(
        subject,
        &format!("{namespace}findingStandpoint"),
        RdfTerm::iri(format!("{namespace}standpointBinding")),
    );
    for node in [
        format!("{subject}/location/0"),
        format!("{subject}/relatedLabel/0"),
    ] {
        has(
            &node,
            &format!("{namespace}gtsTermId"),
            RdfTerm::literal(RdfLiteral::typed(
                u64::MAX.to_string(),
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            )),
        );
        has(
            &node,
            &format!("{namespace}findingLocationLine"),
            RdfTerm::literal(RdfLiteral::typed(
                "17",
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            )),
        );
    }
    assert!(
        !quads
            .iter()
            .any(|quad| quad.predicate == format!("{namespace}findingGateVerdict")),
        "only authored rules may derive the verdict"
    );
    // The public text renderer's established goldens cover its vocabulary;
    // this cross-surface check binds the new native carrier to that product.
    let text = gmeow_errors::render::to_gmeow_rdf(&report);
    let from_text = purrdf::parse_dataset(text.as_bytes(), "application/n-quads", None).unwrap();
    assert_eq!(
        rendered.artifacts["report.nq"],
        crate::stages::superset::canonical_ntriples(&from_text).unwrap()
    );
    assert_eq!(
        rendered.artifacts["report.nq"],
        crate::stages::superset::canonical_ntriples(&rendered.dataset).unwrap()
    );
}

fn report_with_one_finding() -> Report {
    let mut report = Report::new("shacl");
    report.add_finding(
        Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "required value is missing",
        )
        .with_tool("shacl"),
    );
    report
}

/// Findings without an explicit category retain the native ingestion defaults.
/// Explicit producer grades are covered by the source-graph bridge control.
#[test]
fn forward_nodes_carry_expected_code_severity_stage_category() {
    let nodes = finding_nodes(&report_with_one_finding(), "stage-validate");
    assert_eq!(nodes.len(), 1, "one finding → one node");
    let node = &nodes[0];
    assert_eq!(node.code, "shacl.MinCountConstraintComponent");
    assert_eq!(node.grade.severity, Severity::Error);
    assert_eq!(node.stage.as_str(), "stage-validate");
    assert_eq!(
        node.grade.category,
        FindingCategory::ModelingDisciplineViolation
    );
}

/// Findings carry no losses, so every forward node has EMPTY antecedents — there is
/// no cross-stage dangling edge (each producer's node set is self-contained).
#[test]
fn forward_nodes_have_no_dangling_antecedents() {
    let nodes = finding_nodes(&report_with_one_finding(), "stage-validate");
    for node in &nodes {
        assert!(
            node.antecedents.is_empty(),
            "a forward finding node must carry no antecedents"
        );
    }
}

/// The fold is a pure function of report content: repeated calls (and a report
/// whose findings are inserted out of order) yield byte-identical serialized nodes.
#[test]
fn forward_fold_is_deterministic_and_order_independent() {
    let mut a = Report::new("shacl");
    let mut b = Report::new("shacl");
    for (report, order) in [(&mut a, [0usize, 1, 2]), (&mut b, [2, 1, 0])] {
        for i in order {
            report.add_finding(Finding::new(
                Severity::Warning,
                format!("shacl.rule{i}"),
                format!("warning {i}"),
            ));
        }
    }
    let na = serde_json::to_vec(&finding_nodes(&a, "stage-validate")).unwrap();
    let nb = serde_json::to_vec(&finding_nodes(&b, "stage-validate")).unwrap();
    assert_eq!(na, nb, "forward fold is insertion-order independent");
    assert_eq!(finding_nodes(&a, "stage-validate").len(), 3);
}

/// Cross-stage shared fingerprint: the SAME finding folded at two DIFFERENT stages
/// hash-conses to ONE node whose stage min-merges to the lexicographic minimum,
/// byte-identical regardless of replay order (the property the run ledger relies on
/// when a fingerprint is emitted by both producers, one fresh and one cache-hit).
#[test]
fn cross_stage_shared_fingerprint_min_merges_order_independently() {
    let report = report_with_one_finding();
    let from_a = finding_nodes(&report, "stage-a");
    let from_b = finding_nodes(&report, "stage-b");

    let emit = |first: &[DiagNode], second: &[DiagNode]| -> Vec<u8> {
        let mut ledger = DiagLedger::new();
        ledger.replay(first.to_vec());
        ledger.replay(second.to_vec());
        serde_json::to_vec(
            &ledger
                .emit_sorted()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };
    let ab = emit(&from_a, &from_b);
    let ba = emit(&from_b, &from_a);
    assert_eq!(ab, ba, "cross-stage replay is order independent");

    let mut ledger = DiagLedger::new();
    ledger.replay(from_b.clone());
    ledger.replay(from_a.clone());
    let nodes = ledger.emit_sorted();
    assert_eq!(nodes.len(), 1, "one shared fingerprint → one node");
    assert_eq!(
        nodes[0].stage.as_str(),
        "stage-a",
        "the merged stage is the lexicographic minimum, not the first writer"
    );
    // Sanity: attributing to a single stage really does key the node by that stage.
    let single = finding_nodes(&report, "stage-z");
    assert_eq!(single[0].stage, StageId::new("stage-z"));
}

#[test]
fn forward_fold_preserves_distinct_focus_and_secondary_coordinates() {
    let primary = Location::new(
        Some("same.ttl".into()),
        Some(17),
        Some(9),
        Some("urn:anchor".into()),
    );
    let related =
        Location::new(Some("related.ttl".into()), Some(23), Some(4), None).with_gts_quad(7);
    let mut original = DiagLedger::new();
    for focus in ["urn:subject:a", "urn:subject:b"] {
        original.attach(
            Diag::new(
                register_code("test.focus.forward"),
                Grade::for_external_severity(Severity::Error),
                "same observation",
            )
            .with_location(primary.clone())
            .with_focus(focus)
            .with_graph(purrdf::iri::BaseIri::parse("urn:world").unwrap())
            .with_label(gmeow_errors::Label {
                location: related.clone(),
                text: "defined here".into(),
            }),
            StageId::new("synthetic"),
        );
    }
    let mut report = Report::new("synthetic");
    for node in original.emit_sorted() {
        report.add_finding(node.to_finding("synthetic"));
    }
    // Exercise the actual serialized report boundary as well as its normalization.
    let report: Report =
        gmeow_errors::Report::from_json_slice(&report.to_json().unwrap().into_bytes()).unwrap();
    let nodes = finding_nodes(&report, "synthetic");
    assert_eq!(nodes.len(), 2, "distinct source subjects must not collapse");
    let original_fingerprints: std::collections::BTreeSet<_> = original
        .emit_sorted()
        .iter()
        .map(|node| node.fingerprint)
        .collect();
    let folded_fingerprints: std::collections::BTreeSet<_> =
        nodes.iter().map(|node| node.fingerprint).collect();
    assert_eq!(original_fingerprints, folded_fingerprints);
    for node in nodes {
        assert_eq!(node.source_ctx.location, primary);
        assert!(
            node.labels
                .iter()
                .any(|label| label.location == related && label.text == "defined here")
        );
        let projected = node.to_finding("synthetic");
        assert!(projected.related_locations.contains(&related));
        assert!(
            projected
                .related_labels
                .iter()
                .any(|label| label.location == related && label.message == "defined here")
        );
    }
}
