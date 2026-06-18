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
//! Currently provides the EL subsumption closure ([`el`]) and DL
//! consistency / unsatisfiability ([`dl`]); the divergence ledger lands in a
//! sibling module.

pub mod dl;
pub mod el;

pub use dl::{dl_consistency, DlVerdict, InconsistencyWitness, UnsatClass};
pub use el::{el_closure, ElClosure, InferredAxiom};

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow};
use crate::store::WorldStore;
use gmeow_rdf::RdfStore;

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

    // 2. Encode every quad of every world into ternary EDB fact lines.
    let mut edb_facts: Vec<String> = Vec::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
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
