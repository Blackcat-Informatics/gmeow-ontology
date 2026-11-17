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
        builder.push_owned_quad(&fact).unwrap();
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

const EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";

/// A world whose class model carries an equivalence obstruction: one individual,
/// one datatype property value, and `A equivalentClass (B or C)`.
fn obstructed_world(property: &str, range: &str, value: &str, datatype: &str) -> Vec<RdfQuad> {
    vec![
        fact(property, TYPE, DATATYPE_PROPERTY),
        fact(property, RANGE, range),
        literal(S, property, value, datatype),
        fact(S, TYPE, "urn:carrier"),
        fact("urn:A", EQUIVALENT, "urn:union"),
        fact("urn:union", UNION, "urn:u1"),
        fact("urn:u1", FIRST, "urn:B"),
        fact("urn:u1", REST, "urn:u2"),
        fact("urn:u2", FIRST, "urn:C"),
        fact("urn:u2", REST, NIL),
    ]
}

/// Execute the real source-owned program and retain its committed native evidence.
fn joint_result(
    facts: Vec<RdfQuad>,
    rules: Vec<gmeow_logic_compile::ir::LogicRule>,
) -> crate::reason::program::ProgramClosure {
    let program = gmeow_logic_compile::ir::LogicProgram::new(Vec::new(), rules, Vec::new(), None);
    crate::reason::program::execute(
        &crate::program_analysis::prepare_program(&program).expect("tiny native source program"),
        crate::reason::prepare_reasoning_input(source(facts).as_ref())
            .expect("tiny original source admission"),
        &crate::physical::SelectedDomains::new([]).expect("no implicit domain selection"),
        None,
    )
    .expect("the finite source-owned joint graph executes")
}

fn property_outcomes<'a>(
    result: &'a crate::reason::program::ProgramClosure,
    property: &str,
) -> Vec<(&'a NativeFamilyLedger, &'a NativeFamilyOutcome)> {
    result
        .native_families
        .iter()
        .flat_map(|ledger| ledger.outcomes.iter().map(move |outcome| (ledger, outcome)))
        .filter(|(_, outcome)| outcome.family == NativeRefutationFamily::Datatype)
        .filter(|(_, outcome)| {
            matches!(&outcome.obligation, NativeObligationScope::Property { property: p, .. }
                if *p == TermValue::iri(property))
        })
        .collect()
}

/// A real completed input boundary delays this writer; its head can already exist.
fn late_writer(subject: &str, predicate: &str, object: &str) -> gmeow_logic_compile::ir::LogicRule {
    use gmeow_logic_compile::ir::{AtomicTerm, ContextualScope, LogicAxiom, LogicRule};
    let atom = |subject: &str, predicate: &str, object: &str, negated| {
        LogicAxiom::new(
            subject,
            predicate,
            AtomicTerm::Iri(object.to_owned()),
            negated,
            ContextualScope::default(),
        )
        .expect("well-formed source-owned atom")
    };
    LogicRule::new(
        atom(subject, predicate, object, false),
        vec![
            atom(S, "urn:publication:seed", "urn:publication:start", false),
            atom(S, "urn:publication:absent", "urn:publication:never", true),
        ],
        Vec::new(),
        ContextualScope::default(),
    )
}

/// Unrelated class failure cannot hold a property's final value assessment. A
/// duplicate late writer also closes its boundary without adding a row.
#[test]
fn an_unrelated_class_obstruction_does_not_block_an_unrestricted_property() {
    const UNION_RANGE: &str = "urn:string-or-token";
    const TOKEN: &str = "http://www.w3.org/2001/XMLSchema#token";
    for (property, range, value, datatype) in [
        (
            "urn:termIndex",
            "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            "0",
            INTEGER,
        ),
        ("urn:termVariable", UNION_RANGE, "x", STRING),
    ] {
        let mut facts = obstructed_world(property, range, value, datatype);
        facts.push(fact(S, "urn:publication:seed", "urn:publication:start"));
        if range == UNION_RANGE {
            facts.extend([
                fact(UNION_RANGE, UNION, "urn:range:u1"),
                fact("urn:range:u1", FIRST, STRING),
                fact("urn:range:u1", REST, "urn:range:u2"),
                fact("urn:range:u2", FIRST, TOKEN),
                fact("urn:range:u2", REST, NIL),
            ]);
        }
        let result = joint_result(facts, vec![late_writer(property, RANGE, range)]);
        assert!(
            matches!(
                result.native_status,
                super::super::native::NativeClosureStatus::Blocked { .. }
            ),
            "the unrelated world obstruction is retained: {:?}",
            result.native_status
        );
        let outcomes = property_outcomes(&result, property);
        assert!(!outcomes.is_empty(), "{property} has a real obligation");
        for (_, outcome) in outcomes {
            assert_eq!(
                outcome.completion,
                NativeFamilyCompletion::Complete,
                "{property}"
            );
            assert!(outcome.conclusions.is_empty());
            assert!(
                outcome.publication_reads.iter().any(|read| {
                    matches!(&read.pattern, super::super::native::NativeReadPattern::Statement {
                    subject: Some(subject), predicate: Some(predicate), object: None,
                } if *subject == TermValue::iri(property) && predicate == RANGE)
                }),
                "the complete publication inventory survives a completed boundary"
            );
        }
        assert!(
            !result
                .inferred
                .iter()
                .any(|row| !row.is_edb && row.predicate == RANGE),
            "the real late duplicate writer adds no row"
        );
        assert!(
            result
                .native_families
                .iter()
                .flat_map(|ledger| &ledger.outcomes)
                .any(|outcome| outcome.family == NativeRefutationFamily::HasSelf
                    && matches!(outcome.completion, NativeFamilyCompletion::Blocked { .. })),
            "an unreached nonwriting owner cannot claim completion"
        );
    }

    // With no property firing, the scheduled nonwriting owners must still
    // interpret standalone definitions and establish actual unengaged outcomes.
    let standalone = joint_result(vec![fact(D, ONE_OF, NIL)], Vec::new());
    assert!(standalone.native_families.iter().flat_map(|ledger| &ledger.outcomes).any(|outcome|
        outcome.family == NativeRefutationFamily::Datatype
        && matches!(&outcome.obligation, NativeObligationScope::Definition { owner } if *owner == TermValue::iri(D))
        && outcome.completion == NativeFamilyCompletion::Complete));
    assert!(
        standalone
            .native_families
            .iter()
            .flat_map(|ledger| &ledger.outcomes)
            .any(|outcome| outcome.family == NativeRefutationFamily::HasSelf
                && outcome.completion == NativeFamilyCompletion::NotEngaged)
    );
}

/// Late membership, range and restriction selectors must cause the real family
/// to commit its supported clash before publication-only reads can settle.
#[test]
fn a_restricting_class_that_can_still_be_asserted_keeps_the_property_waiting() {
    use super::super::native::{NativeProofOrigin, NativeReadPattern};
    const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
    const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
    for late in [(S, TYPE, R), (P, RANGE, INTEGER), (R, PROPERTY, P)] {
        let mut facts = obstructed_world(P, STRING, "x", STRING);
        facts.push(fact(S, "urn:publication:seed", "urn:publication:start"));
        match late.1 {
            TYPE => facts.extend([fact(R, PROPERTY, P), fact(R, ALL, INTEGER)]),
            RANGE => {}
            PROPERTY => facts.extend([fact(S, TYPE, R), fact(R, ALL, INTEGER)]),
            _ => unreachable!("the three relevant writer roles are explicit"),
        }
        let result = joint_result(facts, vec![late_writer(late.0, late.1, late.2)]);
        let outcomes = property_outcomes(&result, P);
        let (ledger, outcome, clash) = outcomes
            .iter()
            .find_map(|(ledger, outcome)| {
                outcome
                    .conclusions
                    .iter()
                    .find(|clash| clash.committed.is_some())
                    .map(|clash| (*ledger, *outcome, clash))
            })
            .expect("the scheduled datatype family committed its late-row clash");
        let committed = clash
            .committed
            .expect("the actual clash head was committed");
        let proof = ledger
            .proofs
            .iter()
            .find(|proof| proof.id == committed)
            .expect("the committed head has its original native proof");
        assert_eq!(proof.statement.subject, TermValue::iri(S));
        assert_eq!(proof.statement.predicate, INSTANCE);
        assert_eq!(proof.statement.object, TermValue::iri(NOTHING));
        assert!(
            matches!(&proof.origin, NativeProofOrigin::Derived { rule, .. } if rule == RULE_NAME)
        );
        let sources = ledger
            .source_leaves(&clash.support)
            .expect("all original positive support resolves");
        assert!(
            sources
                .iter()
                .any(|source| source.subject == TermValue::iri(S)
                    && source.predicate == "urn:publication:seed"
                    && source.object == TermValue::iri("urn:publication:start"))
        );
        assert!(sources.iter().any(|source| source.subject == TermValue::iri(S)
            && source.predicate == P
            && matches!(&source.object, TermValue::Literal { lexical_form, .. } if lexical_form == "x")));
        let original = |subject: &str, predicate: &str, object: &str| {
            sources.iter().any(|source| {
                source.subject == TermValue::iri(subject)
                    && source.predicate == predicate
                    && source.object == TermValue::iri(object)
            })
        };
        assert!(
            original(P, TYPE, DATATYPE_PROPERTY),
            "the exact datatype declaration supports the clash"
        );
        assert!(
            original(P, RANGE, STRING),
            "the original range selector survives alongside the late row"
        );
        if late.1 != RANGE {
            assert!(
                original(R, ALL, INTEGER),
                "the selected universal restriction supports the clash"
            );
        }
        if late.1 == TYPE {
            assert!(
                original(R, PROPERTY, P),
                "the restriction's property selector is retained"
            );
        } else if late.1 == PROPERTY {
            assert!(
                original(S, TYPE, R),
                "the restriction's exact source membership is retained"
            );
        }
        assert!(
            clash
                .support
                .iter()
                .filter_map(|id| ledger.proofs.iter().find(|proof| proof.id == *id))
                .any(|proof| proof.statement.subject == TermValue::iri(late.0)
                    && (proof.statement.predicate == late.1
                        || (late.1 == TYPE && proof.statement.predicate == INSTANCE))
                    && proof.statement.object == TermValue::iri(late.2)
                    && matches!(proof.origin, NativeProofOrigin::Derived { .. })),
            "the exact late selector/membership is a committed derived premise of the clash"
        );
        assert!(outcome.publication_reads.iter().any(|read| {
            matches!(&read.pattern, NativeReadPattern::Statement { predicate: Some(predicate), .. }
                if predicate == late.1)
        }), "the relevant late writer remains in publication evidence");
        assert!(
            result.inferred.iter().any(|row| !row.is_edb
                && row.subject == S
                && row.predicate == INSTANCE
                && row.object == TermValue::iri(NOTHING)),
            "publication refresh cannot stand in for a real committed positive row"
        );
    }

    // This is the class owner's actual possible head, unlike an arbitrary B
    // membership. Its ordinary obstruction withholds a relevant writer.
    let mut facts = obstructed_world(P, STRING, "x", STRING);
    facts.extend([fact(NOTHING, PROPERTY, P), fact(NOTHING, ALL, INTEGER)]);
    let held = joint_result(facts, Vec::new());
    assert!(matches!(
        held.native_status,
        super::super::native::NativeClosureStatus::Blocked { .. }
    ));
    assert!(
        held.classes
            .iter()
            .any(|class| !class.obstructions.is_empty()),
        "the actual class owner retained its obstruction"
    );
    let outcomes = property_outcomes(&held, P);
    assert!(!outcomes.is_empty());
    for (_, outcome) in outcomes {
        let pending = match &outcome.completion {
            NativeFamilyCompletion::Awaiting { reads }
            | NativeFamilyCompletion::Blocked { reads } => reads,
            other => panic!("the relevant writer is unfinished: {other:?}"),
        };
        assert!(pending.iter().any(|read| matches!(&read.pattern,
            NativeReadPattern::Statement { subject: None, predicate: Some(predicate), object: Some(TermValue::Iri(object)) }
                if predicate == TYPE && object == NOTHING)), "the exact pending TYPE/Nothing envelope is retained");
        assert!(
            outcome.conclusions.is_empty(),
            "a pending class membership cannot fabricate a clash"
        );
    }
    assert!(!held.inferred.iter().any(|row| !row.is_edb
        && row.subject == S
        && row.predicate == INSTANCE
        && row.object == TermValue::iri(NOTHING)));
}
