// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW grouping, stage completion and provenance contracts. Aggregate numeric
//! and RDF conformance remains tested by the upstream accumulator owner.

use std::sync::Arc;

use gmeow_logic_compile::ir::{
    AggregateSpec, AtomicTerm, ContextualScope, LogicAxiom, LogicModality, LogicProgram, LogicRule,
    SemanticProfileId,
};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

use crate::annotation::{AnnotationContract, AnnotationQueryClass, AnnotationRequest};
use crate::materialize::{Materialization, MaterializationLimits, materialize_program};

fn atom(subject: &str, predicate: &str, object: &str) -> LogicAxiom {
    LogicAxiom::new(
        subject,
        predicate,
        AtomicTerm::resource(object),
        false,
        ContextualScope::default(),
    )
    .unwrap()
}

fn rule(name: &str, head: LogicAxiom, body: Vec<LogicAxiom>) -> LogicRule {
    LogicRule::new(
        head,
        body,
        Vec::new(),
        ContextualScope::new(
            None,
            None,
            None,
            LogicModality::None,
            Some(name.into()),
            None,
        )
        .unwrap(),
    )
}

fn program(function: &str) -> LogicProgram {
    let reduce = rule(
        "urn:reduce",
        atom("?group", "urn:result", "?result"),
        vec![
            atom("?member", "urn:group", "?group"),
            atom("?member", "urn:value", "?value"),
        ],
    )
    .with_aggregation(AggregateSpec::new(
        function,
        "?value",
        "?result",
        vec!["?group".into()],
    ));
    LogicProgram::new(
        Vec::new(),
        vec![reduce],
        Vec::new(),
        Some("urn:program".into()),
    )
}

fn input(rows: &[(&str, &str, &str, RdfTerm)]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (world, subject, predicate, object) in rows {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(*subject), *predicate, object.clone())
                .in_graph(RdfTerm::iri(*world)),
        );
    }
    builder.freeze().unwrap()
}

fn members() -> Arc<RdfDataset> {
    input(&[
        ("urn:w1", "urn:a", "urn:group", RdfTerm::iri("urn:g")),
        ("urn:w1", "urn:b", "urn:group", RdfTerm::iri("urn:g")),
        (
            "urn:w1",
            "urn:a",
            "urn:value",
            RdfTerm::iri("urn:equal-value"),
        ),
        (
            "urn:w1",
            "urn:b",
            "urn:value",
            RdfTerm::iri("urn:equal-value"),
        ),
        ("urn:w2", "urn:c", "urn:group", RdfTerm::iri("urn:g")),
        (
            "urn:w2",
            "urn:c",
            "urn:value",
            RdfTerm::iri("urn:equal-value"),
        ),
    ])
}

fn outputs(materialized: &Materialization) -> Vec<(String, TermValue)> {
    materialized
        .quads
        .iter()
        .filter(|row| row.predicate == "urn:result")
        .map(|row| (row.graph.clone(), row.object.clone()))
        .collect()
}

#[test]
fn complete_groups_count_members_preserve_worlds_and_reuse_prepared_identity() {
    let program = program("COUNT");
    let input = members();
    let run = || {
        materialize_program(
            &program,
            &input,
            MaterializationLimits::default(),
            Some(SemanticProfileId::StratifiedNaf),
        )
        .unwrap()
    };
    let first = run();
    let second = run();
    let rows = outputs(&first);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        (
            "urn:w1".into(),
            TermValue::typed_literal("2", "http://www.w3.org/2001/XMLSchema#integer")
        )
    );
    assert_eq!(
        rows[1],
        (
            "urn:w2".into(),
            TermValue::typed_literal("1", "http://www.w3.org/2001/XMLSchema#integer")
        )
    );
    assert_eq!(rows, outputs(&second));
    let supports: Vec<_> = first
        .quads
        .iter()
        .filter(|row| row.predicate == "urn:result")
        .map(|row| row.source_quad_ids.len())
        .collect();
    assert_eq!(supports, [4, 2]);
    assert_eq!(
        crate::lower::lower_eval_rules(&program).unwrap()[0]
            .reduction
            .as_ref()
            .unwrap()
            .name(),
        "COUNT"
    );
}

#[test]
fn reduce_waits_for_transitive_writers_and_never_publishes_partial_counts() {
    let mut program = program("COUNT");
    program.rules.push(rule(
        "urn:late-value",
        atom("?member", "urn:value", "?value"),
        vec![atom("?member", "urn:late", "?value")],
    ));
    program.rules.push(rule(
        "urn:late-seed",
        atom("?member", "urn:late", "?value"),
        vec![atom("?member", "urn:seed", "?value")],
    ));
    let input = input(&[
        ("urn:w1", "urn:a", "urn:group", RdfTerm::iri("urn:g")),
        ("urn:w1", "urn:b", "urn:group", RdfTerm::iri("urn:g")),
        ("urn:w1", "urn:a", "urn:value", RdfTerm::iri("urn:x")),
        ("urn:w1", "urn:b", "urn:seed", RdfTerm::iri("urn:x")),
    ]);
    let full = materialize_program(
        &program,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert_eq!(outputs(&full).len(), 1);
    assert_eq!(
        outputs(&full)[0].1,
        TermValue::typed_literal("2", "http://www.w3.org/2001/XMLSchema#integer")
    );
    let partial = materialize_program(
        &program,
        &input,
        MaterializationLimits { max_steps: Some(1) },
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert!(outputs(&partial).is_empty());
    assert!(!partial.frontier.saturated_preds.contains("urn:result"));
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let crate::physical::NativeOutcome::Decided(joint) = prepared.joint_program().unwrap() else {
        panic!("finite aggregate schedule must be admitted");
    };
    let store = crate::store::WorldStore::new();
    store.load_dataset(&input).unwrap();
    let crate::physical::NativeOutcome::Decided(result) = joint.materialize(&store, None).unwrap()
    else {
        panic!("aggregate materialization must finish");
    };
    let derived: Vec<_> = result
        .result
        .rows
        .iter()
        .filter(|row| row.predicate == "urn:result")
        .collect();
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0].object, outputs(&full)[0].1);
}

#[test]
fn aggregation_is_a_strict_dependency_even_without_negation() {
    let mut program = program("COUNT");
    program.rules.push(rule(
        "urn:feedback",
        atom("?group", "urn:value", "?value"),
        vec![atom("?group", "urn:result", "?value")],
    ));
    let rules = crate::lower::lower_eval_rules(&program).unwrap();
    assert!(
        crate::physical::compile_cached("reduce-cycle", rules)
            .executable
            .is_none()
    );
    let error = materialize_program(
        &program,
        &members(),
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap_err();
    assert!(error.to_string().contains("non-stratifiable"));
}

#[test]
fn guard_filters_members_before_reduction_and_undefined_groups_fail_closed() {
    let mut program = program("COUNT");
    program.rules[0]
        .distinct_pairs
        .push(("?member".into(), "?group".into()));
    let input = input(&[
        ("urn:w1", "urn:g", "urn:group", RdfTerm::iri("urn:g")),
        ("urn:w1", "urn:g", "urn:value", RdfTerm::iri("urn:x")),
        ("urn:w1", "urn:a", "urn:group", RdfTerm::iri("urn:g")),
        ("urn:w1", "urn:a", "urn:value", RdfTerm::iri("urn:x")),
    ]);
    let count = materialize_program(
        &program,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert_eq!(
        outputs(&count)[0].1,
        TermValue::typed_literal("1", "http://www.w3.org/2001/XMLSchema#integer")
    );
    program.rules[0].aggregation.as_mut().unwrap().function = "SUM".into();
    let error = materialize_program(
        &program,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("undefined for its complete group")
    );
}

#[test]
fn annotations_use_complete_group_support_and_incremental_circuits_do_not_erase_reduce() {
    let program = program("COUNT");
    let input = members();
    let contract = AnnotationContract::exact();
    let result = crate::materialize::materialize_program_annotated(
        &program,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
        AnnotationRequest::new(
            &crate::provenance::ZWeightSemiring,
            &contract,
            |_: crate::annotation::AnnotationFactRef<'_>| Some(2),
        ),
    )
    .unwrap();
    assert_eq!(
        result.certification.query_class,
        AnnotationQueryClass::StratifiedAggregate
    );
    assert_eq!(
        result.certification.lineage_contract,
        crate::annotation::AnnotationLineageContract::CompleteGroupSupport
    );
    let values: Vec<_> = result
        .quads
        .iter()
        .filter(|row| row.quad.predicate == "urn:result")
        .map(|row| row.annotation)
        .collect();
    assert_eq!(values, [16, 4]);
    let rules = crate::lower::lower_eval_rules(&program).unwrap();
    let refusal = crate::physical::classify_incremental_fragment(&rules).unwrap_err();
    assert_eq!(
        refusal.reason,
        crate::physical::UnsupportedFragmentReason::Aggregation
    );
    for profile in [
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
    ] {
        assert!(
            materialize_program(
                &program,
                &input,
                MaterializationLimits::default(),
                Some(profile)
            )
            .unwrap_err()
            .to_string()
            .contains("aggregate-aware")
        );
    }
}

#[test]
fn every_reduction_field_participates_in_native_plan_identity() {
    let base = program("COUNT");
    let digest = |program: &LogicProgram| {
        crate::physical::canonical_rule_hash(&crate::lower::lower_eval_rules(program).unwrap())
    };
    for function in ["SUM", "AVG", "MIN", "MAX"] {
        assert_ne!(digest(&base), digest(&program(function)));
    }
    let mut changed = base.clone();
    changed.rules[0].aggregation.as_mut().unwrap().aggregate_var = "?member".into();
    assert_ne!(digest(&base), digest(&changed));
    changed = base.clone();
    changed.rules[0]
        .aggregation
        .as_mut()
        .unwrap()
        .group_keys
        .push("?member".into());
    assert_ne!(digest(&base), digest(&changed));
}

#[test]
fn aggregate_profile_and_empty_group_admission_precede_execution() {
    let program = program("COUNT");
    let verdict = crate::certify::certify_program(&program, "StratifiedNAFProfile").unwrap();
    assert!(verdict.certified, "{:?}", verdict.violations);
    assert!(
        !crate::certify::certify_program(&program, "PositiveHornProfile")
            .unwrap()
            .certified
    );
    let empty = RdfDatasetBuilder::new().freeze().unwrap();
    let error =
        materialize_program(&program, &empty, MaterializationLimits::default(), None).unwrap_err();
    assert!(error.to_string().contains("StratifiedNAFProfile"));

    let mut global = program;
    global.rules[0].head.subject = "urn:global".into();
    global.rules[0]
        .aggregation
        .as_mut()
        .unwrap()
        .group_keys
        .clear();
    let source = input(&[(
        "urn:w1",
        "urn:seed",
        "urn:unrelated",
        RdfTerm::iri("urn:value"),
    )]);
    let result = materialize_program(
        &global,
        &source,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert_eq!(
        outputs(&result),
        [(
            "urn:w1".into(),
            TermValue::typed_literal("0", "http://www.w3.org/2001/XMLSchema#integer")
        )]
    );
    let selected = crate::materialize::materialize_program_view(
        &global,
        source.as_ref(),
        crate::seam::WorldSourceIdentity::new("empty-relevant-extension", "synthetic-reduce-view"),
        &["urn:w1".to_owned()],
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert_eq!(outputs(&selected.materialization), outputs(&result));
    global.rules[0].aggregation.as_mut().unwrap().function = "MIN".into();
    assert!(
        materialize_program(
            &global,
            &source,
            MaterializationLimits::default(),
            Some(SemanticProfileId::StratifiedNaf)
        )
        .unwrap_err()
        .to_string()
        .contains("undefined")
    );
}

#[test]
fn source_bound_joint_schedule_retains_empty_group_and_cycle_effects() {
    use crate::physical::NativeOutcome;
    use std::collections::BTreeMap;
    let mut program = program("COUNT");
    program.rules[0].head.subject = "urn:global".into();
    program.rules[0]
        .aggregation
        .as_mut()
        .unwrap()
        .group_keys
        .clear();
    let facts = BTreeMap::from([("urn:w1".to_owned(), Vec::new())]);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let template = Arc::new(
        crate::physical::JointTemplate::new(
            &prepared.rules,
            &prepared.existential_rules,
            &[],
            crate::native_semantics::SemanticVocabulary::Exact,
        )
        .unwrap(),
    );
    let source = template.input(&facts, Arc::from([]), &[]).unwrap();
    let NativeOutcome::Decided(plan) = source.prepare().unwrap() else {
        panic!("empty global count is finite");
    };
    let NativeOutcome::Decided(result) = plan.materialize_input(&source, None).unwrap() else {
        panic!("count must execute");
    };
    let rows: Vec<_> = result
        .result
        .rows
        .iter()
        .filter(|row| row.predicate == "urn:result")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].object,
        TermValue::typed_literal("0", "http://www.w3.org/2001/XMLSchema#integer")
    );
    program.rules.push(rule(
        "urn:feedback",
        atom("?group", "urn:value", "?value"),
        vec![atom("?group", "urn:result", "?value")],
    ));
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let template = Arc::new(
        crate::physical::JointTemplate::new(
            &prepared.rules,
            &prepared.existential_rules,
            &[],
            crate::native_semantics::SemanticVocabulary::Exact,
        )
        .unwrap(),
    );
    let source = template.input(&facts, Arc::from([]), &[]).unwrap();
    assert!(matches!(
        source.prepare().unwrap(),
        NativeOutcome::Unsupported(_)
    ));
    assert!(!crate::certify::is_stratifiable(
        &crate::lower::lower_eval_rules(&program).unwrap()
    ));
}

#[test]
fn reduction_safety_preserves_bound_negation_and_routes_sessions_to_full_execution() {
    let mut program = program("COUNT");
    let mut negative = atom("?member", "urn:blocked", "?unbound");
    negative.negated = true;
    program.rules[0].body.push(negative);
    assert!(
        crate::lower::lower_eval_rules(&program)
            .unwrap_err()
            .message()
            .contains("negated variables")
    );
    program.rules[0].body.last_mut().unwrap().obj = AtomicTerm::Var("?value".into());
    let result = materialize_program(
        &program,
        &members(),
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
    )
    .unwrap();
    assert_eq!(outputs(&result).len(), 2);
    let session = crate::runtime::ReasoningSession::open(
        &members(),
        &program,
        &gmeow_logic_compile::ir::ReasoningContract::new(),
        &AnnotationContract::exact(),
    )
    .unwrap();
    assert!(matches!(
        session.fragment_disposition(),
        crate::runtime::FragmentDisposition::RequiresFullRebuild(_)
    ));
}

#[test]
fn reduction_and_existential_heads_share_completion_and_explicit_annotation_class() {
    use gmeow_logic_compile::ir::{Formula, Term};
    let conclusion = Formula::Exists {
        vars: vec!["copy".into()],
        body: Box::new(
            Formula::atom(
                Term::iri("urn:copy").unwrap(),
                vec![Term::var("group").unwrap(), Term::var("copy").unwrap()],
            )
            .unwrap(),
        ),
    };
    let source = Formula::atom(
        Term::iri("urn:result").unwrap(),
        vec![Term::var("group").unwrap(), Term::var("count").unwrap()],
    )
    .unwrap();
    let formula = Formula::Forall {
        vars: vec!["group".into(), "count".into()],
        body: Box::new(Formula::Implies(Box::new(source), Box::new(conclusion))),
    };
    let program = program("COUNT").with_formulas(vec![formula]);
    let input = members();
    let contract = AnnotationContract::exact();
    let result = crate::materialize::materialize_program_annotated(
        &program,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::StratifiedNaf),
        AnnotationRequest::new(
            &crate::provenance::ZWeightSemiring,
            &contract,
            |_: crate::annotation::AnnotationFactRef<'_>| Some(2),
        ),
    )
    .unwrap();
    assert_eq!(
        result.certification.query_class,
        AnnotationQueryClass::StratifiedAggregateChase
    );
    assert_eq!(
        result.certification.lineage_contract,
        crate::annotation::AnnotationLineageContract::SelectedPhysicalDerivation
    );
    let copies: Vec<_> = result
        .quads
        .iter()
        .filter(|row| row.quad.predicate == "urn:copy")
        .map(|row| (row.quad.graph.as_str(), row.annotation))
        .collect();
    assert_eq!(copies, [("urn:w1", 16), ("urn:w2", 4)]);
    // FrontierSkolem is keyed by rule + frontier, independently of the world.
    // Both worlds share the recipe for group g, while their facts and scores
    // remain independently scoped as asserted above.
    assert_eq!(result.materialization.witness_derivations.len(), 1);
}
