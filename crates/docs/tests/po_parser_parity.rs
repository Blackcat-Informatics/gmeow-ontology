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
//! The durable pin is `reviewed_coverage_matches_frozen_golden`, which recomputes
//! the survivor set and asserts it against a checked-in golden.

use std::path::{Path, PathBuf};

#[path = "support/reviewed_coverage.rs"]
mod reviewed_coverage;

/// The repo root: `crates/docs` → `../..`, canonicalized.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// ENH-A: every live slice PO, every `ontology-docs-templates.*.po`, and the two
/// fixtures parse cleanly under the survivor.
#[test]
fn all_live_po_files_parse_under_survivor() {
    let root = repo_root();
    reviewed_coverage::assert_all_catalogs_parse(&root);
}

/// The frozen golden path (a `BTreeMap<slice-rel po path, sorted reviewed keys>`).
fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reviewed_coverage_golden.json")
}

/// ENH-B pin (survives deletion): the survivor reviewed-coverage map equals the
/// checked-in golden. The test is strictly read-only.
#[test]
fn reviewed_coverage_matches_frozen_golden() {
    let root = repo_root();
    let computed = reviewed_coverage::reviewed_coverage_map(&root);
    let path = golden_path();

    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}; refresh with `make maint-refresh-reviewed-coverage-golden`",
            path.display()
        )
    });
    let golden: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&text).expect("parse golden JSON");
    assert_eq!(
        golden, computed,
        "survivor reviewed-coverage drifted from the frozen golden; inspect the delta, then \
         refresh with `make maint-refresh-reviewed-coverage-golden`"
    );
}
