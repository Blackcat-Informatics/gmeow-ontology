// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The W3C OWL 2 Full "was-divergent, now-DECIDED" acceptance gate.
//!
//! `w3c-owl2-full-decided` vendors the 32 cases that were once honest DL gaps in
//! the sibling `w3c-owl2-full-divergence` corpus but that the native refutation
//! kernel (`crates/logic/src/reason/refute.rs` + its datatype / counting /
//! case-split sub-deciders) now DECIDES soundly: the native `dl_consistency`
//! verdict is a clean `consistent` / `inconsistent` (an EMPTY `DlVerdict::gaps`)
//! that AGREES with the W3C published verdict. Each case's committed
//! `profile.json` freezes `native_verdict == w3c_published_verdict`, and
//! `expected/verdicts.json` records the same decided token.
//!
//! This gate is the single live-re-run authority for that corpus (the generic
//! per-case consistency harness skips the `decided` lane, mirroring how the
//! `divergence` lane is owned by its gate). It mirrors `el_divergence_gate.rs`'s
//! structure and enforces three invariants:
//!
//! 1. **Decided ∧ agreeing** — re-running each committed `input.nq` through the
//!    exact `dl_consistency` path the grader/runner uses yields a DECIDED token
//!    (never `incomplete`) that equals the W3C published verdict AND the two
//!    committed goldens. A representative cross-family subset runs on the default
//!    gate; the whole 32-case corpus runs off-gate (`_heavy_offgate`).
//! 2. **Coverage floor** — the corpus must retain at least [`DECIDED_FLOOR`]
//!    cases (so it can never be silently emptied), and `corpus.json` must pin
//!    `lane == "decided"` with the W3C provenance.
//! 3. **Partition pin** — the `divergence` and `decided` slug sets are DISJOINT
//!    and their union is EXACTLY the original 154-case W3C-full set. A case may
//!    move between the two corpora (a deliberate reasoner-capability change) but
//!    can never be dropped or double-counted silently.
//!
//! It is offline, deterministic, and — for the representative subset and the
//! partition/floor pins — sub-second.

use std::path::Path;

mod common;

use common::{case_slugs, decided_root, divergence_root, native_token};

/// The minimum number of decided cases the corpus must retain — a floor, not an
/// exact pin, so a future case that migrates *into* the decided corpus (the
/// kernel learning to decide it) still passes while deletion/emptying fails.
const DECIDED_FLOOR: usize = 32;

/// The exact size of the original W3C OWL 2 Full divergence set the two sibling
/// corpora partition (`divergence` ∪ `decided`). Frozen: a deliberate reasoner
/// change moves a slug across the partition, but the union size is invariant.
const ORIGINAL_FULL_SET: usize = 154;

/// A representative cross-family subset re-run on the DEFAULT gate (fast). Each
/// entry is `(slug, expected decided token)`, spanning both verdict directions
/// and the datatype (5), counting (2), inverse-functional (6a), hasSelf (7),
/// malformed-list (6b), and union/disjoint (3) refutation families.
const REPRESENTATIVE: &[(&str, &str)] = &[
    // Family 5 — datatype value-space refutation.
    ("datatype-float-discrete-001", "inconsistent"),
    ("new-feature-rational-002", "inconsistent"),
    // Family 2 — cardinality counting.
    ("webont-cardinality-002", "consistent"),
    // Family 6a — inverse-functional identity collapse.
    ("webont-inversefunctionalproperty-001", "consistent"),
    // Family 7 — owl:hasSelf membership refutation.
    ("footnote-not-about-self", "inconsistent"),
    // Family 6b — malformed rdf:List.
    ("webont-i5-5-003", "inconsistent"),
    // Family 3 — union + disjoint propositional refutation.
    ("webont-description-logic-504", "inconsistent"),
    ("new-feature-disjointunion-001", "consistent"),
];

/// Read and parse a case's `profile.json`.
fn read_profile(case: &Path) -> serde_json::Value {
    let path = case.join("profile.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// The single world's `status` string in a case's `expected/verdicts.json`.
fn read_expected_status(case: &Path, slug: &str) -> String {
    let path = case.join("expected").join("verdicts.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let verdicts: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    verdicts
        .as_object()
        .and_then(|o| o.values().next())
        .and_then(|w| w["status"].as_str())
        .unwrap_or_else(|| panic!("{slug}: expected/verdicts.json has no world status"))
        .to_owned()
}

/// The W3C published verdict frozen in a case's `profile.json`.
fn published_verdict(case: &Path, slug: &str) -> String {
    read_profile(case)["w3c_published_verdict"]
        .as_str()
        .unwrap_or_else(|| panic!("{slug}: profile.json missing w3c_published_verdict"))
        .to_owned()
}

/// Assert one decided case decides, and agrees with W3C and both committed
/// goldens. Returns a failure string, or `None` when the case is sound.
fn check_decided(slug: &str, case: &Path, expected: Option<&str>) -> Option<String> {
    let native = native_token(&case.join("input.nq"));
    if native == "incomplete" {
        return Some(format!(
            "{slug}: native returned an honest gap (incomplete) — a decided-corpus \
             case MUST decide"
        ));
    }
    let published = published_verdict(case, slug);
    if native != published {
        return Some(format!(
            "{slug}: native decided {native:?} but W3C published {published:?} — a \
             decided-corpus case MUST agree with W3C"
        ));
    }
    let frozen_native = read_profile(case)["native_verdict"]
        .as_str()
        .unwrap_or_else(|| panic!("{slug}: profile.json missing native_verdict"))
        .to_owned();
    if frozen_native != native {
        return Some(format!(
            "{slug}: profile.json native_verdict is {frozen_native:?}, live reasoner \
             decided {native:?}"
        ));
    }
    let golden = read_expected_status(case, slug);
    if golden != native {
        return Some(format!(
            "{slug}: expected/verdicts.json world status is {golden:?}, live reasoner \
             decided {native:?}"
        ));
    }
    if let Some(want) = expected
        && native != want
    {
        return Some(format!(
            "{slug}: representative expectation {want:?}, live reasoner decided \
             {native:?}"
        ));
    }
    None
}

/// Invariant 1 (default gate): a representative cross-family subset of decided
/// cases each DECIDES and agrees with W3C + both committed goldens. Fast and
/// sub-second — the on-gate proof that the relocation is live-sound.
#[test]
fn representative_decided_cases_agree_with_w3c() {
    let cases = case_slugs(&decided_root());
    let mut failures: Vec<String> = Vec::new();
    for (slug, expected) in REPRESENTATIVE {
        let Some(case) = cases.get(*slug) else {
            failures.push(format!(
                "{slug}: representative decided case missing from the corpus"
            ));
            continue;
        };
        if let Some(f) = check_decided(slug, case, Some(expected)) {
            failures.push(f);
        }
    }
    assert!(
        failures.is_empty(),
        "w3c-owl2-full-decided representative acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Invariant 1 (off-gate, whole corpus): EVERY decided case decides and agrees
/// with W3C + both committed goldens.
///
/// Off-gate (`_heavy_offgate`): re-runs the live reasoner over all decided cases,
/// so in a debug build it exceeds the default nextest slow-timeout backstop. It
/// runs in the exhaustive `maint-heavy` lane, alongside the other whole-corpus
/// W3C conformance proofs; the representative subset above stays on the gate.
#[test]
fn every_decided_case_agrees_with_w3c_heavy_offgate() {
    let cases = case_slugs(&decided_root());
    let mut failures: Vec<String> = Vec::new();
    for (slug, case) in &cases {
        if let Some(f) = check_decided(slug, case, None) {
            failures.push(f);
        }
    }
    assert!(
        failures.is_empty(),
        "w3c-owl2-full-decided whole-corpus acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Invariant 2: the coverage floor + provenance pin. The corpus retains at least
/// [`DECIDED_FLOOR`] cases; `corpus.json` pins `lane == "decided"`, the W3C SPDX
/// license, and non-empty `source_url` / `version_or_commit`. The
/// published-verdict split is non-degenerate — both `consistent` and
/// `inconsistent` are represented — so the acceptance sweeps exercise both
/// directions.
#[test]
fn decided_corpus_meets_its_coverage_floor() {
    let cases = case_slugs(&decided_root());
    assert!(
        cases.len() >= DECIDED_FLOOR,
        "w3c-owl2-full-decided corpus has only {} cases, below the coverage floor of {}",
        cases.len(),
        DECIDED_FLOOR
    );

    let corpus_json_path = decided_root().join("corpus.json");
    let corpus_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&corpus_json_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", corpus_json_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", corpus_json_path.display()));

    assert_eq!(
        corpus_json["lane"].as_str(),
        Some("decided"),
        "corpus.json must pin lane == \"decided\""
    );
    assert_eq!(
        corpus_json["spdx_license"].as_str(),
        Some("W3C"),
        "corpus.json must pin the W3C SPDX license"
    );
    assert!(
        corpus_json["source_url"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "corpus.json must pin a non-empty source_url"
    );
    assert!(
        corpus_json["version_or_commit"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "corpus.json must pin a non-empty version_or_commit"
    );

    let mut has_consistent = false;
    let mut has_inconsistent = false;
    for (slug, case) in &cases {
        match published_verdict(case, slug).as_str() {
            "consistent" => has_consistent = true,
            "inconsistent" => has_inconsistent = true,
            other => panic!(
                "{slug}: profile.json w3c_published_verdict must be \"consistent\" or \
                 \"inconsistent\", got {other:?}"
            ),
        }
    }
    assert!(
        has_consistent && has_inconsistent,
        "the decided published-verdict split must be non-degenerate: both \
         \"consistent\" and \"inconsistent\" must be represented (found consistent={}, \
         inconsistent={})",
        has_consistent,
        has_inconsistent
    );
}

/// Invariant 3: the `divergence` / `decided` partition. The two slug sets are
/// DISJOINT (no slug appears in both) and their union is EXACTLY the original
/// [`ORIGINAL_FULL_SET`]-case W3C-full set. This is the guard that a case can
/// migrate across the partition (a deliberate reasoner-capability change that
/// updates both corpora) but can never be silently dropped or double-counted.
#[test]
fn divergence_and_decided_partition_the_original_full_set() {
    let divergence: std::collections::BTreeSet<String> =
        case_slugs(&divergence_root()).into_keys().collect();
    let decided: std::collections::BTreeSet<String> =
        case_slugs(&decided_root()).into_keys().collect();

    let intersection: Vec<&String> = divergence.intersection(&decided).collect();
    assert!(
        intersection.is_empty(),
        "the divergence and decided corpora must be DISJOINT, but these slugs appear \
         in both: {intersection:?}"
    );

    let union = divergence.len() + decided.len();
    assert_eq!(
        union,
        ORIGINAL_FULL_SET,
        "divergence ({}) + decided ({}) must partition the original {}-case W3C-full \
         set exactly, got a union of {}",
        divergence.len(),
        decided.len(),
        ORIGINAL_FULL_SET,
        union
    );
}
