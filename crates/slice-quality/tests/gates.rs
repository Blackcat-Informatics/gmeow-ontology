// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The axis→producer binding gate and the projection-target completeness gate,
//! run against the real committed rubric.

use std::path::PathBuf;

use gmeow_slice_quality::gate::{binding_gate, completeness_gate};

fn rubric() -> gmeow_slice_quality::Rubric {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    gmeow_slice_quality::load_repo_rubric(&root).unwrap()
}

#[test]
fn axis_producers_bind_bijectively_to_implemented_primitives() {
    let errs = binding_gate(&rubric());
    assert!(
        errs.is_empty(),
        "axis↔producer binding must be a bijection: {errs:#?}"
    );
}

#[test]
fn every_projection_surface_is_covered_by_an_axis_or_a_dated_exemption() {
    let errs = completeness_gate(&rubric());
    assert!(errs.is_empty(), "projection completeness: {errs:#?}");
}
