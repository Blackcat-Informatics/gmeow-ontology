// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::model::{Location, Report};
use gmeow_errors::render::to_gmeow_rdf_in_graph;

/// A finding's carried (right-invertible) fields — the subset the section
/// reproduces. Comparing these makes the retraction law explicit.
#[derive(Debug, PartialEq)]
struct Carried {
    finding_iri: Option<String>,
    severity: Severity,
    code: String,
    message: String,
    category: Option<FindingCategory>,
    standpoint: Option<Standpoint>,
    anchor_iri: Option<String>,
    anchor_non_trivial: bool,
    antecedents: Vec<String>,
    related_labels: Vec<RelatedLabel>,
    locations: Vec<Location>,
}

fn carried(f: &Finding) -> Carried {
    Carried {
        finding_iri: f.finding_iri.clone(),
        severity: f.severity,
        code: f.code.clone(),
        message: f.message.clone(),
        category: f.category,
        standpoint: f.standpoint,
        anchor_iri: f.anchor_iri.clone(),
        anchor_non_trivial: f.anchor_non_trivial,
        antecedents: f.antecedents.clone(),
        related_labels: f.related_labels.clone(),
        locations: f.locations.clone(),
    }
}

fn finding_iri(hex: &str) -> String {
    format!("https://blackcatinformatics.ca/gmeow/diagnostics/finding/{hex}")
}

/// Build a small multi-node finding DAG: a Fatal root `R` deriving from two
/// antecedents `A` and `B`, both of which derive from a SHARED leaf `C` (a
/// diamond). `R` carries a non-trivial anchor + a text-bearing related label.
fn sample_report() -> Report {
    let r_iri = finding_iri("aaaa0000aaaa0000");
    let a_iri = finding_iri("bbbb1111bbbb1111");
    let b_iri = finding_iri("cccc2222cccc2222");
    let c_iri = finding_iri("dddd3333dddd3333");
    let anchor = "https://blackcatinformatics.ca/gmeow/diagnostics/anchor/aaaa0000aaaa0000";

    let mut report = Report::new("validate");

    // Root R — Fatal (Error + DataShapeViolation + Binding), anchored, labelled.
    let mut r = Finding::new(
        Severity::Error,
        "shape.min-count",
        "focus node fails minCount",
    )
    .with_category(FindingCategory::DataShapeViolation)
    .with_standpoint(Standpoint::Binding)
    .with_tool("validate");
    r.finding_iri = Some(r_iri.clone());
    r.anchor_iri = Some(anchor.to_owned());
    r.anchor_non_trivial = true;
    r.antecedents = vec![a_iri.clone(), b_iri.clone()];
    r.add_location(
        Location::new(Some("core/x.ttl".to_owned()), Some(12), Some(4), None).with_gts_quad(42),
    );
    r.add_related_label(RelatedLabel {
        location: Location::new(Some("core/x.ttl".to_owned()), Some(3), Some(1), None)
            .with_gts_term(7),
        message: "shape declared here".to_owned(),
    });

    // A — derives from C.
    let mut a = Finding::new(Severity::Warning, "logic.derived-a", "intermediate A")
        .with_category(FindingCategory::ModelingDisciplineViolation)
        .with_standpoint(Standpoint::Perspectival);
    a.finding_iri = Some(a_iri.clone());
    a.antecedents = vec![c_iri.clone()];

    // B — also derives from C (the shared antecedent → diamond).
    let mut b = Finding::new(Severity::Note, "logic.derived-b", "intermediate B")
        .with_category(FindingCategory::PolicyWarning)
        .with_standpoint(Standpoint::Advisory);
    b.finding_iri = Some(b_iri.clone());
    b.antecedents = vec![c_iri.clone()];

    // C — the shared leaf root cause.
    let mut c = Finding::new(Severity::Error, "logic.root-cause", "the shared root cause")
        .with_category(FindingCategory::ContradictionWitness)
        .with_standpoint(Standpoint::Advisory);
    c.finding_iri = Some(c_iri.clone());

    report.add_finding(r);
    report.add_finding(a);
    report.add_finding(b);
    report.add_finding(c);
    report
}

/// Expected verdict of a report, computed from the same grade→gate fold the
/// reader uses, so the assertion is an independent recomputation.
fn report_verdict(report: &Report) -> GateVerdict {
    report
        .findings
        .iter()
        .map(finding_gate)
        .fold(GateVerdict::Collected, GateVerdict::join)
}

#[test]
fn read_of_emit_reproduces_the_carried_finding_subset_and_dag() {
    // THE RETRACTION LAW: read(emit(x)) reproduces the carried subset for every
    // finding, and the rehydrated DAG walk reproduces the original structure.
    let report = sample_report();
    let normalized = report.normalized();

    // emit → graph/diagnostics N-Quads → read back through the SPARQL engine.
    let nquads = to_gmeow_rdf_in_graph(&report, GRAPH_DIAGNOSTICS);
    let index = read_findings_from_nquads(nquads.as_bytes()).expect("read back");

    // Same set of findings (keyed by fingerprint IRI).
    assert_eq!(index.len(), normalized.findings.len());

    // Carried subset reproduced for EVERY finding — code, grade (severity +
    // category + standpoint), message, anchor (+ non-trivial flag), antecedent
    // edges, related-label TEXT + location, and primary location.
    for original in &normalized.findings {
        let iri = original.finding_iri.clone().expect("witness has an IRI");
        let round = index.get(&iri).expect("finding rehydrated");
        assert_eq!(
            carried(round),
            carried(original),
            "carried subset mismatch for {iri}"
        );
    }

    // The related-label TEXT specifically survived (the recently-added leg).
    let r_iri = finding_iri("aaaa0000aaaa0000");
    let r = index.get(&r_iri).expect("root present");
    assert_eq!(r.related_labels.len(), 1);
    assert_eq!(r.related_labels[0].message, "shape declared here");
    assert_eq!(r.related_labels[0].location.gts_term_id, Some(7));

    // The rehydrated DAG walk from the root reproduces the original structure:
    // same node set + same edge set as the source finding graph.
    let tree = explain_finding(&index, &r_iri).expect("walk");
    let mut walked_nodes: BTreeSet<String> = BTreeSet::new();
    let mut walked_edges: BTreeSet<(String, String)> = BTreeSet::new();
    for node in tree.preorder() {
        walked_nodes.insert(node.key.clone());
        for child in &node.children {
            walked_edges.insert((node.key.clone(), child.key.clone()));
        }
    }
    // Expected structure straight off the original findings' antecedents.
    let mut expected_nodes: BTreeSet<String> = BTreeSet::new();
    let mut expected_edges: BTreeSet<(String, String)> = BTreeSet::new();
    let by_iri: BTreeMap<&str, &Finding> = normalized
        .findings
        .iter()
        .filter_map(|f| f.finding_iri.as_deref().map(|i| (i, f)))
        .collect();
    let mut stack = vec![r_iri.clone()];
    while let Some(key) = stack.pop() {
        expected_nodes.insert(key.clone());
        if let Some(f) = by_iri.get(key.as_str()) {
            for ant in &f.antecedents {
                expected_edges.insert((key.clone(), ant.clone()));
                stack.push(ant.clone());
            }
        }
    }
    assert_eq!(walked_nodes, expected_nodes, "DAG node set mismatch");
    assert_eq!(walked_edges, expected_edges, "DAG edge set mismatch");

    // The shared leaf C is reached along BOTH R→A→C and R→B→C: the shared-DAG
    // render prints it once in full and back-references it on the second visit.
    let rendered = render_shared_dag(&tree);
    let c_iri = finding_iri("dddd3333dddd3333");
    assert_eq!(
        rendered.matches(&format!("↑ see {c_iri}")).count(),
        1,
        "shared antecedent must be back-referenced exactly once:\n{rendered}"
    );
    assert!(
        rendered.contains("the shared root cause"),
        "shared antecedent must be printed in full once:\n{rendered}"
    );

    // The verdict computed from the rehydrated ledger equals the original's.
    assert_eq!(verdict(&index), report_verdict(&report));
    assert_eq!(verdict(&index), GateVerdict::Fatal, "root R is Fatal");

    // The minimal fatal cut is exactly the Fatal-gated finding (only R gates:
    // C is Error but Advisory, so it never gates).
    let cut = minimal_fatal_cut(&index);
    assert_eq!(cut, BTreeSet::from([r_iri.clone()]));
}

#[test]
fn empty_diagnostics_graph_is_collected_with_no_findings() {
    // A report with no findings projects no diagnostics quads; the reader yields
    // an empty index whose verdict is the bottom (Collected) and whose cut is empty.
    let report = Report::new("validate");
    let nquads = to_gmeow_rdf_in_graph(&report, GRAPH_DIAGNOSTICS);
    let index = read_findings_from_nquads(nquads.as_bytes()).expect("read back");
    assert!(index.is_empty());
    assert_eq!(verdict(&index), GateVerdict::Collected);
    assert!(minimal_fatal_cut(&index).is_empty());
}

#[test]
fn witness_explain_reuses_the_shared_dag_walk_engine() {
    // One machinery over one plane: the invented-null explain descends via
    // the ONE shared `gmeow_errors::dag::walk` engine — the same one `explain_finding`
    // uses — a second entry point, never a hand-rolled parallel walker. Scope the
    // check to the `explain_witness` body region (before the test module) so the
    // assertion does not match its own source.
    let src = include_str!("diagnostics_reader.rs");
    let after = src
        .split_once("pub fn explain_witness")
        .expect("explain_witness present")
        .1;
    let body = after
        .split_once("\npub fn ")
        .map(|(head, _)| head)
        .unwrap_or(after);
    let walk_call = format!("{}(", "walk");
    assert!(
        body.contains(&walk_call),
        "explain_witness must descend via the shared gmeow_errors::dag::walk engine, \
             not a hand-rolled recursion"
    );
}
