// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The W3C OWL 2 EL soundness-divergence regression gate.
//!
//! This pins, exactly, the eight W3C OWL 2 EL cases that once made the native DL
//! reasoner UNSOUND — it returned a wrong `consistent` where the W3C published
//! expected verdict is `inconsistent`. Those eight are now split into two frozen
//! sets, and this gate hard-fails on ANY drift in either:
//!
//! * **Agreeing soundness fixes** (seven of the eight) — the native reasoner now
//!   correctly returns `inconsistent`, matching W3C. A future change that
//!   re-introduces a wrong `consistent`, or flips one of these to `incomplete`,
//!   fails here. These are vendored in the Lane-A `w3c-owl2-el` corpus.
//!
//! * **Honest gaps** (`webont-thing-005`, plus the structurally-identical
//!   `webont-thing-004`) — `owl:Thing oneOf {singleton}` is consistent under OWL
//!   DL yet inconsistent under OWL Full; the native path cannot soundly decide
//!   the same syntactic shape both ways, so it admits an honest gap (`incomplete`,
//!   a non-empty `DlVerdict::gaps`) rather than a wrong decided answer. These are
//!   vendored in the `w3c-owl2-el-divergence` corpus.
//!
//! Soundness is the non-negotiable invariant: NONE of these may EVER be a wrong
//! `consistent`. The gate re-runs the committed `input.nq` of each vendored case
//! through `dl_consistency` and asserts the native verdict token exactly, then
//! asserts the precise membership of each set (so adding/removing a case is
//! caught). It is offline, deterministic, and sub-second (eight tiny consistency
//! checks).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::paths::cases_root;
use gmeow_rdf::{dataset_from_bytes, NativeRdfFormat};

/// The native verdict token for one case (`consistent` / `inconsistent` /
/// `incomplete`), computed exactly as the grader/runner does: a non-empty
/// `gaps` is `incomplete` (an honest "cannot decide"); otherwise the consistency
/// boolean.
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

/// The seven cases the native reasoner was made SOUND on — each now agrees with
/// W3C (`inconsistent`). Frozen by exact slug → expected native token.
fn agreeing_fixes() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("new-feature-bottomdataproperty-001", "inconsistent"),
        ("new-feature-bottomobjectproperty-001", "inconsistent"),
        ("new-feature-keys-002", "inconsistent"),
        ("new-feature-keys-006", "inconsistent"),
        (
            "new-feature-negativedatapropertyassertion-001",
            "inconsistent",
        ),
        (
            "new-feature-negativeobjectpropertyassertion-001",
            "inconsistent",
        ),
        ("webont-thing-003", "inconsistent"),
    ])
}

/// The cases the native reasoner honestly CANNOT decide — `owl:Thing` enumerated
/// to a singleton (DL/Full-divergent). Frozen native token: `incomplete`.
/// `webont-thing-005` is one of the original eight; `webont-thing-004` is the
/// structurally-identical sibling that becomes an honest gap by the same rule.
fn honest_gaps() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("webont-thing-004", "incomplete"),
        ("webont-thing-005", "incomplete"),
    ])
}

/// Every soundness-fix case lives in the Lane-A `w3c-owl2-el` corpus and the
/// native reasoner now returns EXACTLY the frozen `inconsistent` (never a wrong
/// `consistent`, never a regression to `incomplete`).
#[test]
fn agreeing_soundness_fixes_decide_inconsistent() {
    let corpus = external_root().join("w3c-owl2-el");
    let mut failures: Vec<String> = Vec::new();
    for (slug, expected) in agreeing_fixes() {
        let case = corpus.join(slug);
        assert!(
            case.is_dir(),
            "soundness-fix case missing from the vendored Lane-A corpus: {}",
            case.display()
        );
        let token = native_token(&case.join("input.nq"));
        if token != expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, frozen expectation is {expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "EL soundness-fix regression (a wrong/changed native verdict):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Every honest-gap case lives in the `w3c-owl2-el-divergence` corpus and the
/// native reasoner returns EXACTLY `incomplete` (a non-empty `gaps`) — NEVER a
/// wrong `consistent`. Its committed golden + profile carry the frozen native
/// verdict and the W3C published verdict as provenance.
#[test]
fn honest_gaps_stay_incomplete_never_a_wrong_consistent() {
    let corpus = external_root().join("w3c-owl2-el-divergence");
    let mut failures: Vec<String> = Vec::new();
    for (slug, expected) in honest_gaps() {
        let case = corpus.join(slug);
        assert!(
            case.is_dir(),
            "honest-gap case missing from the divergence corpus: {}",
            case.display()
        );
        let token = native_token(&case.join("input.nq"));
        // Soundness floor: an honest gap must NEVER become a wrong `consistent`.
        assert_ne!(
            token, "consistent",
            "{slug}: native returned a WRONG `consistent` for a case W3C declares inconsistent — \
             an unsound regression"
        );
        if token != expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, frozen expectation is {expected:?}"
            ));
        }

        // The committed golden + provenance must record the frozen native verdict
        // and the W3C published verdict.
        let verdicts: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(case.join("expected").join("verdicts.json"))
                .unwrap_or_else(|e| panic!("read verdicts.json for {slug}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse verdicts.json for {slug}: {e}"));
        let committed = verdicts
            .as_object()
            .and_then(|o| o.values().next())
            .and_then(|w| w["status"].as_str())
            .unwrap_or_else(|| panic!("{slug}: verdicts.json has no world status"));
        assert_eq!(
            committed, expected,
            "{slug}: committed golden status must be the frozen native token"
        );

        let profile: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(case.join("profile.json"))
                .unwrap_or_else(|e| panic!("read profile.json for {slug}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse profile.json for {slug}: {e}"));
        assert_eq!(
            profile["native_verdict"].as_str(),
            Some(expected),
            "{slug}: profile.json must freeze the native verdict"
        );
        assert!(
            profile["w3c_published_verdict"].is_string(),
            "{slug}: profile.json must carry the W3C published verdict as provenance"
        );
    }
    assert!(
        failures.is_empty(),
        "EL honest-gap regression (a changed native verdict):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Pin the EXACT divergence-set membership: the divergence corpus contains
/// PRECISELY the honest-gap cases (no more, no fewer). A future reasoner change
/// that flips an honest gap into a decided answer (or vice versa) must update
/// this gate deliberately — drift cannot slip through silently.
#[test]
fn divergence_set_membership_is_exact() {
    let corpus = external_root().join("w3c-owl2-el-divergence");
    assert!(
        corpus.is_dir(),
        "divergence corpus missing: {}",
        corpus.display()
    );
    let mut found: Vec<String> = std::fs::read_dir(&corpus)
        .unwrap_or_else(|e| panic!("read {}: {e}", corpus.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
        .collect();
    found.sort();
    let mut expected: Vec<String> = honest_gaps().keys().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the w3c-owl2-el-divergence corpus must contain EXACTLY the honest-gap cases"
    );
}
