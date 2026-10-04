// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed witness-free relations: membership, writer refusal and exact closure.

use std::sync::Arc;

use purrdf::TermValue;

use super::*;
use crate::physical::seminaive::joint::input::JointTemplate;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const WORLD: &str = "urn:world";
const SEMANTICS: SemanticVocabulary = SemanticVocabulary::GroundedLogicV1;

fn fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    }
}

/// `R ⊑ ≥1 urn:property.class` with one member.
fn restriction(class: &str) -> Vec<Fact> {
    vec![
        fact(
            "urn:restriction",
            &format!("{OWL}onProperty"),
            "urn:property",
        ),
        Fact {
            subject: TermValue::iri("urn:restriction"),
            predicate: format!("{OWL}minQualifiedCardinality"),
            object: TermValue::simple_literal("1"),
        },
        fact("urn:restriction", &format!("{OWL}onClass"), class),
        fact("urn:individual", TYPE, "urn:restriction"),
    ]
}

fn template(rules: &[EvalRule]) -> Arc<JointTemplate> {
    Arc::new(
        JointTemplate::build(
            rules,
            &[],
            crate::reason::schema_laws(),
            SEMANTICS,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::super::JointOperation::Forward,
        )
        .unwrap(),
    )
}

/// The closed relations of one input and their computed extension.
fn closed(rules: &[EvalRule], rows: Vec<Fact>) -> (Closure, RelationStore) {
    let template = template(rules);
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    let Some(Ok(evidence)) = &input.evidence else {
        panic!("no input-specific evidence was observed");
    };
    let closable = evidence.closable.as_ref().expect("candidates within bound");
    let (closure, store, _) = template
        .termination
        .as_ref()
        .expect("a native termination template")
        .close(evidence, closable, SEMANTICS)
        .unwrap()
        .expect("closure within bound");
    (closure, store)
}

fn transitive_marker() -> (String, String) {
    (
        SEMANTICS.predicate(TYPE).to_owned(),
        SEMANTICS.analysis_symbol(TRANSITIVE).to_owned(),
    )
}

#[test]
fn a_marker_no_writer_can_produce_is_closed() {
    // Every `instanceOf` writer takes its class from a settled column (onClass,
    // domain, onProperty, ...) that never holds the transitive marker, so the pair
    // `(?, instanceOf, owl:TransitiveProperty)` is closed with its source rows.
    let mut rows = restriction("urn:witness-class");
    rows.push(fact("urn:chain", TYPE, TRANSITIVE));
    let (closure, store) = closed(&[], rows);
    assert!(
        closure.pairs.contains(&transitive_marker()),
        "{:?}",
        closure.pairs
    );
    assert!(store.contains(
        TYPE,
        &TermValue::iri("urn:chain"),
        &TermValue::iri(TRANSITIVE)
    ));
}

#[test]
fn a_marker_a_witness_may_carry_is_open() {
    // `R ⊑ ≥1 p.owl:TransitiveProperty` types every witness with the marker. The
    // minimum family mints witnesses, so it is never an eligible writer: the pair
    // must stay open, and the transitive law keeps its variable predicate.
    let mut rows = restriction(TRANSITIVE);
    rows.push(fact("urn:chain", TYPE, TRANSITIVE));
    let (closure, _) = closed(&[], rows);
    assert!(
        !closure.pairs.contains(&transitive_marker()),
        "{:?}",
        closure.pairs
    );
}

#[test]
fn the_closure_of_a_witness_free_relation_is_complete() {
    // EL subclass transitivity writes `subClassOf` from `subClassOf` alone: the
    // relation is closed, and its extension holds the derived `a ⊑ c`.
    let mut rows = restriction("urn:witness-class");
    rows.push(fact("urn:a", SUBCLASS, "urn:b"));
    rows.push(fact("urn:b", SUBCLASS, "urn:c"));
    let (closure, store) = closed(&crate::reason::el::structured_el_rules(), rows);
    assert!(
        closure.predicates.contains(SEMANTICS.predicate(SUBCLASS)),
        "{:?}",
        closure.predicates
    );
    assert!(store.contains(SUBCLASS, &TermValue::iri("urn:a"), &TermValue::iri("urn:c")));
}

#[test]
fn a_relation_written_from_an_open_relation_is_open() {
    // `type(x, Meta) → subClassOf(x, Top)` writes `subClassOf` from `instanceOf
    // Meta`, which the witnesses of `R ⊑ ≥1 p.Meta` enter: a witness would become a
    // subclass. That extension is no source closure, so `subClassOf` must stay
    // open. Without the promoter the same rows close it, so the refusal is not
    // vacuous.
    let promote = EvalRule::positive(
        "urn:rule:meta-subclass",
        EvalAtom::positive(EvalTerm::var("?x"), SUBCLASS, EvalTerm::named("urn:Top")),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            TYPE,
            EvalTerm::named("urn:Meta"),
        )],
    );
    let mut rows = restriction("urn:Meta");
    rows.push(fact("urn:a", SUBCLASS, "urn:b"));
    rows.push(fact("urn:b", SUBCLASS, "urn:c"));
    let subclass = SEMANTICS.predicate(SUBCLASS);
    let el = crate::reason::el::structured_el_rules();
    let (control, _) = closed(&el, rows.clone());
    assert!(
        control.predicates.contains(subclass),
        "{:?}",
        control.predicates
    );
    let mut rules = el;
    rules.push(promote);
    let (closure, _) = closed(&rules, rows);
    assert!(
        !closure.predicates.contains(subclass),
        "{:?}",
        closure.predicates
    );
}
