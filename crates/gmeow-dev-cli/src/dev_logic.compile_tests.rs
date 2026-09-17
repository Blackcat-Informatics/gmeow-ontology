// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/// On-gate wiring proof (instant, no pipeline run): every `--mode` name
/// maps to the committed pipeline artifact path — in particular `report`
/// maps to the committed UNION report `PROJECTION_REPORT_PATH`
/// (`stage-mappings`' output), never a compiler-private path. Together
/// with the deletion of the in-process `compile_one_mode`, this pins that
/// `--mode M` can only ever narrow the real pipeline output. No test calls
/// `compile()`, because that entry point runs the corpus producer.
#[test]
fn every_mode_maps_to_its_committed_pipeline_path() {
    use super::{LOGIC_MODES, mode_path};
    use gmeow_pipeline::stages::compile_logic::{
        CANONICAL_RDF12_PATH, CGIF_PATH, CLIF_PATH, DATALOG_PATH, GUFO_PATH, N3_PATH, OWL_DL_PATH,
        OWL_EL_PATH, PROJECTION_REPORT_PATH, XCL_PATH,
    };

    assert_eq!(mode_path("owl-dl"), OWL_DL_PATH);
    assert_eq!(mode_path("owl-el"), OWL_EL_PATH);
    assert_eq!(mode_path("datalog"), DATALOG_PATH);
    assert_eq!(mode_path("n3"), N3_PATH);
    assert_eq!(mode_path("gufo"), GUFO_PATH);
    assert_eq!(mode_path("canonical-rdf12"), CANONICAL_RDF12_PATH);
    assert_eq!(mode_path("clif"), CLIF_PATH);
    assert_eq!(mode_path("cgif"), CGIF_PATH);
    assert_eq!(mode_path("xcl"), XCL_PATH);
    // The discriminator: `report` narrows to the committed UNION report.
    assert_eq!(mode_path("report"), PROJECTION_REPORT_PATH);
    // Every validated mode has a mapping (no mode falls through unhandled
    // to the wrong artifact).
    for m in LOGIC_MODES {
        assert!(!mode_path(m).is_empty(), "mode {m} has no committed path");
    }
}
