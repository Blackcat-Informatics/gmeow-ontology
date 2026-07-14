// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native OWL-RL/RDFS reasoning oracle over a plain RDF dataset.
//!
//! This is the Docker-free, wasm-clean replacement for the retired external
//! DL oracle: it forward-materializes the OWL-RL (or RDFS) closure through
//! `purrdf::entail` — a 70/70 W3C-entailment-conformance-tested native reasoner —
//! and reads the subsumption verdict off the closure:
//!
//! * `owlrl_subsumptions` — the named-class `rdfs:subClassOf` closure (the
//!   subsumption hierarchy an external classifier would report).
//!
//! Unlike the production reasoning engine in [`crate::reason`], this oracle carries NO
//! gmeow-specific calculus: it is the general OWL semantics purrdf implements, run
//! over an arbitrary TBox/ABox, so it can serve as the independent cross-check the
//! divergence ledger compares the native engine against. It lives OUTSIDE `reason`
//! on purpose, so adding it does not perturb `reason::native_contract_hash`.

use purrdf::entail::{Regime, materialize};
use purrdf::{RdfDataset, TermRef};

/// `rdfs:subClassOf`.
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `owl:Thing` — the trivial top; any `X ⊑ owl:Thing` is uninformative.
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
/// `rdfs:Resource` — the RDFS universal; `X ⊑ rdfs:Resource` is uninformative.
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";

/// Resolve a term id to its borrowed IRI, or `None` for a blank/literal term.
///
/// Resolution is a hot closure-scan operation. Keep the dataset borrow through
/// predicate rejection and allocate only when a surviving named-class result
/// must escape into the returned vector.
fn iri_of(ds: &RdfDataset, id: purrdf::TermId) -> Option<&str> {
    match ds.resolve(id) {
        TermRef::Iri(iri) => Some(iri),
        _ => None,
    }
}

/// The OWL-RL closure of `edb`, forward-materialized by `purrdf::entail`.
///
/// # Panics
///
/// Panics if materialization fails. A materialize error is a HARD FAIL — the
/// oracle must never silently downgrade an unclosable graph to "consistent" or an
/// empty hierarchy; the caller is entitled to trust the closure it gets back.
fn owlrl_closure(edb: &RdfDataset) -> std::sync::Arc<RdfDataset> {
    materialize(edb, Regime::OwlRl)
        .expect("purrdf OWL-RL materialization must succeed (an entail error is a hard fail)")
}

/// The named-class `rdfs:subClassOf` closure of `edb` under OWL-RL entailment.
///
/// Returns `(subclass_iri, superclass_iri)` pairs where BOTH terms are named
/// classes (IRIs). Trivial pairs are excluded:
/// * reflexive pairs (`s == o`), and
/// * pairs whose superclass is `owl:Thing` or `rdfs:Resource` (every class is
///   subsumed by the top, so those carry no hierarchy information).
///
/// The output is sorted and deduplicated.
pub fn owlrl_subsumptions(edb: &RdfDataset) -> Vec<(String, String)> {
    let closure = owlrl_closure(edb);
    let mut pairs: Vec<(String, String)> = Vec::new();
    for quad in closure.quads() {
        let Some(pred) = iri_of(&closure, quad.p) else {
            continue;
        };
        if pred != SUBCLASS_OF {
            continue;
        }
        let (Some(sub), Some(sup)) = (iri_of(&closure, quad.s), iri_of(&closure, quad.o)) else {
            continue;
        };
        if sub == sup || sup == OWL_THING || sup == RDFS_RESOURCE {
            continue;
        }
        pairs.push((sub.to_owned(), sup.to_owned()));
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset(ttl: &str) -> std::sync::Arc<RdfDataset> {
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\n{ttl}"))
    }

    const PREFIX: &str = "\
@prefix : <http://gmeow.example/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
";

    fn iri(local: &str) -> String {
        format!("http://gmeow.example/{local}")
    }

    #[test]
    fn owlrl_subsumptions_include_transitive_and_exclude_trivial() {
        // :A ⊑ :B ⊑ :C — OWL-RL must derive the transitive :A ⊑ :C.
        let ds = dataset(&format!(
            "{PREFIX}\
:A a owl:Class . :B a owl:Class . :C a owl:Class .
:A rdfs:subClassOf :B .
:B rdfs:subClassOf :C .
"
        ));
        let subs = owlrl_subsumptions(ds.as_ref());

        assert!(
            subs.contains(&(iri("A"), iri("B"))),
            "asserted :A ⊑ :B present: {subs:?}"
        );
        assert!(
            subs.contains(&(iri("B"), iri("C"))),
            "asserted :B ⊑ :C present: {subs:?}"
        );
        assert!(
            subs.contains(&(iri("A"), iri("C"))),
            "transitively-derived :A ⊑ :C present: {subs:?}"
        );
        // No reflexive pair and no owl:Thing / rdfs:Resource superclass survives.
        assert!(
            !subs.iter().any(|(s, o)| s == o),
            "no reflexive pair: {subs:?}"
        );
        assert!(
            !subs
                .iter()
                .any(|(_, o)| o == OWL_THING || o == RDFS_RESOURCE),
            "no owl:Thing / rdfs:Resource superclass: {subs:?}"
        );
    }
}
