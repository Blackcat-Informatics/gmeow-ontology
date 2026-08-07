// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native OWL-2-EL/RL subsumption closure over the structured chase.
//!
//! `structured_el_rules` is fixed and ontology-independent: it encodes the
//! class-level OWL-2-EL/RL entailment calculus (subclass transitivity,
//! equivalence, type propagation, sub-property transitivity) directly, as a
//! fixed built-in calculus. We feed the TBox/ABox of any kernel store
//! through the native world-scoped fact store and return the derived subsumption
//! closure with raw chase provenance.
//!
//! # Encoding
//!
//! Every fact carries subject, predicate, object, and named world as native typed
//! values. Predicate-quantifying RL constructs are handled by the dedicated RL
//! engine; this EL closure deliberately surfaces its narrower coverage.

use purrdf::RdfDataset;

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
#[allow(dead_code)]
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

pub(crate) fn structured_el_rules() -> Vec<crate::rule_ir::EvalRule> {
    use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

    let v = EvalTerm::var;
    let a = EvalAtom::positive;
    vec![
        EvalRule::positive(
            "el:subClassOf-transitive",
            a(v("?x"), SUBCLASS, v("?z")),
            vec![a(v("?x"), SUBCLASS, v("?y")), a(v("?y"), SUBCLASS, v("?z"))],
        ),
        EvalRule::positive(
            "el:equivalentClass-fwd",
            a(v("?x"), SUBCLASS, v("?y")),
            vec![a(v("?x"), EQUIVALENT, v("?y"))],
        ),
        EvalRule::positive(
            "el:equivalentClass-bwd",
            a(v("?y"), SUBCLASS, v("?x")),
            vec![a(v("?x"), EQUIVALENT, v("?y"))],
        ),
        EvalRule::positive(
            "el:type-propagation",
            a(v("?i"), TYPE, v("?c2")),
            vec![a(v("?i"), TYPE, v("?c1")), a(v("?c1"), SUBCLASS, v("?c2"))],
        ),
        EvalRule::positive(
            "el:subPropertyOf-transitive",
            a(v("?x"), SUBPROPERTY, v("?z")),
            vec![
                a(v("?x"), SUBPROPERTY, v("?y")),
                a(v("?y"), SUBPROPERTY, v("?z")),
            ],
        ),
    ]
}

/// The subsumption predicates the EL closure surfaces. Other derived rows from
/// the chase (none, for [`structured_el_rules`]) are filtered out of
/// [`ElClosure::inferred`].
///
/// `pub(crate)` so the single-chase [`crate::reason::reason_all`] can apply the
/// same subsumption filter to the shared `dl_rules` closure it runs once.
pub(crate) const SUBSUMPTION_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
];

/// One axiom in the EL subsumption closure, carrying its raw chase provenance.
///
/// `subject`/`predicate`/`object` are the decoded display strings of the
/// ternary fact; `world` is the named-graph IRI it was derived in. `is_edb`
/// distinguishes asserted facts (`true`) from rule-derived ones (`false`).
/// `rule_name` is the firing rule's `#[name(...)]` (`None` for EDB), and
/// `premises` are the decoded immediate antecedents (subject, predicate, object).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InferredAxiom {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub world: String,
    pub is_edb: bool,
    pub rule_name: Option<String>,
    pub premises: Vec<(String, String, String)>,
}

/// The result of an EL subsumption closure run.
///
/// `inferred` holds every subsumption-predicate axiom (asserted and derived);
/// `total_facts` is the count of all decoded ternary chase rows; `gaps` names
/// the EL-profile limitations of this narrow encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElClosure {
    pub inferred: Vec<InferredAxiom>,
    pub total_facts: usize,
    pub gaps: Vec<String>,
}

/// Compute the native OWL-2-EL/RL subsumption closure of `edb`.
///
/// Runs the fixed `structured_el_rules()` calculus over `edb` through the shared
/// native structured-rule chase, then filters the decoded
/// closure to the subsumption predicates and surfaces the EL-profile
/// predicate-position limitations for callers that use this narrow surface
/// directly.
///
/// # Errors
///
/// Returns an error if the source store cannot be loaded, native evaluation fails,
/// or a derived row fails to decode.
pub fn el_closure(edb: &RdfDataset) -> gmeow_errors::Result<ElClosure> {
    // 1. Run the fixed EL rule set through the shared chase machinery.
    let all = crate::reason::run_reasoning_rules(edb, structured_el_rules())?;

    // 2. `total_facts` counts every decoded ternary row; filter the surfaced
    //    closure to the subsumption predicates.
    let total_facts = all.len();
    let inferred: Vec<InferredAxiom> = all
        .into_iter()
        .filter(|a| SUBSUMPTION_PREDICATES.contains(&a.predicate.as_str()))
        .collect();

    // 3. EL-profile limitation surface: this narrow ternary encoding cannot
    //    express entailments that quantify over the predicate position.
    let gaps = vec![
        "domain/range and property-chain entailments are NOT expressible in the \
         predicate-as-symbol ternary encoding (the predicate is a relation name, not \
         data); callers that need those entailments must use the native DL/RL \
         authority surface"
            .to_owned(),
    ];

    Ok(ElClosure {
        inferred,
        total_facts,
        gaps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const EQUIV: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const D: &str = "http://gmeow.example/D";
    const E: &str = "http://gmeow.example/E";
    const X: &str = "http://gmeow.example/x";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    /// Find an inferred axiom matching the given triple (any world).
    ///
    /// `o` is the bare object IRI; the stored axiom keeps the decoded
    /// display form (`<iri>`), so we wrap before comparing.
    fn find<'a>(closure: &'a ElClosure, s: &str, p: &str, o: &str) -> Option<&'a InferredAxiom> {
        let object_display = format!("<{o}>");
        closure
            .inferred
            .iter()
            .find(|a| a.subject == s && a.predicate == p && a.object == object_display)
    }

    #[test]
    fn subclass_transitivity_derives_a_subclass_c() {
        // A ⊑ B, B ⊑ C ⇒ A ⊑ C (derived, not asserted).
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
        let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

        let ac = find(&closure, A, SUBCLASS, C).expect("A ⊑ C must be inferred");
        assert!(!ac.is_edb, "A ⊑ C is derived, must be is_edb == false");
        assert_eq!(ac.world, W, "derived axiom carries its world IRI");
        assert!(
            !ac.premises.is_empty(),
            "derived A ⊑ C must carry antecedent premises"
        );
    }

    #[test]
    fn equivalent_class_derives_both_directions() {
        // D ≡ E ⇒ D ⊑ E and E ⊑ D.
        let store = dataset(vec![quad(D, EQUIV, E)]);
        let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

        let de = find(&closure, D, SUBCLASS, E).expect("D ⊑ E must be inferred");
        let ed = find(&closure, E, SUBCLASS, D).expect("E ⊑ D must be inferred");
        assert!(!de.is_edb, "D ⊑ E is derived");
        assert!(!ed.is_edb, "E ⊑ D is derived");
    }

    #[test]
    fn type_propagation_derives_x_type_b() {
        // x : A, A ⊑ B ⇒ x : B.
        let store = dataset(vec![quad(X, TYPE, A), quad(A, SUBCLASS, B)]);
        let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

        let xb = find(&closure, X, TYPE, B).expect("x : B must be inferred");
        assert!(!xb.is_edb, "x : B is derived, must be is_edb == false");
    }

    /// The CANONICAL `logic:` subsumption spelling drives the fixed RDFS-vocabulary
    /// EL calculus, so a taxonomy authored the way every `module.ttl` authors it is
    /// live in the SHIPPED closure — not merely parsed and then dropped.
    ///
    /// This is the EL/DL twin of `rl::tests::canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus`.
    /// It is the lane that matters to a consumer: `generated/logic/inferred-closure.rdf12.ttl`,
    /// the `graph/reasoning` projection folded into `gmeow.gts`, the `DlVerdict`, and
    /// `gmeow entails` all fold from this chase. Without the
    /// [`crate::reason::edb_predicate_spellings`] lowering in
    /// [`crate::reason::build_edb_facts`], a class authored
    /// `logic:subClassOf math:MathConformanceFailure` yields NO entailment at all: the
    /// enforcement fires but the taxonomy is dark to anyone consuming the bundle.
    #[test]
    fn canonical_logic_subsumption_drives_the_rdfs_vocabulary_el_calculus() {
        let logic_subclass = gmeow_ns::LOGIC_SUB_CLASS_OF;
        let store = dataset(vec![
            quad(X, TYPE, A),
            quad(A, logic_subclass, B),
            quad(B, logic_subclass, C),
            // A MIXED chain closes as ONE taxonomy: the canonical edge and its rdfs:
            // projection are the same edge, so D ⊑ E (canonical) ⊑ ... composes.
            quad(D, SUBCLASS, E),
            quad(E, logic_subclass, A),
        ]);
        let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

        let ac = find(&closure, A, SUBCLASS, C).expect("A ⊑ C must be inferred");
        assert!(!ac.is_edb, "A ⊑ C is derived from the canonical chain");
        let xb = find(&closure, X, TYPE, B).expect("x : B must be inferred");
        assert!(!xb.is_edb, "x : B is derived over logic:subClassOf");
        find(&closure, X, TYPE, C).expect("x : C must be inferred transitively");
        find(&closure, D, SUBCLASS, C)
            .expect("the rdfs: and logic: spellings compose into one taxonomy");

        // The projection ADDS the RDFS view rather than rewriting the authored edge
        // away, and it is ASSERTED — a projection of an asserted axiom is asserted,
        // not derived. (The authored `logic:` spelling itself is present in the chase
        // but filtered off THIS surface by [`SUBSUMPTION_PREDICATES`]; its survival is
        // asserted directly over the unfiltered RL encoding in
        // `rl::tests::canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus`.)
        let projected = find(&closure, A, SUBCLASS, B)
            .expect("the canonical edge is materialized under its rdfs: projection");
        assert!(
            projected.is_edb,
            "a projection of an asserted axiom is asserted, not derived"
        );
    }

    #[test]
    fn gaps_names_the_predicate_as_symbol_limitation() {
        let store = dataset(vec![quad(A, SUBCLASS, B)]);
        let closure = el_closure(store.as_ref()).expect("EL closure should succeed");
        assert_eq!(closure.gaps.len(), 1, "exactly one profile-limit entry");
        assert!(
            closure.gaps[0].contains("property-chain")
                && closure.gaps[0].contains("predicate-as-symbol"),
            "profile limit must name domain/range + property-chain inexpressibility: {:?}",
            closure.gaps[0]
        );
    }
}
