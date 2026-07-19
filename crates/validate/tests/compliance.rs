// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Retained Python↔Rust parity tests for the compliance report.
//!
//! These exercise the pure [`gmeow_validate::compliance::build_report`] renderer
//! against the real constitution manifest and fake gate runs, mirroring the six
//! cases that previously lived in `tests/test_compliance.py`.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_validate::compliance::{self, GateRun, META, RUNNER_NAMES, assumed_passed_gate_runs};
use gmeow_validate::constitution::{
    Enforcement, Principle, collect_enforcements, collect_principles,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn manifest_projection() -> (Vec<Principle>, BTreeMap<String, Enforcement>) {
    let manifest = repo_root().join("governance").join("constitution.ttl");
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
    compliance::build_report(
        &principles,
        &enforcements,
        gate_runs,
        "2026-06-12T00:00:00+00:00",
        "deadbeef",
        "0.1.0",
        evidence_mode,
    )
}

fn parse_report(report: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("report is valid Turtle")
}

fn term_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

fn literal_objects(ds: &RdfDataset, subj: TermId, pred_iri: &str) -> Vec<String> {
    let Some(pid) = term_id(ds, pred_iri) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subj), Some(pid), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
            _ => None,
        })
        .collect()
}

fn iri_objects(ds: &RdfDataset, subj: TermId, pred_iri: &str) -> Vec<String> {
    let Some(pid) = term_id(ds, pred_iri) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subj), Some(pid), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_string()),
            _ => None,
        })
        .collect()
}

fn has_triple(ds: &RdfDataset, subj_iri: &str, pred_iri: &str, obj_iri: &str) -> bool {
    let Some(sid) = term_id(ds, subj_iri) else {
        return false;
    };
    iri_objects(ds, sid, pred_iri).contains(&obj_iri.to_string())
}

fn principle_result_iri(number: i64) -> String {
    format!("{META}Principle{number}Result")
}

fn enforcement_local_names(ds: &RdfDataset) -> BTreeMap<TermId, String> {
    let pred = term_id(ds, &format!("{META}enforcement"));
    let Some(pred) = pred else {
        return BTreeMap::new();
    };
    let mut map = BTreeMap::new();
    for q in ds.quads_for_pattern(None, Some(pred), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            map.insert(q.s, iri.strip_prefix(META).unwrap_or(iri).to_string());
        }
    }
    map
}

fn runnable_enforcement_local_names(
    enforcements: &BTreeMap<String, Enforcement>,
) -> BTreeSet<String> {
    let runners: BTreeSet<&str> = RUNNER_NAMES.iter().copied().collect();
    enforcements
        .values()
        .filter(|e| {
            e.make_targets
                .iter()
                .chain(&e.cli_commands)
                .any(|c| runners.contains(c.as_str()))
        })
        .map(|e| e.local_name().to_string())
        .collect()
}

#[test]
fn report_is_valid_turtle_covering_every_principle() {
    let report = render(&fake_runs(), "in-process");
    let ds = parse_report(&report);
    let (principles, _enf) = manifest_projection();

    let type_id =
        term_id(&ds, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type").expect("rdf:type");
    let class_id = term_id(&ds, &format!("{META}PrincipleResult")).expect("PrincipleResult");
    let count = ds
        .quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .count();
    assert_eq!(count, principles.len(), "one PrincipleResult per principle");
}

#[test]
fn report_carries_supersession_edges() {
    let ds = parse_report(&render(&fake_runs(), "in-process"));
    assert!(has_triple(
        &ds,
        &principle_result_iri(2),
        &format!("{META}supersededInPartBy"),
        &format!("{META}Principle17")
    ));
    assert!(has_triple(
        &ds,
        &principle_result_iri(8),
        &format!("{META}supersededInPartBy"),
        &format!("{META}Principle17")
    ));
    assert!(has_triple(
        &ds,
        &principle_result_iri(12),
        &format!("{META}supersededInPartBy"),
        &format!("{META}Principle17")
    ));
    assert!(has_triple(
        &ds,
        &principle_result_iri(18),
        &format!("{META}extends"),
        &format!("{META}Principle17")
    ));
    assert!(has_triple(
        &ds,
        &principle_result_iri(18),
        &format!("{META}extends"),
        &format!("{META}Principle13")
    ));
}

#[test]
fn runnable_gates_report_passed_and_failures_propagate() {
    let passing = render(&fake_runs(), "in-process");
    assert!(
        passing.contains("\"passed\""),
        "passing gates render passed status"
    );
    assert!(
        !passing.contains("\"failed\""),
        "no failure in passing report"
    );

    let mut failing = fake_runs();
    failing.insert("validate".to_string(), GateRun::new(2, Some(0)));
    let failing_report = render(&failing, "in-process");
    assert!(
        failing_report.contains("\"failed\""),
        "validation errors propagate to failed"
    );
}

#[test]
fn out_of_process_enforcement_is_gated_in_ci_and_never_silent() {
    let report = render(&fake_runs(), "in-process");
    assert!(
        report.contains("\"gated-in-ci\""),
        "CI/oracle enforcements are gated-in-ci"
    );
    assert!(
        report.contains("\"declared\""),
        "practice-only enforcements are declared"
    );
}

#[test]
fn report_carries_provenance() {
    let report = render(&fake_runs(), "in-process");
    assert!(report.contains("deadbeef"), "source commit is present");
    assert!(
        report.contains("2026-06-12T00:00:00+00:00"),
        "generatedAt is present"
    );
}

#[test]
fn prior_gate_evidence_mode_marks_runnable_gates_passed() {
    let gate_runs = assumed_passed_gate_runs(None);
    let report = render(&gate_runs, "prior-successful-gates");
    let ds = parse_report(&report);

    assert!(
        report.contains("meta:evidenceMode \"prior-successful-gates\""),
        "evidence mode is prior-successful-gates"
    );
    assert!(
        !report.contains("\"failed\""),
        "no failures in prior-successful-gates report"
    );

    let (_, enforcements) = manifest_projection();
    let runnable = runnable_enforcement_local_names(&enforcements);
    assert!(!runnable.is_empty(), "at least one enforcement is runnable");

    let local_by_bn = enforcement_local_names(&ds);
    for (bn, local) in &local_by_bn {
        if !runnable.contains(local) {
            continue;
        }
        let statuses = literal_objects(&ds, *bn, &format!("{META}status"));
        assert_eq!(
            statuses,
            vec!["passed"],
            "runnable enforcement {local} must be marked passed"
        );

        let error_counts: Vec<i64> = literal_objects(&ds, *bn, &format!("{META}errorCount"))
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        assert_eq!(
            error_counts,
            vec![0],
            "runnable enforcement {local} must have errorCount 0"
        );

        let warning_pred = term_id(&ds, &format!("{META}warningCount"));
        if let Some(pid) = warning_pred {
            let has_warning = ds
                .quads_for_pattern(Some(*bn), Some(pid), None, GraphMatch::Any)
                .next()
                .is_some();
            assert!(
                !has_warning,
                "runnable enforcement {local} must not carry a warningCount in prior-gate mode"
            );
        }
    }
}
