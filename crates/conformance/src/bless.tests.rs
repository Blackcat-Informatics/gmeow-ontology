// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Bless self-consistency: regenerating a case's goldens and re-running the diff
/// yields no mismatches. Comparison is canonical/graph-iso, so the freshly
/// blessed (canonical) goldens are accepted by the gate. (Explanation `.md` is
/// refreshed like every other existing golden, but the renderer is deterministic,
/// so it regenerates to byte-identical content and the cited-IRI skeleton still
/// matches.)
#[test]
fn bless_is_self_consistent() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dst = tmp.path().join("synthetic").join("class-axiom");
    std::fs::create_dir_all(dst.join("expected/projections")).unwrap();
    std::fs::write(
        dst.join("input.logic.ttl"),
        "<urn:test:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:test:B> .",
    )
    .unwrap();
    std::fs::write(dst.join("profile.json"), r#"{"mode":"native"}"#).unwrap();
    for name in [
        "owl-dl.ttl",
        "owl-el.ttl",
        "gufo.ttl",
        "datalog.dl",
        "n3.n3",
        "canonical-rdf12.ttl",
        "preservation-ledger.json",
    ] {
        std::fs::write(dst.join("expected/projections").join(name), "").unwrap();
    }
    for name in [
        "materialized.nq",
        "verdicts.json",
        "certification.json",
        "budget.json",
    ] {
        std::fs::write(dst.join("expected").join(name), "").unwrap();
    }
    // gmeow-test-input: synthetic-only; all inputs are constructed above.
    let out = crate::run::run_case(
        &dst,
        &crate::run::RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .expect("run_case ok");
    write_expected(&dst, &out).expect("bless ok");

    // gmeow-test-input: synthetic-only; no repository source or fixture read.
    let out2 = crate::run::run_case(
        &dst,
        &crate::run::RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .expect("run_case (post-bless) ok");
    let diffs = crate::compare::diff_case(&dst, &out2);
    assert!(diffs.is_empty(), "bless not self-consistent: {diffs:?}");
}
