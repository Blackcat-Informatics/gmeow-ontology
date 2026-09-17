// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

#[test]
fn consistency_information_preserves_conflicts_and_independent_coverage() {
    use crate::result::InformationState;
    let complete = DlVerdict {
        consistent: true,
        unsatisfiable_classes: Vec::new(),
        inconsistencies: Vec::new(),
        coverage: DlCoverage {
            present: Vec::new(),
            decided: Vec::new(),
            unsupported: Vec::new(),
        },
        gaps: Vec::new(),
        boundary_findings: Vec::new(),
    };
    assert_eq!(complete.information_state(), InformationState::Supported);
    let mut gap = complete.clone();
    gap.gaps.push(DlGap::new(
        "reason.dl-gap.native-frontier",
        "unfinished native stratum",
    ));
    let mut unsupported = complete.clone();
    unsupported
        .coverage
        .unsupported
        .push("uncompleted class model".to_owned());
    for mut verdict in [gap, unsupported] {
        assert_eq!(verdict.information_state(), InformationState::Undetermined);
        verdict.consistent = false;
        verdict.inconsistencies.push(InconsistencyWitness {
            individual: "urn:classification:individual".to_owned(),
            world: "urn:classification:conflict".to_owned(),
            premises: vec![(
                "urn:classification:individual".to_owned(),
                RDF_TYPE.to_owned(),
                OWL_NOTHING.to_owned(),
            )],
        });
        let retained = verdict.clone();
        assert_eq!(verdict.information_state(), InformationState::Both);
        assert_eq!(
            verdict, retained,
            "classification must not consume gap or witness evidence"
        );
    }
}

const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const OWL_WITH_RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const OWL_ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const XSD_MIN_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const XSD_MAX_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const XSD_MIN_EXCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minExclusive";
const XSD_PATTERN: &str = "http://www.w3.org/2001/XMLSchema#pattern";
const XSD_MIN_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#minLength";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const OWL_TOP_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#topObjectProperty";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_BOTTOM_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
const OWL_BOTTOM_DATA_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
const OWL_HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";
const LOGIC_FUNCTIONAL_PROPERTY: &str = "https://blackcatinformatics.ca/logic/functionalProperty";
const LOGIC_KEY_ASSERTION: &str = "https://blackcatinformatics.ca/logic/KeyAssertion";
const LOGIC_KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
const LOGIC_KEY_PROPERTY: &str = "https://blackcatinformatics.ca/logic/keyProperty";
const OWL_NEGATIVE_PROPERTY_ASSERTION: &str =
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion";
const OWL_SOURCE_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
const OWL_ASSERTION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
const OWL_TARGET_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#targetIndividual";
const OWL_TARGET_VALUE: &str = "http://www.w3.org/2002/07/owl#targetValue";
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
const OWL_PROPERTY_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const RDF_XML_LITERAL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral";
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_HAS_SELF: &str = "http://www.w3.org/2002/07/owl#hasSelf";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";

fn native_cardinality_clashes(source: &RdfDataset) -> Vec<InconsistencyWitness> {
    let result = crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(source).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("one native execution");
    let execution = result.native_execution().expect("retained native evidence");
    let mut clashes = Vec::new();
    for ledger in &execution.families {
        for outcome in &ledger.outcomes {
            if outcome.family != NativeRefutationFamily::Cardinality {
                continue;
            }
            for clash in &outcome.conclusions {
                assert!(
                    clash.committed.is_some(),
                    "the unbounded product run must publish its supported local clash"
                );
                let premises = ledger
                    .source_leaves(&clash.support)
                    .expect("original native support")
                    .into_iter()
                    .map(|source| {
                        (
                            source
                                .subject
                                .as_iri()
                                .expect("named test subject")
                                .to_owned(),
                            source.predicate,
                            crate::provenance::term_display(&source.object),
                        )
                    })
                    .collect();
                clashes.push(InconsistencyWitness {
                    individual: clash.subject.as_iri().expect("named test clash").to_owned(),
                    world: ledger.world.clone(),
                    premises,
                });
            }
        }
    }
    clashes
}

const W: &str = "http://gmeow.example/w";
const SUBCLASS: &str = RDFS_SUBCLASSOF;
const TYPE: &str = RDF_TYPE;
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";

const A: &str = "http://gmeow.example/A";
const B: &str = "http://gmeow.example/B";
const C: &str = "http://gmeow.example/C";
const R: &str = "http://gmeow.example/R";
const S: &str = "http://gmeow.example/S";
const P: &str = "http://gmeow.example/p";
const X: &str = "http://gmeow.example/x";
const Y: &str = "http://gmeow.example/y";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn literal_quad(s: &str, p: &str, value: &str, datatype: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri(s),
        p,
        RdfTerm::Literal(RdfLiteral::typed(value, datatype)),
    )
    .in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

#[test]
fn nominal_coverage_and_membership_use_the_referenced_shared_tail() {
    let first = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    let rest = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    let nil = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let data = dataset(vec![
        quad(A, first, X),
        quad(A, rest, B),
        quad(B, first, Y),
        quad(B, rest, nil),
        quad(C, OWL_ONE_OF, B),
    ]);
    let (inferred, verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(data.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(verdict.coverage.decided.contains(&"oneOf".to_owned()));
    assert!(
        inferred.iter().any(|row| row.subject == Y
            && row.predicate == TYPE
            && row.object == TermValue::iri(C))
    );
    assert!(
        !inferred.iter().any(|row| row.subject == X
            && row.predicate == TYPE
            && row.object == TermValue::iri(C))
    );
}

#[test]
fn malformed_pairwise_list_withholds_coverage_and_cannot_emit_prefix_distinctness() {
    let first = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    let rest = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    let data = dataset(vec![
        quad(C, TYPE, OWL_ALL_DIFFERENT),
        quad(C, OWL_MEMBERS, A),
        quad(A, first, X),
        quad(A, rest, B),
        quad(B, first, Y),
        quad(B, rest, A),
    ]);
    let error = crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(data.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect_err("cyclic selected AllDifferent grammar refuses before any writer runs");
    let refusal = error
        .downcast_ref::<crate::error::NativeSourceAdmission>()
        .expect("original source admission has its typed refusal");
    assert_ne!(refusal.input_contract, [0; 32]);
    assert_eq!(
        refusal.admission.refusal_class().unwrap(),
        Some(crate::reason::refute::ClassSourceRefusal::Invalid)
    );
    let world = &refusal.admission.selected_worlds[W];
    assert_eq!(world.graph, Some(TermValue::iri(W)));
    assert!(world.refusal.is_some());
    assert!(
        world
            .definitions
            .iter()
            .any(|row| row.subject == TermValue::iri(C)
                && row.predicate == OWL_MEMBERS
                && row.object == TermValue::iri(A))
    );
    // An Err carries neither a consistency verdict nor a publishable closure,
    // so a valid prefix cannot leak invented differentFrom assertions.
}

#[test]
fn authored_existential_rules_are_read_and_certified_per_world() {
    // An authored general existential rule (arbitrary body/head atoms — NOT an OWL
    // restriction) is read per-world and certified by the termination-class ladder.
    const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
    const EX_P: &str = "http://ex/p";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let lx = |s: &str| format!("{LX}{s}");
    // A `?var` term is encoded as a string literal; a constant as an IRI.
    let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
    // MSA swap-diagonal program: `p(x,x) → ∃y. p(x,y)` and `p(x,y) → p(y,x)`.
    let store = dataset(vec![
        quad("http://ex/demo/invent", TYPE, &lx("ExistentialRule")),
        quad("http://ex/demo/invent", &lx("body"), "http://ex/demo/b1"),
        quad("http://ex/demo/invent", &lx("head"), "http://ex/demo/h1"),
        var("http://ex/demo/b1", &lx("s"), "?x"),
        quad("http://ex/demo/b1", &lx("p"), EX_P),
        var("http://ex/demo/b1", &lx("o"), "?x"),
        var("http://ex/demo/h1", &lx("s"), "?x"),
        quad("http://ex/demo/h1", &lx("p"), EX_P),
        var("http://ex/demo/h1", &lx("o"), "?y"),
        quad("http://ex/demo/swap", TYPE, &lx("ExistentialRule")),
        quad("http://ex/demo/swap", &lx("body"), "http://ex/demo/b2"),
        quad("http://ex/demo/swap", &lx("head"), "http://ex/demo/h2"),
        var("http://ex/demo/b2", &lx("s"), "?x"),
        quad("http://ex/demo/b2", &lx("p"), EX_P),
        var("http://ex/demo/b2", &lx("o"), "?y"),
        var("http://ex/demo/h2", &lx("s"), "?y"),
        quad("http://ex/demo/h2", &lx("p"), EX_P),
        var("http://ex/demo/h2", &lx("o"), "?x"),
    ]);
    let by_world = super::super::prepare_reasoning_input(store.as_ref())
        .and_then(|input| input.sources.prepare())
        .expect("well-formed authored rules");
    assert!(
        by_world.rules.iter().all(|source| source.world == W),
        "authored rules land in their graph's world"
    );
    let rules: Vec<_> = by_world
        .rules
        .iter()
        .map(|source| source.rule.clone())
        .collect();
    assert_eq!(rules.len(), 2, "both authored rules parsed");
    match crate::physical::ChaseAdmission::certify(&rules) {
        crate::physical::ChaseAdmission::ModelSummarizingAcyclic { .. } => {}
        other => panic!("swap-diagonal must certify as ModelSummarizingAcyclic, got {other:?}"),
    }
}

#[test]
fn authored_rule_with_malformed_atom_hard_fails_not_silently_dropped() {
    // A declared body atom missing its `logicx:o` must HARD FAIL — never a silent drop.
    // Silently dropping the conjunct would leave the rule with a smaller body, firing
    // more often and deriving facts the author never wrote (no-optionality violation).
    const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
    const EX_P: &str = "http://ex/p";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let lx = |s: &str| format!("{LX}{s}");
    let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
    let store = dataset(vec![
        quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
        quad("http://ex/demo/r", &lx("body"), "http://ex/demo/b1"),
        quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
        // b1 declares its subject and predicate but is MISSING its logicx:o (object).
        var("http://ex/demo/b1", &lx("s"), "?x"),
        quad("http://ex/demo/b1", &lx("p"), EX_P),
        var("http://ex/demo/h1", &lx("s"), "?x"),
        quad("http://ex/demo/h1", &lx("p"), EX_P),
        var("http://ex/demo/h1", &lx("o"), "?y"),
    ]);
    let err = super::super::prepare_reasoning_input(store.as_ref())
        .and_then(|input| input.sources.prepare())
        .expect_err("a declared atom missing logicx:o must hard-fail, not be dropped");
    let msg = format!("{err}");
    assert!(
        msg.contains("logicx:o") && msg.contains("b1"),
        "error must name the missing slot and the offending atom: {msg}"
    );
}

#[test]
fn authored_rule_with_non_resource_body_ref_hard_fails() {
    // A `logicx:body` whose value is a literal (not a resource) cannot name an atom node.
    // Silently dropping it would leave the rule with fewer body conjuncts than authored —
    // a broadening. It must HARD FAIL at collection time, not be skipped.
    const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
    const EX_P: &str = "http://ex/p";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let lx = |s: &str| format!("{LX}{s}");
    let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
    let store = dataset(vec![
        quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
        // logicx:body points at a LITERAL — not a resource, so it names no atom node.
        literal_quad("http://ex/demo/r", &lx("body"), "not-a-node", XSD_STRING),
        quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
        var("http://ex/demo/h1", &lx("s"), "?x"),
        quad("http://ex/demo/h1", &lx("p"), EX_P),
        var("http://ex/demo/h1", &lx("o"), "?y"),
    ]);
    let err = super::super::prepare_reasoning_input(store.as_ref())
        .and_then(|input| input.sources.prepare())
        .expect_err("a non-resource logicx:body must hard-fail, not be silently dropped");
    let msg = format!("{err}");
    assert!(
        msg.contains("logicx:body") && msg.contains("not a resource"),
        "error must name the slot and the non-resource cause: {msg}"
    );
}

#[test]
fn authored_rule_with_duplicate_slot_hard_fails() {
    // Two `logicx:s` triples on one atom node would silently OVERWRITE the first,
    // reinterpreting the authored atom. A duplicate slot must HARD FAIL, not pick a winner.
    const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
    const EX_P: &str = "http://ex/p";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let lx = |s: &str| format!("{LX}{s}");
    let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
    let store = dataset(vec![
        quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
        quad("http://ex/demo/r", &lx("body"), "http://ex/demo/b1"),
        quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
        // b1 declares its subject TWICE — a duplicate slot.
        var("http://ex/demo/b1", &lx("s"), "?x"),
        var("http://ex/demo/b1", &lx("s"), "?z"),
        quad("http://ex/demo/b1", &lx("p"), EX_P),
        var("http://ex/demo/b1", &lx("o"), "?y"),
        var("http://ex/demo/h1", &lx("s"), "?x"),
        quad("http://ex/demo/h1", &lx("p"), EX_P),
        var("http://ex/demo/h1", &lx("o"), "?y"),
    ]);
    let err = super::super::prepare_reasoning_input(store.as_ref())
        .and_then(|input| input.sources.prepare())
        .expect_err("a duplicate logicx:s must hard-fail, not silently overwrite");
    let msg = format!("{err}");
    assert!(
        msg.contains("logicx:s") && msg.contains("more than one"),
        "error must name the duplicated slot: {msg}"
    );
}

#[test]
fn disjoint_superclasses_make_a_unsat_and_x_inconsistent() {
    // A ⊑ B, A ⊑ C, B disjointWith C, x : A
    // ⇒ A is unsatisfiable, and x is forced into owl:Nothing (inconsistent).
    let store = dataset(vec![
        quad(A, SUBCLASS, B),
        quad(A, SUBCLASS, C),
        quad(B, DISJOINT, C),
        quad(X, TYPE, A),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "x forced into owl:Nothing must make the ontology inconsistent"
    );
    assert!(
        verdict.unsatisfiable_classes.iter().any(|u| u.class == A),
        "A must be reported unsatisfiable: {:?}",
        verdict.unsatisfiable_classes
    );
    let witness = verdict
        .inconsistencies
        .iter()
        .find(|w| w.individual == X)
        .expect("x must be an inconsistency witness");
    assert_eq!(witness.world, W, "witness carries its world IRI");
    assert!(
        !witness.premises.is_empty(),
        "derived inconsistency must carry antecedent premises"
    );
}

#[test]
fn no_disjointness_is_consistent() {
    // A ⊑ B, x : A — no disjointness ⇒ consistent, no inconsistencies.
    let store = dataset(vec![quad(A, SUBCLASS, B), quad(X, TYPE, A)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(verdict.consistent, "no clash ⇒ consistent");
    assert!(
        verdict.inconsistencies.is_empty(),
        "no individual should be forced into owl:Nothing"
    );
}

#[test]
fn complement_of_is_decided_and_can_clash() {
    // A complementOf B, x : A, x : B ⇒ x : owl:Nothing. This construct is
    // decided natively, so it must NOT surface as a DlGap.
    let store = dataset(vec![
        quad(A, OWL_COMPLEMENT_OF, B),
        quad(X, TYPE, A),
        quad(X, TYPE, B),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(!verdict.consistent, "complement clash must be inconsistent");
    assert!(
        verdict.gaps.is_empty(),
        "owl:complementOf is decided natively, not a gap: {:?}",
        verdict.gaps
    );
    assert!(
        verdict
            .coverage
            .present
            .contains(&"complementOf".to_owned()),
        "coverage records complementOf as present: {:?}",
        verdict.coverage
    );
}

// ── Refutation-shape withholds (Wave B) ──────────────────────────────────
// Each feeds a native-undecidable refutation shape and asserts the verdict
// honestly WITHHOLDS: a non-empty `gaps` (the `incomplete` token) and NOT a
// wrong decided verdict. Falsifiable: a reasoner that silently ignored the
// axiom would report a decided `consistent` with empty `gaps` and fail here.

const ONE_OF: &str = OWL_ONE_OF;
const UNION_OF: &str = OWL_UNION_OF;
const COMPLEMENT_OF: &str = OWL_COMPLEMENT_OF;
const EQUIV_CLASS: &str = OWL_EQUIVALENT_CLASS;
const MIN_CARDINALITY: &str = OWL_MIN_CARDINALITY;
const CARDINALITY: &str = OWL_CARDINALITY;
const DIFFERENT_FROM: &str = OWL_DIFFERENT_FROM;
const HAS_SELF: &str = OWL_HAS_SELF;
const INVERSE_FUNCTIONAL: &str = OWL_INVERSE_FUNCTIONAL_PROPERTY;
const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

fn assert_withheld(verdict: &DlVerdict, token: &str) {
    assert!(
        !verdict.gaps.is_empty(),
        "expected a non-empty gap (honest cannot-decide), got none"
    );
    assert!(
        verdict.coverage.unsupported.iter().any(|u| u == token),
        "expected withheld family {token:?} in coverage.unsupported, got {:?}",
        verdict.coverage.unsupported
    );
}

#[test]
fn complement_in_class_definition_is_decided_consistent() {
    // A ⊑ ¬D — the complement node is a `rdfs:subClassOf` superclass (a class
    // definition), with NO individual forced into it. The Family-1 case-split
    // refutation sub-decider ([`crate::reason::refute::casesplit`]) now COMPLETELY
    // decides this propositional-fragment case: an individual-free complement TBox
    // is trivially satisfiable (the empty interpretation is a model), so it is
    // decided CONSISTENT with no honest gap — no longer the pre-sub-decider
    // conservative withhold.
    let store = dataset(vec![
        quad(A, SUBCLASS, "http://gmeow.example/ncomp"),
        quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "an individual-free complement TBox is satisfiable"
    );
    assert!(
        !verdict.gaps.iter().any(|g| g.code.contains("complementOf"))
            && !verdict
                .coverage
                .unsupported
                .iter()
                .any(|u| u == "complementOf"),
        "the case-split sub-decider certifies this — no complementOf gap: {:?}",
        verdict.coverage
    );
}

#[test]
fn complement_filler_in_restriction_is_withheld() {
    // A ⊑ ∃p.(¬D): complement as a `someValuesFrom` filler is a class-definition
    // position ⇒ honest gap.
    let store = dataset(vec![
        quad(A, SUBCLASS, R),
        quad(R, ON_PROPERTY, P),
        quad(R, SOME_VALUES_FROM, "http://gmeow.example/ncomp"),
        quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert_withheld(&verdict, "complementOf");
}

#[test]
fn complement_typed_individual_without_asserted_membership_is_decided_consistent() {
    // x : ¬D, but x is NOT asserted x : D. The Family-1 case-split sub-decider now
    // decides this: `x ∈ ¬D` with no forced `x ∈ D` saturates clash-free inside
    // the certified-complete fragment, so it is decided CONSISTENT (a model has
    // `x ∉ D`) — no longer the pre-sub-decider withhold. Contrast the decided
    // clash test where BOTH memberships are asserted (decided INCONSISTENT).
    let store = dataset(vec![
        quad(X, TYPE, "http://gmeow.example/ncomp"),
        quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "x ∈ ¬D with no forced x ∈ D is satisfiable"
    );
    assert!(
        !verdict.gaps.iter().any(|g| g.code.contains("complementOf"))
            && !verdict
                .coverage
                .unsupported
                .iter()
                .any(|u| u == "complementOf"),
        "the case-split sub-decider certifies this — no complementOf gap: {:?}",
        verdict.coverage
    );
}

#[test]
fn min_cardinality_on_a_class_definition_is_decided_consistent() {
    // C ≡ (≥2 p): a pure TBox cardinality counting definition. The Family-2
    // counting refutation sub-decider now COMPLETELY decides the pure
    // class-definition cardinality fragment: an uncollapsed bound (no `min > max`
    // conflict) on a class is satisfiable, so this is decided CONSISTENT with no
    // honest gap — no longer the pre-sub-decider withhold.
    let store = dataset(vec![
        quad(C, EQUIV_CLASS, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MIN_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "an uncollapsed ≥2 definition is satisfiable"
    );
    assert!(
        verdict.gaps.is_empty(),
        "the counting sub-decider certifies this — no honest gap: {:?}",
        verdict.gaps
    );
    assert!(
        verdict
            .coverage
            .decided
            .iter()
            .any(|d| d == "minCardinality"),
        "the minCardinality family is promoted to decided: {:?}",
        verdict.coverage
    );
}

/// Canonical class operators and blank owners drive the same native rule
/// execution while retaining the exact source terms in their proof leaves.
#[test]
fn canonical_logic_restriction_on_a_blank_anchor_drives_the_closure() {
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    const OWL: &str = "http://www.w3.org/2002/07/owl#";
    let blank_quad = |s: &str, p: &str, o: RdfTerm| {
        RdfQuad::new(RdfTerm::BlankNode(s.to_owned()), p, o).in_graph(RdfTerm::iri(W))
    };
    // Both spellings of: C ⊑ ∀p.D, x : C, x p y  ⊨  y : D.
    let body = |ns: &str, subclass: &str| {
        dataset(vec![
            RdfQuad::new(
                RdfTerm::iri(C),
                subclass,
                RdfTerm::BlankNode("r".to_owned()),
            )
            .in_graph(RdfTerm::iri(W)),
            blank_quad("r", RDF_TYPE, RdfTerm::iri(format!("{ns}Restriction"))),
            blank_quad("r", &format!("{ns}onProperty"), RdfTerm::iri(P)),
            blank_quad("r", &format!("{ns}allValuesFrom"), RdfTerm::iri(D)),
            quad(X, TYPE, C),
            quad(X, P, Y),
        ])
    };
    let types_the_filler = |edb: &RdfDataset| {
        crate::reason::reason_closure_axioms(
            crate::reason::prepare_reasoning_input(edb).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
        )
        .expect("closure")
        .iter()
        .any(|ax| {
            ax.subject == Y
                && ax.predicate == RDF_TYPE
                && crate::provenance::term_display(&ax.object) == format!("<{D}>")
        })
    };
    assert!(
        types_the_filler(body(OWL, RDFS_SUBCLASSOF).as_ref()),
        "control: the `owl:`-spelled body must type the filler"
    );
    assert!(
        types_the_filler(body(LOGIC, &format!("{LOGIC}subClassOf")).as_ref()),
        "the CANONICAL `logic:` body must type the filler identically — an authored \
             class expression is reasoner content, not derived-shape-only content"
    );
}

/// The H2 class-definition cardinality withhold fires on the shape it is FOR.
///
/// An effective per-class/per-property bound of exactly one is the functional
/// statement the engine already declines to withhold for in its one-node `owl:cardinality 1`
/// spelling, so the two-node `min 1` + `max 1` spelling is decided too. A one-sided
/// bound and an effective minimum ≥ 2 stay honest gaps.
#[test]
fn class_definition_cardinality_withhold_is_scoped_to_the_non_functional_bound() {
    let r2 = "http://gmeow.example/r2";
    // The pure-fragment sub-decider must NOT be the thing deciding these, or the
    // narrowing under test is never reached: an ordinary domain predicate takes the
    // case out of its allowlist, exactly as a real bundle does.
    let noise = quad(C, "http://gmeow.example/unrelated", D);

    let exact_one = dataset(vec![
        quad(C, SUBCLASS, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MIN_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(C, SUBCLASS, r2),
        quad(r2, ON_PROPERTY, P),
        literal_quad(r2, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        noise.clone(),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(exact_one.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.gaps.is_empty(),
        "an effective `= 1` bound is the functional case the engine decides: {:?}",
        verdict.gaps
    );

    let one_sided = dataset(vec![
        quad(C, SUBCLASS, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MIN_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        noise.clone(),
    ]);
    assert_withheld(
        &dl_consistency(
            crate::reason::prepare_reasoning_input(one_sided.as_ref()).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
        )
        .expect("dl consistency should succeed"),
        "minCardinality",
    );

    let counting = dataset(vec![
        quad(C, SUBCLASS, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MIN_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(C, SUBCLASS, r2),
        quad(r2, ON_PROPERTY, P),
        literal_quad(r2, MAX_CARDINALITY, "3", XSD_NON_NEGATIVE_INTEGER),
        noise,
    ]);
    assert_withheld(
        &dl_consistency(
            crate::reason::prepare_reasoning_input(counting.as_ref()).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
        )
        .expect("dl consistency should succeed"),
        "minCardinality",
    );
}

/// A simple-literal cardinality bound reads the same before and after the RDF 1.1
/// `xsd:string` normalization a snapshot round-trip applies — `"1"` and
/// `"1"^^xsd:string` are the same term, so they cannot yield different verdicts.
/// A non-numeric lexical form is still refused under both.
#[test]
fn a_simple_literal_bound_reads_identically_typed_and_untyped() {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let mut values = crate::reason::value::NativeValues::default();
    let mut cardinality = |literal: &RdfLiteral| {
        values.cardinality(&TermValue::Literal {
            lexical_form: literal.lexical_form.clone(),
            datatype: literal.datatype_iri().to_owned(),
            language: literal.language.clone(),
            direction: literal.direction,
        })
    };
    assert_eq!(cardinality(&RdfLiteral::simple("1")), Some(1));
    assert_eq!(cardinality(&RdfLiteral::typed("1", XSD_STRING)), Some(1));
    assert_eq!(cardinality(&RdfLiteral::simple("many")), None);
    assert_eq!(cardinality(&RdfLiteral::typed("many", XSD_STRING)), None);
    // A bound in a datatype that is not an integer tower member stays refused.
    assert_eq!(
        cardinality(&RdfLiteral::typed(
            "1",
            "http://www.w3.org/2001/XMLSchema#decimal"
        )),
        None
    );
}

#[test]
fn collapsed_cardinality_on_a_populated_class_is_decided_inconsistent() {
    // C ⊑ (≥2 p) ⊓ (≤1 p), i : C — the collapsed bound makes the populated class
    // unsatisfiable, so the Family-2 sub-decider materializes `owl:Nothing` on the
    // instance: decided INCONSISTENT with no honest gap.
    let r2 = "http://gmeow.example/r2";
    let store = dataset(vec![
        quad(C, SUBCLASS, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MIN_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(C, SUBCLASS, r2),
        quad(r2, ON_PROPERTY, P),
        literal_quad(r2, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, C),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "a collapsed min>max bound on a populated class is inconsistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "the counting sub-decider decides this — no honest gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn exact_cardinality_over_finite_datatype_range_is_decided_by_family5() {
    // Family 5 (the datatype value-space refutation sub-decider) now DECIDES the
    // datatype value-space counting the forward chase cannot refute. With p an
    // `owl:DatatypeProperty` whose `rdfs:range` is `xsd:byte` (value-space size
    // 256, derived from the `math:`-grounded facts):
    //   * `x : (=257 p)` forces 257 distinct byte values into a 256-element
    //     space ⇒ pigeonhole INCONSISTENT (an `owl:Nothing` clash, empty gaps);
    //   * `x : (=256 p)` fits exactly ⇒ CONSISTENT (empty gaps).
    const BYTE: &str = "http://www.w3.org/2001/XMLSchema#byte";
    const RANGE: &str = RDFS_RANGE;

    let inconsistent = dataset(vec![
        quad(X, TYPE, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, CARDINALITY, "257", XSD_NON_NEGATIVE_INTEGER),
        quad(P, TYPE, DATATYPE_PROPERTY),
        quad(P, RANGE, BYTE),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(inconsistent.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "257 distinct xsd:byte values overflow the 256-element value space ⇒ inconsistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "Family 5 decides this — no honest gap remains: {:?}",
        verdict.gaps
    );
    assert!(
        verdict.coverage.decided.iter().any(|d| d == "cardinality"),
        "the cardinality family is promoted to decided: {:?}",
        verdict.coverage
    );

    let consistent = dataset(vec![
        quad(X, TYPE, R),
        quad(R, ON_PROPERTY, P),
        literal_quad(R, CARDINALITY, "256", XSD_NON_NEGATIVE_INTEGER),
        quad(P, TYPE, DATATYPE_PROPERTY),
        quad(P, RANGE, BYTE),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(consistent.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "256 distinct xsd:byte values fit the value space exactly ⇒ consistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "Family 5 certifies the consistent count — no honest gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn individual_typed_to_two_enumerations_is_withheld() {
    // x : {a,b} AND x : {c,d} — nominal counting across two enumerations ⇒ gap.
    // (The single-enumeration `differentFrom`-all-members clash stays decided.)
    let e1 = "http://gmeow.example/E1";
    let e2 = "http://gmeow.example/E2";
    let store = dataset(vec![
        quad(e1, ONE_OF, "http://gmeow.example/l0"),
        quad("http://gmeow.example/l0", FIRST, A),
        quad("http://gmeow.example/l0", REST, NIL),
        quad(e2, ONE_OF, "http://gmeow.example/l1"),
        quad("http://gmeow.example/l1", FIRST, B),
        quad("http://gmeow.example/l1", REST, NIL),
        quad(X, TYPE, e1),
        quad(X, TYPE, e2),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert_withheld(&verdict, "oneOf");
}

#[test]
fn class_with_two_union_superclasses_is_decided_consistent() {
    // C ⊑ (A∪B) AND C ⊑ (X∪Y): the multi-disjunction propositional shape. With NO
    // individual forced into C, the Family-3 case-split sub-decider decides it
    // CONSISTENT (the empty model satisfies every disjunctive superclass) — no
    // longer the pre-sub-decider conservative withhold. The propositional
    // refutation (every branch closing) is exercised end-to-end on the committed
    // `webont-description-logic-503`/`504` SAT pair.
    let u1 = "http://gmeow.example/u1";
    let u2 = "http://gmeow.example/u2";
    let store = dataset(vec![
        quad(C, SUBCLASS, u1),
        quad(u1, UNION_OF, "http://gmeow.example/lu1"),
        quad("http://gmeow.example/lu1", FIRST, A),
        quad("http://gmeow.example/lu1", REST, NIL),
        quad(C, SUBCLASS, u2),
        quad(u2, UNION_OF, "http://gmeow.example/lu2"),
        quad("http://gmeow.example/lu2", FIRST, B),
        quad("http://gmeow.example/lu2", REST, NIL),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "an individual-free union TBox is satisfiable"
    );
    assert!(
        !verdict.gaps.iter().any(|g| g.code.contains("unionOf"))
            && !verdict.coverage.unsupported.iter().any(|u| u == "unionOf"),
        "the case-split sub-decider certifies this — no unionOf gap: {:?}",
        verdict.coverage
    );
}

#[test]
fn hasself_disjoint_refutation_is_decided_inconsistent() {
    // C disjointWith [∃p.Self], x : C, x p x. The Family-7 counting sub-decider
    // now infers `x ∈ ∃p.Self` from the self-edge and clashes it against the
    // disjoint class x also holds: decided INCONSISTENT with no honest gap — no
    // longer the pre-sub-decider withhold.
    let store = dataset(vec![
        quad(C, DISJOINT, R),
        quad(R, TYPE, "http://www.w3.org/2002/07/owl#Restriction"),
        literal_quad(
            R,
            HAS_SELF,
            "true",
            "http://www.w3.org/2001/XMLSchema#boolean",
        ),
        quad(R, ON_PROPERTY, P),
        quad(X, TYPE, C),
        quad(X, P, X),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "a self-edge inhabiting a disjoint self-restriction is inconsistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "the hasSelf sub-decider decides this — no honest gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn hasself_typed_onto_individual_is_decided_not_a_gap() {
    // x : ∃p.Self with no disjointness — a benign OWL 2 EL self-restriction.
    // Decided consistent, NO gap (the EL grade's `selfrestriction` case).
    let store = dataset(vec![
        quad(X, TYPE, R),
        quad(R, TYPE, "http://www.w3.org/2002/07/owl#Restriction"),
        literal_quad(
            R,
            HAS_SELF,
            "true",
            "http://www.w3.org/2001/XMLSchema#boolean",
        ),
        quad(R, ON_PROPERTY, P),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(verdict.consistent, "a bare self-restriction is consistent");
    assert!(
        !verdict.coverage.unsupported.iter().any(|u| u == "hasSelf"),
        "benign hasSelf must NOT be withheld: {:?}",
        verdict.coverage.unsupported
    );
}

#[test]
fn malformed_nil_data_never_invents_a_global_contradiction() {
    let store = dataset(vec![quad(NIL, FIRST, A)]);
    let (inferred, verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(verdict.consistent);
    assert!(verdict.inconsistencies.is_empty());
    assert!(
        verdict.gaps.is_empty(),
        "an unselected list edge is ordinary source data"
    );
    assert!(
        !inferred
            .iter()
            .any(|row| row.object.as_iri() == Some(OWL_NOTHING))
    );

    let selected = dataset(vec![quad(C, UNION_OF, NIL), quad(NIL, FIRST, A)]);
    let error = crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(selected.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect_err("selected malformed nil grammar refuses before semantic execution");
    let refusal = error
        .downcast_ref::<crate::error::NativeSourceAdmission>()
        .expect("the invalid list retains a typed original-source refusal");
    assert_eq!(
        refusal.admission.refusal_class().unwrap(),
        Some(crate::reason::refute::ClassSourceRefusal::Invalid)
    );
    let world = &refusal.admission.selected_worlds[W];
    assert_eq!(world.graph, Some(TermValue::iri(W)));
    assert!(world.refusal.is_some());
    assert!(
        world
            .definitions
            .iter()
            .any(|row| row.subject == TermValue::iri(C)
                && row.predicate == UNION_OF
                && row.object == TermValue::iri(NIL))
    );
    // Source grammar failure cannot be published as owl:Nothing or a
    // conclusive consistency answer.
}

#[test]
fn self_disjoint_class_is_empty_without_a_population_clash() {
    let store = dataset(vec![quad(C, DISJOINT, C), quad(X, TYPE, A)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(verdict.consistent);
    assert!(verdict.inconsistencies.is_empty());
    assert!(
        verdict
            .unsatisfiable_classes
            .iter()
            .any(|empty| empty.class == C && empty.world == W)
    );
    assert!(
        !verdict
            .coverage
            .unsupported
            .iter()
            .any(|family| family == "selfDisjointClass")
    );
}

#[test]
fn inverse_functional_property_is_decided_consistent() {
    // owl:InverseFunctionalProperty has no native identity-merge clash rule, but
    // the Family-6a counting sub-decider now wires the real inverse-functional
    // `sameAs` propagation. A single assertion with no distinctness merges
    // nothing to clash — decided CONSISTENT with no honest gap (the pure
    // assertional/identity fragment).
    let store = dataset(vec![quad(P, TYPE, INVERSE_FUNCTIONAL), quad(X, P, Y)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(verdict.consistent, "a lone IFP assertion is consistent");
    assert!(
        verdict.gaps.is_empty(),
        "the identity sub-decider certifies this — no honest gap: {:?}",
        verdict.gaps
    );
    assert!(
        verdict
            .coverage
            .decided
            .iter()
            .any(|d| d == "inverseFunctionalProperty"),
        "the inverseFunctionalProperty family is promoted to decided: {:?}",
        verdict.coverage
    );
}

#[test]
fn inverse_functional_collapse_with_differentfrom_is_decided_inconsistent() {
    // s1 p o, s2 p o, p IFP, s1 differentFrom s2 — the `1 = 2` collapse: the IFP
    // merges s1 and s2, contradicting their asserted distinctness. Decided
    // INCONSISTENT with no honest gap.
    let s1 = "http://gmeow.example/s1";
    let s2 = "http://gmeow.example/s2";
    let store = dataset(vec![
        quad(P, TYPE, INVERSE_FUNCTIONAL),
        quad(s1, P, Y),
        quad(s2, P, Y),
        quad(s1, DIFFERENT_FROM, s2),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "an IFP-merged pair asserted differentFrom is inconsistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "the identity sub-decider decides this — no honest gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn single_enumeration_differentfrom_clash_stays_decided_under_withholds() {
    // Regression: the sound single-enumeration nominal clash (x : {a,b},
    // x differentFrom a, x differentFrom b ⇒ Nothing) must NOT be withheld by the
    // multi-enumeration trigger — it has ONE enumeration.
    let e1 = "http://gmeow.example/E1";
    let store = dataset(vec![
        quad(e1, ONE_OF, "http://gmeow.example/l0"),
        quad("http://gmeow.example/l0", FIRST, A),
        quad("http://gmeow.example/l0", REST, "http://gmeow.example/l1"),
        quad("http://gmeow.example/l1", FIRST, B),
        quad("http://gmeow.example/l1", REST, NIL),
        quad(X, TYPE, e1),
        quad(X, DIFFERENT_FROM, A),
        quad(X, DIFFERENT_FROM, B),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "single-enumeration clash is decided inconsistent"
    );
    assert!(
        verdict.gaps.is_empty(),
        "single-enumeration clash must NOT be withheld: {:?}",
        verdict.gaps
    );
}

#[test]
fn union_of_with_resolvable_list_is_decided_not_a_gap() {
    // owl:unionOf is a positive finite class-expression consequence in the
    // predicate-as-DATA path *when its list resolves*: A = unionOf (B C).
    // Its presence with a walkable list must not emit a DlGap.
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let l0 = "http://gmeow.example/l0";
    let l1 = "http://gmeow.example/l1";
    let store = dataset(vec![
        quad(A, OWL_UNION_OF, l0),
        quad(l0, FIRST, B),
        quad(l0, REST, l1),
        quad(l1, FIRST, C),
        quad(l1, REST, NIL),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(verdict.consistent, "bare union axiom is consistent");
    assert!(
        verdict.gaps.is_empty(),
        "owl:unionOf with a resolvable list is decided natively, not a gap: {:?}",
        verdict.gaps
    );
    assert!(
        verdict.coverage.present.contains(&"unionOf".to_owned()),
        "coverage records unionOf as present: {:?}",
        verdict.coverage
    );
    assert!(
        verdict.coverage.decided.contains(&"unionOf".to_owned()),
        "coverage records unionOf as decided: {:?}",
        verdict.coverage
    );
}

#[test]
fn one_of_closure_forces_a_non_member_instance_into_nothing() {
    // Colour = oneOf (red green); x : Colour but x differentFrom both red and
    // green ⇒ x can be no member ⇒ x : owl:Nothing ⇒ INCONSISTENT. This is the
    // CLOSURE half of oneOf (the member→type direction is the easy half), the
    // beyond-EL nominal reasoning the frozen external OWL 2 DL oracle gold demands
    // native catch (native ⊇ oracle).
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let colour = "http://gmeow.example/Colour";
    let red = "http://gmeow.example/red";
    let green = "http://gmeow.example/green";
    let l0 = "http://gmeow.example/l0";
    let l1 = "http://gmeow.example/l1";
    let store = dataset(vec![
        quad(colour, OWL_ONE_OF, l0),
        quad(l0, FIRST, red),
        quad(l0, REST, l1),
        quad(l1, FIRST, green),
        quad(l1, REST, NIL),
        quad(X, TYPE, colour),
        quad(X, OWL_DIFFERENT_FROM, red),
        quad(X, OWL_DIFFERENT_FROM, green),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "an enumeration instance distinct from every member must clash: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict.inconsistencies.iter().any(|w| w.individual == X),
        "x must be the inconsistency witness: {:?}",
        verdict.inconsistencies
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn one_of_with_a_member_instance_is_consistent() {
    // Same enumeration, but x is NOT asserted distinct from the members, so by
    // the open identity stance it may be one of them ⇒ NO clash ⇒ CONSISTENT.
    // Proves the closure clash is real (driven by differentFrom), not a blanket
    // "any instance of a oneOf class is unsat".
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let colour = "http://gmeow.example/Colour";
    let red = "http://gmeow.example/red";
    let green = "http://gmeow.example/green";
    let l0 = "http://gmeow.example/l0";
    let l1 = "http://gmeow.example/l1";
    let store = dataset(vec![
        quad(colour, OWL_ONE_OF, l0),
        quad(l0, FIRST, red),
        quad(l0, REST, l1),
        quad(l1, FIRST, green),
        quad(l1, REST, NIL),
        quad(X, TYPE, colour),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict.consistent,
        "an enumeration instance not asserted distinct from the members is \
             consistent: {:?}",
        verdict.inconsistencies
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

const SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const D: &str = "http://gmeow.example/D";
const Z: &str = "http://gmeow.example/z";

#[test]
fn exists_p_c_and_all_p_d_with_disjoint_c_d_is_inconsistent() {
    // GAP B keystone — the case the old inert handler missed.
    // R = ∃p.C, S = ∀p.D, C disjointWith D, x : R, x : S, NO asserted filler.
    // The chase must invent a scoped witness w with p(x,w) and type(w,C);
    // ∀p.D then types w as D; C disjoint D ⇒ w : owl:Nothing ⇒ INCONSISTENT.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, SOME_VALUES_FROM, C),
        quad(S, ON_PROPERTY, P),
        quad(S, ALL_VALUES_FROM, D),
        quad(C, DISJOINT, D),
        quad(X, TYPE, R),
        quad(X, TYPE, S),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "∃p.C ⊓ ∀p.D with C⊓D⊑⊥ must be inconsistent via an invented witness: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict
            .inconsistencies
            .iter()
            .any(|w| w.individual.starts_with(crate::facts::SKOLEM_PREFIX)),
        "the inconsistency witness must be the invented Skolem filler: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict.gaps.is_empty(),
        "someValuesFrom is now genuinely decided — no gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn some_values_from_satisfiable_filler_is_consistent_and_terminates() {
    // R = ∃p.C, x : R, no disjointness anywhere. The chase invents w, types
    // it C, and reaches a fixed point WITHOUT regenerating witnesses
    // (content-addressed identity). If termination were broken this test
    // would hang rather than fail — its mere completion is the witness.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, SOME_VALUES_FROM, C),
        quad(X, TYPE, R),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(verdict.consistent, "a satisfiable ∃p.C is consistent");
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"someValuesFrom".to_owned()),
        "someValuesFrom is decided: {:?}",
        verdict.coverage
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn cyclic_some_values_from_terminates() {
    // C ⊑ ∃p.C (R = ∃p.C, C ⊑ R). x : C forces a witness w typed C, which is
    // therefore R, which needs a p-filler of type C — the SAME class-set in
    // the SAME world, so the witness pool is reused (no fresh chain). The
    // restricted-chase blocking by class-set guarantees this terminates; the
    // test completing at all is the termination proof.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, SOME_VALUES_FROM, C),
        quad(C, SUBCLASS, R),
        quad(X, TYPE, C),
    ]);
    let (closure, verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("cyclic DL existential reasoning should terminate");

    assert!(verdict.consistent, "cyclic but satisfiable ∃ is consistent");
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    let filler = closure
        .iter()
        .find(|axiom| axiom.subject == X && axiom.predicate == P)
        .map(|axiom| axiom.object.as_iri().expect("resource axiom").to_owned())
        .expect("the root receives one existential filler");
    assert!(closure.iter().any(|axiom| {
        axiom.subject == filler
            && axiom.predicate == P
            && axiom.object.as_iri().expect("resource axiom") == filler
    }));
    assert_eq!(
        closure.iter().filter(|axiom| axiom.predicate == P).count(),
        2,
        "ancestor blocking must close the recursive model instead of growing a witness chain"
    );
}

#[test]
fn qualified_min_two_uses_two_distinct_native_chase_witnesses() {
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_CLASS, C),
        literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
    ]);
    let (closure, verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("structured native existential chase should decide >=2");
    assert!(verdict.consistent);

    let mut fillers = closure
        .iter()
        .filter(|axiom| axiom.subject == X && axiom.predicate == P)
        .map(|axiom| axiom.object.as_iri().expect("resource axiom").to_owned())
        .collect::<Vec<_>>();
    fillers.sort();
    fillers.dedup();
    assert_eq!(fillers.len(), 2, ">=2 must invent exactly two witnesses");
    for filler in &fillers {
        assert!(closure.iter().any(|axiom| {
            axiom.subject == *filler
                && axiom.predicate == TYPE
                && axiom.object.as_iri().expect("resource axiom") == C
        }));
    }
    assert!(closure.iter().any(|axiom| {
        axiom.predicate == OWL_DIFFERENT_FROM
            && ((axiom.subject == fillers[0]
                && axiom.object.as_iri().expect("resource axiom") == fillers[1])
                || (axiom.subject == fillers[1]
                    && axiom.object.as_iri().expect("resource axiom") == fillers[0]))
    }));
    assert!(closure.iter().any(|axiom| {
        axiom
            .rule_name
            .as_deref()
            .is_some_and(|name| name.contains("dl-existential"))
    }));
}

#[test]
fn existential_witnesses_are_frontier_bound_per_subject_and_deterministic() {
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, SOME_VALUES_FROM, C),
        quad(X, TYPE, R),
        quad(Y, TYPE, R),
    ]);
    let (first, first_verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("structured native existential chase should decide both obligations");
    let (second, second_verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("repeated native chase should be deterministic");

    assert!(first_verdict.consistent);
    assert!(second_verdict.consistent);
    assert_eq!(first, second, "frontier-addressed witnesses must be stable");

    let fillers = |subject: &str| {
        first
            .iter()
            .filter(|axiom| axiom.subject == subject && axiom.predicate == P)
            .map(|axiom| axiom.object.as_iri().expect("resource axiom").to_owned())
            .collect::<BTreeSet<_>>()
    };
    let x_fillers = fillers(X);
    let y_fillers = fillers(Y);
    assert_eq!(x_fillers.len(), 1, "x needs exactly one existential filler");
    assert_eq!(y_fillers.len(), 1, "y needs exactly one existential filler");
    assert!(
        x_fillers.is_disjoint(&y_fillers),
        "different frontier bindings must not share a rule-scoped witness"
    );
    for filler in x_fillers.iter().chain(&y_fillers) {
        assert!(first.iter().any(|axiom| {
            axiom.subject == *filler
                && axiom.predicate == TYPE
                && axiom.object.as_iri().expect("resource axiom") == C
        }));
    }
    for subject in [X, Y] {
        let link = first
            .iter()
            .find(|axiom| axiom.subject == subject && axiom.predicate == P)
            .expect("each subject must have a derived existential link");
        assert_eq!(
            link.premises,
            vec![(subject.to_owned(), TYPE.to_owned(), format!("<{R}>"))],
            "the production explanation must cite the matched restriction membership"
        );
    }

    let result = crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("production reasoning should retain chase certificates");
    assert_eq!(result.inferred(), first.as_slice());
    let certified = result
        .native_execution()
        .expect("one retained native execution");
    assert_eq!(
        certified.chase_certificates.len(),
        1,
        "the repeated DL fixpoint must deduplicate the same world/program certificate"
    );
    let finding = certified.chase_certificates[0].to_finding();
    assert_eq!(finding.code, "chase.certificate.weakly-acyclic");
    assert!(
        finding
            .message
            .contains("existential edge(s), none in a cycle")
            && !finding.message.contains("0 existential edge(s)"),
        "frontier certification must carry non-vacuous special-edge evidence: {finding:?}"
    );

    // The witness derivations swept out of the chase carry the exact recipe an
    // explain(witness) consumer decomposes: one per invented null, each pinning
    // its firing rule, existential ordinal, and frontier binding. This is the
    // reasoning-result surface the pipeline projects into graph/diagnostics.
    let all_fillers: BTreeSet<String> = x_fillers.iter().chain(&y_fillers).cloned().collect();
    assert_eq!(
        certified.witness_derivations.len(),
        2,
        "each frontier binding mints one decomposable witness derivation"
    );
    let derived_witnesses: BTreeSet<String> = certified
        .witness_derivations
        .iter()
        .map(|derivation| derivation.witness.clone())
        .collect();
    assert_eq!(
        derived_witnesses, all_fillers,
        "every invented filler must carry a swept-out witness derivation"
    );
    for derivation in &certified.witness_derivations {
        assert_eq!(derivation.ordinal, 0, "the single ∃-head fills ordinal 0");
        assert!(
            !derivation.rule_iri.is_empty(),
            "the firing rule must be pinned on the derivation"
        );
        let frontier_iris: Vec<&str> = derivation
            .frontier
            .iter()
            .map(|term| match term {
                TermValue::Iri(iri) => iri.as_str(),
                other => panic!("frontier binding must be an IRI: {other:?}"),
            })
            .collect();
        assert_eq!(
            frontier_iris.len(),
            1,
            "the DL restriction has exactly one frontier subject"
        );
        assert!(
            frontier_iris[0] == X || frontier_iris[0] == Y,
            "the frontier binding must be a bound demonstrand subject: {frontier_iris:?}"
        );
    }
}

#[test]
fn max_one_with_two_provably_distinct_fillers_clashes() {
    // R = ≤1 p (maxCardinality 1), x : R, x p y, x p z, y owl:differentFrom z.
    // The two fillers are PROVABLY distinct (explicit differentFrom — no UNA
    // shortcut) ⇒ the ≤1 maximum is violated ⇒ INCONSISTENT.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, Y),
        quad(X, P, Z),
        quad(Y, OWL_DIFFERENT_FROM, Z),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "two provably-distinct fillers under ≤1 must clash: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"maxCardinality".to_owned()),
        "positive maxCardinality is now decided: {:?}",
        verdict.coverage
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn max_one_with_two_named_fillers_no_differentfrom_is_consistent() {
    // SOUNDNESS FLOOR (no unique-name assumption): the SAME ≤1 p shape but the
    // two named fillers carry NO owl:differentFrom. Standard OWL does not
    // assume unique names, so y and z may be owl:sameAs ⇒ the ≤1 maximum is NOT
    // violated ⇒ CONSISTENT. (The old UNA default reported a FALSE
    // inconsistency here — the OWL-2 restrict-maxcard-inst-obj-one regression.)
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, Y),
        quad(X, P, Z),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "two named fillers without differentFrom must NOT clash under ≤1: {:?}",
        verdict.inconsistencies
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn max_one_with_mergeable_fillers_is_consistent() {
    // Same ≤1 p, but y owl:sameAs z merges the two fillers ⇒ NOT distinct ⇒
    // no clash ⇒ CONSISTENT. Proves the anti-merge is real, not a count of
    // raw IRIs.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, Y),
        quad(X, P, Z),
        quad(Y, SAME_AS, Z),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict.consistent,
        "mergeable fillers (sameAs) must NOT clash under ≤1: {:?}",
        verdict.inconsistencies
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn maximum_clash_ignores_extra_fillers_with_unknown_equality() {
    let data = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "+01", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, A),
        quad(X, P, Y),
        quad(X, P, Z),
        // Only the reverse orientation is asserted by this source.
        quad(Z, OWL_DIFFERENT_FROM, Y),
    ]);
    let inferred = native_cardinality_clashes(data.as_ref());
    let clash = inferred
        .iter()
        .find(|row| row.individual == X)
        .expect("a witnessed pair exceeds one despite the extra unknown filler");
    let mut expected: BTreeSet<_> = [
        (X, TYPE, R),
        (R, ON_PROPERTY, P),
        (X, P, Y),
        (X, P, Z),
        (Z, OWL_DIFFERENT_FROM, Y),
    ]
    .map(|(s, p, o)| (s.to_owned(), p.to_owned(), format!("<{o}>")))
    .into();
    expected.insert((
        R.to_owned(),
        MAX_CARDINALITY.to_owned(),
        format!("\"+01\"^^<{XSD_NON_NEGATIVE_INTEGER}>"),
    ));
    assert_eq!(
        clash.premises.iter().cloned().collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(clash.world, W);
}

#[test]
fn maximum_clash_searches_beyond_a_partial_distinct_set() {
    // A can join B but cannot extend to a triple. The later B,C,Y triple
    // violates the bound; greedily keeping A would miss the contradiction.
    let data = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, A),
        quad(X, P, B),
        quad(X, P, C),
        quad(X, P, Y),
        quad(A, OWL_DIFFERENT_FROM, B),
        quad(B, OWL_DIFFERENT_FROM, C),
        quad(B, OWL_DIFFERENT_FROM, Y),
        quad(C, OWL_DIFFERENT_FROM, Y),
    ]);
    let inferred = native_cardinality_clashes(data.as_ref());
    let clash = inferred
        .iter()
        .find(|row| row.individual == X)
        .expect("the later distinct triple exceeds two");
    let counted: BTreeSet<_> = clash
        .premises
        .iter()
        .filter(|(s, p, _)| s == X && p == P)
        .map(|(_, _, o)| o.clone())
        .collect();
    assert_eq!(counted, [B, C, Y].map(|iri| format!("<{iri}>")).into());
}

#[test]
fn maximum_clash_needs_a_complete_pairwise_witness_in_one_world() {
    let mut quads = vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(R, MAX_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, A),
        quad(X, P, B),
        quad(X, P, C),
        quad(X, P, Y),
        quad(A, OWL_DIFFERENT_FROM, B),
        quad(B, OWL_DIFFERENT_FROM, C),
        quad(C, OWL_DIFFERENT_FROM, Y),
    ];
    quads.push(quad(A, OWL_DIFFERENT_FROM, C).in_graph(RdfTerm::iri("urn:other-world")));
    let inferred = native_cardinality_clashes(dataset(quads).as_ref());
    assert!(
        !inferred.iter().any(|row| { row.individual == X }),
        "a path or a cross-world triangle is not a pairwise distinct triple"
    );
}

#[test]
fn qualified_maximum_clash_preserves_filler_type_evidence() {
    let data = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_CLASS, C),
        literal_quad(
            R,
            OWL_MAX_QUALIFIED_CARDINALITY,
            "1",
            XSD_NON_NEGATIVE_INTEGER,
        ),
        quad(X, TYPE, R),
        quad(X, P, A),
        quad(X, P, Y),
        quad(X, P, Z),
        quad(A, TYPE, C),
        quad(Y, TYPE, C),
        quad(Z, TYPE, C),
        quad(Y, OWL_DIFFERENT_FROM, Z),
    ]);
    let inferred = native_cardinality_clashes(data.as_ref());
    let clash = inferred
        .iter()
        .find(|row| row.individual == X)
        .expect("a qualified distinct pair exceeds one");
    for (s, p, o) in [(R, ON_CLASS, C), (Y, TYPE, C), (Z, TYPE, C)] {
        assert!(
            clash
                .premises
                .contains(&(s.to_owned(), p.to_owned(), format!("<{o}>")))
        );
    }
    assert!(
        !clash
            .premises
            .iter()
            .any(|(s, _, o)| s == A || o == &format!("<{A}>"))
    );
}

#[test]
fn min_qualified_two_generates_two_distinct_witnesses_and_terminates() {
    // R = ≥2 p.C (minQualifiedCardinality 2, onClass C), x : R, no asserted
    // filler. The chase must invent TWO distinct witnesses w0,w1 both typed C
    // and both p-fillers of x. Consistent, terminating, and the ≥2 obligation
    // is then met (so re-running invents nothing new).
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_CLASS, C),
        literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(verdict.consistent, "a satisfiable ≥2 p.C is consistent");
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"minQualifiedCardinality".to_owned()),
        "minQualifiedCardinality is decided: {:?}",
        verdict.coverage
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn min_two_then_max_one_qualified_clashes() {
    // R = ≥2 p.C AND ≤1 p.C on the same restriction (minQualifiedCardinality 2
    // + qualifiedCardinality... use min 2 + maxCardinality 1 unqualified for a
    // crisp clash). The min generates 2 distinct C-witnesses, the max-1 then
    // counts 2 distinct fillers ⇒ clash ⇒ INCONSISTENT. Demonstrates the
    // generation and counting interlock.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_CLASS, C),
        literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "≥2 p.C with ≤1 p must clash after witness generation: {:?}",
        verdict.inconsistencies
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn qualified_cardinality_one_with_two_distinct_c_fillers_clashes() {
    // R = =1 p.C (qualifiedCardinality 1, onClass C), x : R, x p y, x p z,
    // y:C, z:C, y owl:differentFrom z (PROVABLY distinct — no UNA) ⇒ the =1
    // maximum clashes ⇒ INCONSISTENT.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_CLASS, C),
        literal_quad(R, QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        quad(X, P, Y),
        quad(X, P, Z),
        quad(Y, TYPE, C),
        quad(Z, TYPE, C),
        quad(Y, OWL_DIFFERENT_FROM, Z),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "=1 p.C with two distinct C-fillers must clash: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"qualifiedCardinality".to_owned()),
        "qualifiedCardinality is decided: {:?}",
        verdict.coverage
    );
    assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
}

#[test]
fn unparseable_cardinality_bound_stays_unsupported_so_the_gate_can_fire() {
    // The gate must still be able to fire for a genuinely-undecidable case.
    // A maxCardinality whose literal is NOT a non-negative integer cannot be
    // acted on by the handler, so it stays `unsupported` → a non-empty gaps,
    // proving the gate is not dead code.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        literal_quad(
            R,
            MAX_CARDINALITY,
            "not-a-number",
            "http://www.w3.org/2001/XMLSchema#string",
        ),
        quad(X, TYPE, R),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict
            .coverage
            .unsupported
            .contains(&"maxCardinality".to_owned()),
        "an unparsable cardinality bound is not genuinely decided: {:?}",
        verdict.coverage
    );
    assert!(
        !verdict.gaps.is_empty(),
        "an undecidable cardinality instance must yield a gap so the gate can fire: {:?}",
        verdict.gaps
    );
    assert!(
        verdict
            .gaps
            .iter()
            .any(|g| g.code == "reason.dl-gap.maxCardinality"),
        "gaps must name the undecided construct: {:?}",
        verdict.gaps
    );
}

#[test]
fn datatype_qualified_max_without_violation_is_decided_inert() {
    // R = ≤1 p.decimal (maxQualifiedCardinality 1, onDataRange xsd:decimal),
    // x : R, one decimal filler. A datatype-qualified maximum counts LITERAL
    // fillers the IRI chase does not carry; with no subject exceeding the bound
    // it is INERT and the native path decides it (no gap), never withholding the
    // whole qualified family on the absent `owl:onClass`.
    const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
    const MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
    const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_DATA_RANGE, DECIMAL),
        literal_quad(R, MAX_QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        literal_quad(X, P, "1.5", DECIMAL),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict.consistent,
        "a ≤1 p.decimal with one value is consistent: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"maxQualifiedCardinality".to_owned()),
        "an inert datatype-qualified maximum is decided: {:?}",
        verdict.coverage
    );
    assert!(
        verdict.gaps.is_empty(),
        "no gap for an inert datatype-qualified maximum: {:?}",
        verdict.gaps
    );
}

#[test]
fn datatype_qualified_max_overflow_stays_unsupported_so_the_gate_can_fire() {
    // Same ≤1 p.decimal, but x carries TWO value-distinct decimal literals — a
    // live max overflow the literal-blind IRI chase cannot decide. The family is
    // honestly WITHHELD (unsupported → gap), never wrongly reported decided.
    const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
    const MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
    const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ON_DATA_RANGE, DECIMAL),
        literal_quad(R, MAX_QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
        literal_quad(X, P, "1.5", DECIMAL),
        literal_quad(X, P, "2.5", DECIMAL),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict
            .coverage
            .unsupported
            .contains(&"maxQualifiedCardinality".to_owned()),
        "a live datatype-max overflow is not genuinely decided: {:?}",
        verdict.coverage
    );
    assert!(
        verdict
            .gaps
            .iter()
            .any(|g| g.code == "reason.dl-gap.maxQualifiedCardinality"),
        "gaps must name the withheld construct: {:?}",
        verdict.gaps
    );
}

#[test]
fn some_values_from_without_on_property_stays_unsupported() {
    // A malformed ∃ restriction (someValuesFrom but no onProperty) cannot be
    // discharged by the chase, so someValuesFrom stays unsupported → gap.
    let store = dataset(vec![quad(R, SOME_VALUES_FROM, C), quad(X, TYPE, R)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict
            .coverage
            .unsupported
            .contains(&"someValuesFrom".to_owned()),
        "a someValuesFrom with no onProperty is not decidable: {:?}",
        verdict.coverage
    );
    assert!(
        !verdict.gaps.is_empty(),
        "must yield a gap: {:?}",
        verdict.gaps
    );
}

#[test]
fn all_values_from_pushes_type_into_existing_fillers() {
    // R = ∀p.B, x : R, x p y, y : C, B disjoint C ⇒ y : owl:Nothing.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, ALL_VALUES_FROM, B),
        quad(B, DISJOINT, C),
        quad(X, TYPE, R),
        quad(X, P, Y),
        quad(Y, TYPE, C),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "allValuesFrom must type y as B and clash with y : C"
    );
    assert!(verdict.gaps.is_empty(), "allValuesFrom is decided");
}

#[test]
fn has_value_emits_required_property_and_can_clash_with_max_zero() {
    // R = (= p y) and R maxCardinality 0 on p. x : R forces x p y, then
    // max 0 detects the contradiction.
    let store = dataset(vec![
        quad(R, ON_PROPERTY, P),
        quad(R, HAS_VALUE, Y),
        literal_quad(R, MAX_CARDINALITY, "0", XSD_NON_NEGATIVE_INTEGER),
        quad(X, TYPE, R),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.consistent,
        "hasValue plus maxCardinality 0 must be inconsistent"
    );
    assert!(verdict.gaps.is_empty(), "hasValue/cardinality are decided");
    assert!(
        verdict.coverage.present.contains(&"hasValue".to_owned())
            && verdict
                .coverage
                .present
                .contains(&"maxCardinality".to_owned()),
        "coverage records hasValue and maxCardinality: {:?}",
        verdict.coverage
    );
}

const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

fn bnode_quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

// ── owl:bottomObjectProperty / owl:bottomDataProperty ─────────────────────

/// `i : ∃ owl:bottomObjectProperty . owl:Thing` is unsatisfiable — the bottom
/// property is empty, so an obligation to bear a value on it forces
/// owl:Nothing.
#[test]
fn bottom_object_property_some_values_from_is_inconsistent() {
    let restriction = "http://gmeow.example/r";
    let store = dataset(vec![
        quad(restriction, ON_PROPERTY, OWL_BOTTOM_OBJECT_PROPERTY),
        quad(restriction, OWL_SOME_VALUES_FROM, OWL_THING),
        quad(X, TYPE, restriction),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "bottom-object-property obligation clashes"
    );
    assert!(
        verdict.gaps.is_empty(),
        "bottomObjectProperty is decided, not a gap: {:?}",
        verdict.gaps
    );
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"bottomObjectProperty".to_owned())
    );
}

/// The data-property analog: `i : ∃ owl:bottomDataProperty . rdfs:Literal`.
#[test]
fn bottom_data_property_some_values_from_is_inconsistent() {
    let restriction = "http://gmeow.example/r";
    let literal_class = "http://www.w3.org/2000/01/rdf-schema#Literal";
    let store = dataset(vec![
        quad(restriction, ON_PROPERTY, OWL_BOTTOM_DATA_PROPERTY),
        quad(restriction, OWL_SOME_VALUES_FROM, literal_class),
        quad(X, TYPE, restriction),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "bottom-data-property obligation clashes"
    );
    assert!(verdict.gaps.is_empty(), "bottomDataProperty is decided");
}

// ── owl:NegativePropertyAssertion (object + data) ─────────────────────────

/// NPA(Peter, hasSon, Meg) co-present with hasSon(Peter, Meg) ⇒ inconsistent.
#[test]
fn negative_object_property_assertion_clashes_with_positive() {
    let peter = "http://gmeow.example/Peter";
    let meg = "http://gmeow.example/Meg";
    let has_son = "http://gmeow.example/hasSon";
    let npa = "npa";
    let store = dataset(vec![
        quad(peter, has_son, meg),
        bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
        bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, peter),
        bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_son),
        bnode_quad(npa, OWL_TARGET_INDIVIDUAL, meg),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(!verdict.consistent, "NPA contradicted by its positive");
    assert!(verdict.gaps.is_empty(), "NPA is decided");
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"negativePropertyAssertion".to_owned())
    );
}

/// The data analog: NPA(Meg, hasAge, "5") + hasAge(Meg, "5") ⇒ inconsistent,
/// and a DIFFERENT literal value must NOT clash (literal-aware target match).
#[test]
fn negative_data_property_assertion_is_literal_aware() {
    let meg = "http://gmeow.example/Meg";
    let has_age = "http://gmeow.example/hasAge";
    let int_ty = "http://www.w3.org/2001/XMLSchema#integer";
    let npa = "npa";
    // Positive value "5" matches the negated target "5" — clash.
    let store = dataset(vec![
        literal_quad(meg, has_age, "5", int_ty),
        bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
        bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
        bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
        RdfQuad::new(
            RdfTerm::blank_node(npa),
            OWL_TARGET_VALUE,
            RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
        )
        .in_graph(RdfTerm::iri(W)),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "matching literal value clashes the NPA"
    );

    // A DIFFERENT positive value ("6") does not match the negated "5".
    let store_ok = dataset(vec![
        literal_quad(meg, has_age, "6", int_ty),
        bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
        bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
        bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
        RdfQuad::new(
            RdfTerm::blank_node(npa),
            OWL_TARGET_VALUE,
            RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
        )
        .in_graph(RdfTerm::iri(W)),
    ]);
    let verdict_ok = dl_consistency(
        crate::reason::prepare_reasoning_input(store_ok.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict_ok.consistent,
        "a non-matching literal value must NOT clash the NPA"
    );
}

// ── owl:FunctionalProperty (data values) ──────────────────────────────────

/// A functional data property with two distinct literal values forces
/// owl:Nothing; a single value is consistent.
#[test]
fn functional_data_property_two_literals_clash() {
    let peter = "http://gmeow.example/Peter";
    let has_name = "http://gmeow.example/hasName";
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let store = dataset(vec![
        quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
        literal_quad(peter, has_name, "Peter", str_ty),
        literal_quad(peter, has_name, "Kichwa-Tembo", str_ty),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "two distinct literal values on a functional property clash"
    );
    assert!(
        verdict
            .coverage
            .decided
            .contains(&"functionalProperty".to_owned())
    );

    let store_ok = dataset(vec![
        quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
        literal_quad(peter, has_name, "Peter", str_ty),
    ]);
    let verdict_ok = dl_consistency(
        crate::reason::prepare_reasoning_input(store_ok.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(verdict_ok.consistent, "a single value is consistent");
}

/// Functionality declared ONLY by the canonical `logic:PropertyCharacteristicAssertion`
/// carrier record (no `owl:FunctionalProperty` marker) still forces owl:Nothing on a subject
/// with two distinct literal values — the derivation source the object-level reasoning EDB
/// relies on once the `owl:FunctionalProperty` slice source declarations are removed. Coverage
/// stays honest: with no OWL functional construct in the EDB, `functionalProperty` is not
/// reported present, yet the clash is still decided from the carrier.
#[test]
fn functional_data_property_carrier_record_two_literals_clash() {
    let peter = "http://gmeow.example/Peter";
    let has_name = "http://gmeow.example/hasName";
    let rec = "http://gmeow.example/hasName-functional-record";
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let store = dataset(vec![
        quad(rec, LOGIC_CHARACTERIZES, has_name),
        quad(rec, LOGIC_CHARACTERISTIC_SORT, LOGIC_FUNCTIONAL_PROPERTY),
        literal_quad(peter, has_name, "Peter", str_ty),
        literal_quad(peter, has_name, "Kichwa-Tembo", str_ty),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "two distinct literal values on a carrier-declared functional property clash"
    );

    let store_ok = dataset(vec![
        quad(rec, LOGIC_CHARACTERIZES, has_name),
        quad(rec, LOGIC_CHARACTERISTIC_SORT, LOGIC_FUNCTIONAL_PROPERTY),
        literal_quad(peter, has_name, "Peter", str_ty),
    ]);
    let verdict_ok = dl_consistency(
        crate::reason::prepare_reasoning_input(store_ok.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict_ok.consistent,
        "a single value under the carrier record is consistent"
    );
}

// ── owl:hasKey ────────────────────────────────────────────────────────────

/// hasKey(owl:Thing, [hasSSN]); two differentFrom individuals sharing the key
/// literal are forced into owl:Nothing.
#[test]
fn has_key_collision_with_explicit_distinctness_clashes() {
    let peter = "http://gmeow.example/Peter";
    let pg = "http://gmeow.example/Peter_Griffin";
    let has_ssn = "http://gmeow.example/hasSSN";
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let key_list = "keylist";
    let store = dataset(vec![
        bnode_quad(key_list, FIRST, has_ssn),
        bnode_quad(key_list, REST, NIL),
        RdfQuad::new(
            RdfTerm::iri(OWL_THING),
            OWL_HAS_KEY,
            RdfTerm::blank_node(key_list),
        )
        .in_graph(RdfTerm::iri(W)),
        literal_quad(peter, has_ssn, "123-45-6789", str_ty),
        literal_quad(pg, has_ssn, "123-45-6789", str_ty),
        quad(peter, OWL_DIFFERENT_FROM, pg),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "key-agreeing differentFrom individuals clash"
    );
    assert!(verdict.gaps.is_empty(), "hasKey is decided");
    assert!(verdict.coverage.decided.contains(&"hasKey".to_owned()));
}

/// WITHOUT explicit owl:differentFrom, two key-agreeing individuals are
/// merely owl:sameAs (no UNA in standard OWL) — consistent, NOT a false clash.
#[test]
fn has_key_collision_without_distinctness_is_consistent() {
    let peter = "http://gmeow.example/Peter";
    let pg = "http://gmeow.example/Peter_Griffin";
    let has_ssn = "http://gmeow.example/hasSSN";
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let key_list = "keylist";
    let store = dataset(vec![
        bnode_quad(key_list, FIRST, has_ssn),
        bnode_quad(key_list, REST, NIL),
        RdfQuad::new(
            RdfTerm::iri(OWL_THING),
            OWL_HAS_KEY,
            RdfTerm::blank_node(key_list),
        )
        .in_graph(RdfTerm::iri(W)),
        literal_quad(peter, has_ssn, "123-45-6789", str_ty),
        literal_quad(pg, has_ssn, "123-45-6789", str_ty),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.consistent,
        "without differentFrom, key agreement just merges the two (sameAs) — consistent"
    );
}

/// The gtsHeadId / GTSSegment key expressed ONLY through the greenfield `logic:KeyAssertion`
/// carrier (no `owl:hasKey`) still DECIDES the DL verdict: two GTSSegment individuals asserted
/// `owl:differentFrom` yet sharing one gtsHeadId content-id are forced into owl:Nothing, and the
/// `hasKey` family is reported present + decided from the carrier. This is the falsifiable
/// no-regression guard after the `owl:hasKey ( gmeow:gtsHeadId )` slice declaration is migrated
/// to `logic:gtsSegmentHeadKey` — the object-level reasoning EDB carries the key on the carrier,
/// not on an `owl:hasKey` triple, exactly as the shipped gts slice now authors it.
#[test]
fn gts_segment_head_key_carrier_decides_and_clashes() {
    let seg_a = "https://blackcatinformatics.ca/gmeow/segA";
    let seg_b = "https://blackcatinformatics.ca/gmeow/segB";
    let gts_segment = "https://blackcatinformatics.ca/gmeow/GTSSegment";
    let gts_head_id = "https://blackcatinformatics.ca/gmeow/gtsHeadId";
    let key_rec = "https://blackcatinformatics.ca/logic/gtsSegmentHeadKey";
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let head = "blake3:9f2c";
    let store = dataset(vec![
        // logic:KeyAssertion carrier: a GTSSegment is keyed by its gtsHeadId.
        quad(key_rec, TYPE, LOGIC_KEY_ASSERTION),
        quad(key_rec, LOGIC_KEY_CLASS, gts_segment),
        quad(key_rec, LOGIC_KEY_PROPERTY, gts_head_id),
        // Two distinct segments sharing one content-id head — a full-history BLAKE3 collision.
        quad(seg_a, TYPE, gts_segment),
        quad(seg_b, TYPE, gts_segment),
        literal_quad(seg_a, gts_head_id, head, str_ty),
        literal_quad(seg_b, gts_head_id, head, str_ty),
        quad(seg_a, OWL_DIFFERENT_FROM, seg_b),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "two differentFrom segments sharing a gtsHeadId key clash under the carrier"
    );
    assert!(
        verdict.coverage.present.contains(&"hasKey".to_owned()),
        "the logic:KeyAssertion carrier makes the hasKey family present"
    );
    assert!(
        verdict.coverage.decided.contains(&"hasKey".to_owned()),
        "the carrier keeps hasKey decided after the owl:hasKey source is migrated"
    );
    assert!(
        verdict.gaps.is_empty(),
        "hasKey is decided from the carrier, not a gap"
    );

    // Consistency guard: WITHOUT the owl:differentFrom, the shared key merely merges the two
    // (owl:sameAs, no unique-name assumption) — consistent, yet still decided from the carrier.
    let store_ok = dataset(vec![
        quad(key_rec, TYPE, LOGIC_KEY_ASSERTION),
        quad(key_rec, LOGIC_KEY_CLASS, gts_segment),
        quad(key_rec, LOGIC_KEY_PROPERTY, gts_head_id),
        quad(seg_a, TYPE, gts_segment),
        quad(seg_b, TYPE, gts_segment),
        literal_quad(seg_a, gts_head_id, head, str_ty),
        literal_quad(seg_b, gts_head_id, head, str_ty),
    ]);
    let ok = dl_consistency(
        crate::reason::prepare_reasoning_input(store_ok.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        ok.consistent,
        "without owl:differentFrom the shared gtsHeadId key merges the segments — consistent"
    );
    assert!(
        ok.coverage.decided.contains(&"hasKey".to_owned()),
        "hasKey stays decided from the carrier even with no clash"
    );
}

#[test]
fn malformed_canonical_keys_withhold_coverage_even_without_candidate_instances() {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    for defect in [
        "two-classes",
        "literal-class",
        "literal-property",
        "blank-property",
        "missing-property",
    ] {
        let record = "urn:key:record";
        let mut quads = vec![
            quad(record, TYPE, LOGIC_KEY_ASSERTION),
            quad(record, LOGIC_KEY_CLASS, A),
        ];
        if defect != "missing-property" {
            quads.push(quad(record, LOGIC_KEY_PROPERTY, P));
        }
        match defect {
            "two-classes" => quads.push(quad(record, LOGIC_KEY_CLASS, B)),
            "literal-class" => quads.push(literal_quad(record, LOGIC_KEY_CLASS, "bad", XSD_STRING)),
            "literal-property" => {
                quads.push(literal_quad(record, LOGIC_KEY_PROPERTY, "bad", XSD_STRING))
            }
            "blank-property" => quads.push(
                RdfQuad::new(
                    RdfTerm::iri(record),
                    LOGIC_KEY_PROPERTY,
                    RdfTerm::blank_node("bad"),
                )
                .in_graph(RdfTerm::iri(W)),
            ),
            _ => {}
        }
        let data = dataset(quads);
        let verdict = dl_consistency(
            crate::reason::prepare_reasoning_input(&data).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        assert!(verdict.coverage.present.contains(&"hasKey".to_owned()));
        assert!(
            !verdict.coverage.decided.contains(&"hasKey".to_owned()),
            "{defect}"
        );
        assert!(!verdict.gaps.is_empty(), "{defect}");
    }
}

// ── owl:Thing forced empty / constrained ──────────────────────────────────

/// owl:Thing ≡ owl:Nothing (or ⊑) makes the always-populated top class empty
/// — inconsistent.
#[test]
fn thing_equivalent_to_nothing_is_inconsistent() {
    let store = dataset(vec![quad(OWL_THING, OWL_EQUIVALENT_CLASS, OWL_NOTHING)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([crate::reason::SelectedLogicalWorld::new(
            crate::reason::LogicalGraph::Named(TermValue::iri(W)),
            crate::reason::DomainProfile::NonemptyObjectDomainV1,
            "urn:test:dl-nonempty-universe".to_owned(),
            [1; 32],
        )
        .unwrap()])
        .unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !verdict.consistent,
        "owl:Thing forced empty must be inconsistent"
    );
    assert!(verdict.gaps.is_empty(), "Thing≡Nothing is decided");
}

/// owl:Thing oneOf {s} is the DL/Full-divergent singleton-universe case. The
/// native path does NOT perform the universe-cardinality argument, so it must
/// stay an HONEST gap (incomplete) — NEVER a wrong `consistent` decided answer.
#[test]
fn one_of_on_thing_is_an_honest_gap_not_a_decided_answer() {
    let s = "http://gmeow.example/s";
    let list = "onelist";
    let store = dataset(vec![
        bnode_quad(list, FIRST, s),
        bnode_quad(list, REST, NIL),
        RdfQuad::new(
            RdfTerm::iri(OWL_THING),
            OWL_ONE_OF,
            RdfTerm::blank_node(list),
        )
        .in_graph(RdfTerm::iri(W)),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([crate::reason::SelectedLogicalWorld::new(
            crate::reason::LogicalGraph::Named(TermValue::iri(W)),
            crate::reason::DomainProfile::NonemptyObjectDomainV1,
            "urn:test:dl-nonempty-universe".to_owned(),
            [1; 32],
        )
        .unwrap()])
        .unwrap(),
    )
    .expect("dl consistency should succeed");
    // Honesty over a wrong decided answer: oneOf-on-Thing is undecided, so it
    // surfaces as a gap and the case grades DlGap, never CorpusOnly.
    assert!(
        !verdict.gaps.is_empty(),
        "oneOf-on-owl:Thing must surface as an honest gap"
    );
    assert!(
        verdict.coverage.unsupported.contains(&"oneOf".to_owned()),
        "oneOf is undecided when it constrains owl:Thing: {:?}",
        verdict.coverage
    );
}

/// A normal `owl:oneOf` on an ordinary class (not owl:Thing) stays DECIDED —
/// the Thing carve-out must not regress the general enumeration handling.
#[test]
fn one_of_on_ordinary_class_stays_decided() {
    let list = "onelist";
    let mut quads = vec![
        bnode_quad(list, FIRST, X),
        bnode_quad(list, REST, NIL),
        RdfQuad::new(RdfTerm::iri(A), OWL_ONE_OF, RdfTerm::blank_node(list))
            .in_graph(RdfTerm::iri(W)),
    ];
    // Compiled program graphs carry vocabulary IRIs as DATA: graph/logic uses a flat
    // repeated-member enumeration and graph/relational-core reifies target terms. They
    // remain available to their owning engines but must not be reinterpreted as OWL
    // syntax by the DL coverage scanner. Extending this existing tiny fixture prevents
    // a corpus-scale regression test.
    quads.extend([
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/flat-enumeration"),
            RDF_TYPE,
            RdfTerm::iri("https://blackcatinformatics.ca/logic/Enumeration"),
        )
        .in_graph(RdfTerm::iri(crate::reasoning_graphs::GRAPH_LOGIC)),
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/flat-enumeration"),
            "https://blackcatinformatics.ca/logic/oneOf",
            RdfTerm::iri(X),
        )
        .in_graph(RdfTerm::iri(crate::reasoning_graphs::GRAPH_LOGIC)),
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/reified-fact"),
            "https://blackcatinformatics.ca/logic/rcObject",
            RdfTerm::iri(OWL_INVERSE_FUNCTIONAL_PROPERTY),
        )
        .in_graph(RdfTerm::iri(crate::reasoning_graphs::GRAPH_RELATIONAL_CORE)),
    ]);
    let store = dataset(quads);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        verdict.coverage.decided.contains(&"oneOf".to_owned()),
        "ordinary oneOf stays decided: {:?}",
        verdict.coverage
    );
    assert!(
        !verdict
            .coverage
            .present
            .contains(&"inverseFunctionalProperty".to_owned()),
        "a reified relational-core value is data, not an inverse-functional axiom"
    );
}

// ── Out-of-fragment soundness: honest cannot-decide, never a wrong consistent ──

/// `owl:topObjectProperty` (the universal property) is not implemented by the
/// native chase: an ontology whose (in)consistency turns on the universal
/// property obligation must be reported as an honest cannot-decide (non-empty
/// `gaps`, the construct `unsupported`), NEVER a wrong `consistent` by ignoring
/// the axiom.
#[test]
fn top_object_property_is_an_honest_gap_never_a_wrong_consistent() {
    const TOP: &str = OWL_TOP_OBJECT_PROPERTY;
    let store = dataset(vec![quad(X, TOP, Y), quad(A, SUBCLASS, B)]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.gaps.is_empty(),
        "the universal property is out of fragment ⇒ honest gap, not a silent ignore: {:?}",
        verdict.coverage
    );
    assert!(
        verdict
            .coverage
            .unsupported
            .contains(&"topObjectProperty".to_owned()),
        "owl:topObjectProperty is unsupported (never decided): {:?}",
        verdict.coverage
    );
    assert!(
        !verdict
            .coverage
            .decided
            .contains(&"topObjectProperty".to_owned()),
        "owl:topObjectProperty must NOT be reported decided"
    );
}

/// A supported facet restriction participates in the native value-space decision:
/// `p` ranges over `xsd:integer[<= 5]`, so the asserted value `10` is a proved
/// contradiction rather than an unsupported or silently ignored obligation.
#[test]
fn datatype_facet_restriction_with_constrained_literal_is_decided() {
    const WITH_RESTRICTIONS: &str = OWL_WITH_RESTRICTIONS;
    const ON_DATATYPE: &str = OWL_ON_DATATYPE;
    const MAX_INCLUSIVE: &str = XSD_MAX_INCLUSIVE;
    const RANGE: &str = RDFS_RANGE;
    let dt = "http://gmeow.example/SmallInt";
    let facet = "http://gmeow.example/facet0";
    let facet_list = "http://gmeow.example/facets";
    let store = dataset(vec![
        // dt = xsd:integer restricted by [ xsd:maxInclusive "5" ]
        quad(dt, ON_DATATYPE, XSD_INTEGER),
        quad(dt, WITH_RESTRICTIONS, facet_list),
        quad(facet_list, RDF_FIRST, facet),
        quad(facet_list, RDF_REST, RDF_NIL),
        literal_quad(facet, MAX_INCLUSIVE, "5", XSD_INTEGER),
        quad(P, RDF_TYPE, DATATYPE_PROPERTY),
        // p ranges into the facet-restricted datatype, and x carries the
        // out-of-range value 10 on p — a LIVE obligation the native path
        // cannot validate.
        quad(P, RANGE, dt),
        literal_quad(X, P, "10", XSD_INTEGER),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(!verdict.consistent, "the out-of-range literal must clash");
    assert!(verdict.gaps.is_empty(), "the supported facet is decided");
    for family in ["onDatatype", "withRestrictions", "maxInclusive"] {
        assert!(
            verdict.coverage.decided.contains(&family.to_owned()),
            "{family} must be decided for the constrained literal: {:?}",
            verdict.coverage
        );
    }
}

/// A facet-restricted datatype that is merely DEFINED but constrains no
/// asserted/inferred literal is INERT — it cannot cause an inconsistency, so
/// the native path DECIDES it: the datatype-facet families do NOT appear in the
/// gaps, and the verdict stays decided consistent. This mirrors the production
/// `gmeow:bic` datatype (a TBox-only facet definition with no ABox literal
/// subject to it). (Falsifiable case (a): a facet WITHOUT a constrained literal
/// is decided.)
#[test]
fn datatype_facet_definition_without_constrained_literal_is_decided_inert() {
    const WITH_RESTRICTIONS: &str = OWL_WITH_RESTRICTIONS;
    const ON_DATATYPE: &str = OWL_ON_DATATYPE;
    const MIN_LENGTH: &str = XSD_MIN_LENGTH;
    const ALL_VALUES_FROM: &str = OWL_ALL_VALUES_FROM;
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    let dt = "dt"; // blank-node facet datatype (as in the production bundle)
    let facet = "facet0";
    let facet_list = "facet-list";
    let restriction = "http://gmeow.example/BicClass";
    let store = dataset(vec![
        // A facet-restricted datatype: onDatatype xsd:string, minLength 8.
        RdfQuad::new(
            RdfTerm::blank_node(dt),
            ON_DATATYPE,
            RdfTerm::iri("http://www.w3.org/2001/XMLSchema#string"),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(dt),
            WITH_RESTRICTIONS,
            RdfTerm::blank_node(facet_list),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet_list),
            RDF_FIRST,
            RdfTerm::blank_node(facet),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet_list),
            RDF_REST,
            RdfTerm::iri(RDF_NIL),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet),
            MIN_LENGTH,
            RdfTerm::Literal(RdfLiteral::typed("8", XSD_NON_NEGATIVE_INTEGER)),
        )
        .in_graph(RdfTerm::iri(W)),
        // A class restricts property p's values to the facet datatype — but NO
        // individual asserts any literal on p, so the facet is inert.
        quad(restriction, ON_PROPERTY, P),
        RdfQuad::new(
            RdfTerm::iri(restriction),
            ALL_VALUES_FROM,
            RdfTerm::blank_node(dt),
        )
        .in_graph(RdfTerm::iri(W)),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        verdict.consistent,
        "an inert (defined-but-unused) facet datatype is consistent: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict.gaps.is_empty(),
        "an inert facet definition must NOT surface as a gap: {:?}",
        verdict.gaps
    );
    for family in ["onDatatype", "withRestrictions", "minLength"] {
        assert!(
            !verdict.coverage.unsupported.contains(&family.to_owned()),
            "{family} must NOT be unsupported for an inert facet: {:?}",
            verdict.coverage
        );
        assert!(
            verdict.coverage.decided.contains(&family.to_owned()),
            "{family} must be decided (inert) for an unused facet: {:?}",
            verdict.coverage
        );
    }
}

/// A facet-restricted datatype used as the filler of an `owl:someValuesFrom`
/// existential (`∃p.D`) is a LIVE obligation even with NO asserted literal: the
/// existential forces a datatype value to exist, and native cannot decide the
/// datatype's (non)emptiness (the W3C `Datatype-Float-Discrete-001` shape — the
/// discrete `xsd:float` range `(0.0, 1.4e-45)` is empty). It must stay withheld
/// (facet families NOT decided ⇒ honest gap), never a wrong `consistent`.
#[test]
fn datatype_facet_somevaluesfrom_existential_is_withheld_without_a_literal() {
    const WITH_RESTRICTIONS: &str = OWL_WITH_RESTRICTIONS;
    const ON_DATATYPE: &str = OWL_ON_DATATYPE;
    const MIN_EXCLUSIVE: &str = XSD_MIN_EXCLUSIVE;
    const SOME_VALUES_FROM: &str = OWL_SOME_VALUES_FROM;
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    let dt = "dt";
    let facet = "facet0";
    let facet_list = "facet-list";
    // Absolute, like every sibling fixture in this module. A bare `"restriction"` is a
    // relative reference with no base in scope, which the RDF IR refuses to intern.
    let restriction = "http://gmeow.example/restriction";
    let store = dataset(vec![
        // dt = xsd:float restricted by [ xsd:minExclusive "0.0" ] (a facet datatype)
        RdfQuad::new(
            RdfTerm::blank_node(dt),
            ON_DATATYPE,
            RdfTerm::iri("http://www.w3.org/2001/XMLSchema#float"),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(dt),
            WITH_RESTRICTIONS,
            RdfTerm::blank_node(facet_list),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet_list),
            RDF_FIRST,
            RdfTerm::blank_node(facet),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet_list),
            RDF_REST,
            RdfTerm::iri(RDF_NIL),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(
            RdfTerm::blank_node(facet),
            MIN_EXCLUSIVE,
            RdfTerm::Literal(RdfLiteral::typed(
                "0.0",
                "http://www.w3.org/2001/XMLSchema#float",
            )),
        )
        .in_graph(RdfTerm::iri(W)),
        // a rdf:type [ ∃dp.dt ] — the existential obligation, no asserted literal.
        quad(restriction, ON_PROPERTY, P),
        RdfQuad::new(
            RdfTerm::blank_node(restriction),
            SOME_VALUES_FROM,
            RdfTerm::blank_node(dt),
        )
        .in_graph(RdfTerm::iri(W)),
        RdfQuad::new(RdfTerm::iri(X), TYPE, RdfTerm::blank_node(restriction))
            .in_graph(RdfTerm::iri(W)),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.gaps.is_empty(),
        "an existential into a facet datatype is a live obligation ⇒ honest gap: {:?}",
        verdict.coverage
    );
    for family in ["onDatatype", "withRestrictions", "minExclusive"] {
        assert!(
            verdict.coverage.unsupported.contains(&family.to_owned()),
            "{family} must be unsupported for an existential into a facet datatype: {:?}",
            verdict.coverage
        );
    }
}

/// The PRODUCTION facet shape: a named class carries its value restriction as an
/// anonymous `rdfs:subClassOf` filler (`C ⊑ ∀p.D`), an individual is typed to the
/// NAMED class, and the property additionally has a plain `rdfs:range`. The
/// literal satisfies the facet, and the value-space sub-decider proves it, so the
/// facet families are DECIDED — no gap. Nothing here reaches the individual
/// through a direct `rdf:type` on the restriction, and the property is
/// constrained by TWO datatypes at once; before both were wired the decider saw
/// no obligation at all and the facets were reported out-of-fragment even though
/// the engine can settle them.
#[test]
fn datatype_facet_through_subclass_restriction_is_decided_when_satisfied() {
    let verdict = subclass_facet_verdict("0.3");
    assert!(
        verdict.consistent,
        "a literal inside the facet range is consistent: {:?}",
        verdict.inconsistencies
    );
    assert!(
        verdict.gaps.is_empty(),
        "a satisfied facet the sub-decider proves must NOT be a gap: {:?}",
        verdict.gaps
    );
    for family in [
        "onDatatype",
        "withRestrictions",
        "minInclusive",
        "maxInclusive",
    ] {
        assert!(
            verdict.coverage.decided.contains(&family.to_owned()),
            "{family} must be decided for a proven-satisfied facet: {:?}",
            verdict.coverage
        );
    }
}

/// The falsifiable twin: the SAME shape with a literal OUTSIDE the facet range is
/// decided INCONSISTENT — the value-space clash is materialized as an
/// `owl:Nothing` witness — never a silent pass. A promotion to `decided` that
/// could not also catch the violation would be the wrong-`consistent` this
/// coverage machinery exists to prevent.
#[test]
fn datatype_facet_through_subclass_restriction_is_decided_inconsistent_when_violated() {
    let verdict = subclass_facet_verdict("1.5");
    assert!(
        !verdict.consistent,
        "a literal outside the facet range must be decided INCONSISTENT: {:?}",
        verdict.coverage
    );
    assert!(
        verdict.gaps.is_empty(),
        "the violation is DECIDED, so it is a verdict and not a gap: {:?}",
        verdict.gaps
    );
}

/// A `xsd:pattern` facet in the same production shape stays an honest gap: sound
/// XSD-pattern value-space reasoning is outside the sub-decider's certified
/// fragment, so the witness is never definitively evaluated and the families are
/// NOT promoted. This is the falsifiable control on the per-obligation coverage
/// check — it promotes exactly what the sub-decider settles, never a whole family
/// because a sibling obligation happened to be settled.
#[test]
fn datatype_pattern_facet_through_subclass_restriction_stays_an_honest_gap() {
    const WITH_RESTRICTIONS: &str = OWL_WITH_RESTRICTIONS;
    const ON_DATATYPE: &str = OWL_ON_DATATYPE;
    const PATTERN: &str = XSD_PATTERN;
    const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let dt = "http://gmeow.example/CodeString";
    let cell = "http://gmeow.example/facetCell";
    let facet = "http://gmeow.example/facet0";
    let restriction = "http://gmeow.example/restr";
    let store = dataset(vec![
        quad(dt, ON_DATATYPE, XSD_STRING),
        quad(dt, WITH_RESTRICTIONS, cell),
        quad(cell, RDF_FIRST, facet),
        quad(cell, RDF_REST, RDF_NIL),
        literal_quad(facet, PATTERN, "^[A-Z]+$", XSD_STRING),
        quad(P, TYPE, DATATYPE_PROPERTY),
        quad(C, SUBCLASS, restriction),
        quad(restriction, ON_PROPERTY, P),
        quad(restriction, ALL_VALUES_FROM, dt),
        quad(X, TYPE, C),
        literal_quad(X, P, "AB", XSD_STRING),
    ]);
    let verdict = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");

    assert!(
        !verdict.gaps.is_empty(),
        "a pattern facet is outside the certified fragment ⇒ honest gap: {:?}",
        verdict.coverage
    );
    for family in ["onDatatype", "withRestrictions", "pattern"] {
        assert!(
            verdict.coverage.unsupported.contains(&family.to_owned()),
            "{family} must stay unsupported under a pattern facet: {:?}",
            verdict.coverage
        );
    }
}

/// The shared production-shaped fixture for the two facet tests above:
/// `C ⊑ ∀p.(xsd:decimal[0,1])`, `p rdfs:range xsd:decimal`, `x a C`, `x p value`.
fn subclass_facet_verdict(value: &str) -> DlVerdict {
    const WITH_RESTRICTIONS: &str = OWL_WITH_RESTRICTIONS;
    const ON_DATATYPE: &str = OWL_ON_DATATYPE;
    const MIN_INCLUSIVE: &str = XSD_MIN_INCLUSIVE;
    const MAX_INCLUSIVE: &str = XSD_MAX_INCLUSIVE;
    const RANGE: &str = RDFS_RANGE;
    const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let dt = "http://gmeow.example/UnitInterval";
    let cell1 = "http://gmeow.example/facetCell1";
    let cell2 = "http://gmeow.example/facetCell2";
    let lo = "http://gmeow.example/facetLo";
    let hi = "http://gmeow.example/facetHi";
    let restriction = "http://gmeow.example/restr";
    let store = dataset(vec![
        // dt = xsd:decimal restricted to [0, 1] through a two-cell facet list.
        quad(dt, ON_DATATYPE, DECIMAL),
        quad(dt, WITH_RESTRICTIONS, cell1),
        quad(cell1, RDF_FIRST, lo),
        quad(cell1, RDF_REST, cell2),
        quad(cell2, RDF_FIRST, hi),
        quad(cell2, RDF_REST, RDF_NIL),
        literal_quad(lo, MIN_INCLUSIVE, "0", DECIMAL),
        literal_quad(hi, MAX_INCLUSIVE, "1", DECIMAL),
        // p is a datatype property with a plain range AND a class-local ∀p.dt.
        quad(P, TYPE, DATATYPE_PROPERTY),
        quad(P, RANGE, DECIMAL),
        quad(C, SUBCLASS, restriction),
        quad(restriction, ON_PROPERTY, P),
        quad(restriction, ALL_VALUES_FROM, dt),
        // The individual is typed to the NAMED class, never to the restriction.
        quad(X, TYPE, C),
        literal_quad(X, P, value, DECIMAL),
    ]);
    dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed")
}

/// The typed `ReasoningResult` fold: an out-of-fragment bundle with no derived
/// contradiction is `information=undetermined` (honest cannot-decide) — it is
/// NOT `is_decided_consistent()`, and its completeness drops to `Incomplete`.
/// This pins the API-level withholding of the positive consistency verdict.
#[test]
fn out_of_fragment_reasoning_result_is_undetermined_not_decided_consistent() {
    use crate::result::InformationState;
    const TOP: &str = OWL_TOP_OBJECT_PROPERTY;
    let store = dataset(vec![quad(X, TOP, Y), quad(A, SUBCLASS, B)]);
    let result = crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("native reason_all must decide");

    assert_eq!(
        result.information,
        InformationState::Undetermined,
        "out-of-fragment consistency is undetermined, never a positive verdict"
    );
    assert!(
        !result.is_decided_consistent(),
        "cannot-decide is NOT a decided-consistent verdict"
    );
    assert!(
        !result.preservation.unsupported_constructs.is_empty(),
        "the undecided construct is disclosed in the preservation set"
    );
}

// ── Wave A: property-characteristic + disjointness/identity clash families ──

fn bn_iri(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}
fn bn_bn(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::blank_node(o)).in_graph(RdfTerm::iri(W))
}
/// A three-member RDF list `[a b c]` rooted at blank node `root`.
fn list3(root: &str, a: &str, b: &str, c: &str) -> Vec<RdfQuad> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let n1 = root.to_string();
    let n2 = format!("{root}-2");
    let n3 = format!("{root}-3");
    vec![
        bn_iri(&n1, RDF_FIRST, a),
        bn_bn(&n1, RDF_REST, &n2),
        bn_iri(&n2, RDF_FIRST, b),
        bn_bn(&n2, RDF_REST, &n3),
        bn_iri(&n3, RDF_FIRST, c),
        bn_iri(&n3, RDF_REST, RDF_NIL),
    ]
}

const P1: &str = "http://gmeow.example/p1";
const P2: &str = "http://gmeow.example/p2";
const P3: &str = "http://gmeow.example/p3";
const O: &str = "http://gmeow.example/o";

#[test]
fn asymmetric_property_cycle_is_inconsistent() {
    // p AsymmetricProperty, x p y, y p x ⇒ Nothing(x).
    let store = dataset(vec![
        quad(P, TYPE, OWL_ASYMMETRIC_PROPERTY),
        quad(X, P, Y),
        quad(Y, P, X),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "asymmetric-property cycle clashes: {:?}",
        v.inconsistencies
    );
    assert!(v.gaps.is_empty(), "no gap: {:?}", v.gaps);
    assert!(
        v.coverage
            .decided
            .contains(&"asymmetricProperty".to_owned())
    );
}

#[test]
fn asymmetric_property_without_cycle_is_consistent() {
    // Falsifiable: a single directed edge on an asymmetric property is fine.
    let store = dataset(vec![quad(P, TYPE, OWL_ASYMMETRIC_PROPERTY), quad(X, P, Y)]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        v.consistent,
        "no reverse edge ⇒ consistent: {:?}",
        v.inconsistencies
    );
}

#[test]
fn symmetric_plus_asymmetric_edge_is_inconsistent() {
    // p is BOTH Symmetric and Asymmetric (the OWL-2 `-term` shape). x p y ⇒
    // (symmetric) y p x ⇒ (asymmetric) Nothing(x).
    let store = dataset(vec![
        quad(P, TYPE, OWL_SYMMETRIC_PROPERTY),
        quad(P, TYPE, OWL_ASYMMETRIC_PROPERTY),
        quad(X, P, Y),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "symmetric+asymmetric edge clashes: {:?}",
        v.inconsistencies
    );
}

#[test]
fn irreflexive_property_self_loop_is_inconsistent() {
    // p IrreflexiveProperty, x p x ⇒ Nothing(x).
    let store = dataset(vec![quad(P, TYPE, OWL_IRREFLEXIVE_PROPERTY), quad(X, P, X)]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "irreflexive self-loop clashes: {:?}",
        v.inconsistencies
    );
    assert!(
        v.coverage
            .decided
            .contains(&"irreflexiveProperty".to_owned())
    );
}

#[test]
fn property_disjoint_with_shared_object_is_inconsistent() {
    // p1 propertyDisjointWith p2, s p1 o, s p2 o ⇒ Nothing(s).
    let store = dataset(vec![
        quad(P1, OWL_PROPERTY_DISJOINT_WITH, P2),
        quad(X, P1, O),
        quad(X, P2, O),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "disjoint properties sharing a value clash: {:?}",
        v.inconsistencies
    );
    assert!(
        v.coverage
            .decided
            .contains(&"propertyDisjointWith".to_owned())
    );
}

#[test]
fn property_disjoint_with_shared_literal_is_inconsistent() {
    // Data-property disjointness is literal-aware: same literal VALUE on two
    // disjoint data properties clashes (the disjointdataproperties shape).
    let str_ty = "http://www.w3.org/2001/XMLSchema#string";
    let store = dataset(vec![
        quad(P1, OWL_PROPERTY_DISJOINT_WITH, P2),
        literal_quad(X, P1, "Peter Griffin", str_ty),
        literal_quad(X, P2, "Peter Griffin", str_ty),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "shared literal on disjoint data properties clashes: {:?}",
        v.inconsistencies
    );
}

#[test]
fn self_disjoint_property_with_a_value_is_inconsistent() {
    // p propertyDisjointWith p (irreflexive shape): any single asserted value
    // is a value on both `p` and `p` ⇒ clash.
    let store = dataset(vec![quad(P, OWL_PROPERTY_DISJOINT_WITH, P), quad(X, P, O)]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "self-disjoint property with a value clashes: {:?}",
        v.inconsistencies
    );
}

#[test]
fn equivalent_disjoint_properties_clash_via_propagation() {
    // p1 ≡ p2 AND p1 disjointWith p2, s1 p1 o1, s2 p2 o2. Equivalence copies
    // each assertion onto the other property, so s1 carries p1(o1) and p2(o1)
    // ⇒ clash.
    let o1 = "http://gmeow.example/o1";
    let s1 = "http://gmeow.example/s1";
    let store = dataset(vec![
        quad(P1, OWL_EQUIVALENT_PROPERTY, P2),
        quad(P1, OWL_PROPERTY_DISJOINT_WITH, P2),
        quad(s1, P1, o1),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "equivalent+disjoint properties clash: {:?}",
        v.inconsistencies
    );
}

#[test]
fn all_disjoint_properties_expands_and_clashes() {
    // AllDisjointProperties [p1 p2 p3], s p1 o, s p2 o ⇒ Nothing(s).
    let mut quads = vec![
        quad(
            "http://gmeow.example/adp",
            TYPE,
            OWL_ALL_DISJOINT_PROPERTIES,
        ),
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/adp"),
            OWL_MEMBERS,
            RdfTerm::blank_node("adplist"),
        )
        .in_graph(RdfTerm::iri(W)),
        quad(X, P1, O),
        quad(X, P2, O),
    ];
    quads.extend(list3("adplist", P1, P2, P3));
    let store = dataset(quads);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "AllDisjointProperties collision clashes: {:?}",
        v.inconsistencies
    );
    assert!(
        v.coverage
            .decided
            .contains(&"allDisjointProperties".to_owned())
    );
}

#[test]
fn same_as_and_different_from_is_inconsistent() {
    // x sameAs y AND x differentFrom y ⇒ Nothing(x).
    let store = dataset(vec![
        quad(X, OWL_SAME_AS, Y),
        quad(X, OWL_DIFFERENT_FROM, Y),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "sameAs ⊓ differentFrom clashes: {:?}",
        v.inconsistencies
    );
}

#[test]
fn reflexive_different_from_is_inconsistent() {
    // x differentFrom x ⇒ Nothing(x) (everything is sameAs itself).
    let store = dataset(vec![quad(X, OWL_DIFFERENT_FROM, X)]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "reflexive differentFrom clashes: {:?}",
        v.inconsistencies
    );
}

#[test]
fn all_different_with_sameas_member_is_inconsistent() {
    // AllDifferent [w1 w2 w3] with w1 sameAs w2 ⇒ Nothing (the expanded
    // differentFrom(w1,w2) contradicts sameAs).
    let w1 = "http://gmeow.example/w1";
    let w2 = "http://gmeow.example/w2";
    let w3 = "http://gmeow.example/w3";
    let mut quads = vec![
        quad("http://gmeow.example/ad", TYPE, OWL_ALL_DIFFERENT),
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/ad"),
            OWL_MEMBERS,
            RdfTerm::blank_node("adlist"),
        )
        .in_graph(RdfTerm::iri(W)),
        quad(w1, OWL_SAME_AS, w2),
    ];
    quads.extend(list3("adlist", w1, w2, w3));
    let store = dataset(quads);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "AllDifferent with a sameAs member clashes: {:?}",
        v.inconsistencies
    );
    assert!(v.coverage.decided.contains(&"allDifferent".to_owned()));
}

#[test]
fn all_disjoint_classes_expands_and_clashes() {
    // AllDisjointClasses [c1 c2 c3], w a c1, w a c2 ⇒ Nothing(w) (via the
    // expanded disjointWith + existing individual-clash).
    let c1 = "http://gmeow.example/c1";
    let c2 = "http://gmeow.example/c2";
    let c3 = "http://gmeow.example/c3";
    let w = "http://gmeow.example/w-ind";
    let mut quads = vec![
        quad("http://gmeow.example/adc", TYPE, OWL_ALL_DISJOINT_CLASSES),
        RdfQuad::new(
            RdfTerm::iri("http://gmeow.example/adc"),
            OWL_MEMBERS,
            RdfTerm::blank_node("adclist"),
        )
        .in_graph(RdfTerm::iri(W)),
        quad(w, TYPE, c1),
        quad(w, TYPE, c2),
    ];
    quads.extend(list3("adclist", c1, c2, c3));
    let store = dataset(quads);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "AllDisjointClasses membership clashes: {:?}",
        v.inconsistencies
    );
    assert!(
        v.coverage
            .decided
            .contains(&"allDisjointClasses".to_owned())
    );
}

#[test]
fn negative_property_assertion_by_shape_without_type_is_inconsistent() {
    // The OWL-2 `-fw` NPA shape: the NPA node carries source/property/target
    // but NO `rdf:type owl:NegativePropertyAssertion`; native infers NPA-hood
    // structurally and clashes it against the positive assertion.
    let s = "http://gmeow.example/s";
    let store = dataset(vec![
        bn_iri("npa", OWL_SOURCE_INDIVIDUAL, s),
        bn_iri("npa", OWL_ASSERTION_PROPERTY, P),
        bn_iri("npa", OWL_TARGET_INDIVIDUAL, O),
        quad(s, P, O),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "structural NPA clashes its positive: {:?}",
        v.inconsistencies
    );
}

#[test]
fn functional_property_two_named_fillers_no_differentfrom_is_consistent() {
    // SOUNDNESS FLOOR (no UNA): a functional property with two merely-named
    // fillers and NO owl:differentFrom is CONSISTENT — they may be owl:sameAs.
    // (The old UNA default reported a FALSE inconsistency here: the OWL-2
    // char-functional-inst / WebOnt-FunctionalProperty-00{1,2} regression.)
    let store = dataset(vec![
        quad(P, TYPE, OWL_FUNCTIONAL_PROPERTY),
        quad(X, P, Y),
        quad(X, P, Z),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        v.consistent,
        "two named functional-property fillers without differentFrom must NOT clash: {:?}",
        v.inconsistencies
    );
    assert!(v.gaps.is_empty(), "no gap: {:?}", v.gaps);
}

#[test]
fn functional_property_two_provably_distinct_fillers_clash() {
    // With explicit owl:differentFrom the two fillers ARE provably distinct ⇒
    // the functional property clashes (soundly).
    let store = dataset(vec![
        quad(P, TYPE, OWL_FUNCTIONAL_PROPERTY),
        quad(X, P, Y),
        quad(X, P, Z),
        quad(Y, OWL_DIFFERENT_FROM, Z),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        !v.consistent,
        "differentFrom fillers on a functional property clash: {:?}",
        v.inconsistencies
    );
}

#[test]
fn functional_property_distinct_xml_literals_are_withheld() {
    // Two lexically-distinct rdf:XMLLiteral values on a functional property:
    // the chase cannot canonicalize XML, so the family is honestly WITHHELD (a
    // gap), never a wrong clash. (OWL-2 WebOnt-miscellaneous-202.)
    let store = dataset(vec![
        quad(P, TYPE, OWL_FUNCTIONAL_PROPERTY),
        literal_quad(X, P, "<a></a>", RDF_XML_LITERAL),
        literal_quad(X, P, "<a/>", RDF_XML_LITERAL),
    ]);
    let v = dl_consistency(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("dl consistency should succeed");
    assert!(
        v.coverage
            .unsupported
            .contains(&"functionalProperty".to_owned()),
        "unresolvable XMLLiteral functional values are withheld: {:?}",
        v.coverage
    );
    assert!(
        !v.gaps.is_empty(),
        "the withheld XMLLiteral shape surfaces as a gap: {:?}",
        v.gaps
    );
}

#[test]
fn native_conflicts_publish_only_supported_local_membership() {
    let conflicting = dataset(vec![quad(X, SAME_AS, Y), quad(X, DIFFERENT_FROM, Y)]);
    let (inferred, verdict) = crate::reason::reason_closure(
        crate::reason::prepare_reasoning_input(conflicting.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(!verdict.consistent);
    assert!(
        verdict
            .inconsistencies
            .iter()
            .any(|row| row.individual == X && row.world == W)
    );
    assert!(
        inferred
            .iter()
            .filter(|row| row
                .object
                .as_iri()
                .and_then(|object| EmptyClassAssertion::classify(&row.predicate, object))
                == Some(EmptyClassAssertion::Membership))
            .all(|row| [X, Y].contains(&row.subject.as_str())
                && row.world == W
                && !row.premises.is_empty())
    );
    for benign in [
        dataset(vec![quad(X, SAME_AS, Y)]),
        dataset(vec![quad(X, "urn:ordinary-data", Y)]),
    ] {
        let (inferred, verdict) = crate::reason::reason_closure(
            crate::reason::prepare_reasoning_input(benign.as_ref()).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        assert!(verdict.consistent);
        assert!(!inferred.iter().any(|row| {
            row.object
                .as_iri()
                .and_then(|object| EmptyClassAssertion::classify(&row.predicate, object))
                == Some(EmptyClassAssertion::Membership)
        }));
    }
}
