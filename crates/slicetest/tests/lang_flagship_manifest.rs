// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship acceptance-manifest cross-check for the language grounding layer.
//!
//! The third gate surface for `lang:FlagshipScenario` (the dataset-split resolver SHACL
//! and the structural ASK cannot supply). It runs the identical shared contract math
//! runs — [`gmeow_slicetest::flagship::assert_flagship_manifest`] — bound to lang's
//! namespace and canonical set. Flip any competency IRI to a non-registered value, delete
//! a referenced fixture, or add/drop a flagship, and this test fails.

use gmeow_slicetest::flagship::assert_flagship_manifest;
use gmeow_slicetest::paths::slices_root;

/// The five canonical flagship-scenario IRIs the epic's depth bar requires.
const CANONICAL: [&str; 5] = [
    "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/sentenceToFormula",
    "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/proseSelfReading",
    "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/docsAsTranslation",
    "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/serializationsAsGrammars",
    "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/ambiguityHeldHonestly",
];

#[test]
fn every_flagship_scenario_is_wired_to_a_green_competency_and_real_artifacts() {
    let slice = slices_root().join("grounding").join("lang");
    assert_flagship_manifest(&slice, &CANONICAL);
}
