// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{compile_logic_report, surfaces};

/// The canonical offline dev-gate surface set `feedback` folds into one report
/// — the Rust twin of the retired Python `_EXPECTED_SURFACES`. Pinned here so a
/// future edit that adds or drops a fold surface without updating this set fails
/// the gate (the drift guard the deleted `test_feedback_surfaces.py` provided).
const EXPECTED_SURFACES: &[&str] = &[
    "alignment",
    "coverage",
    "acceptance",
    "wikidata",
    "constitution",
    "crate-layering",
    "repo-static",
    "box-roles",
    "audit",
    "generated",
    "logic-compile",
    "statement-compile",
    "mapping-compile",
    "slice-ownership",
];

/// The separately admitted native producer plus the report-returning surfaces
/// cover the exact canonical set. This reads declarations only; no thunk runs.
#[test]
fn surfaces_cover_exactly_the_canonical_set() {
    let mut got: Vec<&str> = surfaces().iter().map(|(label, _)| *label).collect();
    got.push("generated"); // generated_feedback owns the native producer verdict.
    got.sort_unstable();
    let mut expected: Vec<&str> = EXPECTED_SURFACES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        got, expected,
        "feedback surface set drifted from the canonical dev-gate surfaces"
    );
}

/// A `logic:` source that DECLARES a `logic:Correspondence` (an isomorphism with a
/// realized get leg) must compile cleanly through the public `compile_logic_report`
/// surface — its correspondence gates run against EXECUTED lens-law verdicts computed
/// by the caller. This pins the fix for the missing-verdict hard-fail: before the
/// caller discharged the verdicts, `compile_program` fed the gates an empty verdict map
/// and the round-trip gate PANICKED on this exact input (a `PanicException` on the PyO3
/// twin) instead of returning a result. The assertion is `is_ok`; a regression re-arms
/// the panic and aborts the test process rather than returning `Err`.
#[test]
fn correspondence_bearing_source_compiles_without_missing_verdict_panic() {
    // An Isomorphism cell with a single-step get leg. Its lawful put is the structural
    // inverse, which the round-trip gate composes + discharges — reaching `verdict_for`.
    let source = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://gmeow.example/corr/> .
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .

ex:iso a logic:Correspondence ;
    logic:correspondenceRelation logic:Equiv ;
    logic:morphismClass logic:Isomorphism ;
    logic:morphismKind logic:InstitutionMorphism ;
    logic:mnemomorphic \"true\"^^xsd:boolean ;
    logic:getLeg ex:isoGet .

ex:isoGet gm:path ex:isoStep .
";
    let report = compile_logic_report(source);
    assert!(
        report.is_ok(),
        "correspondence-bearing source must compile (verdicts discharged), got {report:?}"
    );
}
