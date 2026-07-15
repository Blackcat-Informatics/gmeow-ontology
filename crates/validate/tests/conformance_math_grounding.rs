// SPDX-License-Identifier: AGPL-3.0-only

//! Ownership and completeness gate for the math-owned quantity correspondence catalog.
//!
//! Quantity semantics are authored once under `slices/grounding/math/mappings/`.
//! Observation catalogs may map their own acts, roles, units, and qualifiers, but
//! cannot become a second authoring surface for `math:Quantity` or
//! `math:quantityValue` (Principles 4, 17, and 19).

mod conformance_support;
use conformance_support::*;

use gmeow_validate::store::{parse_file_dataset, shacl_validate_dataset};
use std::collections::BTreeSet;

const MATH: &str = "https://blackcatinformatics.ca/math/";

const SOURCE_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const TARGET_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";

#[test]
fn quantity_bridge_catalog_conforms_to_the_mapping_dsl() {
    let shapes_ttl = std::fs::read_to_string(repo_root().join("shapes/mapping-dsl-shapes.ttl"))
        .expect("mapping DSL shapes must be readable");
    let shapes =
        purrdf::shapes::engine::parse_shapes(&shapes_ttl).expect("mapping DSL shapes must parse");
    let catalog = parse_file_dataset(
        &repo_root().join("slices/grounding/math/mappings/quantity-bridges.ttl"),
    )
    .expect("quantity bridge catalog must parse");
    let report = shacl_validate_dataset(&catalog, &shapes);
    assert!(
        report.conforms,
        "complete math quantity bridges must conform: {:?}",
        report.results
    );
}

#[test]
fn quantity_bridges_are_complete_math_owned_and_absent_from_observations() {
    let catalog = GraphStore::parse_ttl_file(
        &repo_root().join("slices/grounding/math/mappings/quantity-bridges.ttl"),
    );
    let cells = catalog.subjects_of_type(GROUNDING_CORRESPONDENCE);
    assert!(!cells.is_empty(), "the quantity catalog must not be empty");

    let mut actual = BTreeSet::new();
    for cell in cells {
        assert!(
            catalog.has(Some(&cell), Some(RDF_TYPE), Some(TERM_EQUIVALENCE)),
            "{cell} must also be a gmeow:TermEquivalence frontend cell"
        );
        let source = exactly_one(catalog.objects(&cell, ALIGN_SUBJECT), &cell, "alignSubject");
        let target = exactly_one(catalog.objects(&cell, ALIGN_OBJECT), &cell, "alignObject");
        assert!(
            source.starts_with(MATH),
            "{cell} must be oriented from math:"
        );
        assert_eq!(
            exactly_one(
                catalog.objects(&cell, SOURCE_ENDPOINT),
                &cell,
                "sourceEndpoint"
            ),
            source,
            "{cell} source endpoint must equal alignSubject"
        );
        assert_eq!(
            exactly_one(
                catalog.objects(&cell, TARGET_ENDPOINT),
                &cell,
                "targetEndpoint"
            ),
            target,
            "{cell} target endpoint must equal alignObject"
        );
        exactly_one(
            catalog.objects(&cell, MORPHISM_CLASS),
            &cell,
            "morphismClass",
        );
        exactly_one(catalog.objects(&cell, MORPHISM_KIND), &cell, "morphismKind");
        exactly_one(
            catalog.objects(&cell, PRESERVATION_KIND),
            &cell,
            "preservationKind",
        );
        exactly_one(catalog.objects_lex(&cell, CONFIDENCE), &cell, "confidence");
        let file = exactly_one(catalog.objects_lex(&cell, SSSOM_FILE), &cell, "sssomFile");
        actual.insert((source, target, file));
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

    let observations = GraphStore::parse_ttl_file(
        &repo_root().join("slices/core/observations/mappings/equivalences.ttl"),
    );
    for cell in observations.subjects_of_type(TERM_EQUIVALENCE) {
        for source in observations.objects(&cell, ALIGN_SUBJECT) {
            assert!(
                source != format!("{MATH}Quantity") && source != format!("{MATH}quantityValue"),
                "observation catalog re-authors math-owned source {source} in {cell}"
            );
        }
    }
}
