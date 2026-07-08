// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship acceptance-manifest cross-check for the math grounding layer.
//!
//! This is the third gate surface (the dataset-split resolver). Its logic lives in the
//! shared [`gmeow_slicetest::flagship::assert_flagship_manifest`] helper so math and lang
//! run the identical contract; this test binds it to math's namespace and canonical set.
//! Flip any competency IRI to a non-registered value, delete a referenced file, or
//! add/drop a scenario, and this test fails.

use gmeow_slicetest::flagship::assert_flagship_manifest;
use gmeow_slicetest::paths::slices_root;

/// The math layer namespace the manifest's scenarios and properties live under.
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

/// The five canonical flagship-scenario IRIs the epic's depth bar requires.
const CANONICAL: [&str; 5] = [
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/e8Symmetry",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/homomorphicEncryption",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/proofAsProcess",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/rBridge",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/aiSelfStructure",
];

#[test]
fn every_flagship_scenario_is_wired_to_a_green_competency_and_real_artifacts() {
    let slice = slices_root().join("grounding").join("math");
    assert_flagship_manifest(&slice, MATH_NS, &CANONICAL);
}
