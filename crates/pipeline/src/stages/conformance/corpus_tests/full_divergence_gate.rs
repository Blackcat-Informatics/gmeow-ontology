// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The W3C OWL 2 Full soundness-divergence SCALING gate.
//!
//! `w3c-owl2-full-divergence` vendors the 122 cases where OWL DL and OWL Full
//! diverge AND the native refutation kernel still honestly cannot decide them (a
//! non-empty `DlVerdict::gaps`, frozen as `native_verdict = "incomplete"` in each
//! case's `profile.json`), while W3C published a decided `consistent` /
//! `inconsistent` verdict under OWL Full semantics the native path does not
//! implement. (The 32 cases the kernel now DECIDES soundly were relocated to the
//! sibling `w3c-owl2-full-native` corpus, guarded by `full_native_gate`; the two
//! corpora together partition the original 154-case W3C-full set.) The corpus's
//! frozen `expected/verdicts.json` goldens record that honest gap — but a frozen
//! golden alone does not protect against a *future* reasoner change that silently
//! turns one of these honest gaps into a WRONG decided verdict (e.g. deciding
//! `consistent` for a case W3C published as `inconsistent`). Nothing else
//! re-executes the live reasoner over all 122 cases on every run, so that
//! regression would slip through unnoticed.
//!
//! An explicit O3/full-LTO producer observes every committed input once and
//! publishes the complete verdicts under an authenticated fixture selection.
//! These tests read that selection without invoking the native engine. A failed
//! execution cannot become an `incomplete` verdict: only identified native gaps
//! can produce that token.
//!
//! Two invariants are enforced:
//!
//! 1. **Soundness (non-negotiable):** the native token must NEVER contradict
//!    the W3C published verdict. `incomplete` is always sound (an honest "I
//!    cannot decide this"); a decided token that disagrees with W3C is not.
//! 2. **Drift pin (scaling):** every case's native token today is EXACTLY
//!    `incomplete`, matching both the committed `expected/verdicts.json`
//!    world status and the `profile.json` `native_verdict`. A regression that
//!    flips this — in either the reasoner or the frozen goldens — is caught
//!    here rather than discovered downstream.
//!
//! A coverage-floor test guards against the corpus being silently emptied,
//! and pins the corpus's W3C provenance and the non-degeneracy of the
//! published-verdict split (both `consistent` and `inconsistent` published
//! verdicts must be represented, so the soundness test actually exercises
//! both contradiction directions).

use std::path::Path;

use super::common::{case_slugs, divergence_root};

fn native_token(path: &Path) -> String {
    static OBSERVATIONS: std::sync::OnceLock<super::super::heavy::Observations> =
        std::sync::OnceLock::new();
    let root = gmeow_conformance::paths::repo_root();
    let observations = OBSERVATIONS.get_or_init(|| {
        super::super::heavy::load(&root).expect("authenticated exhaustive observations")
    });
    let key = path
        .strip_prefix(&root)
        .expect("corpus input in checkout")
        .to_string_lossy();
    observations
        .consistency
        .get(key.as_ref())
        .expect("selected case observation")
        .as_ref()
        .expect("native execution succeeded")
        .token()
        .to_owned()
}

/// The minimum number of vendored cases the corpus must retain. A floor, not
/// an exact pin: legitimate future additions still pass; deletion/emptying
/// fails. Lowered from 154 to 122 when the 32 now-decided cases were relocated
/// to the sibling `w3c-owl2-full-native` corpus; the disjoint-union partition
/// (122 + 32 == 154) is pinned by `full_native_gate`.
const MIN_CASE_COUNT: usize = 122;

/// Read and parse a case's `profile.json`.
fn read_profile(case: &Path) -> serde_json::Value {
    let path = case.join("profile.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Read and parse a case's `expected/verdicts.json`, returning the single
/// world's `status` string.
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

/// THE non-negotiable soundness invariant: for every one of the 122 remaining
/// divergence cases,
/// the live native reasoner's verdict on the committed `input.nq` must NEVER
/// contradict the W3C published verdict recorded in `profile.json`. An
/// `incomplete` native token is always sound — an honest "cannot decide". A
/// wrong DECIDED verdict (`consistent` where W3C published `inconsistent`, or
/// vice versa) is the exact class of regression this gate exists to catch: a
/// future reasoner change turning an honest gap into a confidently wrong
/// answer. All violations are collected and reported together, not just the
/// first.
///
/// The exhaustive `maint-heavy` lane consumes the separately produced breadth
/// observations. The coverage-floor test below remains on the default gate.
#[test]
fn never_a_wrong_decided_verdict_heavy_offgate() {
    let cases = case_slugs(&divergence_root());
    let mut failures: Vec<String> = Vec::new();
    for (slug, case) in &cases {
        let profile = read_profile(case);
        let published = profile["w3c_published_verdict"]
            .as_str()
            .unwrap_or_else(|| panic!("{slug}: profile.json missing w3c_published_verdict"));
        let native = native_token(&case.join("input.nq"));
        let contradicts = (native == "consistent" && published == "inconsistent")
            || (native == "inconsistent" && published == "consistent");
        if contradicts {
            failures.push(format!(
                "{slug}: native decided {native:?} but W3C published {published:?} — a WRONG \
                 decided verdict, unsound"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "w3c-owl2-full-divergence SOUNDNESS violation(s) — native reasoner decided a verdict \
         that CONTRADICTS the W3C published verdict:\n  • {}",
        failures.join("\n  • ")
    );
}

/// Drift pin (scaling): for every one of the 122 remaining divergence cases, the
/// live native
/// reasoner reproduces the frozen honest gap EXACTLY — `native_token ==
/// "incomplete"`, matching both the committed `expected/verdicts.json` world
/// status and the `profile.json` `native_verdict`. This catches a reasoner
/// change that silently flips a frozen honest gap into a decided verdict (a
/// regression this gate must catch even when the newly-decided verdict
/// happens to agree with W3C — deciding what was once an honest gap without a
/// deliberate implementation of OWL Full semantics is drift, not progress),
/// or the committed golden/profile going stale. Frozen values are read from
/// each case's OWN committed files — nothing is hardcoded here.
///
/// Like the soundness test, this consumes the explicit breadth producer output
/// in the `maint-heavy` lane.
#[test]
fn native_reproduces_the_frozen_gap_heavy_offgate() {
    let cases = case_slugs(&divergence_root());
    let mut failures: Vec<String> = Vec::new();
    for (slug, case) in &cases {
        let native = native_token(&case.join("input.nq"));
        let profile = read_profile(case);
        let frozen_native = profile["native_verdict"]
            .as_str()
            .unwrap_or_else(|| panic!("{slug}: profile.json missing native_verdict"));
        let expected_status = read_expected_status(case, slug);

        if native != "incomplete" {
            failures.push(format!(
                "{slug}: native decided {native:?}, expected the frozen honest gap \
                 \"incomplete\""
            ));
            continue;
        }
        if frozen_native != "incomplete" {
            failures.push(format!(
                "{slug}: profile.json native_verdict is {frozen_native:?}, expected \
                 \"incomplete\""
            ));
        }
        if expected_status != "incomplete" {
            failures.push(format!(
                "{slug}: expected/verdicts.json world status is {expected_status:?}, expected \
                 \"incomplete\""
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "w3c-owl2-full-divergence drift — the native reasoner or a committed golden no longer \
         agrees on the frozen honest gap:\n  • {}",
        failures.join("\n  • ")
    );
}

/// Coverage floor + provenance pin: the corpus must retain at least
/// [`MIN_CASE_COUNT`] cases (a floor, not an exact pin, so legitimate
/// additions still pass while deletion/emptying fails), `corpus.json` must
/// pin `lane == "divergence"`, the W3C SPDX license, and a non-empty
/// `source_url` / `version_or_commit`. The published-verdict split must be
/// non-degenerate — both `consistent` and `inconsistent` published verdicts
/// represented — so [`never_a_wrong_decided_verdict_heavy_offgate`] actually exercises both
/// contradiction directions.
#[test]
fn full_divergence_corpus_meets_its_coverage_floor() {
    let cases = case_slugs(&divergence_root());
    assert!(
        cases.len() >= MIN_CASE_COUNT,
        "w3c-owl2-full-divergence corpus has only {} cases, below the coverage floor of {}",
        cases.len(),
        MIN_CASE_COUNT
    );

    let corpus_json_path = divergence_root().join("corpus.json");
    let corpus_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&corpus_json_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", corpus_json_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", corpus_json_path.display()));

    assert_eq!(
        corpus_json["lane"].as_str(),
        Some("divergence"),
        "corpus.json must pin lane == \"divergence\""
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
        let profile = read_profile(case);
        match profile["w3c_published_verdict"].as_str() {
            Some("consistent") => has_consistent = true,
            Some("inconsistent") => has_inconsistent = true,
            other => panic!(
                "{slug}: profile.json w3c_published_verdict must be \"consistent\" or \
                 \"inconsistent\", got {other:?}"
            ),
        }
    }
    assert!(
        has_consistent && has_inconsistent,
        "the published-verdict split must be non-degenerate: both \"consistent\" and \
         \"inconsistent\" W3C published verdicts must be represented (found consistent={}, \
         inconsistent={})",
        has_consistent,
        has_inconsistent
    );
}
