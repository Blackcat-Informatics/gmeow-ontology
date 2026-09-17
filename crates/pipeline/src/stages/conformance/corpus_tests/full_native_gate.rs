// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact operation-partitioned W3C native inventory. Thirty cases assert semantic
//! agreement; two preserve bare list data while observing that no source owner
//! selects the native class-expression/list grammar. Their W3C published verdicts
//! remain independent source attribution. All native work is producer-owned.

use std::path::Path;

use super::common::{
    case_slugs, divergence_root, native_admission, native_full_root, native_token,
};

/// Exact semantic-decision partition of this authored native operation inventory.
const SEMANTIC_CASES: usize = 30;
const ADMISSION_CASES: [&str; 2] = ["webont-i5-5-003", "webont-i5-5-004"];

/// The exact size of the original W3C OWL 2 Full divergence set the two sibling
/// corpora partition (`divergence` ∪ `native-profiled`). Frozen: a deliberate reasoner
/// change moves a slug across the partition, but the union size is invariant.
const ORIGINAL_FULL_SET: usize = 154;

/// A representative cross-family subset read from authenticated producer output. Each
/// entry is `(slug, expected decided token)`, spanning both verdict directions
/// and the datatype (5), counting (2), inverse-functional (6a), hasSelf (7),
/// and union/disjoint (3) refutation families.
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
            "{slug}: native returned an honest gap (incomplete) — a semantic-profile \
             case MUST decide"
        ));
    }
    let published = published_verdict(case, slug);
    if native != published {
        return Some(format!(
            "{slug}: native decided {native:?} but W3C published {published:?} — a \
             semantic-profile case MUST agree with W3C"
        ));
    }
    let frozen_native = read_profile(case)["native_verdict"]
        .as_str()
        .unwrap_or_else(|| panic!("{slug}: profile.json missing native_verdict"))
        .to_owned();
    if frozen_native != native {
        return Some(format!(
            "{slug}: profile.json native_verdict is {frozen_native:?}, authenticated producer \
             decided {native:?}"
        ));
    }
    let golden = read_expected_status(case, slug);
    if golden != native {
        return Some(format!(
            "{slug}: expected/verdicts.json world status is {golden:?}, authenticated producer \
             decided {native:?}"
        ));
    }
    if let Some(want) = expected
        && native != want
    {
        return Some(format!(
            "{slug}: representative expectation {want:?}, authenticated producer decided \
             {native:?}"
        ));
    }
    None
}

/// The authenticated semantic-profile representatives each agree with W3C and
/// both committed goldens. This consumer performs no source reasoning.
#[test]
fn representative_decided_cases_agree_with_w3c() {
    let cases = case_slugs(&native_full_root());
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
        "w3c-owl2-full-native representative acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Invariant 1, complete required inventory: every decided case agrees with W3C
/// and both frozen verdicts. The producer has already run the native operation;
/// this consumer reads the same observation as the representative-family checks.
#[test]
fn every_decided_case_agrees_with_w3c() {
    let cases = case_slugs(&native_full_root());
    let mut failures: Vec<String> = Vec::new();
    for (slug, case) in &cases {
        if ADMISSION_CASES.contains(&slug.as_str()) {
            continue;
        }
        if let Some(f) = check_decided(slug, case, None) {
            failures.push(f);
        }
    }
    assert!(
        failures.is_empty(),
        "w3c-owl2-full-native whole-corpus acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// Invariant 2: exact semantic/admission inventory and provenance. The corpus retains
/// [`SEMANTIC_CASES`] semantic decisions plus both source-admission observations; `corpus.json` pins `lane == "native-profiled"`, the W3C SPDX
/// license, and non-empty `source_url` / `version_or_commit`. The
/// published-verdict split is non-degenerate — both `consistent` and
/// `inconsistent` are represented — so the acceptance sweeps exercise both
/// directions.
#[test]
fn native_corpus_preserves_its_exact_inventory_and_provenance() {
    let cases = case_slugs(&native_full_root());
    assert_eq!(
        cases.len(),
        SEMANTIC_CASES + ADMISSION_CASES.len(),
        "native corpus must retain its complete semantic/admission partition"
    );

    let corpus_json_path = native_full_root().join("corpus.json");
    let corpus_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&corpus_json_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", corpus_json_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", corpus_json_path.display()));

    assert_eq!(
        corpus_json["lane"].as_str(),
        Some("native-profiled"),
        "corpus.json must pin lane == \"native-profiled\""
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

/// Invariant 3: the `divergence` / `native-profiled` partition. The two slug sets are
/// DISJOINT (no slug appears in both) and their union is EXACTLY the original
/// [`ORIGINAL_FULL_SET`]-case W3C-full set. This is the guard that a case can
/// migrate across the partition (a deliberate reasoner-capability change that
/// updates both corpora) but can never be silently dropped or double-counted.
#[test]
fn divergence_and_native_profiles_partition_the_original_full_set() {
    let divergence: std::collections::BTreeSet<String> =
        case_slugs(&divergence_root()).into_keys().collect();
    let native: std::collections::BTreeSet<String> =
        case_slugs(&native_full_root()).into_keys().collect();

    let intersection: Vec<&String> = divergence.intersection(&native).collect();
    assert!(
        intersection.is_empty(),
        "the divergence and native-profiled corpora must be DISJOINT, but these slugs appear \
         in both: {intersection:?}"
    );

    let union = divergence.len() + native.len();
    assert_eq!(
        union,
        ORIGINAL_FULL_SET,
        "divergence ({}) + native-profiled ({}) must partition the original {}-case W3C-full \
         set exactly, got a union of {}",
        divergence.len(),
        native.len(),
        ORIGINAL_FULL_SET,
        union
    );
}

/// Every original case belongs to exactly one selected operation, with no lost input.
#[test]
fn source_admission_cases_are_exactly_partitioned_and_preserve_external_attribution() {
    let cases = case_slugs(&native_full_root());
    let mut semantic = std::collections::BTreeSet::new();
    let mut admission = std::collections::BTreeSet::new();
    for (slug, case) in &cases {
        let profile = read_profile(case);
        let typed = gmeow_conformance::profile::parse_profile(slug, &profile)
            .expect("explicit case operation");
        match typed.verdict_mode {
            gmeow_conformance::profile::VerdictMode::Consistency => {
                semantic.insert(slug.as_str());
            }
            gmeow_conformance::profile::VerdictMode::ClassSourceAdmission => {
                admission.insert(slug.as_str());
                assert!(
                    profile.get("native_verdict").is_none(),
                    "source admission cannot claim consistency"
                );
                assert_eq!(profile["w3c_published_verdict"], "inconsistent");
                assert_eq!(
                    profile["source_admission_contract"],
                    "nativeClassExpressionListAdmission"
                );
                assert_eq!(profile["source_admission_status"], "outside-selection");
                let observed = native_admission(&case.join("input.nq"));
                assert!(observed.admission.outside_selection());
                assert_eq!(observed.admission.source_worlds.len(), 1);
                let graph = format!("https://gmeow.example/w3c-owl2-full/{slug}/w");
                let world = &observed.admission.source_worlds[&graph];
                assert_eq!(world.graph, Some(purrdf::TermValue::iri(graph)));
                assert_eq!(world.assertions, 1);
                let expected: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(case.join("expected/verdicts.json")).expect("admission golden"),
                )
                .expect("golden JSON");
                assert_eq!(serde_json::to_value(&observed.admission).unwrap(), expected);
            }
            other => panic!("unexpected native inventory operation {other:?}"),
        }
    }
    assert_eq!(semantic.len(), SEMANTIC_CASES);
    assert_eq!(admission, ADMISSION_CASES.into_iter().collect());
    assert!(semantic.is_disjoint(&admission));
    assert_eq!(semantic.len() + admission.len(), cases.len());
}
