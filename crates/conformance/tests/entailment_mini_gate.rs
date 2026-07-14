// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `entailment-mini` coverage-floor + divergence regression gate.
//!
//! `entailment-mini` is the self-authored, license-clean corpus of both-inline
//! W3C-`otest:`-style entailment tests the native `dl_entails` reduction grades
//! end-to-end (`A ⊨ C` iff `premise ∪ ¬C` is inconsistent). Unlike the upstream OWL 2
//! profile suites — whose entailment premises/conclusions are reference documents the
//! inline vendoring path cannot grade — this corpus gives the entailment lane a
//! NON-VACUOUS, drift-free coverage floor.
//!
//! This gate is the executable acceptance check for "broaden the reasoner's W3C
//! entailment coverage": a build in which the entailment reduction stopped grading
//! (every case routed to a gap) FAILS the coverage-floor test — the soundness-only
//! divergence check below does NOT substitute for it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::paths::cases_root;
use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// The native verdict token for one case, computed exactly as the grader does: a
/// non-empty `gaps` is `incomplete`; otherwise the consistency boolean.
fn native_token(input_nq: &Path) -> String {
    let bytes =
        std::fs::read(input_nq).unwrap_or_else(|e| panic!("read {}: {e}", input_nq.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {}: {e}", input_nq.display()));
    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .unwrap_or_else(|e| panic!("dl_consistency on {}: {e}", input_nq.display()));
    if !verdict.gaps.is_empty() {
        "incomplete".to_owned()
    } else if verdict.consistent {
        "consistent".to_owned()
    } else {
        "inconsistent".to_owned()
    }
}

fn external_root() -> PathBuf {
    cases_root().join("external")
}

/// The frozen Lane-A entailment cases: each a previously-ungradeable W3C-style
/// entailment test now graded with a real (non-gap) native verdict AGREEING with the
/// declared outcome. A `PositiveEntailmentTest` (`A ⊨ C`) reduces to `inconsistent`;
/// a `NegativeEntailmentTest` (`A ⊭ C`) to `consistent`.
fn graded_entailment_cases() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("subclass-transitive", "inconsistent"),
        ("type-membership", "inconsistent"),
        ("subclass-not-entailed", "consistent"),
        ("type-not-entailed", "consistent"),
    ])
}

/// The pinned minimum number of non-gap entailment cases the reduction must grade
/// (the executable coverage floor for deliverable a). Set from the vendored corpus.
const ENTAILMENT_COVERAGE_FLOOR: usize = 4;

/// The frozen divergence cases: conclusions outside the soundly-refutable single-EDB
/// fragment, each an honest structured gap.
fn divergence_gaps() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("multi-triple-conclusion", "multi-triple"),
        ("role-conclusion", "role-assertion"),
    ])
}

/// COVERAGE FLOOR (the headline check for deliverable a): the native entailment
/// reduction grades AT LEAST the pinned number of entailment cases with a real
/// (non-gap) verdict agreeing with the W3C-declared outcome. A build where the
/// reduction stopped grading (all cases → gap) drops the corpus below the floor and
/// FAILS here.
#[test]
fn entailment_reduction_meets_the_non_gap_coverage_floor() {
    let corpus = external_root().join("entailment-mini");
    assert!(
        corpus.is_dir(),
        "entailment-mini corpus missing: {}",
        corpus.display()
    );

    let mut graded = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for (slug, expected) in graded_entailment_cases() {
        let case = corpus.join(slug);
        assert!(
            case.is_dir(),
            "graded entailment case missing from the Lane-A corpus: {}",
            case.display()
        );
        let token = native_token(&case.join("input.nq"));
        if token != expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, frozen (W3C-agreeing) expectation is {expected:?}"
            ));
        } else {
            graded += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "entailment coverage regression (a wrong/changed native verdict):\n  • {}",
        failures.join("\n  • ")
    );
    assert!(
        graded >= ENTAILMENT_COVERAGE_FLOOR,
        "entailment reduction graded only {graded} non-gap agreeing cases, below the pinned \
         floor {ENTAILMENT_COVERAGE_FLOOR} — 'broaden entailment coverage' is not delivered"
    );
}

/// Every divergence case is an honest structured gap: its committed native verdict is
/// `incomplete` (NEVER a wrong decided answer), and its `profile.json` records the
/// structured `gmeow:gapShape` token (the data the pipeline reifier projects as
/// first-class `gmeow:CapabilityGap`).
#[test]
fn divergence_gaps_are_incomplete_and_carry_a_shape() {
    let corpus = external_root().join("entailment-mini-divergence");
    let mut failures: Vec<String> = Vec::new();
    for (slug, expected_shape) in divergence_gaps() {
        let case = corpus.join(slug);
        assert!(case.is_dir(), "divergence case missing: {}", case.display());

        // Soundness floor: a gap must never be a wrong decided verdict.
        let token = native_token(&case.join("input.nq"));
        assert_ne!(
            token, "inconsistent",
            "{slug}: a gap case decided `inconsistent` — the divergence bucket must carry an \
             honest `incomplete`, never a decided verdict"
        );

        let profile: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(case.join("profile.json"))
                .unwrap_or_else(|e| panic!("read profile.json for {slug}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse profile.json for {slug}: {e}"));
        assert_eq!(
            profile["native_verdict"].as_str(),
            Some("incomplete"),
            "{slug}: profile.json must freeze the native verdict as `incomplete`"
        );
        if profile["gap_shape"].as_str() != Some(expected_shape) {
            failures.push(format!(
                "{slug}: gap_shape is {:?}, frozen expectation is {expected_shape:?}",
                profile["gap_shape"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "entailment divergence gap-shape regression:\n  • {}",
        failures.join("\n  • ")
    );
}

/// Pin the EXACT divergence-set membership: a future change that grades one of these
/// gaps (or introduces a new one) must update this gate deliberately.
#[test]
fn entailment_divergence_membership_is_exact() {
    let corpus = external_root().join("entailment-mini-divergence");
    assert!(
        corpus.is_dir(),
        "entailment divergence corpus missing: {}",
        corpus.display()
    );
    let mut found: Vec<String> = std::fs::read_dir(&corpus)
        .unwrap_or_else(|e| panic!("read {}: {e}", corpus.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
        .collect();
    found.sort();
    let mut expected: Vec<String> = divergence_gaps().keys().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the entailment-mini-divergence corpus must contain EXACTLY the frozen gap cases"
    );
}
