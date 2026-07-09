// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship acceptance-manifest cross-check for the logic grounding layer.
//!
//! The third gate surface for the logic layer's `gmeow:FlagshipScenario` manifest (the
//! dataset-split resolver SHACL and the structural ASK cannot supply). It runs the
//! identical shared contract math and lang run —
//! [`gmeow_slicetest::flagship::assert_flagship_manifest`] — bound to the logic slice's
//! canonical set. Flip any competency IRI to a non-registered value, delete a referenced
//! fixture, or add/drop a flagship, and this test fails.

use gmeow_slicetest::flagship::assert_flagship_manifest;
use gmeow_slicetest::paths::slices_root;

/// The five canonical flagship-scenario IRIs the epic's depth bar requires.
const CANONICAL: [&str; 5] = [
    "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/elRlDlClosure",
    "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/correspondenceSection",
    "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/counterfactualStratumC",
    "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/symmetricConjecture",
    "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/chaseTerminationCertificate",
];

#[test]
fn every_flagship_scenario_is_wired_to_a_green_competency_and_real_artifacts() {
    let slice = slices_root().join("grounding").join("logic");
    assert_flagship_manifest(&slice, &CANONICAL);
}
