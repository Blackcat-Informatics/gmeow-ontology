// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native OWL-RL/RDFS reasoning oracle over a plain RDF dataset.
//!
//! This is the Docker-free, wasm-clean replacement for the retired external
//! DL oracle: it forward-materializes the OWL-RL (or RDFS) closure through
//! `purrdf::entail` — a 70/70 W3C-entailment-conformance-tested native reasoner —
//! and reads two independent verdicts off the closure:
//!
//! * `owlrl_subsumptions` — the named-class `rdfs:subClassOf` closure (the
//!   subsumption hierarchy an external classifier would report), and
//! * `consistency` — the satisfiability verdict, keyed on any class forced to be
//!   equivalent to (or populated as) `owl:Nothing`.
//!
//! Unlike the production reasoning engine in [`crate::reason`], this oracle carries NO
//! gmeow-specific calculus: it is the general OWL semantics purrdf implements, run
//! over an arbitrary TBox/ABox, so it can serve as the independent cross-check the
//! divergence ledger compares the native engine against. It lives OUTSIDE `reason`
//! on purpose, so adding it does not perturb `reason::native_contract_hash`.

use purrdf::entail::{EntailError, QNode, QTriple, Regime, materialize, materialize_dl};
use purrdf::{RdfDataset, TermRef, TermValue};

/// `rdfs:subClassOf`.
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `owl:Thing` — the trivial top; any `X ⊑ owl:Thing` is uninformative.
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
/// `owl:Nothing` — the bottom; any named `X ⊑ owl:Nothing` is unsatisfiable.
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
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

/// The satisfiability verdict for `edb` under OWL 2 **Direct** semantics.
///
/// Returns `(is_consistent, unsat_classes)`. This runs `purrdf::entail`'s
/// OWL-Direct **ALCOIQ tableau** (via [`materialize_dl`]) — NOT the OWL-RL forward
/// closure — because only the tableau performs the clash detection a consistency
/// check needs. OWL-RL is a sound *positive* forward-closure: it never derives an
/// `owl:Nothing` edge from a disjointness clash, so it cannot see that
/// `X ⊑ Y, X ⊑ Z, Y owl:disjointWith Z` makes `X` empty. The tableau does.
///
/// Two distinct outcomes are folded into the boolean:
/// * **Global inconsistency** (an ABox clash — e.g. an individual inhabiting an
///   unsatisfiable class): the tableau returns [`EntailError::Inconsistent`]; every
///   class is then trivially unsatisfiable, so there is no meaningful per-class list
///   and `unsat_classes` is empty.
/// * **Class unsatisfiability in a consistent ontology** (an empty class): the
///   tableau answers the query `?c rdfs:subClassOf owl:Nothing`, and those `?c`
///   bindings are the unsatisfiable named classes.
///
/// # Panics
///
/// Panics on any non-`Inconsistent` entail error (malformed KB, build failure): an
/// entail error must never be silently reported as consistent.
pub fn consistency(edb: &RdfDataset) -> (bool, Vec<String>) {
    let query = vec![QTriple {
        s: QNode::Var("c".to_owned()),
        p: QNode::Term(TermValue::iri(SUBCLASS_OF)),
        o: QNode::Term(TermValue::iri(OWL_NOTHING)),
    }];
    let answers = match materialize_dl(edb, &query) {
        Ok(answers) => answers,
        Err(EntailError::Inconsistent) => return (false, Vec::new()),
        Err(e) => panic!("purrdf OWL-Direct consistency check failed (a hard fail): {e}"),
    };

    let mut unsat: Vec<String> = Vec::new();
    for quad in answers.quads() {
        let (Some(pred), Some(sub), Some(obj)) = (
            iri_of(&answers, quad.p),
            iri_of(&answers, quad.s),
            iri_of(&answers, quad.o),
        ) else {
            continue;
        };
        if pred == SUBCLASS_OF && obj == OWL_NOTHING && sub != OWL_NOTHING {
            unsat.push(sub.to_owned());
        }
    }
    unsat.sort();
    unsat.dedup();
    (unsat.is_empty(), unsat)
}

/// The **global** consistency verdict for `edb` at OWL-Direct tableau depth — a
/// thin, boolean-only view over [`consistency`] for the world-scoped cross-check.
///
/// `true` iff the OWL-Direct tableau finds no GLOBAL inconsistency (no individual
/// forced into an unsatisfiable class). An empty-but-unpopulated class (class
/// unsatisfiability) is NOT a global inconsistency and leaves the verdict `true` —
/// exactly the `(false, [])` (ABox clash) vs `(false, [X…])` (empty class)
/// distinction [`consistency`] draws. This is what the per-world cross-check folds:
/// the bundle is globally consistent iff every world is.
pub fn globally_consistent(edb: &RdfDataset) -> bool {
    let (flag, unsat) = consistency(edb);
    // A `false` flag is a global inconsistency ONLY when it is not explained by
    // class unsatisfiability (an empty class): `(false, [])` is an ABox clash;
    // `(false, [X…])` is a consistent ontology that merely has empty classes.
    flag || !unsat.is_empty()
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

    #[test]
    fn consistency_true_for_a_satisfiable_tbox() {
        let ds = dataset(&format!(
            "{PREFIX}\
:A a owl:Class . :B a owl:Class . :C a owl:Class .
:A rdfs:subClassOf :B .
:B rdfs:subClassOf :C .
"
        ));
        let (is_consistent, unsat) = consistency(ds.as_ref());
        assert!(is_consistent, "satisfiable TBox is consistent");
        assert!(unsat.is_empty(), "no unsatisfiable classes: {unsat:?}");
    }

    #[test]
    fn consistency_detects_disjointness_class_unsatisfiability() {
        // The tableau (OWL-Direct) — unlike the OWL-RL forward closure — performs
        // clash detection: :X ⊑ :Y, :X ⊑ :Z, :Y disjointWith :Z makes :X provably
        // EMPTY (⊑ owl:Nothing) while the ontology as a whole stays consistent (no
        // individual asserts membership in the empty class). This is exactly the
        // class-unsatisfiability a sound OWL 2 DL classification would report.
        let ds = dataset(&format!(
            "{PREFIX}\
:X a owl:Class . :Y a owl:Class . :Z a owl:Class .
:X rdfs:subClassOf :Y .
:X rdfs:subClassOf :Z .
:Y owl:disjointWith :Z .
"
        ));
        let (is_consistent, unsat) = consistency(ds.as_ref());
        assert!(
            !is_consistent,
            "the tableau derives :X ⊑ owl:Nothing from disjointness; unsat={unsat:?}"
        );
        assert!(
            unsat.contains(&iri("X")),
            ":X must be flagged empty: {unsat:?}"
        );
    }

    #[test]
    fn consistency_detects_direct_subclass_of_nothing() {
        // The floor case: an explicit :X ⊑ owl:Nothing is always reported.
        let ds = dataset(&format!(
            "{PREFIX}\
:X a owl:Class .
:X rdfs:subClassOf owl:Nothing .
"
        ));
        let (is_consistent, unsat) = consistency(ds.as_ref());
        assert!(!is_consistent, "explicit :X ⊑ owl:Nothing is unsatisfiable");
        assert!(unsat.contains(&iri("X")), ":X must be flagged: {unsat:?}");
    }

    #[test]
    fn globally_consistent_true_for_clean_tbox() {
        let ds = dataset(&format!(
            "{PREFIX}\
:A a owl:Class . :B a owl:Class .
:A rdfs:subClassOf :B .
"
        ));
        assert!(
            globally_consistent(ds.as_ref()),
            "a clean TBox is globally consistent"
        );
    }

    #[test]
    fn globally_consistent_true_for_unpopulated_empty_class() {
        // An empty class (class unsatisfiability) is NOT a global inconsistency: the
        // tableau reports `(false, [X])` and `globally_consistent` folds it to `true`.
        let ds = dataset(&format!(
            "{PREFIX}\
:X a owl:Class . :Y a owl:Class . :Z a owl:Class .
:X rdfs:subClassOf :Y .
:X rdfs:subClassOf :Z .
:Y owl:disjointWith :Z .
"
        ));
        assert!(
            globally_consistent(ds.as_ref()),
            "an unpopulated empty class leaves the ontology globally consistent"
        );
    }

    #[test]
    fn globally_consistent_false_for_populated_clash() {
        // An individual in two disjoint classes is a global ABox clash: the tableau
        // returns `EntailError::Inconsistent` → `(false, [])` → not globally consistent.
        let ds = dataset(&format!(
            "{PREFIX}\
:Y a owl:Class . :Z a owl:Class .
:Y owl:disjointWith :Z .
:x a :Y , :Z .
"
        ));
        assert!(
            !globally_consistent(ds.as_ref()),
            "a populated disjoint-class clash is globally inconsistent"
        );
    }

    #[test]
    fn consistency_global_inconsistency_from_populated_empty_class() {
        // An individual inhabiting an unsatisfiable class is a GLOBAL (ABox) clash:
        // the tableau returns EntailError::Inconsistent, which the oracle folds to
        // `(false, [])` — every class is trivially unsatisfiable, so there is no
        // meaningful per-class list.
        let ds = dataset(&format!(
            "{PREFIX}\
:X a owl:Class . :Y a owl:Class . :Z a owl:Class .
:X rdfs:subClassOf :Y .
:X rdfs:subClassOf :Z .
:Y owl:disjointWith :Z .
:x a :X .
"
        ));
        let (is_consistent, unsat) = consistency(ds.as_ref());
        assert!(
            !is_consistent,
            "a populated empty class is globally inconsistent"
        );
        assert!(
            unsat.is_empty(),
            "global inconsistency yields no per-class list: {unsat:?}"
        );
    }
}
