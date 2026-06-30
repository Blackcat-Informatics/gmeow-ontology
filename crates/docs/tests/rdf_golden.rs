// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden test of the `gmeow:` RDF projection of the documentation model (#853
//! T5 dogfooding).
//!
//! The snapshot is over a SMALL, hand-built deterministic model — not the live
//! ~2k-term catalog — so the golden stays KB-sized and pins the N-Quads
//! vocabulary shape (predicates, the documentation named graph, the
//! `xsd:boolean` typing) rather than churning on ontology content.

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use gmeow_docs::{
    to_gmeow_rdf, DocConcern, DocMappingSet, DocSlice, DocTerm, DocTermCategory, DocsModel,
    Translations, UiCatalog,
};
use pretty_assertions::assert_eq;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn small_model() -> DocsModel {
    DocsModel {
        title: "GMEOW Ontology Documentation".to_string(),
        version: "2".to_string(),
        slices: vec![DocSlice {
            iri: format!("{GMEOW}slice/zoo"),
            label: Some("Zoo".to_string()),
            title: None,
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            artifacts: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
        }],
        terms: vec![
            DocTerm {
                iri: format!("{GMEOW}Cat"),
                curie: "gmeow:Cat".to_string(),
                label: Some("Cat".to_string()),
                definition: Some("A small domesticated felid.".to_string()),
                category: DocTermCategory::Class,
                owner_slice: format!("{GMEOW}slice/zoo"),
                parents: Vec::new(),
                domain: Vec::new(),
                range: Vec::new(),
                scope_notes: Vec::new(),
                examples: Vec::new(),
                use_when: Vec::new(),
                avoid_when: Vec::new(),
                how_to_use: Vec::new(),
                use_for_consumer: Vec::new(),
                avoid_for_consumer: Vec::new(),
                ..Default::default()
            },
            DocTerm {
                iri: format!("{GMEOW}hasOwner"),
                curie: "gmeow:hasOwner".to_string(),
                label: None,
                definition: None,
                category: DocTermCategory::Property,
                owner_slice: format!("{GMEOW}slice/zoo"),
                parents: Vec::new(),
                domain: Vec::new(),
                range: Vec::new(),
                scope_notes: Vec::new(),
                examples: Vec::new(),
                use_when: Vec::new(),
                avoid_when: Vec::new(),
                how_to_use: Vec::new(),
                use_for_consumer: Vec::new(),
                avoid_for_consumer: Vec::new(),
                ..Default::default()
            },
        ],
        dependency_edges: Vec::new(),
        mapping_sets: vec![DocMappingSet {
            iri: format!("{GMEOW}mapping/zoo-sssom"),
            curie: "gmeow:mapping/zoo-sssom".to_string(),
            set_id: None,
            sssom_file: None,
            license: None,
            comment: None,
            owner_slice: format!("{GMEOW}slice/zoo"),
            equivalence_count: 0,
        }],
        linkages: Vec::new(),
        examples: Vec::new(),
        shapes: Vec::new(),
        competencies: Vec::new(),
        concerns: vec![DocConcern {
            iri: format!("{GMEOW}concern/animals"),
            curie: "gmeow:concern/animals".to_string(),
            label: Some("Animals".to_string()),
            definition: None,
            terms: Vec::new(),
            slices: Vec::new(),
        }],
        external_terms: Vec::new(),
        recipes: Vec::new(),
        learning_paths: Vec::new(),
        four_boxes: None,
        concept_doi: None,
        available_languages: vec!["english".to_string()],
        translations: Translations::default(),
        ui_catalog: UiCatalog::default(),
    }
}

#[test]
fn gmeow_rdf_projection_golden() {
    let nq = to_gmeow_rdf(&small_model());
    insta::assert_snapshot!("gmeow_rdf_small", nq);
}

#[test]
fn gmeow_rdf_projection_is_deterministic() {
    let model = small_model();
    assert_eq!(to_gmeow_rdf(&model), to_gmeow_rdf(&model));
}

// ── R5 (#859): vocabulary-shape + round-trip-valid RDF (beyond the golden) ──────

const DOCUMENTATION_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";

#[test]
fn gmeow_rdf_carries_the_documentation_vocabulary_in_the_named_graph() {
    let nq = to_gmeow_rdf(&small_model());

    // Every non-empty quad lands in the documentation named graph.
    for line in nq.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            line.ends_with(&format!("<{DOCUMENTATION_GRAPH}> .")),
            "quad not in the documentation graph: {line}"
        );
    }

    // The documentation vocabulary surfaces (types + predicates).
    for needle in [
        "gmeow/DocumentedTerm",
        "gmeow/DocumentedSlice",
        "gmeow/DocumentedConcern",
        "gmeow/DocumentedMappingSet",
        "gmeow/documents",
        "gmeow/docCategory",
        "gmeow/docHasDefinition",
        "gmeow/docUrl",
        "gmeow/docOwnerSlice",
    ] {
        assert!(nq.contains(needle), "projection missing `{needle}`");
    }
}

#[test]
fn gmeow_rdf_types_the_definition_flag_as_xsd_boolean() {
    let nq = to_gmeow_rdf(&small_model());
    // Cat has a definition (true); hasOwner has none (false). Both ride as typed
    // xsd:boolean literals.
    assert!(
        nq.contains("docHasDefinition> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"),
        "docHasDefinition true not typed xsd:boolean"
    );
    assert!(
        nq.contains("docHasDefinition> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean>"),
        "docHasDefinition false not typed xsd:boolean"
    );
}

/// N-Quads validity cross-check for the hand-built `to_gmeow_rdf` projection.
///
/// `to_gmeow_rdf` assembles its N-Quads document by hand (`format!`/`push_str`),
/// NOT through any gmeow-rdf serializer — so re-parsing it through the
/// *independent* native N-Quads reader ([`gmeow_rdf::parse_dataset`]) proves the
/// projection emits valid, round-trippable N-Quads without testing the codec
/// against itself. (EPIC #906: docs is oxigraph-free; this carve-out moved from
/// the oxigraph reader to the native reader, exactly the slice-crate migration.)
#[test]
fn gmeow_rdf_reparses_through_native_codec() {
    use gmeow_rdf::{DatasetView, GraphMatch, TermRef, TermValue};

    let nq = to_gmeow_rdf(&small_model());
    let dataset = gmeow_rdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .expect("to_gmeow_rdf must emit valid, round-trippable N-Quads");

    let non_empty = nq.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(dataset.quad_count(), non_empty, "every quad must parse");

    // Every parsed quad is in the documentation named graph.
    let graph_id = dataset
        .term_id_by_value(&TermValue::iri(DOCUMENTATION_GRAPH))
        .expect("documentation graph IRI interned");
    let in_graph = dataset
        .quads_for_pattern(None, None, None, GraphMatch::Named(graph_id))
        .count();
    assert_eq!(
        in_graph,
        dataset.quad_count(),
        "all quads must be in the documentation graph"
    );
    // No quad escaped into the default graph.
    assert_eq!(
        dataset
            .quads_for_pattern(None, None, None, GraphMatch::Default)
            .count(),
        0,
        "no quad should land in the default graph"
    );
    // Sanity: the documentation graph IRI resolves back to the same IRI.
    assert!(matches!(
        dataset.resolve(graph_id),
        TermRef::Iri(iri) if iri == DOCUMENTATION_GRAPH
    ));
}
