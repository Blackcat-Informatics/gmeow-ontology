// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::{JointProgram, NativeOutcome};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};
const WORLD: &str = "urn:native-family:world";
const SAME: &str = "http://www.w3.org/2002/07/owl#sameAs";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";

fn fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    }
}
struct Frame {
    store: FactStore,
    rel: RelationStore,
    rows: Vec<DerivedRow>,
    evidence: NativeEvidenceIndex,
}
impl Frame {
    fn new(facts: &[Fact]) -> Self {
        let mut store = FactStore::new();
        let mut rel = RelationStore::with_semantics(
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        );
        for fact in facts {
            store.insert(fact.clone());
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        let sources = facts
            .iter()
            .map(|fact| RefutationPremise {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
                graph: Some(TermValue::iri(WORLD)),
            })
            .collect::<Vec<_>>();
        let evidence = NativeEvidenceIndex::new(
            WORLD.to_owned(),
            Some(TermValue::iri(WORLD)),
            [7; 32],
            sources.into(),
            &store,
            &crate::physical::SelectedDomains::new(Vec::new()).unwrap(),
        )
        .unwrap();
        Self {
            store,
            rel,
            rows: Vec::new(),
            evidence,
        }
    }
    fn ledger(&self, completed: bool, allowance: Option<u64>) -> NativeFamilyLedger {
        let completion = if completed {
            BTreeSet::from([NativeRead {
                marker: None,
                predicate: None,
                kind: NativeReadKind::Completed,
            }])
        } else {
            BTreeSet::new()
        };
        let input = NativeFamilyInput::new(
            &self.store,
            &self.rel,
            &self.rows,
            &self.evidence,
            &completion,
        )
        .unwrap();
        let mut ledger = NativeFamilyLedger::new(
            WORLD.to_owned(),
            Some(TermValue::iri(WORLD)),
            [7; 32],
            allowance,
        );
        let mut coverage = crate::reason::dl::SourceCoverageWorld {
            graph: Some(TermValue::iri(WORLD)),
            constructs: Vec::new(),
            admissions: Vec::new(),
        };
        let mut values = SchemaValues::default();
        let mut lists = LogicalListCache::default();
        for fact in self.store.facts() {
            crate::reason::dl::observe_construct(fact, &input, &mut ledger, &mut coverage).unwrap();
        }
        crate::reason::dl::admit_source_constructs(
            &input,
            &mut values,
            &mut lists,
            &mut ledger,
            &mut coverage,
        )
        .unwrap();
        analyze(
            &input.with_admissions(&coverage),
            &mut values,
            &mut lists,
            &mut ledger,
        )
        .unwrap();
        ledger
    }
}

#[test]
fn late_native_equality_waits_for_completion_and_preserves_committed_derivation_support() {
    let facts = vec![
        fact("urn:a", DIFFERENT, "urn:b"),
        fact("urn:a", "urn:late-equality", "urn:b"),
    ];
    let mut frame = Frame::new(&facts);
    let before = frame.ledger(false, None);
    assert!(!before.has_conflict());
    assert!(!before.complete());
    assert!(
        before
            .outcomes
            .iter()
            .any(|outcome| outcome.family == NativeRefutationFamily::Identity
                && matches!(outcome.completion, NativeFamilyCompletion::Awaiting { .. }))
    );
    let rule = EvalRule::positive(
        "urn:rule:late-equality",
        EvalAtom::positive(EvalTerm::var("?x"), SAME, EvalTerm::var("?y")),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            "urn:late-equality",
            EvalTerm::var("?y"),
        )],
    );
    let NativeOutcome::Decided(program) = JointProgram::prepare(&[rule], &[]).unwrap() else {
        panic!("finite ordinary producer")
    };
    let NativeOutcome::Decided(result) = program
        .materialize_facts(&BTreeMap::from([(WORLD.to_owned(), facts)]), None)
        .unwrap()
    else {
        panic!("finite ordinary closure")
    };
    assert_eq!(result.result.status, crate::seam::BudgetStatus::Ok);
    frame.rows = result.result.rows;
    for row in &frame.rows {
        let fact = Fact {
            subject: row.subject.clone(),
            predicate: row.predicate.clone(),
            object: row.object.clone(),
        };
        if frame.store.insert(fact.clone()).is_some() {
            frame
                .rel
                .insert(&fact.predicate, &fact.subject, &fact.object);
        }
    }
    frame
        .evidence
        .observe(&frame.store, &frame.rows, &result.witness_derivations)
        .unwrap();
    let after = frame.ledger(true, None);
    assert!(after.has_conflict());
    let equality = after
        .proofs
        .iter()
        .find(|proof| proof.statement.predicate == SAME)
        .unwrap();
    let NativeProofOrigin::Derived { rule, premises, .. } = &equality.origin else {
        panic!("committed equality is not asserted input")
    };
    assert_eq!(rule, "urn:rule:late-equality");
    assert_eq!(premises.len(), 1);
    assert!(
        after.proofs.iter().any(
            |proof| proof.id == premises[0] && proof.statement.predicate == "urn:late-equality"
        )
    );
    assert!(
        after
            .outcomes
            .iter()
            .filter(|outcome| outcome.family == NativeRefutationFamily::Identity)
            .all(|outcome| outcome.completion == NativeFamilyCompletion::Complete)
    );
}

#[test]
fn missing_or_relabelled_native_support_fails_closed() {
    let mut frame = Frame::new(&[fact("urn:a", DIFFERENT, "urn:a")]);
    let mut valid = frame.ledger(true, None);
    assert!(valid.has_conflict());
    valid.validate().unwrap();
    let proof = valid.proofs.first_mut().unwrap();
    if let NativeProofOrigin::Asserted { sources, .. } = &mut proof.origin {
        sources[0].graph = Some(TermValue::iri("urn:other"));
    }
    assert!(valid.validate().is_err());
    let absent = fact("urn:unaccounted", SAME, "urn:a");
    frame.store.insert(absent.clone());
    frame
        .rel
        .insert(&absent.predicate, &absent.subject, &absent.object);
    assert!(frame.evidence.observe(&frame.store, &[], &[]).is_err());
}

#[test]
fn evidence_cannot_be_reused_in_another_native_world_or_contract() {
    let frame = Frame::new(&[fact("urn:a", DIFFERENT, "urn:a")]);
    let completed = BTreeSet::new();
    let input = NativeFamilyInput::new(
        &frame.store,
        &frame.rel,
        &frame.rows,
        &frame.evidence,
        &completed,
    )
    .unwrap();
    for (world, contract) in [("urn:another", [7; 32]), (WORLD, [8; 32])] {
        let mut ledger = NativeFamilyLedger::new(
            world.to_owned(),
            Some(TermValue::iri(WORLD)),
            contract,
            None,
        );
        assert!(input.support(frame.store.facts(), &mut ledger).is_err());
    }
}

#[test]
fn zero_analysis_budget_never_publishes_an_unexamined_conflict() {
    let frame = Frame::new(&[fact("urn:a", DIFFERENT, "urn:a")]);
    let ledger = frame.ledger(true, Some(0));
    assert!(!ledger.has_conflict());
    assert!(!ledger.complete());
    assert!(ledger.work.exhausted);
    assert_eq!(ledger.work.consumed, 0);
    assert!(
        ledger
            .outcomes
            .iter()
            .any(|outcome| outcome.completion == NativeFamilyCompletion::Exhausted)
    );
}

fn source(
    rows: &[(&str, &str, &str)],
    literals: &[(&str, &str, &str, &str)],
) -> Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for &(s, p, o) in rows {
        builder.push_owned_quad(&RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)));
    }
    for &(s, p, o, dt) in literals {
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::iri(s),
            p,
            RdfTerm::literal(RdfLiteral::typed(o, dt)),
        ));
    }
    builder.freeze().unwrap()
}

#[test]
fn every_independent_property_and_source_bound_survives_positive_refutation() {
    let source = source(
        &[
            (
                "urn:x",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "urn:C",
            ),
            (
                "urn:C",
                "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "urn:r1",
            ),
            (
                "urn:C",
                "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "urn:r2",
            ),
            (
                "urn:r1",
                "http://www.w3.org/2002/07/owl#onProperty",
                "urn:p",
            ),
            (
                "urn:r2",
                "http://www.w3.org/2002/07/owl#onProperty",
                "urn:q",
            ),
        ],
        &[
            (
                "urn:r1",
                "http://www.w3.org/2002/07/owl#minCardinality",
                "2",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                "urn:r1",
                "http://www.w3.org/2002/07/owl#maxCardinality",
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                "urn:r2",
                "http://www.w3.org/2002/07/owl#minCardinality",
                "3",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                "urn:r2",
                "http://www.w3.org/2002/07/owl#maxCardinality",
                "0",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                "urn:r2",
                "http://www.w3.org/2002/07/owl#minCardinality",
                "invalid",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
        ],
    );
    let ledgers = testing::fixtures(source.as_ref(), false);
    let outcomes: Vec<_> = ledgers
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .filter(|outcome| outcome.family == NativeRefutationFamily::Cardinality)
        .collect();
    let properties = outcomes
        .iter()
        .filter_map(|outcome| match &outcome.obligation {
            NativeObligationScope::Property { property, .. } if !outcome.conclusions.is_empty() => {
                Some(property.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(properties, BTreeSet::from([TermValue::iri("urn:p")]));
    assert!(outcomes.iter().any(|outcome| matches!(&outcome.obligation,
        NativeObligationScope::Definition { owner } if *owner == TermValue::iri("urn:r2"))
        && !outcome.obstructions.is_empty()));
    assert!(
        outcomes
            .iter()
            .any(|outcome| !outcome.obstructions.is_empty())
    );
    let bounds: BTreeSet<_> = outcomes
        .iter()
        .flat_map(|outcome| &outcome.bounds)
        .map(|bound| (&bound.owner, &bound.predicate, &bound.value))
        .collect();
    assert_eq!(bounds.len(), 5);
    assert!(
        outcomes
            .iter()
            .flat_map(|outcome| &outcome.bounds)
            .any(|bound| bound.interpreted.is_none() && !bound.support.is_empty())
    );
}

#[test]
fn a_datatype_conflict_does_not_erase_an_unrelated_invalid_definition() {
    let source = source(
        &[
            (
                "urn:p",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://www.w3.org/2002/07/owl#DatatypeProperty",
            ),
            (
                "urn:p",
                "http://www.w3.org/2000/01/rdf-schema#range",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                "urn:bad",
                "http://www.w3.org/2002/07/owl#datatypeComplementOf",
                "urn:bad",
            ),
        ],
        &[(
            "urn:x",
            "urn:p",
            "text",
            "http://www.w3.org/2001/XMLSchema#string",
        )],
    );
    let ledgers = testing::fixtures(source.as_ref(), false);
    let outcomes: Vec<_> = ledgers
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .filter(|outcome| outcome.family == NativeRefutationFamily::Datatype)
        .collect();
    assert!(
        outcomes
            .iter()
            .any(|outcome| !outcome.conclusions.is_empty())
    );
    assert!(outcomes.iter().any(|outcome| matches!(&outcome.obligation, NativeObligationScope::Definition { owner } if *owner == TermValue::iri("urn:bad")) && !outcome.obstructions.is_empty()));
}

#[test]
fn malformed_distinctness_lists_are_not_semantic_contradictions() {
    let source = source(
        &[
            (
                "urn:list-owner",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://www.w3.org/2002/07/owl#AllDifferent",
            ),
            (
                "urn:list-owner",
                "http://www.w3.org/2002/07/owl#distinctMembers",
                "urn:list",
            ),
            (
                "urn:list",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
                "urn:a",
            ),
        ],
        &[],
    );
    let ledgers = testing::fixtures(source.as_ref(), false);
    assert!(ledgers.iter().all(|ledger| !ledger.has_conflict()));
    assert!(
        ledgers
            .iter()
            .flat_map(|ledger| &ledger.outcomes)
            .any(|outcome| outcome.family == NativeRefutationFamily::Identity
                && outcome.completion == NativeFamilyCompletion::Obstructed)
    );
}

#[test]
fn original_native_blank_occurrences_survive_execution_identity_lowering() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(
        RdfTerm::blank_node("owned"),
        DIFFERENT,
        RdfTerm::blank_node("owned"),
    ));
    let source = builder.freeze().unwrap();
    let ledgers = testing::fixtures(source.as_ref(), false);
    let ledger = ledgers.iter().find(|ledger| ledger.has_conflict()).unwrap();
    let proof = ledger
        .proofs
        .iter()
        .find(|proof| proof.statement.predicate == DIFFERENT)
        .unwrap();
    let NativeProofOrigin::Asserted { sources, .. } = &proof.origin else {
        panic!("original assertion")
    };
    assert!(matches!(sources[0].subject, TermValue::Blank { .. }));
    assert_eq!(sources[0].subject, sources[0].object);
    assert_eq!(
        crate::facts::skolemize(&sources[0].subject).as_ref(),
        &proof.statement.subject
    );
    assert!(proof.statement.subject.as_iri().is_some());
    ledger.validate().unwrap();
}

#[test]
fn canonical_roles_preserve_source_spellings_without_aliasing_ordinary_terms() {
    let rows = [
        (
            "urn:p",
            "https://blackcatinformatics.ca/logic/instanceOf",
            "https://blackcatinformatics.ca/logic/DatatypeProperty",
        ),
        (
            "urn:p",
            "https://blackcatinformatics.ca/logic/range",
            "http://www.w3.org/2001/XMLSchema#integer",
        ),
        (
            "urn:ordinary",
            "urn:mentions",
            "https://blackcatinformatics.ca/logic/DatatypeProperty",
        ),
    ];
    let source = source(
        &rows,
        &[(
            "urn:x",
            "urn:p",
            "text",
            "http://www.w3.org/2001/XMLSchema#string",
        )],
    );
    let ledgers = testing::fixtures(source.as_ref(), false);
    let ledger = ledgers.iter().find(|ledger| ledger.has_conflict()).unwrap();
    assert!(ledger.proofs.iter().any(|proof| proof.statement.predicate
        == "https://blackcatinformatics.ca/logic/instanceOf"
        && proof.statement.object
            == TermValue::iri("https://blackcatinformatics.ca/logic/DatatypeProperty")));
    assert!(!ledger.proofs.iter().any(|proof| proof.statement.subject
        == TermValue::iri("urn:ordinary")
        && proof.statement.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"));
}

#[test]
fn intrinsic_domain_support_requires_the_actual_committed_scoped_receipt() {
    use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    let selected = SelectedLogicalWorld::new(
        LogicalGraph::Named(TermValue::iri(WORLD)),
        DomainProfile::NonemptyObjectDomainV1,
        "urn:test:family-domain-admission".to_owned(),
        [9; 32],
    )
    .unwrap();
    let domains = SelectedDomains::new([selected]).unwrap();
    let source =
        gmeow_logic_compile::ir::LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None);
    let prepared = crate::program_analysis::prepare_program(&source).unwrap();
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    let facts = BTreeMap::from([(WORLD.to_owned(), Vec::new())]);
    let laws = crate::reason::program::applicable_schema_laws(
        &prepared,
        &facts,
        &sources,
        &domains,
        &[],
        &[],
    );
    let selected = prepared
        .reasoning_input(
            &laws,
            &sources,
            &domains,
            &facts,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    let NativeOutcome::Decided(program) = prepared.reasoning_schema_program(&selected).unwrap()
    else {
        panic!("selected intrinsic domain")
    };
    let graphs = BTreeMap::from([(WORLD.to_owned(), Some(TermValue::iri(WORLD)))]);
    let origins = BTreeMap::from([(WORLD.to_owned(), Arc::from([]))]);
    let binding = selected
        .bind_native(
            &origins,
            &graphs,
            Some(crate::modal::native::NativeModalProgram::prepare(&origins).unwrap()),
            Some(Arc::new(
                crate::contextual::native::NativeContextualProgram::prepare(&origins).unwrap(),
            )),
        )
        .unwrap();
    let NativeOutcome::Decided(result) = program
        .materialize_input_governed(
            &selected,
            binding,
            &mut crate::physical::StepGovernor::new(None),
            &mut crate::physical::SkolemRegistry::new(),
        )
        .unwrap()
    else {
        panic!("shared intrinsic executor")
    };
    let mut store = FactStore::new();
    let mut rel =
        RelationStore::with_semantics(crate::native_semantics::SemanticVocabulary::GroundedLogicV1);
    let mut evidence = NativeEvidenceIndex::new(
        WORLD.to_owned(),
        Some(TermValue::iri(WORLD)),
        [9; 32],
        Arc::from([]),
        &store,
        &domains,
    )
    .unwrap();
    for row in &result.result.rows {
        let fact = Fact {
            subject: row.subject.clone(),
            predicate: row.predicate.clone(),
            object: row.object.clone(),
        };
        store.insert(fact.clone());
        rel.insert(&fact.predicate, &fact.subject, &fact.object);
    }
    assert!(evidence.observe(&store, &result.result.rows, &[]).is_err());
    evidence
        .observe(&store, &result.result.rows, &result.witness_derivations)
        .unwrap();
    let completed = BTreeSet::new();
    let input =
        NativeFamilyInput::new(&store, &rel, &result.result.rows, &evidence, &completed).unwrap();
    let receipt = result.witness_derivations.first().unwrap();
    let head = &receipt.heads[0].statement;
    let mut ledger =
        NativeFamilyLedger::new(WORLD.to_owned(), Some(TermValue::iri(WORLD)), [9; 32], None);
    let support = input
        .support(
            &[Fact {
                subject: head.subject.clone(),
                predicate: head.predicate.clone(),
                object: head.object.clone(),
            }],
            &mut ledger,
        )
        .unwrap();
    assert_eq!(support.len(), 1);
    assert!(
        matches!(&ledger.proofs[0].origin, NativeProofOrigin::Intrinsic { witness } if witness == receipt)
    );
    assert!(
        !ledger
            .proofs
            .iter()
            .any(|proof| matches!(proof.origin, NativeProofOrigin::Asserted { .. }))
    );
    ledger.validate().unwrap();
}

#[test]
fn relational_native_blank_identity_cannot_be_relabelled_as_rdf_ingress() {
    let blank = TermValue::blank("native-identity");
    let fact = Fact {
        subject: blank,
        predicate: DIFFERENT.to_owned(),
        object: TermValue::iri("urn:other"),
    };
    let mut store = FactStore::new();
    let mut rel = RelationStore::with_semantics(crate::native_semantics::SemanticVocabulary::Exact);
    store.insert(fact.clone());
    rel.insert(&fact.predicate, &fact.subject, &fact.object);
    let originals = vec![RefutationPremise {
        subject: fact.subject.clone(),
        predicate: fact.predicate.clone(),
        object: fact.object.clone(),
        graph: None,
    }];
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    let evidence = NativeEvidenceIndex::from_native_facts(
        "urn:relational".to_owned(),
        [19; 32],
        originals.into(),
        &store,
        &domains,
    )
    .unwrap();
    let complete = BTreeSet::new();
    let input = NativeFamilyInput::new(&store, &rel, &[], &evidence, &complete).unwrap();
    let mut ledger = NativeFamilyLedger::new("urn:relational".to_owned(), None, [19; 32], None);
    ledger.source_terms = NativeSourceTerms::NativeFacts;
    input
        .support(std::slice::from_ref(&fact), &mut ledger)
        .unwrap();
    ledger.validate().unwrap();
    assert_eq!(ledger.proofs[0].statement.subject, fact.subject);
    assert!(matches!(
        ledger.proofs[0].origin,
        NativeProofOrigin::Asserted {
            terms: NativeSourceTerms::NativeFacts,
            ..
        }
    ));
    ledger.source_terms = NativeSourceTerms::RdfSkolem;
    let NativeProofOrigin::Asserted { terms, .. } = &mut ledger.proofs[0].origin else {
        panic!("source proof")
    };
    *terms = NativeSourceTerms::RdfSkolem;
    let identity = ledger.proof_id(&ledger.proofs[0].statement, &ledger.proofs[0].origin);
    ledger.proofs[0].id = identity;
    assert!(
        ledger.validate().is_err(),
        "rehashed mode cannot reinterpret an exact native blank as RDF lowering"
    );
}
