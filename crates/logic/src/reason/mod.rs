// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning over the Nemo chase.
//!
//! This module hosts fixed entailment rule sets — like ELK's built-in
//! calculus — that run over an arbitrary TBox/ABox through the world-scoped
//! ternary gmeow encoding. Unlike the user-authored `logic:` programs the
//! the compiler pipeline projects, these rule sets are intrinsic to the
//! reasoner: they encode the OWL semantics themselves, not a domain ontology.
//!
//! Provides the EL subsumption closure ([`el`]), the predicate-as-DATA RL/DL
//! native closure ([`rl`] + [`dl`]), and the divergence ledger ([`ledger`])
//! comparing the native engine against the classic oracles.

pub mod artifacts;
pub mod dl;
pub mod el;
pub mod ledger;
pub mod rl;

pub use dl::{dl_consistency, DlVerdict, InconsistencyWitness, UnsatClass};
pub use el::{el_closure, ElClosure, InferredAxiom};
pub use ledger::{
    build_ledger, compare_consistency, compare_external_corpus, compare_subsumption, dl_gap_rows,
    DivergenceKind, DivergenceLedger, ExternalComparison, LedgerRow,
};
pub use rl::{rl_closure, RlClosure, RlTriple};

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow};
use crate::result::{ReasoningResult, ResultProvenance};
use crate::store::WorldStore;
use gmeow_rdf::RdfDataset;

/// The content-addressed identity of the native EL/DL/RL reasoning contract —
/// the `contract_hash` every native-reason result is produced under.
///
/// The hash covers ALL source that defines the reasoning contract:
/// * the three fixed rule texts (`dl_rules()`, `EL_RULES`, `RL_RULES`) whose
///   change alters which axioms the Nemo chase derives;
/// * the full source of `dl.rs`, which owns the post-pass functions
///   `augment_inferred_with_dl`, `verdict_from_inferred`, `scan_coverage`, and
///   `classify_coverage` — any edit to those changes the contract semantics even
///   when the rule text is unchanged;
/// * the source of this file (`mod.rs`), which owns the `run_reasoning` and
///   `reason_closure` orchestration glue.
///
/// A change to any of these files will produce a different hash, invalidating
/// cached results produced under the old contract.
fn native_contract_hash() -> String {
    let contract = format!(
        "{dl_rules}\n{el_rules}\n{rl_rules}\n{dl_src}\n{mod_src}",
        dl_rules = dl::dl_rules(),
        el_rules = el::EL_RULES,
        rl_rules = rl::RL_RULES,
        dl_src = include_str!("dl.rs"),
        mod_src = include_str!("mod.rs"),
    );
    crate::provenance::sha1_hex(&contract)
}

/// Run the native single-chase pipeline and return the shared
/// `(closure, DlVerdict)` it produces.
///
/// `run_reasoning → augment_inferred_with_dl → sort → verdict_from_inferred`:
/// the closure is the asserted + derived IRI-object triples; the verdict is the
/// DL consistency / unsatisfiability record read off that same closure. Both the
/// typed [`reason_all`] result and the verdict-only [`dl::dl_consistency`] entry
/// point fold from this one pipeline so they can never disagree.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo chase
/// fails to parse/validate/evaluate/decode, or if coverage/consistency scanning
/// fails.
pub(crate) fn reason_closure(
    edb: &RdfDataset,
) -> Result<(Vec<InferredAxiom>, dl::DlVerdict), String> {
    let mut inferred = run_reasoning(edb, &dl::dl_rules())?;
    dl::augment_inferred_with_dl(&mut inferred, edb)?;
    inferred.sort();
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;
    Ok((inferred, verdict))
}

/// Run native predicate-as-DATA entailment + DL consistency, returning the typed
/// [`ReasoningResult`] (#768, ME2) — the single shared result model every
/// consumer reads.
///
/// The DL verdict is folded into the result via
/// [`ReasoningResult::from_dl_verdict`]: an inconsistent verdict becomes
/// `information=both` carrying its contradiction witnesses; a consistent verdict
/// is `information=supported` (conclusively, when no construct is uncovered);
/// uncovered DL constructs surface in `preservation.unsupported_constructs` and
/// drop the completeness to `incomplete`. The DL-only diagnostics not part of the
/// shared model (the construct coverage inventory, the unsatisfiable-class set)
/// are recovered from the shared closure by [`dl::scan_coverage`] /
/// [`dl::unsatisfiable_from_inferred`] where a consumer needs them.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if coverage/consistency
/// scanning fails.
pub fn reason_all(edb: &RdfDataset) -> Result<ReasoningResult, String> {
    let (inferred, verdict) = reason_closure(edb)?;
    Ok(typed_result(inferred, &verdict))
}

/// Fold a `(closure, DlVerdict)` pair into the typed [`ReasoningResult`] under the
/// native reasoning contract. Shared by [`reason_all`] and the PyO3 boundary so
/// the typed result and the historical DL dict are projected from one fold.
///
/// The native consistency run spans every world in the bundle; the per-axiom
/// worlds are carried on the closure payload, so the result-level context world
/// is left unset (the aggregate run is not pinned to one world).
pub(crate) fn typed_result(
    inferred: Vec<InferredAxiom>,
    verdict: &dl::DlVerdict,
) -> ReasoningResult {
    let provenance = ResultProvenance::native(native_contract_hash(), "");
    ReasoningResult::from_dl_verdict(inferred, verdict, provenance)
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
pub(crate) fn run_reasoning(edb: &RdfDataset, rules: &str) -> Result<Vec<InferredAxiom>, String> {
    // 1. Load the source into a fresh world-indexed store.
    let store = WorldStore::new();
    store.load_dataset(edb)?;

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
        let mut premises = prov
            .antecedent_rows
            .iter()
            .map(decode_premise)
            .collect::<Result<Vec<_>, String>>()?;
        premises.sort();

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
    use gmeow_rdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

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

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<gmeow_rdf::RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn reason_all_single_chase_yields_inconsistent_and_nonempty_closure() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A — one chase must derive both the
        // subsumption closure AND the inconsistency verdict (x forced into Nothing).
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let result = reason_all(store.as_ref()).expect("reason_all should succeed");

        assert!(
            !result.is_consistent(),
            "x forced into owl:Nothing must make the verdict inconsistent (information=both)"
        );
        assert_eq!(
            result.information,
            crate::result::InformationState::Both,
            "an inconsistent verdict is the four-valued Belnap glut"
        );
        assert!(
            !result.inferred().is_empty(),
            "the subsumption closure must be non-empty (asserted + derived axioms)"
        );
        assert!(
            result
                .provenance
                .contradiction_witnesses
                .iter()
                .any(|w| w.individual == X),
            "x must be a contradiction witness: {:?}",
            result.provenance.contradiction_witnesses
        );
    }
}
