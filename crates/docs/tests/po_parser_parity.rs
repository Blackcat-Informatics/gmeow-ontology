// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pre-deletion golden sequencing for the PO-parser unification.
//!
//! Two fuzzy-blind PO parsers once existed in `gmeow-docs`: the fuzzy-aware survivor
//! `i18n_compile::parse_po` and the inferior `i18n::parse_po`. Before deleting the
//! loser, a `reviewed_coverage_survivor_matches_loser` differential proved — over
//! EVERY live slice catalog — that the reviewed translation-coverage set was
//! identical whether measured through the loser or the survivor. That was the
//! executable proof that the deletion changes no measured coverage; it has served
//! its purpose and is removed now that the loser is gone (a survivor-vs-survivor
//! re-run would be a tautology).
//!
//! The durable pin compares authenticated producer observations to the frozen
//! golden. Tests never rediscover or reparse the repository translation corpus.

#[path = "support/reviewed_coverage.rs"]
mod reviewed_coverage;

use reviewed_coverage::{golden_path, repo_root};

fn observation() -> gmeow_docs_model::reviewed_coverage::ReviewedCoverageObservation {
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &repo_root(),
        "stage-conformance",
        "pipeline/reviewed-translation-observation.json",
    )
    .expect("authenticated translation observation");
    let observation: Result<_, gmeow_errors::RecordedDiag> =
        serde_json::from_slice(&bytes).expect("typed translation observation");
    observation.expect("producer parsed every declared translation catalog")
}

/// ENH-A: every live slice PO, every `ontology-docs-templates.*.po`, and the two
/// fixtures parse cleanly under the survivor.
#[test]
fn corpus_all_live_po_files_parse_under_survivor() {
    let observation = observation();
    assert!(observation.reviewed_coverage.len() > 50);
    for path in observation.reviewed_coverage.keys() {
        assert!(observation.parsed_catalogs.contains_key(path), "{path}");
    }
    for path in ["tests/fixtures/i18n/fr.po", "tests/fixtures/i18n/zh.po"] {
        assert!(observation.parsed_catalogs.contains_key(path), "{path}");
    }
}

/// ENH-B pin (survives deletion): the survivor reviewed-coverage map equals the
/// checked-in golden. The test is strictly read-only.
#[test]
fn corpus_reviewed_coverage_matches_frozen_golden() {
    let computed = observation().reviewed_coverage;
    let path = golden_path();

    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}; run `make maint-refresh-reviewed-coverage-golden`",
            path.display()
        )
    });
    let golden: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&text).expect("parse golden JSON");
    assert_eq!(
        golden, computed,
        "survivor reviewed-coverage drifted from the frozen golden"
    );
}
