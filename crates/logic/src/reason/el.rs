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
/// `subject` and `predicate` are resource IRIs; `object` retains the native
/// RDF term, including scoped blanks and recursive triple terms. `world` is
/// the named-graph IRI it was derived in. `is_edb`
/// distinguishes asserted facts (`true`) from rule-derived ones (`false`).
/// `rule_name` is the firing rule's `#[name(...)]` (`None` for EDB), and
/// `premises` are the decoded immediate antecedents (subject, predicate, object),
/// preserving the native firing's order. Diagnostic projections may sort their
/// own copies without changing this execution trace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct InferredAxiom {
    /// Contextual evidence is mandatory for native modal firings and absent otherwise.
    /// Its separate allocation keeps ordinary closure rows independent of frame size.
    pub modal_evaluation: Option<Box<crate::modal::ModalEvaluation>>,
    pub subject: String,
    pub predicate: String,
    #[serde(with = "crate::term_serde")]
    pub object: purrdf::TermValue,
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

#[path = "el.tests.rs"]
#[cfg(test)]
mod tests;
