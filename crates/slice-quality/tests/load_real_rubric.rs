// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Load the real, committed slice-quality rubric and assert its structure.
//!
//! This is the executable proof that the rubric is data-driven: the loader reads
//! the axes, tiers, thresholds, and exemptions out of
//! `slices/core/slice-quality-rubric/module.ttl` with no hardcoded rubric in Rust.
//! Editing a threshold in the ontology changes what this loader returns with no
//! recompilation of the primitives.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // crates/slice-quality → repo root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Load the whole committed rubric from the canonical rubric module — the test-side
/// reconstruction of the retired `load_repo_rubric` from the crate's public
/// primitives (`dataset_from_paths` + `rubric::load_rubric`).
fn load_repo_rubric() -> gmeow_errors::Result<gmeow_slice_quality::model::Rubric> {
    let module = repo_root().join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module])?;
    gmeow_slice_quality::rubric::load_rubric(&ds)
}

#[test]
fn loads_the_real_rubric_structure() {
    let rubric = load_repo_rubric().expect("the committed rubric slice must load");

    // Five-rung tier ladder, contiguous ranks 0..=4.
    assert_eq!(rubric.standard.tiers.len(), 5, "five tier rungs");
    let ranks: Vec<i64> = rubric.standard.tiers.iter().map(|t| t.rank).collect();
    assert_eq!(ranks, vec![0, 1, 2, 3, 4], "contiguous ranks, ascending");
    assert_eq!(
        rubric.standard.bottom_tier().unwrap().rank,
        0,
        "Registered is the floor"
    );

    // Fourteen quality axes, each with a producer, a dimension, a scope, and floors.
    assert_eq!(rubric.standard.axes.len(), 14, "fourteen quality axes");
    for axis in &rubric.standard.axes {
        assert!(!axis.producer.is_empty(), "{} binds a producer", axis.iri);
        assert!(
            !axis.dimension_iri.is_empty(),
            "{} names a dimension",
            axis.iri
        );
        assert!(!axis.thresholds.is_empty(), "{} pins thresholds", axis.iri);
        // Every threshold names a real tier (loader would have errored otherwise).
        for t in &axis.thresholds {
            assert!(
                rubric.standard.tier(&t.tier_iri).is_some(),
                "{} threshold tier {} resolves",
                axis.iri,
                t.tier_iri
            );
        }
    }

    // The provenance-honesty axis is present and slice-local.
    let prov = rubric
        .standard
        .axes
        .iter()
        .find(|a| a.iri.ends_with("axisProvenanceHonesty"))
        .expect("provenance-honesty axis present");
    assert!(matches!(
        prov.scope,
        gmeow_slice_quality::ContextScope::SliceLocal
    ));
    assert_eq!(prov.producer, "provenance_honesty");

    // The DocMaturity axis is present with its pinned producer/dimension identity —
    // pinning the exact IRIs so a substitution of the wrong axis/producer fails here
    // rather than only tripping an aggregate count.
    let doc_maturity = rubric
        .standard
        .axes
        .iter()
        .find(|a| a.iri.ends_with("axisDocMaturity"))
        .expect("DocMaturity axis present");
    assert_eq!(
        doc_maturity.iri, "https://blackcatinformatics.ca/gmeow/axisDocMaturity",
        "DocMaturity axis IRI"
    );
    assert_eq!(
        doc_maturity.producer, "DocMaturity",
        "DocMaturity axis producer"
    );
    assert_eq!(
        doc_maturity.dimension_iri,
        "https://blackcatinformatics.ca/gmeow/qualityDimensionDocumentation",
        "DocMaturity axis dimension"
    );

    // Two dated, self-cleaning exemptions, each naming a producer symbol. Pin the
    // EXACT remaining set — a substitution of the wrong exemption would still pass
    // an aggregate-count-only check — and assert the retired DocMaturity exemption
    // (retired when the axisDocMaturity axis landed) is ABSENT.
    assert_eq!(rubric.floors.exemptions.len(), 2, "two dated exemptions");
    let mut exemption_iris: Vec<&str> = rubric
        .floors
        .exemptions
        .iter()
        .map(|e| e.iri.as_str())
        .collect();
    exemption_iris.sort_unstable();
    assert_eq!(
        exemption_iris,
        vec![
            "https://blackcatinformatics.ca/gmeow/exemptionDocsPanels",
            "https://blackcatinformatics.ca/gmeow/exemptionGmnProjection",
        ],
        "the exact remaining exemption set"
    );
    assert!(
        !exemption_iris.contains(&"https://blackcatinformatics.ca/gmeow/exemptionDocMaturity"),
        "the retired exemptionDocMaturity IRI must be absent — retired when axisDocMaturity landed"
    );
    for ex in &rubric.floors.exemptions {
        assert!(
            !ex.producer.is_empty(),
            "{} names a producer symbol",
            ex.iri
        );
        assert!(!ex.date.is_empty(), "{} is dated", ex.iri);
    }
}

#[test]
fn every_axis_producer_is_unique() {
    let rubric = load_repo_rubric().unwrap();
    let mut producers: Vec<&str> = rubric
        .standard
        .axes
        .iter()
        .map(|a| a.producer.as_str())
        .collect();
    producers.sort_unstable();
    let before = producers.len();
    producers.dedup();
    assert_eq!(before, producers.len(), "axis producers are distinct");
}
