// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored source roles, catalog and DSL. No repository source,
//! corpus, GTS, generator or authenticated fixture is constructed here.

use super::*;
use gmeow_logic_compile::{ingest::DslView, projections::grounding_catalog::OwnedGroundingSource};

const OWNED: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
lang:literalValue a logic:DatatypeProperty ; logic:range rdfs:Literal ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/lang> .
<urn:authored:value> a logic:DatatypeProperty ; logic:subPropertyOf lang:literalValue .
lang:literalValue skos:relatedMatch <https://schema.org/value> .
lang:authoredBridge rdf:reifies <<(lang:literalValue skos:relatedMatch <https://schema.org/value>)>> ;
    a logic:GroundingCorrespondence ; gm:sssomFile "authored.tsv" ;
    gm:justification <urn:authored:curation> ; logic:sourceEndpoint lang:literalValue ;
    logic:targetEndpoint <https://schema.org/value> ; logic:morphismClass logic:BridgeView ;
    logic:morphismKind logic:CommitmentShiftingBridge ; logic:preservationKind logic:ValidationOnly .
"#;
const SURFACE: &str = r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
<urn:authored:mapping> a gm:ProjectionMapping ;
    gm:hasMappingPattern [ gm:anchor "s" ; gm:atom (
        [ gm:subjectVar "s" ; gm:predicate <urn:authored:value> ; gm:objectVar "v" ] ) ] ;
    gm:hasBinding [ gm:profile "publication" ; gm:relation "<=" ;
        gm:templateAtoms ( [ gm:tSubj "s" ; gm:tPredFromCorrespondence
            <https://blackcatinformatics.ca/lang/authoredBridge> ; gm:tObj "v" ] ) ] .
"#;

fn catalog(source: &str) -> GroundingCatalog {
    let ontology = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let owned = OwnedGroundingSource::new(
        crate::model::GroundingOwners::LANG,
        "slices/grounding/lang/mappings/authored-counter.ttl",
        source.as_bytes(),
    )
    .unwrap();
    GroundingCatalog::from_sources(&[owned], &DslView::new(&ontology)).unwrap()
}

fn vocabulary(owners: &[&str]) -> ProjectionVocabulary {
    ProjectionVocabulary {
        prefix: "authored-schema-value".to_owned(),
        namespaces: vec!["https://schema.org/value".to_owned()],
        subsumed_by: crate::model::GroundingOwners::LOGIC.to_owned(),
        owners: crate::model::GroundingOwners::new(owners.iter().map(|owner| (*owner).to_owned()))
            .unwrap(),
        count_kind: CountKind::TypedAxiom,
        default_ceiling: 0,
        preservation: "ValidationOnly".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
    }
}

#[test]
fn projection_literal_counter_retains_virtual_proof_without_catalog_denominator() {
    let catalog = catalog(OWNED);
    let dataset = purrdf::parse_dataset(SURFACE.as_bytes(), "text/turtle", None).unwrap();
    let surface = AdmittedProjectionSurface::admit(&dataset, &catalog).unwrap();
    let vocabulary = vocabulary(&[
        crate::model::GroundingOwners::LANG,
        crate::model::GroundingOwners::MATH,
    ]);
    let constructs = enumerate(
        &surface,
        &vocabulary,
        CountMode::FullResidue,
        crate::DSL_MAPPING_SURFACE_IRI,
    );
    assert_eq!(constructs.len(), 1);
    assert!(constructs[0].grounded);
    assert_eq!(constructs[0].witness.anchor(), Some("urn:authored:mapping"));
    assert_eq!(surface.dataset().quads().count(), dataset.quads().count());
    assert_eq!(
        residue_for_surface(&surface, &vocabulary, crate::DSL_MAPPING_SURFACE_IRI),
        0
    );
    assert_eq!(
        residue_for_surface(
            &surface,
            &self::vocabulary(&[crate::model::GroundingOwners::MATH]),
            crate::DSL_MAPPING_SURFACE_IRI
        ),
        1
    );
}

#[test]
fn projection_literal_counter_admits_all_selectors_before_namespace_filtering() {
    let catalog = catalog(OWNED);
    for source in [
        SURFACE.replace("lang/authoredBridge", "lang/missingBridge"),
        SURFACE.replace(
            "gm:tObj \"v\"",
            "gm:tObj \"v\" ; gm:tPred <https://unrelated.example/p>",
        ),
        SURFACE.replace(
            "gm:objectVar \"v\"",
            "gm:objectVar \"v\" ; gm:optional \"garbage\"",
        ),
    ] {
        let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
        assert!(AdmittedProjectionSurface::admit(&dataset, &catalog).is_err());
    }
    let dataset = purrdf::parse_dataset(SURFACE.as_bytes(), "text/turtle", None).unwrap();
    assert!(
        AdmittedProjectionSurface::admit(&dataset, &crate::independent_grounding_catalog())
            .is_err()
    );
}

#[test]
fn projection_literal_counter_prefix_suffix_and_file_text_adapters_agree() {
    let owned = OWNED.replace("schema.org/value>", "schema.org/valueReference>");
    let catalog = catalog(&owned);
    let vocabulary = vocabulary(&[crate::model::GroundingOwners::LANG]);
    let texts = vec![SURFACE.to_owned()];
    let from_text = crate::residue_constructs_over_texts(
        &texts,
        &vocabulary,
        crate::DSL_MAPPING_SURFACE_IRI,
        &catalog,
    )
    .unwrap();
    assert!(from_text.is_empty());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored.ttl");
    std::fs::write(&path, SURFACE).unwrap();
    let from_file = crate::measure_surface_residue_constructs(
        &[path],
        crate::DSL_MAPPING_SURFACE_IRI,
        std::slice::from_ref(&vocabulary),
        &catalog,
    )
    .unwrap();
    assert!(!from_file.contains_key(&vocabulary.prefix));
    let dataset = purrdf::parse_dataset(SURFACE.as_bytes(), "text/turtle", None).unwrap();
    let surface = AdmittedProjectionSurface::admit(&dataset, &catalog).unwrap();
    assert_eq!(
        enumerate(
            &surface,
            &vocabulary,
            CountMode::FullResidue,
            crate::DSL_MAPPING_SURFACE_IRI
        )
        .len(),
        1
    );
}
