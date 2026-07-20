// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Family 2/6a/7 acceptance: the native counting / arithmetic-feasibility
//! refutation sub-decider DECIDES the committed W3C OWL 2 Full divergence slugs it
//! now covers, matching the W3C published verdict EXACTLY.
//!
//! Each slug's `input.nq` is run through the SAME `dl_consistency` path the
//! grader/runner uses. The native token — `incomplete` when a construct is
//! undecided (a non-empty `gaps`), otherwise the consistency boolean — must equal
//! the W3C ground truth. These cases were `native_verdict = "incomplete"` before
//! the counting sub-decider; it now decides them soundly and completely (an empty
//! `gaps` plus the correct consistency), so the token is the W3C verdict.
//!
//! Cases the sub-decider leaves WITHHELD are deliberately NOT listed (soundness over
//! coverage): `one-two` and `webont-description-logic-035` mix the counting shape
//! with existential/nominal/property-chain constructs the fragment does not fold in
//! (full-DL SAT), and `rolechainviolationlumen` carries a non-binary
//! `owl:propertyChainAxiom` gap outside every counting family. They stay
//! `incomplete` — an honest boundary, never a forced verdict.

use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// The committed W3C-divergence slugs the counting sub-decider now DECIDES, with the
/// W3C published verdict each must reproduce.
const DECIDED: &[(&str, &str)] = &[
    // Family 2 — number/cardinality counting (pure class-definition cardinality).
    ("webont-cardinality-002", "consistent"),
    ("webont-cardinality-003", "consistent"),
    ("webont-cardinality-004", "consistent"),
    ("owl2-rl-valid-mincard", "consistent"),
    // Family 6a — inverse-functional / functional identity collapse.
    ("webont-inversefunctionalproperty-001", "consistent"),
    ("webont-inversefunctionalproperty-002", "consistent"),
    ("webont-inversefunctionalproperty-003", "consistent"),
    ("rdfbased-sem-char-inversefunc-inst", "consistent"),
    ("rdfbased-sem-char-inversefunc-data", "consistent"),
    ("owl2-rl-rules-ifp-differentfrom", "consistent"),
    ("owl2-rl-rules-ifp-askey", "consistent"),
    // Family 7 — owl:hasSelf membership refutation.
    ("footnote-not-about-self", "inconsistent"),
];

/// Resolve a slug's `input.nq`, looking in the `w3c-owl2-full-decided` corpus
/// first (the relocated now-decided cases) and falling back to the sibling
/// `w3c-owl2-full-divergence` corpus. The two corpora partition the original
/// W3C-full set, so exactly one holds the slug.
fn case_input(slug: &str) -> String {
    let decided = format!(
        "{}/../../conformance/logic/cases/external/w3c-owl2-full-decided/{slug}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    );
    if std::path::Path::new(&decided).is_file() {
        return decided;
    }
    format!(
        "{}/../../conformance/logic/cases/external/w3c-owl2-full-divergence/{slug}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn native_token(slug: &str) -> String {
    let path = case_input(slug);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {path}: {e}"));
    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .unwrap_or_else(|e| panic!("dl_consistency on {slug}: {e}"));
    if !verdict.gaps.is_empty() {
        "incomplete".to_owned()
    } else if verdict.consistent {
        "consistent".to_owned()
    } else {
        "inconsistent".to_owned()
    }
}

#[test]
fn counting_decides_the_divergence_slugs_matching_w3c() {
    let mut failures = Vec::new();
    for (slug, expected) in DECIDED {
        let token = native_token(slug);
        if token != *expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, W3C published {expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "counting acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

/// The complex counting cases the sub-decider deliberately WITHHOLDS stay
/// `incomplete` — an honest boundary, never a wrong `consistent`/`inconsistent`.
#[test]
fn withheld_counting_cases_stay_incomplete() {
    for slug in [
        "one-two",
        "webont-description-logic-035",
        "rolechainviolationlumen",
    ] {
        assert_eq!(
            native_token(slug),
            "incomplete",
            "{slug} must stay an honest boundary (withheld), not a forced verdict"
        );
    }
}
