// SPDX-License-Identifier: AGPL-3.0-only

//! Ownership and completeness gate for the math-owned quantity correspondence catalog.
//!
//! Quantity semantics are authored once under `slices/grounding/math/mappings/`.
//! Observation catalogs may map their own acts, roles, units, and qualifiers, but
//! cannot become a second authoring surface for `math:Quantity` or
//! `math:quantityValue` (Principles 4, 17, and 19).

mod conformance_support;
use conformance_support::*;

use std::collections::BTreeSet;

const MATH: &str = "https://blackcatinformatics.ca/math/";

#[test]
fn quantity_bridge_catalog_transpiles_clean() {
    // Alignment-cell well-formedness moved from the mapping SHACL (the deleted
    // `TermEquivalenceShape`) into the fail-closed Rust correspondence transpiler; the math
    // quantity bridge catalog must transpile clean (every cell carries its complete envelope).
    let ttl = std::fs::read_to_string(
        repo_root().join("slices/grounding/math/mappings/quantity-bridges.ttl"),
    )
    .expect("quantity bridge catalog must read");
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("quantity bridge catalog must parse");
    let view = gmeow_logic_compile::ingest::DslView::new(ds.as_ref());
    gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed(
        &view, &view,
    )
    .expect("the math quantity bridge catalog must transpile clean");
}

#[test]
fn quantity_bridges_are_complete_math_owned_and_absent_from_observations() {
    // Native grounding cells (the align* cell node was deleted): read the reifier-preserving
    // parse through the canonical `equivalence_cells` reader.
    let cells = native_grounding_cells(
        &repo_root().join("slices/grounding/math/mappings/quantity-bridges.ttl"),
    );
    assert!(!cells.is_empty(), "the quantity catalog must not be empty");

    let mut actual = BTreeSet::new();
    for c in &cells {
        let source = c
            .source_endpoint
            .clone()
            .unwrap_or_else(|| c.subject.clone());
        let target = c.target_endpoint.clone().unwrap_or_else(|| c.obj.clone());
        assert!(
            source.starts_with(MATH),
            "{source} must be oriented from math:"
        );
        assert_eq!(
            source, c.subject,
            "source endpoint must equal the match subject"
        );
        assert_eq!(target, c.obj, "target endpoint must equal the match object");
        assert!(
            c.morphism_class.is_some(),
            "{source} requires a morphismClass"
        );
        assert!(
            c.morphism_kind.is_some(),
            "{source} requires a morphismKind"
        );
        assert!(
            c.preservation.is_some(),
            "{source} requires a preservationKind"
        );
        assert!(c.confidence.is_some(), "{source} requires a confidence");
        actual.insert((source, target, c.sssom_file.clone()));
    }

    let expected = BTreeSet::from([
        (
            format!("{MATH}Quantity"),
            "http://www.w3.org/ns/sosa/Result".to_owned(),
            "gmeow-observations.sssom.tsv".to_owned(),
        ),
        (
            format!("{MATH}Quantity"),
            "http://www.wurvoc.org/vocabularies/om-1.8/Measure".to_owned(),
            "gmeow-observations.sssom.tsv".to_owned(),
        ),
        (
            format!("{MATH}Quantity"),
            "http://www.ivoa.net/rdf/ObsCore#observable".to_owned(),
            "gmeow-observations.sssom.tsv".to_owned(),
        ),
        (
            format!("{MATH}Quantity"),
            "http://loinc.org/rdf/Quantity".to_owned(),
            "gmeow-observations.sssom.tsv".to_owned(),
        ),
        (
            format!("{MATH}Quantity"),
            "http://qudt.org/schema/qudt/QuantityValue".to_owned(),
            "gmeow-qudt.sssom.tsv".to_owned(),
        ),
        (
            format!("{MATH}quantityValue"),
            "http://qudt.org/schema/qudt/quantityValue".to_owned(),
            "gmeow-qudt.sssom.tsv".to_owned(),
        ),
    ]);
    assert!(
        expected.is_subset(&actual),
        "the required quantity bridge surface must remain covered; missing {:?}",
        expected.difference(&actual).collect::<Vec<_>>()
    );

    for c in native_cells(&repo_root().join("slices/core/observations/mappings/equivalences.ttl")) {
        assert!(
            c.subject != format!("{MATH}Quantity") && c.subject != format!("{MATH}quantityValue"),
            "observation catalog re-authors math-owned source {} in a native alignment cell",
            c.subject
        );
    }
}

/// Every native alignment cell in `path`, via the canonical reader over a reifier-preserving
/// parse (GraphStore flattens, dropping the reifier side tables the reader needs).
fn native_cells(
    path: &std::path::Path,
) -> Vec<gmeow_logic_compile::projections::sssom::EquivalenceCell> {
    let ttl = std::fs::read_to_string(path).expect("read mapping catalog");
    let ds =
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("catalog must parse");
    let view = gmeow_logic_compile::ingest::DslView::new(ds.as_ref());
    gmeow_logic_compile::projections::sssom::equivalence_cells(&view)
        .expect("native cells must read")
}

/// Native grounding alignment cells (those carrying the `logic:GroundingCorrespondence` envelope).
fn native_grounding_cells(
    path: &std::path::Path,
) -> Vec<gmeow_logic_compile::projections::sssom::EquivalenceCell> {
    native_cells(path)
        .into_iter()
        .filter(|c| c.grounding)
        .collect()
}
