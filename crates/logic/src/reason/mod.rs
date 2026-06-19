// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning over the Nemo chase.
//!
//! This module hosts fixed entailment rule sets — like ELK's built-in
//! calculus — that run over an arbitrary TBox/ABox through the world-scoped
//! ternary gmeow encoding. Unlike the user-authored `logic:` programs the
//! [`crate::compile`] pipeline projects, these rule sets are intrinsic to the
//! reasoner: they encode the OWL semantics themselves, not a domain ontology.
//!
//! Currently provides the EL subsumption closure ([`el`]), DL consistency /
//! unsatisfiability ([`dl`]), and the report-only divergence ledger
//! ([`ledger`]) comparing the native engine against the classic oracles.

pub mod dl;
pub mod el;
pub mod ledger;

pub use dl::{dl_consistency, DlVerdict, InconsistencyWitness, UnsatClass};
pub use el::{el_closure, ElClosure, InferredAxiom};
pub use ledger::{
    build_ledger, compare_consistency, compare_subsumption, dl_gap_rows, DivergenceKind,
    DivergenceLedger, LedgerRow,
};

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow};
use crate::store::WorldStore;
use gmeow_rdf::RdfStore;

/// The combined result of a single-chase native reasoning run.
///
/// `inferred` is the subsumption-relevant closure (filtered to
/// [`el::SUBSUMPTION_PREDICATES`], asserted + derived); `verdict` is the DL
/// consistency / unsatisfiability verdict. Both are read off the SAME
/// `Vec<InferredAxiom>` so the Nemo chase runs exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonResult {
    pub inferred: Vec<InferredAxiom>,
    pub verdict: crate::reason::dl::DlVerdict,
}

/// Run native EL subsumption + DL consistency in ONE chase.
///
/// Runs the full [`dl::dl_rules`] set (the EL calculus plus the clash-detection
/// rules) through [`run_reasoning`] exactly once, then derives both surfaces
/// from the shared `Vec<InferredAxiom>`:
///
/// - `inferred` — the subsumption-relevant closure, filtered to
///   [`el::SUBSUMPTION_PREDICATES`] (the same filter [`el::el_closure`] applies).
/// - `verdict` — read off by [`dl::verdict_from_inferred`] (the same logic
///   [`dl::dl_consistency`] uses), with `gaps` scanned over `edb`.
///
/// This avoids running Nemo twice (once for the closure, once for consistency)
/// when a caller needs both — e.g. the PyO3 `reason_native` surface.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if the gap scan fails to
/// read a quad from `edb`.
pub fn reason_all(edb: &impl RdfStore) -> Result<ReasonResult, String> {
    let all = run_reasoning(edb, &dl::dl_rules())?;
    let verdict = dl::verdict_from_inferred(&all, edb)?;
    let inferred: Vec<InferredAxiom> = all
        .into_iter()
        .filter(|a| el::SUBSUMPTION_PREDICATES.contains(&a.predicate.as_str()))
        .collect();
    Ok(ReasonResult { inferred, verdict })
}

/// Decode one antecedent chase row into a `(subject, predicate, object)` triple.
///
/// The antecedent rows are the same ternary shape as derived rows: subject is
/// an IRI term, object is any Nemo term (decoded to its display string), and
/// the third value is the world string constant (dropped here — premises carry
/// only the triple shape).
fn decode_premise(row: &ChaseRow) -> Result<(String, String, String), String> {
    if row.values.len() != 3 {
        return Err(format!(
            "antecedent row has arity {} (expected 3): {row:?}",
            row.values.len()
        ));
    }
    let subject = decode_iri_term(&row.values[0])?;
    let object = decode_nemo_term(&row.values[1])?.to_string();
    Ok((subject, row.predicate.clone(), object))
}

/// Run a fixed entailment rule set over `edb` through the Nemo chase.
///
/// Loads `edb` into a fresh [`WorldStore`], encodes every quad of every world
/// into the ternary gmeow EDB, prepends those facts to `rules`, runs the chase,
/// and decodes every 3-arity chase row into an [`InferredAxiom`] carrying its
/// raw provenance (EDB/IDB flag, firing rule name, immediate premises).
///
/// This is the shared chase machinery both [`el::el_closure`] and
/// [`dl::dl_consistency`] build on: the rule set is the only difference.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate, or if a derived row fails to decode.
pub(crate) fn run_reasoning(
    edb: &impl RdfStore,
    rules: &str,
) -> Result<Vec<InferredAxiom>, String> {
    // 1. Load the source into a fresh world-indexed store.
    let store = WorldStore::new();
    store.load_rdf_store(edb)?;

    // 2. Encode every IRI-object quad of every world into ternary EDB fact lines.
    //    The fixed EL/DL calculi only fire on axioms whose object is an IRI
    //    (subClassOf, type, disjointWith, equivalentClass, subPropertyOf), so a
    //    literal-object quad (an annotation such as rdfs:comment / dc:creator)
    //    can never participate in any rule. Skipping them is therefore sound for
    //    the closure AND the verdict, and it is also necessary: real ontology
    //    annotations carry embedded newlines that would split the line-based Nemo
    //    .rls program (`encode_literal` escapes `\`/`"` but not control chars).
    let mut edb_facts: Vec<String> = Vec::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
            if !matches!(quad.object, oxigraph::model::Term::NamedNode(_)) {
                continue;
            }
            edb_facts.push(encode_quad_to_nemo_fact(
                &quad.subject,
                &quad.predicate,
                &quad.object,
                &world,
            ));
        }
    }

    // 3. Build the program and run the chase.
    let rls = format!("{}\n{}", edb_facts.join("\n"), rules);
    let rows = run_chase(rls)?;

    // 4. Decode each ternary row into an InferredAxiom.
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for rwp in &rows {
        let row = &rwp.row;
        // Every reasoning fact is the ternary `predicate(subject, object, world)`;
        // a row with any other arity is not an inferred axiom (e.g. an internal
        // bookkeeping atom the chase may surface), so skip it rather than misdecode.
        if row.values.len() != 3 {
            continue;
        }

        let predicate = row.predicate.clone();
        let subject = decode_iri_term(&row.values[0])?;
        let object = decode_nemo_term(&row.values[1])?.to_string();
        let world = decode_string_constant(&row.values[2])?;

        let prov = &rwp.provenance;
        let premises = prov
            .antecedent_rows
            .iter()
            .map(decode_premise)
            .collect::<Result<Vec<_>, String>>()?;

        inferred.push(InferredAxiom {
            subject,
            predicate,
            object,
            world,
            is_edb: prov.is_edb,
            rule_name: prov.rule_name.clone(),
            premises,
        });
    }

    Ok(inferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::{RdfQuad, RdfTerm, VecRdfStore};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const X: &str = "http://gmeow.example/x";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    #[test]
    fn reason_all_single_chase_yields_inconsistent_and_nonempty_closure() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A — one chase must derive both the
        // subsumption closure AND the inconsistency verdict (x forced into Nothing).
        let store = VecRdfStore::with_quads(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let result = reason_all(&store).expect("reason_all should succeed");

        assert!(
            !result.verdict.consistent,
            "x forced into owl:Nothing must make the verdict inconsistent"
        );
        assert!(
            !result.inferred.is_empty(),
            "the subsumption closure must be non-empty (asserted + derived axioms)"
        );
        assert!(
            result
                .verdict
                .inconsistencies
                .iter()
                .any(|w| w.individual == X),
            "x must be an inconsistency witness: {:?}",
            result.verdict.inconsistencies
        );
    }
}
