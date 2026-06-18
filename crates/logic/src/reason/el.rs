// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native OWL-2-EL/RL subsumption closure over the Nemo chase.
//!
//! The rule set [`EL_RULES`] is fixed and ontology-independent: it encodes the
//! class-level OWL-2-EL/RL entailment calculus (subclass transitivity,
//! equivalence, type propagation, sub-property transitivity) directly, the way
//! ELK ships its calculus built in. We feed the TBox/ABox of any kernel store
//! through the world-scoped ternary gmeow encoding and run the chase, returning
//! the derived subsumption closure with raw chase provenance.
//!
//! # Encoding
//!
//! Every fact is the ternary `<predicate>(subject, object, "world")` form owned
//! by [`crate::encode`]; the world IRI threads through unchanged as the `?w`
//! variable. Because the predicate is a Nemo *symbol* (not data), this encoding
//! cannot express entailments that quantify over the predicate position
//! (domain/range, property chains) — see the [`ElClosure::gaps`] surface.

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow};
use crate::store::WorldStore;
use gmeow_rdf::RdfStore;

/// The fixed OWL-2-EL/RL class-level entailment rule set, in the world-scoped
/// ternary gmeow encoding. Full IRIs in angle brackets; `?w` threads the world.
pub const EL_RULES: &str = r#"
#[name("el:subClassOf-transitive")]
<http://www.w3.org/2000/01/rdf-schema#subClassOf>(?x,?z,?w) :- <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?x,?y,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?y,?z,?w) .
#[name("el:equivalentClass-fwd")]
<http://www.w3.org/2000/01/rdf-schema#subClassOf>(?x,?y,?w) :- <http://www.w3.org/2002/07/owl#equivalentClass>(?x,?y,?w) .
#[name("el:equivalentClass-bwd")]
<http://www.w3.org/2000/01/rdf-schema#subClassOf>(?y,?x,?w) :- <http://www.w3.org/2002/07/owl#equivalentClass>(?x,?y,?w) .
#[name("el:type-propagation")]
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c2,?w) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c1,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c1,?c2,?w) .
#[name("el:subPropertyOf-transitive")]
<http://www.w3.org/2000/01/rdf-schema#subPropertyOf>(?x,?z,?w) :- <http://www.w3.org/2000/01/rdf-schema#subPropertyOf>(?x,?y,?w), <http://www.w3.org/2000/01/rdf-schema#subPropertyOf>(?y,?z,?w) .
"#;

/// The subsumption predicates the EL closure surfaces. Other derived rows from
/// the chase (none, for [`EL_RULES`]) are filtered out of [`ElClosure::inferred`].
const SUBSUMPTION_PREDICATES: &[&str] = &[
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// the honest limitations of this encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElClosure {
    pub inferred: Vec<InferredAxiom>,
    pub total_facts: usize,
    pub gaps: Vec<String>,
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

/// Compute the OWL-2-EL/RL subsumption closure of `edb` via the Nemo chase.
///
/// Loads `edb` into a fresh [`WorldStore`], encodes every quad of every world
/// into the ternary gmeow EDB, runs the fixed [`EL_RULES`] through the chase,
/// and returns the decoded subsumption closure with raw provenance.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate, or if a derived row fails to decode.
pub fn el_closure(edb: &impl RdfStore) -> Result<ElClosure, String> {
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
    let rls = format!("{}\n{}", edb_facts.join("\n"), EL_RULES);
    let rows = run_chase(rls)?;

    // 4. Decode each ternary row into an InferredAxiom, filtering to the
    //    subsumption predicates. `total_facts` counts all ternary rows.
    let mut total_facts = 0usize;
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for rwp in &rows {
        let row = &rwp.row;
        if row.values.len() != 3 {
            continue;
        }
        total_facts += 1;

        let predicate = row.predicate.clone();
        if !SUBSUMPTION_PREDICATES.contains(&predicate.as_str()) {
            continue;
        }

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

    // 5. Honest DL-gap surface: the ternary predicate-as-symbol encoding cannot
    //    express entailments that quantify over the predicate position.
    let gaps = vec![
        "domain/range and property-chain entailments are NOT expressible in the \
         predicate-as-symbol ternary encoding (the predicate is a Nemo symbol, not \
         data); they require a predicate-as-data reformulation and are deferred to \
         the DL-gap surface"
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
    use gmeow_rdf::{RdfQuad, RdfTerm, VecRdfStore};

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

    /// Find an inferred axiom matching the given triple (any world).
    ///
    /// `o` is the bare object IRI; the stored axiom keeps the decoded Nemo
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
        let store = VecRdfStore::with_quads(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
        let closure = el_closure(&store).expect("EL closure should succeed");

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
        let store = VecRdfStore::with_quads(vec![quad(D, EQUIV, E)]);
        let closure = el_closure(&store).expect("EL closure should succeed");

        let de = find(&closure, D, SUBCLASS, E).expect("D ⊑ E must be inferred");
        let ed = find(&closure, E, SUBCLASS, D).expect("E ⊑ D must be inferred");
        assert!(!de.is_edb, "D ⊑ E is derived");
        assert!(!ed.is_edb, "E ⊑ D is derived");
    }

    #[test]
    fn type_propagation_derives_x_type_b() {
        // x : A, A ⊑ B ⇒ x : B.
        let store = VecRdfStore::with_quads(vec![quad(X, TYPE, A), quad(A, SUBCLASS, B)]);
        let closure = el_closure(&store).expect("EL closure should succeed");

        let xb = find(&closure, X, TYPE, B).expect("x : B must be inferred");
        assert!(!xb.is_edb, "x : B is derived, must be is_edb == false");
    }

    #[test]
    fn gaps_names_the_predicate_as_symbol_limitation() {
        let store = VecRdfStore::with_quads(vec![quad(A, SUBCLASS, B)]);
        let closure = el_closure(&store).expect("EL closure should succeed");
        assert_eq!(closure.gaps.len(), 1, "exactly one honest gap entry");
        assert!(
            closure.gaps[0].contains("property-chain")
                && closure.gaps[0].contains("predicate-as-symbol"),
            "gap must name domain/range + property-chain inexpressibility: {:?}",
            closure.gaps[0]
        );
    }
}
