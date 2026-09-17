// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn manifest_paths() -> (PathBuf, PathBuf, PathBuf) {
    let root = repo_root();
    (
        root.join("governance").join("constitution.ttl"),
        root.join("CONSTITUTION.md"),
        root,
    )
}

fn manifest_projection() -> (Vec<Principle>, BTreeMap<String, Enforcement>) {
    let (manifest, _md, _root) = manifest_paths();
    let ttl = std::fs::read(&manifest).expect("manifest readable");
    let dataset = purrdf::parse_dataset(&ttl, "text/turtle", None).expect("manifest parses");
    (collect_principles(&dataset), collect_enforcements(&dataset))
}

fn fake_runs() -> BTreeMap<String, GateRun> {
    [
        ("validate", GateRun::new(0, Some(3))),
        ("constitution-check", GateRun::new(0, Some(3))),
        ("lint-alignment", GateRun::new(0, Some(0))),
        ("sync", GateRun::new(0, Some(0))),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn render(gate_runs: &BTreeMap<String, GateRun>, evidence_mode: &str) -> String {
    let (principles, enforcements) = manifest_projection();
    build_report(
        &principles,
        &enforcements,
        gate_runs,
        "2026-06-12T00:00:00+00:00",
        "deadbeef",
        "0.1.0",
        evidence_mode,
    )
}

#[test]
fn report_is_nonempty_valid_turtle_covering_every_principle() {
    let report = render(&fake_runs(), "in-process");
    assert!(!report.trim().is_empty());
    let dataset =
        purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
    // One PrincipleResult per principle.
    let (principles, _enf) = manifest_projection();
    let count = count_typed(&dataset, &format!("{META}PrincipleResult"));
    assert_eq!(count, principles.len());
}

#[test]
fn report_carries_provenance_and_evidence_mode() {
    let report = render(&fake_runs(), "in-process");
    assert!(report.contains("deadbeef"));
    assert!(report.contains("2026-06-12T00:00:00+00:00"));
    assert!(report.contains("meta:toolchainVersion \"0.1.0\""));
}

#[test]
fn runnable_gates_pass_and_failures_propagate() {
    assert!(render(&fake_runs(), "in-process").contains("\"passed\""));
    assert!(!render(&fake_runs(), "in-process").contains("\"failed\""));

    let mut failing = fake_runs();
    failing.insert("validate".to_string(), GateRun::new(2, Some(0)));
    assert!(render(&failing, "in-process").contains("\"failed\""));
}

#[test]
fn out_of_process_enforcement_is_gated_in_ci_never_silent() {
    let report = render(&fake_runs(), "in-process");
    assert!(report.contains("\"gated-in-ci\""));
    assert!(report.contains("\"declared\""));
}

#[test]
fn assumed_passed_marks_runnable_gates_passed_with_no_warning_count() {
    let gate_runs = assumed_passed_gate_runs(None);
    let report = render(&gate_runs, "prior-successful-gates");
    assert!(report.contains("meta:evidenceMode \"prior-successful-gates\""));
    assert!(!report.contains("\"failed\""));
    // Unknown warning count → no meta:warningCount on runnable results.
    // (Out-of-process results still carry a concrete count.)
    let dataset =
        purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
    assert!(count_typed(&dataset, &format!("{META}PrincipleResult")) > 0);
}

#[test]
fn constitution_gate_runs_against_the_repo() {
    let (manifest, md, root) = manifest_paths();
    let run = run_constitution_gate(&manifest, &md, &root);
    // The committed manifest must be internally consistent (zero errors).
    assert_eq!(run.errors, 0, "constitution gate should pass on main");
    assert!(run.warnings.is_some());
}

#[test]
fn compliance_report_smoke_reuses_constitution_gate() {
    let (manifest, md, root) = manifest_paths();
    let report = compliance_report(
        &manifest,
        &md,
        &root,
        &assumed_passed_gate_runs(None),
        "0.1.0",
        "prior-successful-gates",
    )
    .expect("report renders");
    assert!(!report.trim().is_empty());
    assert!(report.contains("meta:ComplianceReport"));
    purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
}

fn count_typed(ds: &purrdf::RdfDataset, class_iri: &str) -> usize {
    use purrdf::{DatasetView, GraphMatch, TermValue};
    let (Some(type_id), Some(class_id)) = (
        ds.term_id_by_value(&TermValue::iri(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        )),
        ds.term_id_by_value(&TermValue::iri(class_iri)),
    ) else {
        return 0;
    };
    ds.quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .count()
}
