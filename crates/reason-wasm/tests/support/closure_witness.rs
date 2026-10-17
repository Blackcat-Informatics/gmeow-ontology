// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure native computation of the small, explicit browser closure witness.

use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};

/// A self-contained EDB whose structured-DL closure is non-empty: an RDFS subclass
/// axiom + a typed individual entails the individual's membership in the superclass.
pub const INPUT: &str = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:Cat rdfs:subClassOf ex:Animal .\n\
     ex:Animal rdfs:subClassOf ex:Organism .\n\
     ex:felix rdf:type ex:Cat .\n";

/// The exact pipeline the wasm `reason(data, format)` shim runs.
pub fn reason_nquads(data: &str, format: &str) -> String {
    let edb = purrdf::parse_dataset(data.as_bytes(), format, None).expect("parse EDB");
    let input = prepare_reasoning_input(&edb).expect("admit submitted theory");
    // This API explicitly treats every context in the submitted document as a
    // logical theory, retaining default, IRI and scoped blank graph identities.
    let domains = SelectedDomains::new(
        input
            .source_contexts()
            .values()
            .map(|graph| {
                SelectedLogicalWorld::new(
                    LogicalGraph::from_graph(graph.clone()),
                    DomainProfile::NonemptyObjectDomainV1,
                    "gmeow.reason-wasm.document-theories.v1".to_owned(),
                    *input.ingress_contract(),
                )
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()
            .expect("admit submitted theory"),
    )
    .expect("admit submitted theory");
    let closure = gmeow_logic::reason::reason_closure_dataset(input, &domains)
        .expect("admit submitted theory");
    let bytes = purrdf::serialize_dataset(
        &*closure,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .expect("serialize closure");
    String::from_utf8(bytes).expect("closure is utf-8")
}
