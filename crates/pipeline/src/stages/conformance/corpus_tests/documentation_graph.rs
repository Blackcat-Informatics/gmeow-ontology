// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated declarations complete the docs coverage key and maturity join.

use gmeow_docs::coverage::{DIMENSIONS, SLICE_DIMENSIONS};
use gmeow_docs::maturity::Dimension;
use std::collections::BTreeSet;

#[test]
fn every_coverage_key_maps_to_a_declared_doc_coverage_dimension_individual() {
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &gmeow_conformance::paths::repo_root(),
        "stage-conformance",
        super::super::documentation_graph::CHANNEL,
    )
    .expect("authenticated original documentation declarations");
    let declared: BTreeSet<String> = serde_json::from_slice(&bytes)
        .expect("typed documentation coverage dimension declarations");
    // Non-vacuity: the eighteen dimensions must genuinely be declared.
    assert_eq!(
        declared.len(),
        Dimension::ALL.len(),
        "expected {} gmeow:DocCoverageDimension individuals, found {}: {declared:?}",
        Dimension::ALL.len(),
        declared.len(),
    );
    // Every coverage key's dimension local name is declared.
    for cd in DIMENSIONS.iter().chain(SLICE_DIMENSIONS.iter()) {
        let local = cd.dimension.local_name();
        assert!(
            declared.contains(local),
            "coverage key `{}` → dimension `{local}` has no gmeow:DocCoverageDimension declaration",
            cd.key
        );
    }
}
