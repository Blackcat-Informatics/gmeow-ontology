// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native datatype obligation integration, independent of the terminal DL rescue.
use super::*;
use crate::reason::refute::Decision;
use crate::reason::refute::native::testing;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const P: &str = "urn:p";
const S: &str = "urn:s";
const R: &str = "urn:restriction";
const D: &str = "urn:datatype";
const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#datatypeComplementOf";
const MIN_I: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const MAX_I: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const MIN_E: &str = "http://www.w3.org/2001/XMLSchema#minExclusive";
const MAX_E: &str = "http://www.w3.org/2001/XMLSchema#maxExclusive";

fn fact(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}
fn literal(s: &str, p: &str, value: &str, datatype: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri(s),
        p,
        RdfTerm::literal(RdfLiteral::typed(value, datatype)),
    )
}
fn source(facts: Vec<RdfQuad>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for fact in facts {
        builder.push_owned_quad(&fact);
    }
    builder.freeze().unwrap()
}
fn base() -> Vec<RdfQuad> {
    vec![
        fact(P, TYPE, DATATYPE_PROPERTY),
        fact(S, TYPE, R),
        fact(R, PROPERTY, P),
    ]
}
fn restricted(datatype: &str) -> Vec<RdfQuad> {
    let mut facts = base();
    facts.extend([
        fact(R, SOME, D),
        fact(D, ON_DATATYPE, datatype),
        fact(D, RESTRICTIONS, "urn:list"),
        fact("urn:list", FIRST, "urn:facet"),
        fact("urn:list", REST, NIL),
    ]);
    facts
}
fn observations(facts: Vec<RdfQuad>) -> Vec<NativeFamilyLedger> {
    testing::fixtures(source(facts).as_ref(), false)
}
fn decision(facts: Vec<RdfQuad>) -> Option<Decision> {
    testing::decision(&observations(facts), |family| {
        family == NativeRefutationFamily::Datatype
    })
}

fn whole_case_decided_consistent(facts: Vec<RdfQuad>) -> bool {
    let source = source(facts);
    crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(source.as_ref()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
    )
    .unwrap()
    .is_decided_consistent()
}

#[test]
fn existential_fillers_do_not_constrain_unrelated_existing_values_or_each_other() {
    let mut facts = base();
    facts.extend([
        fact(R, SOME, INTEGER),
        fact(R, SOME, STRING),
        literal(S, P, "unrelated", STRING),
    ]);
    assert_eq!(decision(facts.clone()), Some(Decision::Consistent));
    facts.push(fact(R, ALL, INTEGER));
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
}

#[test]
fn an_independent_datatype_value_cannot_be_hidden_by_a_satisfied_existential() {
    for extra in [
        vec![
            fact("urn:q", TYPE, DATATYPE_PROPERTY),
            fact("urn:q", RANGE, INTEGER),
            literal("urn:other", "urn:q", "text", STRING),
        ],
        vec![
            fact("urn:q", TYPE, DATATYPE_PROPERTY),
            fact("urn:other", "urn:q", "urn:resource"),
        ],
    ] {
        let mut facts = base();
        facts.push(fact(R, SOME, STRING));
        facts.extend(extra);
        assert_eq!(decision(facts), Some(Decision::Inconsistent));
    }
}

#[test]
fn unmodeled_class_and_property_obligations_withhold_whole_case_consistency() {
    for extra in [
        vec![fact(
            "urn:other",
            TYPE,
            "http://www.w3.org/2002/07/owl#Nothing",
        )],
        vec![
            fact("urn:other", TYPE, "urn:other:restriction"),
            fact("urn:other:restriction", PROPERTY, "urn:undeclared:property"),
            literal("urn:other:restriction", MIN, "1", INTEGER),
            literal("urn:other:restriction", MAX, "0", INTEGER),
        ],
        vec![
            fact("urn:other", TYPE, "urn:empty:class"),
            fact("urn:empty:class", ONE_OF, NIL),
        ],
    ] {
        let mut facts = base();
        facts.push(fact(R, SOME, STRING));
        facts.extend(extra);
        assert!(!whole_case_decided_consistent(facts));
    }
}

#[test]
fn unreachable_datatype_definitions_still_require_complete_admission() {
    for definition in [
        vec![
            fact("urn:unused", ON_DATATYPE, "urn:unknown:datatype"),
            fact("urn:unused", RESTRICTIONS, NIL),
        ],
        vec![
            fact(INTEGER, ON_DATATYPE, STRING),
            fact(INTEGER, RESTRICTIONS, NIL),
        ],
        vec![
            fact("urn:unused", ON_DATATYPE, INTEGER),
            fact("urn:unused", RESTRICTIONS, "urn:incomplete:list"),
        ],
    ] {
        let mut facts = base();
        facts.push(fact(R, SOME, STRING));
        facts.extend(definition);
        assert_eq!(decision(facts), None);
    }
}

#[test]
fn large_source_counts_retain_their_full_value_in_clash_evidence() {
    let count = 1u128 << 64;
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, "http://www.w3.org/2001/XMLSchema#byte"),
        literal(R, MIN, &count.to_string(), INTEGER),
    ]);
    let ledgers = observations(facts);
    assert_eq!(
        testing::decision(&ledgers, |family| family
            == NativeRefutationFamily::Datatype),
        Some(Decision::Inconsistent)
    );
    assert!(
        ledgers
            .iter()
            .flat_map(|ledger| &ledger.outcomes)
            .flat_map(|outcome| &outcome.bounds)
            .any(|bound| bound.interpreted == Some(count) && !bound.support.is_empty())
    );
}

#[test]
fn byte_capacity_and_explicit_qualified_bounds() {
    let byte = "http://www.w3.org/2001/XMLSchema#byte";
    for qualified in [false, true] {
        for (count, expected) in [
            ("256", Decision::Consistent),
            ("257", Decision::Inconsistent),
        ] {
            let mut facts = base();
            facts.push(literal(
                R,
                if qualified { QUALIFIED_MIN } else { MIN },
                count,
                INTEGER,
            ));
            facts.push(if qualified {
                fact(R, QUALIFIER, byte)
            } else {
                fact(P, RANGE, byte)
            });
            assert_eq!(decision(facts), Some(expected));
        }
    }
    let mut missing = base();
    missing.push(literal(R, QUALIFIED_MIN, "257", INTEGER));
    assert_eq!(decision(missing), None);
}

#[test]
fn every_bound_is_conjunctive_and_exact_zero_conflicts_with_existence() {
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, "http://www.w3.org/2001/XMLSchema#byte"),
        literal(R, MIN, "257", INTEGER),
        literal(R, MIN, "1", INTEGER),
    ]);
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
    let mut facts = base();
    facts.extend([fact(R, SOME, INTEGER), literal(R, EXACT, "0", INTEGER)]);
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
    let mut facts = base();
    facts.extend([fact(R, SOME, INTEGER), literal(R, MIN, "1.0", DECIMAL)]);
    assert_eq!(decision(facts), None);
}

#[test]
fn float_discrete_range_is_empty_and_all_bounds_apply() {
    for (upper, expected) in [
        ("1.401298464324817e-45", Decision::Inconsistent),
        ("1.0", Decision::Consistent),
    ] {
        let mut facts = restricted(FLOAT);
        facts.extend([
            literal("urn:facet", MIN_E, "0.0", FLOAT),
            literal("urn:facet", MAX_E, upper, FLOAT),
        ]);
        assert_eq!(decision(facts), Some(expected));
    }
    let mut facts = restricted(INTEGER);
    facts.extend([
        literal("urn:facet", MIN_I, "2", INTEGER),
        literal("urn:facet", MIN_E, "2", INTEGER),
        literal("urn:facet", MAX_I, "2", INTEGER),
    ]);
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
}

#[test]
fn whitespace_padded_numeric_facet_keeps_the_shared_native_interpretation() {
    for padding in ["5", "  5  "] {
        let mut facts = restricted(INTEGER);
        facts.extend([
            literal("urn:facet", MIN_I, padding, INTEGER),
            literal("urn:facet", MAX_I, "3", INTEGER),
        ]);
        assert_eq!(decision(facts), Some(Decision::Inconsistent));
    }
}

#[test]
fn length_facet_emptiness_and_pattern_withholding_remain_explicit() {
    for (minimum, expected) in [("5", Decision::Inconsistent), ("2", Decision::Consistent)] {
        let mut facts = restricted(STRING);
        facts.extend([
            literal(
                "urn:facet",
                "http://www.w3.org/2001/XMLSchema#minLength",
                minimum,
                INTEGER,
            ),
            literal(
                "urn:facet",
                "http://www.w3.org/2001/XMLSchema#maxLength",
                "3",
                INTEGER,
            ),
        ]);
        assert_eq!(decision(facts), Some(expected));
    }
    let mut facts = restricted(STRING);
    facts.push(literal(
        "urn:facet",
        "http://www.w3.org/2001/XMLSchema#pattern",
        ".*",
        STRING,
    ));
    assert_eq!(decision(facts), None);
}

#[test]
fn positive_integer_complement_membership_and_double_complement_capacity() {
    for (value, datatype, expected) in [
        ("-1", INTEGER, Decision::Consistent),
        ("text", STRING, Decision::Consistent),
        ("5", INTEGER, Decision::Inconsistent),
    ] {
        let facts = vec![
            fact(P, TYPE, DATATYPE_PROPERTY),
            fact(P, RANGE, D),
            fact(
                D,
                COMPLEMENT,
                "http://www.w3.org/2001/XMLSchema#positiveInteger",
            ),
            literal(S, P, value, datatype),
        ];
        assert_eq!(decision(facts), Some(expected));
    }
    let mut facts = base();
    facts.extend([
        fact(R, SOME, D),
        fact(D, COMPLEMENT, "urn:notempty"),
        fact("urn:notempty", COMPLEMENT, "urn:empty"),
        fact("urn:empty", ONE_OF, NIL),
    ]);
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
}

#[test]
fn enumeration_capacity_uses_exact_rational_values() {
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, D),
        literal(R, MIN, "2", INTEGER),
        fact(D, ONE_OF, "urn:enum:1"),
        literal("urn:enum:1", FIRST, "0.5", DECIMAL),
        fact("urn:enum:1", REST, "urn:enum:2"),
        literal(
            "urn:enum:2",
            FIRST,
            "1/2",
            "http://www.w3.org/2002/07/owl#rational",
        ),
        fact("urn:enum:2", REST, NIL),
    ]);
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
}

#[test]
fn unknown_enumerands_withhold_models_even_when_a_known_member_meets_the_minimum() {
    for (value, datatype) in [("unknown", "urn:unknown:datatype"), ("invalid", INTEGER)] {
        let mut facts = base();
        facts.extend([
            fact(P, RANGE, D),
            literal(R, MIN, "1", INTEGER),
            fact(D, ONE_OF, "urn:enum:1"),
            literal("urn:enum:1", FIRST, "1", INTEGER),
            fact("urn:enum:1", REST, "urn:enum:2"),
            literal("urn:enum:2", FIRST, value, datatype),
            fact("urn:enum:2", REST, NIL),
        ]);
        assert_eq!(decision(facts), None);
    }
}

#[test]
fn datatype_enumerations_retain_language_and_direction_and_exclude_plain_strings() {
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, D),
        literal(R, MIN, "4", INTEGER),
        fact(D, ONE_OF, "urn:enum:0"),
    ]);
    for (index, (language, direction)) in [
        ("en", None),
        ("fr", None),
        ("ar", Some(purrdf::RdfTextDirection::Ltr)),
        ("ar", Some(purrdf::RdfTextDirection::Rtl)),
    ]
    .into_iter()
    .enumerate()
    {
        let node = format!("urn:enum:{index}");
        facts.push(RdfQuad::new(
            RdfTerm::iri(&node),
            FIRST,
            RdfTerm::literal(RdfLiteral {
                lexical_form: "word".to_owned(),
                datatype: None,
                language: Some(language.to_owned()),
                direction,
            }),
        ));
        facts.push(fact(
            &node,
            REST,
            if index == 3 {
                NIL.to_owned()
            } else {
                format!("urn:enum:{}", index + 1)
            }
            .as_str(),
        ));
    }
    assert_eq!(decision(facts.clone()), Some(Decision::Consistent));
    facts.push(literal(S, P, "word", STRING));
    assert_eq!(decision(facts), Some(Decision::Inconsistent));
}

#[test]
fn world_local_definitions_and_actual_inherited_proof_paths_are_preserved() {
    let mut facts = Vec::new();
    for (world, minimum) in [("urn:world:blocked", "3"), ("urn:world:open", "1")] {
        let mut local = restricted(INTEGER);
        local.retain(|quad| !(quad.subject == RdfTerm::iri(S) && quad.predicate == TYPE));
        local.extend([
            fact(S, TYPE, "urn:class"),
            fact("urn:class", SUBCLASS, R),
            literal("urn:facet", MIN_I, minimum, INTEGER),
            literal("urn:facet", MAX_I, "2", INTEGER),
        ]);
        facts.extend(
            local
                .into_iter()
                .map(|quad| quad.in_graph(RdfTerm::iri(world))),
        );
    }
    let ledgers = observations(facts);
    let clashes: Vec<_> = ledgers
        .iter()
        .flat_map(|ledger| {
            ledger
                .outcomes
                .iter()
                .filter(|outcome| outcome.family == NativeRefutationFamily::Datatype)
                .flat_map(move |outcome| {
                    outcome.conclusions.iter().map(move |clash| (ledger, clash))
                })
        })
        .collect();
    assert_eq!(clashes.len(), 1);
    let (ledger, clash) = clashes[0];
    assert_eq!(ledger.world, "urn:world:blocked");
    let premises: Vec<_> = clash
        .support
        .iter()
        .map(|id| {
            &ledger
                .proofs
                .iter()
                .find(|proof| proof.id == *id)
                .unwrap()
                .statement
        })
        .collect();
    assert!(premises.iter().any(|fact| fact.subject == TermValue::iri(S)
        && fact.predicate == TYPE
        && fact.object == TermValue::iri("urn:class")));
    assert!(
        premises
            .iter()
            .any(|fact| fact.subject == TermValue::iri("urn:class")
                && fact.predicate == SUBCLASS
                && fact.object == TermValue::iri(R))
    );
    assert!(
        !premises.iter().any(|fact| fact.subject == TermValue::iri(S)
            && fact.predicate == TYPE
            && fact.object == TermValue::iri(R))
    );
    assert!(
        premises
            .iter()
            .any(|fact| fact.subject == TermValue::iri("urn:facet") && fact.predicate == MIN_I)
    );
}

#[test]
fn malformed_or_cyclic_definitions_do_not_become_empty_prefix_enumerations() {
    for definition in [
        vec![
            fact(D, ONE_OF, "urn:list"),
            literal("urn:list", FIRST, "1", INTEGER),
        ],
        vec![fact(D, COMPLEMENT, D)],
    ] {
        let mut facts = base();
        facts.push(fact(R, SOME, D));
        facts.extend(definition);
        assert_eq!(decision(facts), None);
    }
}
