// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Engine-level tests for the repo-free `gmeow validate <data>` path
//! ([`gmeow_validate::data_validate`]). These exercise the Rust orchestration
//! directly against the committed `gmeow.gts` bundle, independent of the Python
//! CLI surface — the CLI test asserts the wheel-resolution and rendering on top.

use std::path::PathBuf;

use gmeow_validate::data_validate;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The committed snapshot bundle that carries the SHACL shape surface
/// (`shapes-archive`) the consumer path validates against.
fn bundle_bytes() -> Vec<u8> {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "generated",
        "dist",
        "gmeow.gts",
    ]
    .iter()
    .collect();
    std::fs::read(&path).unwrap_or_else(|e| panic!("read bundle {}: {e}", path.display()))
}

/// A data graph with exactly three Tier-1 problems: a disjoint identity-axis
/// overtyping (error), an under-mediated Commitment missing its beneficiary
/// (error), and a frame-less Event (warning). The Event carries an `eventType`
/// so only the frame-relativity warning — not the temporal-placement advisory —
/// fires. This is the epic's pinned 2-errors-1-warning acceptance shape.
const FAIL_TTL: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:p a gmeow:PronounSet, gmeow:Honorific .

ex:c a gmeow:Commitment ;
    gmeow:committedAgent ex:agent ;
    gmeow:intentionGoal ex:goal .

ex:e1 a gmeow:Event ;
    gmeow:eventType ex:meeting .
"#;

#[test]
fn fail_fixture_yields_two_errors_one_warning_with_locations() {
    let gts = bundle_bytes();
    let report = data_validate::run(
        FAIL_TTL.as_bytes(),
        "turtle",
        &gts,
        NS,
        "fixture-fail.ttl",
        false,
    )
    .expect("validate_data run");

    let errors: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == gmeow_diagnostics::Severity::Error)
        .collect();
    let warnings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == gmeow_diagnostics::Severity::Warning)
        .collect();

    assert_eq!(
        errors.len(),
        2,
        "expected exactly two errors, got: {errors:#?}"
    );
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one warning, got: {warnings:#?}"
    );

    // Assert stable rule identity via (code, source-shape IRI) — not prose.
    // Shapes discovered by running the test with --nocapture:
    //   disjointness: shacl.SPARQLConstraintComponent + IdentityAxisOrthogonalityShape
    //   commitment:   shacl.MinCountConstraintComponent + CommitmentShape
    //   frame:        shacl.MinCountConstraintComponent + EventFrameRequirementShape (Warning)
    const IDENTITY_SHAPE: &str = "IdentityAxisOrthogonalityShape";
    const COMMITMENT_SHAPE: &str = "CommitmentShape";
    const FRAME_SHAPE: &str = "EventFrameRequirementShape";

    assert!(
        errors.iter().any(|f| {
            f.code == "shacl.SPARQLConstraintComponent"
                && f.detail
                    .as_deref()
                    .is_some_and(|d| d.contains(IDENTITY_SHAPE))
        }),
        "missing P9 disjointness error (IdentityAxisOrthogonalityShape / SPARQLConstraintComponent)"
    );
    assert!(
        errors.iter().any(|f| {
            f.code == "shacl.MinCountConstraintComponent"
                && f.detail
                    .as_deref()
                    .is_some_and(|d| d.contains(COMMITMENT_SHAPE))
        }),
        "missing Commitment-mediation error (CommitmentShape / MinCountConstraintComponent)"
    );
    assert!(
        warnings[0].code == "shacl.MinCountConstraintComponent"
            && warnings[0]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains(FRAME_SHAPE))
            && warnings[0].severity == gmeow_diagnostics::Severity::Warning,
        "warning is not the frame-relativity one (EventFrameRequirementShape)"
    );

    // Every finding carries a location: the data file as the physical artifact
    // (for SARIF) and the focus-node IRI as the logical anchor.
    for finding in &report.findings {
        let loc = finding
            .locations
            .first()
            .unwrap_or_else(|| panic!("finding has no location: {finding:?}"));
        assert_eq!(loc.path.as_deref(), Some("fixture-fail.ttl"));
        assert!(loc.logical.is_some(), "finding lacks a logical anchor");
    }
}

#[test]
fn clean_fixture_passes() {
    let gts = bundle_bytes();
    let clean = "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n";
    let report = data_validate::run(clean.as_bytes(), "turtle", &gts, NS, "clean.ttl", false)
        .expect("run clean");
    assert_eq!(report.error_count(), 0, "clean graph reported errors");
    assert_eq!(report.warning_count(), 0, "clean graph reported warnings");
}

#[test]
fn nquads_named_graph_is_flattened_and_validated() {
    let gts = bundle_bytes();
    // The two type quads sit in a named graph; flattening must surface them so
    // the disjointness check fires.
    let nq = concat!(
        "<https://example.org/p> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<https://blackcatinformatics.ca/gmeow/PronounSet> <https://example.org/g> .\n",
        "<https://example.org/p> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<https://blackcatinformatics.ca/gmeow/Honorific> <https://example.org/g> .\n",
    );
    let report =
        data_validate::run(nq.as_bytes(), "nquads", &gts, NS, "g.nq", false).expect("run nq");
    assert_eq!(
        report.error_count(),
        1,
        "named-graph quads were not flattened"
    );
}

#[test]
fn json_ld_is_parsed_as_rdf() {
    let gts = bundle_bytes();
    let jsonld = r#"{"@context":{"gmeow":"https://blackcatinformatics.ca/gmeow/"},
        "@id":"https://example.org/p",
        "@type":["gmeow:PronounSet","gmeow:Honorific"]}"#;
    let report = data_validate::run(jsonld.as_bytes(), "json-ld", &gts, NS, "p.jsonld", false)
        .expect("run jsonld");
    assert_eq!(report.error_count(), 1, "JSON-LD was not validated as RDF");
}

#[test]
fn unknown_format_hard_fails() {
    let gts = bundle_bytes();
    let err = data_validate::run(b"{}", "application/json", &gts, NS, "x.json", false)
        .expect_err("JSON instance is not an RDF format");
    assert!(err.contains("parse error") || err.contains("unsupported"));
}
