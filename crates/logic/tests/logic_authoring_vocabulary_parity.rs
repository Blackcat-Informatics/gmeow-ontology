//! The native reasoner must decide a slice authored in the canonical `logic:`
//! vocabulary IDENTICALLY to the same ontology authored in the legacy `owl:`/`rdfs:` spelling.
//!
//! Previously, the DL/EL/RL readers hardcoded the `owl:` restriction and typing IRIs and read
//! restriction bodies straight off the frozen dataset, so a `logic:`-authored class-expression or
//! typing axiom reached only the derived SHACL surface and contributed NOTHING to the reasoned
//! closure — it was dark. This test pins the fix: extending `CALCULUS_VOCABULARY` to the full
//! slice-authorable typing + class-axiom vocabulary and normalizing the raw-scan object position
//! makes the two spellings produce a byte-identical closure, and makes the `logic:`-only body
//! actually fire.
//!
//! It is the Stage-1 executable acceptance check for the `logic:`-authoring-vocabulary migration.
//! It runs the shipped [`gmeow_logic::reason::reason_closure_dataset`] entry — the production
//! closure, not a hand-built internal — on both spellings.

use gmeow_logic::reason::reason_closure_dataset;
use purrdf::{NativeRdfFormat, RdfDataset, RdfTerm, dataset_from_bytes};
use std::collections::BTreeSet;

/// The same tiny ontology in both spellings. Domain entities (`ex:` IRIs) are identical; only the
/// vocabulary namespace differs. It exercises, in one graph, every family the fix touches:
///   * the restriction body + a value-filler slot (`hasValue`), read raw off the dataset;
///   * the class-subsumption anchor that attaches the body;
///   * a property-characteristic TYPE marker in object position (`a *:TransitiveProperty`), the
///     RL-path normalization;
///   * a class-axiom predicate (`disjointWith`);
///   * the property domain/range anchors (`*:domain`/`*:range`), whose RL `prp-dom`/`prp-rng`
///     rules must fire on the canonical `logic:` spelling once it lowers to `rdfs:`.
const PREFIXES_OWL: &str = "\
@prefix ex: <http://ex/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
";

const BODY_OWL: &str = "\
ex:C rdfs:subClassOf ex:D .
ex:x rdf:type ex:C .
ex:p rdf:type owl:TransitiveProperty .
ex:a ex:p ex:b .
ex:b ex:p ex:c .
ex:E owl:disjointWith ex:F .
ex:D rdfs:subClassOf [ rdf:type owl:Restriction ; owl:onProperty ex:q ; owl:hasValue ex:v ] .
ex:r rdfs:domain ex:G .
ex:r rdfs:range ex:H .
ex:m ex:r ex:n .
";

const PREFIXES_LOGIC: &str = "\
@prefix ex: <http://ex/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";

const BODY_LOGIC: &str = "\
ex:C logic:subClassOf ex:D .
ex:x rdf:type ex:C .
ex:p rdf:type logic:transitiveProperty .
ex:a ex:p ex:b .
ex:b ex:p ex:c .
ex:E logic:disjointWith ex:F .
ex:D logic:subClassOf [ rdf:type logic:Restriction ; logic:onProperty ex:q ; logic:hasValue ex:v ] .
ex:r logic:domain ex:G .
ex:r logic:range ex:H .
ex:m ex:r ex:n .
";

fn term_key(t: &RdfTerm) -> String {
    match t {
        RdfTerm::Iri(iri) => iri.clone(),
        other => format!("{other:?}"),
    }
}

/// The set of inferred `(subject, predicate, object)` triples of the reasoned closure.
fn closure_triples(turtle: &str) -> BTreeSet<(String, String, String)> {
    let edb = dataset_from_bytes(turtle.as_bytes(), NativeRdfFormat::Turtle)
        .expect("parse the fixture turtle");
    let closure: std::sync::Arc<RdfDataset> =
        reason_closure_dataset(&edb).expect("reason the closure");
    closure
        .owned_quads()
        .map(|q| {
            (
                term_key(&q.subject),
                q.predicate.clone(),
                term_key(&q.object),
            )
        })
        .collect()
}

#[test]
fn logic_authored_closure_equals_owl_authored_closure() {
    let owl = closure_triples(&format!("{PREFIXES_OWL}{BODY_OWL}"));
    let logic = closure_triples(&format!("{PREFIXES_LOGIC}{BODY_LOGIC}"));

    // Parity: the reasoner normalizes the canonical `logic:` spelling onto the `owl:`/`rdfs:`
    // vocabulary the fixed calculi are specified in, so the two authorings must yield the SAME
    // reasoned closure. A single differing triple is the dark-vocabulary regression this pins.
    assert_eq!(
        owl,
        logic,
        "the logic:-authored closure diverged from the owl:-authored closure; a slice-authorable \
         construct is still read only under its owl: spelling.\nonly-in-owl: {:?}\nonly-in-logic: \
         {:?}",
        owl.difference(&logic).collect::<Vec<_>>(),
        logic.difference(&owl).collect::<Vec<_>>(),
    );
}

#[test]
fn logic_authored_body_is_not_dark() {
    let logic = closure_triples(&format!("{PREFIXES_LOGIC}{BODY_LOGIC}"));

    // The closure must be non-vacuous and must contain the specific entailments that ONLY exist
    // if the logic:-authored typing + restriction vocabulary is actually read:
    //   * transitivity of `ex:p` (needs the `logic:TransitiveProperty` type marker to normalize):
    let transitive = (
        "http://ex/a".to_owned(),
        "http://ex/p".to_owned(),
        "http://ex/c".to_owned(),
    );
    //   * the `logic:hasValue` restriction firing on the propagated `ex:x a ex:D` membership:
    let has_value = (
        "http://ex/x".to_owned(),
        "http://ex/q".to_owned(),
        "http://ex/v".to_owned(),
    );
    //   * plain type propagation through the `logic:subClassOf` anchor:
    let type_prop = (
        "http://ex/x".to_owned(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
        "http://ex/D".to_owned(),
    );
    //   * RL `prp-dom`: `ex:m ex:r ex:n`, `ex:r logic:domain ex:G` ⟹ `ex:m a ex:G` (needs
    //     `logic:domain` to lower onto `rdfs:domain`):
    let domain_entail = (
        "http://ex/m".to_owned(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
        "http://ex/G".to_owned(),
    );
    //   * RL `prp-rng`: `ex:m ex:r ex:n`, `ex:r logic:range ex:H` ⟹ `ex:n a ex:H`:
    let range_entail = (
        "http://ex/n".to_owned(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
        "http://ex/H".to_owned(),
    );

    assert!(
        logic.contains(&transitive),
        "transitive closure of ex:p missing — logic:TransitiveProperty went dark; got {logic:?}"
    );
    assert!(
        logic.contains(&has_value),
        "logic:hasValue restriction did not fire — the logic: restriction body went dark; got {logic:?}"
    );
    assert!(
        logic.contains(&type_prop),
        "type propagation through logic:subClassOf missing; got {logic:?}"
    );
    assert!(
        logic.contains(&domain_entail),
        "prp-dom did not fire — logic:domain went dark (no lowering to rdfs:domain); got {logic:?}"
    );
    assert!(
        logic.contains(&range_entail),
        "prp-rng did not fire — logic:range went dark (no lowering to rdfs:range); got {logic:?}"
    );
}
