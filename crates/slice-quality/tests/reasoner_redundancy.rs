// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoner-derived acceptance: a closure-redundant authored triple that the text
//! lints cannot see is caught by proof.
//!
//! `ex:A ⊑ ex:C` is redundant because it is entailed by `ex:A ⊑ ex:B ⊑ ex:C`. A
//! text scan sees three ordinary subClassOf triples; only the reasoner knows one
//! is dead weight. Leave-one-out over the closure proves it.

use gmeow_slice_quality::reasoner::closure_redundant_subclasses;

const FIXTURE: &str = r#"
@prefix ex:   <http://example.org/> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:A a owl:Class ; rdfs:subClassOf ex:B .
ex:B a owl:Class ; rdfs:subClassOf ex:C .
ex:C a owl:Class .

# Redundant: entailed by A ⊑ B ⊑ C. A text lint cannot see this.
ex:A rdfs:subClassOf ex:C .
"#;

#[test]
fn closure_redundant_subclass_is_caught_by_proof() {
    let ds =
        purrdf::parse_dataset(FIXTURE.as_bytes(), "text/turtle", None).expect("fixture parses");
    let redundant = closure_redundant_subclasses(&ds).expect("reasoning succeeds");

    let a = "http://example.org/A".to_owned();
    let b = "http://example.org/B".to_owned();
    let c = "http://example.org/C".to_owned();

    assert!(
        redundant.contains(&(a.clone(), c.clone())),
        "A ⊑ C must be flagged closure-redundant; got {redundant:?}"
    );
    assert!(
        !redundant.contains(&(a, b.clone())),
        "A ⊑ B is load-bearing, not redundant"
    );
    assert!(
        !redundant.contains(&(b, c)),
        "B ⊑ C is load-bearing, not redundant"
    );
}
