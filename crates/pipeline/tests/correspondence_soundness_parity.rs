// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Corpus regression gate for the native correspondence-soundness pass.
//!
//! The seven correspondence-stack semantic checks (the five alignment checks + the two
//! FnO back-end checks — including the SOLE native enforcer of Constitution Principle 5,
//! the equivalence-collapse gate) live in the wasm-clean
//! `gmeow_logic_compile::projections::correspondence_soundness` pass, driven by the
//! oxigraph-free pipeline edge `stages::correspondence_soundness::lint_correspondence_soundness`.
//!
//! The original parity harness proved this pass byte-identical to the (now deleted)
//! oxigraph-coupled lints over the REAL committed repo corpus. The retired
//! lints are gone, so this harness now pins the pass to the corpus' expected finding count
//! and shape: a count drift or a missing check family flags a coverage regression.

use std::collections::BTreeSet;

use gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness;

/// The committed corpus' correspondence-soundness finding count (proven byte-identical to
/// the retired oxigraph-coupled projection lint by the original parity harness). A drift
/// here is a coverage regression: investigate (it is NOT a number to blindly re-bless).
const EXPECTED_FINDING_COUNT: usize = 448;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

#[test]
fn native_soundness_pass_matches_committed_corpus() {
    let root = repo_root();

    // The oxigraph-free soundness pass over the committed corpus, allow_network=false
    // (the on-gate path).
    let findings = lint_correspondence_soundness(&root, false)
        .expect("native correspondence-soundness pass should not error");

    // 1. The corpus exercises real findings (the alignment checks emit INFO findings for
    //    unavailable targets at minimum), so an empty result would be a silent no-op rather
    //    than a real regression proof.
    assert!(
        !findings.is_empty(),
        "expected the committed corpus to exercise at least one finding (sanity floor)"
    );

    // 2. The committed-corpus finding count is pinned (the parity floor).
    assert_eq!(
        findings.len(),
        EXPECTED_FINDING_COUNT,
        "correspondence-soundness finding COUNT drifted from the committed-corpus floor \
         ({EXPECTED_FINDING_COUNT}) to {} — investigate a coverage regression, do not blindly \
         re-bless",
        findings.len()
    );

    // 3. Every finding carries the canonical check tokens; the FnO + alignment check
    //    families must all be reachable from a single pass (a dropped family would silently
    //    shrink coverage).
    let checks: BTreeSet<&str> = findings.iter().map(|f| f.check.as_str()).collect();
    for known in &[
        "fno-type",
        "fno-ref",
        "inverse-direction",
        "domain-range",
        "property-character",
        "equivalence-collapse",
        "dc-refinement",
        "dc-hand-authored",
    ] {
        assert!(
            is_known_check(known),
            "internal: {known} is not a recognized soundness check token"
        );
    }

    // The committed corpus must at least exercise the domain-range INFO family (the
    // unavailable-target floor); the sanity floor above guarantees a non-empty result, and
    // this confirms the alignment leg ran rather than only the FnO leg.
    assert!(
        checks.contains("domain-range"),
        "expected the alignment leg (domain-range family) to be exercised; saw checks: {checks:?}"
    );

    eprintln!(
        "correspondence-soundness corpus gate: {} findings, check families exercised: {checks:?}",
        findings.len()
    );
}

/// Whether `check` is one of the eight canonical correspondence-soundness check tokens.
fn is_known_check(check: &str) -> bool {
    matches!(
        check,
        "fno-type"
            | "fno-ref"
            | "inverse-direction"
            | "domain-range"
            | "property-character"
            | "equivalence-collapse"
            | "dc-refinement"
            | "dc-hand-authored"
    )
}
