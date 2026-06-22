// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden test of the `gmeow:` RDF projection of the documentation model (#853
//! T5 dogfooding).
//!
//! The snapshot is over a SMALL, hand-built deterministic model — not the live
//! ~2k-term catalog — so the golden stays KB-sized and pins the N-Quads
//! vocabulary shape (predicates, the documentation named graph, the
//! `xsd:boolean` typing) rather than churning on ontology content.

use gmeow_docs::{
    to_gmeow_rdf, DocConcern, DocMappingSet, DocSlice, DocTerm, DocTermCategory, DocsModel,
};

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
        concerns: vec![DocConcern {
            iri: format!("{GMEOW}concern/animals"),
            curie: "gmeow:concern/animals".to_string(),
            label: Some("Animals".to_string()),
            definition: None,
            terms: Vec::new(),
            slices: Vec::new(),
        }],
        external_terms: Vec::new(),
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
