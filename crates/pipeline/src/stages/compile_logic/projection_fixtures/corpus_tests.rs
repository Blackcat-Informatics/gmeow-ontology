// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Byte-exact comparisons of producer-selected projections, with no source compilation.

use super::{CHANNEL, Observations};

fn run_case(case: &str) {
    static OBSERVATIONS: std::sync::OnceLock<Observations> = std::sync::OnceLock::new();
    let observations = OBSERVATIONS.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
            .expect("authenticated selected projection observations");
        serde_json::from_slice(&bytes).expect("decode projection observations")
    });
    let observation = observations
        .get(case)
        .expect("producer selected the case")
        .as_ref()
        .expect("source parsed");
    assert!(
        observation.diagnostics.is_empty(),
        "[{case}] unexpected parse diagnostics: {:?}",
        observation.diagnostics
    );
    let projections = observation
        .projections
        .as_ref()
        .expect("all projections produced");
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(case);
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for target in [
            "datalog",
            "n3",
            "owl-dl",
            "owl-el",
            "gufo",
            "canonical-rdf12",
            "projection-report",
        ] {
            insta::assert_snapshot!(
                target,
                projections
                    .get(target)
                    .expect("required projection present")
            );
        }
        assert_eq!(projections.len(), 7);
    });
}

#[test]
fn parity_confidence_scoped_axiom() {
    run_case("confidence-scoped-axiom");
}

#[test]
fn parity_kind_hierarchy() {
    run_case("kind-hierarchy");
}

#[test]
fn parity_relator_mediation() {
    run_case("relator-mediation");
}
