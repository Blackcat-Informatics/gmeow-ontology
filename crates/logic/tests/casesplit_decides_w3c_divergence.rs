// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Families 1/3/6b (+ entangled Family 4) acceptance + soundness sweep: the native
//! bounded case-split / complement / union-disjoint / malformed-list refutation
//! sub-decider DECIDES the committed W3C OWL 2 Full divergence slugs its fragment
//! covers, matching the W3C published verdict EXACTLY — and NEVER contradicts W3C on
//! any case it now decides (proving `corpus_only` stays 0).
//!
//! Each slug's `input.nq` is run through the SAME `dl_consistency` path the
//! grader/runner uses. The native token — `incomplete` when a construct is undecided
//! (a non-empty `gaps`), otherwise the consistency boolean — must equal the W3C
//! ground truth.

use std::path::{Path, PathBuf};

use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// The committed W3C-divergence slugs the case-split sub-decider now DECIDES, with
/// the W3C published verdict each must reproduce. These are the NAMED Task-5 targets
/// its certified-complete fragment covers.
const DECIDED: &[(&str, &str)] = &[
    // Family 6b — malformed rdf:List.
    ("webont-i5-5-003", "inconsistent"),
    ("webont-i5-5-004", "inconsistent"),
    // Family 3 — union + disjoint / disjointUnion refutation.
    ("new-feature-disjointunion-001", "consistent"),
    // Family 3 — the pure propositional (unionOf × disjointWith) SAT pair: a single
    // named individual typed to a class whose superclasses are disjunctive clauses.
    ("webont-description-logic-504", "inconsistent"),
    ("webont-description-logic-503", "consistent"),
];

fn divergence_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/logic/cases/external/w3c-owl2-full-divergence")
}

fn native_token(slug: &str) -> String {
    let path = divergence_dir().join(slug).join("input.nq");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
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
fn casesplit_decides_the_named_divergence_slugs_matching_w3c() {
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
        "case-split acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

// The whole-corpus SOUNDNESS SWEEP — for every divergence case the case-split
// DECIDER now decides, the decided verdict must equal the W3C published verdict —
// lives IN-CRATE (`crates/logic/src/reason/refute/casesplit.rs`,
// `corpus_soundness_sweep_no_decider_contradicts_w3c`). It runs the decider
// DIRECTLY over every case, so it is fast and isolates the new engine's soundness
// WITHOUT invoking the native existential chase (which independently hangs on some
// heavy `owl:someValuesFrom`/cardinality/`inverseOf` corpus cases the case-split
// family never engages). This file's acceptance test above pins the end-to-end
// `dl_consistency` token on the NAMED decided slugs (a small, fast, hang-free set).

/// The complex Family-4 cases the sub-decider deliberately WITHHOLDS (their
/// inconsistency turns on existential/cardinality arithmetic outside the
/// propositional-plus-nominal fragment) stay `incomplete` — an honest boundary,
/// never a wrong verdict.
#[test]
fn entangled_cardinality_cases_stay_incomplete() {
    for slug in ["one-two", "webont-description-logic-035"] {
        assert_eq!(
            native_token(slug),
            "incomplete",
            "{slug} must stay an honest boundary (withheld), not a forced verdict"
        );
    }
}

/// The nominal-set-equality SAT pair (`webont-description-logic-501`/`502`) encodes
/// its (in)consistency in MULTIPLE `owl:oneOf` enumerations of one class (a
/// cross-enumeration set-equality the case-split consistent fragment does not
/// model). The decider soundly WITHHOLDS both rather than risk a false verdict — an
/// honest boundary. (The propositional `503`/`504` pair, which uses `owl:unionOf` ×
/// `owl:disjointWith` instead, IS decided; see `DECIDED`.)
#[test]
fn nominal_set_equality_cases_stay_incomplete() {
    for slug in [
        "webont-description-logic-501",
        "webont-description-logic-502",
    ] {
        assert_eq!(
            native_token(slug),
            "incomplete",
            "{slug} must stay an honest boundary (withheld nominal set-equality)"
        );
    }
}
