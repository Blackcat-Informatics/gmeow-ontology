// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Off-gate independent OWL-Direct consistency cross-check teeth tests.
//!
//! These drive the PRODUCTION [`gmeow_logic::consistency_crosscheck`] surface over
//! real world-scoped [`purrdf::RdfDataset`]s — mirroring `coherence_gate.rs`, which
//! co-locates schema + ABox in ONE named graph so the world-scoped chase fires. The
//! cross-check is a native-SOUNDNESS anti-regression tripwire: it hard-fails ONLY on
//! an `OracleOnly` row (native decided consistent while the sound OWL-Direct oracle
//! proves inconsistent/empty). Native by-design incompleteness (a withheld
//! out-of-fragment construct) is recorded NON-failing as `oracle-supplement`, and a
//! budget-exceeded oracle world is recorded NON-failing as `oracle-undecided`.

use std::time::Duration;

use gmeow_logic::consistency_crosscheck::{
    WatchdogOutcome, classify_consistency, oracle_undecided_row, oracle_within_budget,
    run_consistency_crosscheck,
};
use gmeow_logic::reason::divergence_findings;
use gmeow_logic::reason::dl::{DlCoverage, DlVerdict};
use gmeow_logic::reason::ledger::{DivergenceKind, build_ledger, enforce};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const W: &str = "http://gmeow.example/world";

fn iri(local: &str) -> String {
    format!("http://gmeow.example/{local}")
}

/// Build a single-named-world dataset from `(subject, predicate, object)` IRI
/// triples, co-located in ONE named graph `W` (default-graph triples are invisible
/// to the world-scoped chase, so every fact must carry the world). A local object
/// name is prefixed with the example namespace; a full IRI (containing `://`) is
/// kept verbatim.
fn world_dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
    fn obj(o: &str) -> RdfTerm {
        if o.contains("://") {
            RdfTerm::iri(o.to_owned())
        } else {
            RdfTerm::iri(iri(o))
        }
    }
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        let quad = RdfQuad::new(RdfTerm::iri(iri(s)), *p, obj(o)).in_graph(RdfTerm::iri(W));
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

/// A DlVerdict that DECIDED (gap-free) with the given global consistency and no
/// unsatisfiable classes / inconsistencies — the minimal decided-facts shape the
/// tripwire regression pair needs.
fn decided_verdict(consistent: bool) -> DlVerdict {
    DlVerdict {
        consistent,
        unsatisfiable_classes: Vec::new(),
        inconsistencies: Vec::new(),
        coverage: DlCoverage {
            present: Vec::new(),
            decided: Vec::new(),
            unsupported: Vec::new(),
        },
        gaps: Vec::new(),
    }
}

#[test]
fn clean_world_passes_all_agree() {
    // :A ⊑ :B — both engines decide consistent → one global Agree row, verdict passes.
    let ds = world_dataset(&[
        ("A", RDF_TYPE, OWL_CLASS),
        ("B", RDF_TYPE, OWL_CLASS),
        ("A", SUBCLASS_OF, "B"),
    ]);
    let outcome = run_consistency_crosscheck(ds.as_ref()).expect("cross-check runs");

    assert!(
        outcome.verdict.passed,
        "a clean world must pass: {:?}",
        outcome.verdict
    );
    assert_eq!(
        outcome.oracle_only, 0,
        "no soundness misses on a clean world"
    );
    assert!(
        outcome
            .ledger
            .rows
            .iter()
            .all(|r| r.kind == DivergenceKind::Agree),
        "every row on a clean world is an agreement: {:#?}",
        outcome.ledger.rows
    );
    assert_eq!(outcome.source_worlds, 1, "exactly one named world");
    assert!(outcome.agree >= 1, "at least the global agreement row");
}

#[test]
fn populated_clash_both_engines_agree_and_pass() {
    // :x a :Y, :Z ; :Y owl:disjointWith :Z — schema + ABox co-located in ONE world.
    // Native derives x ⊑ owl:Nothing (inconsistent); the oracle sees the same ABox
    // clash. Both decide INCONSISTENT → Agree, verdict passes (no soundness miss).
    let ds = world_dataset(&[
        ("Y", RDF_TYPE, OWL_CLASS),
        ("Z", RDF_TYPE, OWL_CLASS),
        ("Y", OWL_DISJOINT_WITH, "Z"),
        ("x", RDF_TYPE, "Y"),
        ("x", RDF_TYPE, "Z"),
    ]);
    let outcome = run_consistency_crosscheck(ds.as_ref()).expect("cross-check runs");

    assert!(
        outcome.verdict.passed,
        "both-engines-inconsistent must pass (agreement, not a divergence): {:?}",
        outcome.verdict
    );
    assert_eq!(
        outcome.oracle_only, 0,
        "agreement yields no OracleOnly rows"
    );
    assert!(
        outcome
            .ledger
            .rows
            .iter()
            .any(|r| r.kind == DivergenceKind::Agree && r.detail.contains("inconsistent")),
        "a both-inconsistent agreement row must be present: {:#?}",
        outcome.ledger.rows
    );
}

#[test]
fn native_withheld_construct_is_non_failing_oracle_supplement() {
    // A world whose only non-trivial content is owl:oneOf — an out-of-fragment
    // construct native WITHHOLDS (a genuine native gap, verified by dl_consistency
    // reporting reason.dl-gap.oneOf). The oracle decides; the world is recorded as a
    // NON-failing oracle-supplement, NOT OracleOnly / DlGap. This is the load-bearing
    // "native by-design incompleteness is not a failure" proof.
    let ds = world_dataset(&[("C", RDF_TYPE, OWL_CLASS), ("C", OWL_ONE_OF, "enum")]);

    // Confirm native genuinely withholds here (non-empty gaps on the projection).
    let native = gmeow_logic::reason::dl_consistency(&ds.project_named_graph(W))
        .expect("native decides the projection");
    assert!(
        !native.gaps.is_empty(),
        "owl:oneOf must produce a native gap: {:?}",
        native.gaps
    );

    let outcome = run_consistency_crosscheck(ds.as_ref()).expect("cross-check runs");
    assert!(
        outcome.verdict.passed,
        "a native-withheld construct must not fail the gate: {:?}",
        outcome.verdict
    );
    assert_eq!(
        outcome.oracle_only, 0,
        "a native gap is never a soundness miss"
    );
    assert_eq!(
        outcome.ledger.dl_gap, 0,
        "a native gap is never a DlGap here"
    );
    assert_eq!(
        outcome.oracle_supplement, 1,
        "the withheld world is recorded as one oracle-supplement: {:#?}",
        outcome.ledger.rows
    );
}

#[test]
fn tripwire_fails_on_native_soundness_miss_via_production_seam() {
    // A sound native engine yields NO natural OracleOnly, so the tripwire is
    // exercised by feeding the PRODUCTION classifier the regression it guards: a
    // native verdict that DECIDED the world consistent (gap-free) paired with an
    // oracle that proved a GLOBAL inconsistency (false, []). This is a native
    // soundness miss → OracleOnly → enforce fails. No test-only classifier copy.
    let native = decided_verdict(true);
    let oracle = (false, Vec::<String>::new());
    let rows = classify_consistency(&native, &oracle, W);

    assert!(
        rows.iter().any(|r| r.kind == DivergenceKind::OracleOnly),
        "the regression pair must classify as OracleOnly: {rows:#?}"
    );
    let ledger = build_ledger(Vec::new(), rows, Vec::new(), Vec::new());
    let verdict = enforce(&ledger);
    assert!(
        !verdict.passed,
        "an OracleOnly soundness miss must FAIL the gate: {verdict:?}"
    );
    assert!(
        verdict.reasons.iter().any(|r| r.contains("oracle-only")),
        "the failing reason names the oracle-only soundness miss: {verdict:?}"
    );
}

#[test]
fn per_class_dimension_is_not_discarded() {
    // MAXIMAL INFORMATION: native decided satisfiable, oracle proved class :C empty
    // (a consistent ontology with an empty class). The per-class unsatisfiability
    // flows through compare_subsumption → an OracleOnly per-class row → FAIL. This
    // guards the class-level soundness signal (native ⊉ oracle at class depth).
    let native = decided_verdict(true);
    let oracle = (false, vec![iri("C")]); // consistent-but-empty-class
    let rows = classify_consistency(&native, &oracle, W);

    // Global dimension: oracle (false, [C]) is globally CONSISTENT, native consistent
    // → Agree (no global soundness miss).
    assert!(
        rows.iter()
            .any(|r| r.kind == DivergenceKind::Agree && r.detail.contains("agree consistent")),
        "global dimension agrees consistent: {rows:#?}"
    );
    // Per-class dimension: the oracle-only empty class is a soundness miss.
    let per_class: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == DivergenceKind::OracleOnly)
        .collect();
    assert_eq!(
        per_class.len(),
        1,
        "the oracle-only empty class is a per-class OracleOnly row: {rows:#?}"
    );
    assert!(
        per_class[0].detail.contains("per-class unsatisfiability"),
        "the per-class row is labelled and carries the world: {:?}",
        per_class[0].detail
    );
    assert!(!enforce(&build_ledger(Vec::new(), rows, Vec::new(), Vec::new())).passed);
}

#[test]
fn watchdog_maps_budget_miss_to_non_failing_oracle_undecided() {
    // The oracle is uninterruptible, so a per-world budget miss must be recorded, not
    // hang the lane. Drive the PRODUCTION watchdog seam with a computation that
    // deterministically outlives a tiny budget → Undecided; the decided path returns
    // the verdict. Injecting the slow computation exercises the real timeout→row
    // mapping (the watchdog IS production code; only the timed computation differs).
    let tiny = Duration::from_millis(20);
    let slow = oracle_within_budget(
        move || {
            std::thread::sleep(Duration::from_secs(30));
            (true, Vec::new())
        },
        tiny,
    )
    .expect("watchdog itself does not error on a timeout");
    assert_eq!(
        slow,
        WatchdogOutcome::Undecided,
        "a computation outliving the budget maps to Undecided"
    );

    let fast = oracle_within_budget(|| (true, Vec::new()), Duration::from_secs(30))
        .expect("watchdog returns the verdict");
    assert_eq!(
        fast,
        WatchdogOutcome::Decided((true, Vec::new())),
        "an in-budget computation returns its verdict"
    );

    // The undecided row the run builds from a timeout is NON-failing.
    let row = oracle_undecided_row(W, tiny);
    assert_eq!(
        row.kind,
        DivergenceKind::Agree,
        "oracle-undecided is non-failing"
    );
    let ledger = build_ledger(Vec::new(), vec![row], Vec::new(), Vec::new());
    assert!(
        enforce(&ledger).passed,
        "a budget miss must never redden the gate"
    );
}

#[test]
fn every_divergence_row_grounds_as_a_finding_carrying_its_world() {
    // MAXIMAL GROUNDING: the outcome projects to one gmeow:Finding per ledger row,
    // and each row carries its world IRI in the message (the world is the row's
    // distinctness key, so per-world rows never merge across worlds).
    let ds = world_dataset(&[
        ("A", RDF_TYPE, OWL_CLASS),
        ("B", RDF_TYPE, OWL_CLASS),
        ("A", SUBCLASS_OF, "B"),
    ]);
    let outcome = run_consistency_crosscheck(ds.as_ref()).expect("cross-check runs");

    let findings = divergence_findings(&outcome.ledger);
    assert_eq!(
        findings.len(),
        outcome.ledger.rows.len(),
        "one finding per ledger row"
    );
    assert!(
        !findings.is_empty(),
        "the clean world still grounds a finding"
    );
    assert!(
        findings.iter().all(|f| f.message.contains(W)),
        "every finding's message carries the world IRI: {:#?}",
        findings.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
    assert!(
        findings
            .iter()
            .all(|f| f.tool.as_deref() == Some("conformance")),
        "the divergence findings are conformance-tool observations"
    );
}
