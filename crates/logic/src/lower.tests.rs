// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{
    AggregateSpec, AtomicTerm, ContextualScope, LogicModality, ReasoningContract, SemanticProfileId,
};

fn input() -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &purrdf::RdfQuad::new(
            purrdf::RdfTerm::iri("urn:subject"),
            "urn:input",
            purrdf::RdfTerm::iri("urn:value"),
        )
        .in_graph(purrdf::RdfTerm::iri("urn:world")),
    );
    builder.freeze().unwrap()
}

fn assert_session_refuses_source(program: &LogicProgram, reason: &str) {
    let error = crate::runtime::ReasoningSession::open(
        &input(),
        program,
        &ReasoningContract::new(),
        &crate::annotation::AnnotationContract::exact(),
    )
    .expect_err("source refusal must survive session classification");
    assert!(error.message().contains(reason), "{}", error.message());
}

fn program() -> LogicProgram {
    let atom = |predicate| {
        LogicAxiom::new(
            "?x",
            predicate,
            AtomicTerm::Var("?value".into()),
            false,
            ContextualScope::default(),
        )
        .unwrap()
    };
    LogicProgram::new(
        Vec::new(),
        vec![LogicRule::new(
            atom("urn:derived"),
            vec![atom("urn:input")],
            Vec::new(),
            ContextualScope::new(
                None,
                None,
                None,
                LogicModality::None,
                Some("urn:source-rule".into()),
                None,
            )
            .unwrap(),
        )],
        Vec::new(),
        Some("urn:source-theory".into()),
    )
}

#[test]
fn malformed_aggregates_and_negative_heads_cannot_be_erased_into_positive_rules() {
    let original = program();
    let prepared = crate::program_analysis::prepare_program(&original).unwrap();
    assert_eq!(prepared.rules.len(), 1);
    let input = input();
    let mut unsupported = Vec::new();
    for function in ["SUM", "MIN", "MAX", "COUNT", "AVG", "PRODUCT"] {
        let mut changed = original.clone();
        changed.rules[0].aggregation = Some(AggregateSpec::new(
            function,
            "?value",
            "?value",
            vec!["?x".into()],
        ));
        unsupported.push((changed, "aggregation"));
    }
    let mut negative = original.clone();
    negative.rules[0].head.negated = true;
    unsupported.push((negative, "negative head"));
    for (changed, reason) in unsupported {
        let before = changed.canonical_key();
        let error = lower_eval_rules(&changed).unwrap_err();
        assert!(error.message().contains(reason));
        assert!(error.message().contains("urn:source-rule"));
        assert_session_refuses_source(&changed, reason);
        // A previously cached positive template cannot admit a different
        // source operator, including on a second attempt.
        for _ in 0..2 {
            assert!(crate::program_analysis::prepare_program(&changed).is_err());
        }
        assert!(crate::certify::certify_program(&changed, "positive-horn").is_err());
        assert!(crate::cost::RepeatForwardSession::prepare(&input, &changed, "contract").is_err());
        assert!(
            crate::materialize::materialize_program(
                &changed,
                &input,
                crate::materialize::MaterializationLimits { max_steps: None },
                None,
            )
            .is_err()
        );
        assert_eq!(
            before,
            changed.canonical_key(),
            "failed admission preserves the complete source"
        );
    }
}

#[test]
fn source_kinds_cannot_be_reinterpreted_as_object_level_rule_execution() {
    for position in 0..3 {
        let mut changed = program();
        match position {
            0 => changed.rules[0].node_kind = NodeKind::Constraint,
            1 => changed.rules[0].head.node_kind = NodeKind::MetaLevelFormula,
            _ => changed.rules[0].body[0].node_kind = NodeKind::MetaLevelFormula,
        }
        assert!(lower_eval_rules(&changed).is_err());
        assert!(crate::program_analysis::prepare_program(&changed).is_err());
        assert_session_refuses_source(&changed, "kind");
    }
    let mut ordinary = program();
    ordinary.rules[0].body[0].negated = true;
    assert!(
        lower_eval_rules(&ordinary).unwrap()[0].body[0].negated,
        "a supported negative premise retains its sign for the selected evaluator"
    );
}

#[test]
fn direct_execution_and_certification_enforce_the_retained_context_admission() {
    for position in 0..3 {
        for axis in 0..4 {
            let mut changed = program();
            let scope = match position {
                0 => &mut changed.rules[0].scope,
                1 => &mut changed.rules[0].head.scope,
                _ => &mut changed.rules[0].body[0].scope,
            };
            match axis {
                0 => scope.standpoint = Some("urn:standpoint".into()),
                1 => scope.time = Some("urn:time".into()),
                2 => scope.module = Some("urn:module".into()),
                _ => scope.modality = LogicModality::Deontic,
            }
            let prepared = crate::program_analysis::prepare_program(&changed).unwrap();
            assert!(prepared.admission.admit_world_local_template().is_err());
            assert!(lower_eval_rules(&changed).is_err());
            assert!(crate::certify::certify_program(&changed, "positive-horn").is_err());
            assert_session_refuses_source(&changed, "world-local template admission");
        }
    }
}

#[test]
fn cached_materialization_cannot_execute_unadmitted_scope_in_any_selected_profile() {
    use crate::annotation::{AnnotationFactRef, AnnotationRequest};
    use crate::materialize::{MaterializationLimits, materialize_program};
    let input = input();
    let limits = MaterializationLimits::default();
    let profiles = [
        SemanticProfileId::PositiveHorn,
        SemanticProfileId::StratifiedNaf,
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
    ];
    for profile in profiles {
        let ordinary = materialize_program(&program(), &input, limits, Some(profile)).unwrap();
        assert!(
            ordinary
                .quads
                .iter()
                .any(|quad| quad.predicate == "urn:derived")
        );
    }
    for position in 0..3 {
        let mut scoped = program();
        let scope = match position {
            0 => &mut scoped.rules[0].scope,
            1 => &mut scoped.rules[0].head.scope,
            _ => &mut scoped.rules[0].body[0].scope,
        };
        scope.standpoint = Some("urn:standpoint".into());
        let prepared = crate::program_analysis::prepare_program(&scoped).unwrap();
        let again = crate::program_analysis::prepare_program(&scoped).unwrap();
        assert!(std::sync::Arc::ptr_eq(&prepared, &again));
        let publication = prepared.publication();
        assert!(publication.joint_program().is_err());
        for profile in profiles {
            let error = materialize_program(&scoped, &input, limits, Some(profile)).unwrap_err();
            assert!(error.to_string().contains("urn:standpoint"));
            assert!(
                crate::materialize::materialize_prepared(&publication, &input, limits, profile)
                    .is_err()
            );
            let annotation = crate::annotation::AnnotationContract::exact();
            let visited = std::cell::Cell::new(false);
            let result = crate::materialize::materialize_program_annotated(
                &scoped,
                &input,
                limits,
                Some(profile),
                AnnotationRequest::new(
                    &crate::provenance::ZWeightSemiring,
                    &annotation,
                    |_fact: AnnotationFactRef<'_>| {
                        visited.set(true);
                        Some(1)
                    },
                ),
            );
            assert!(result.is_err());
            assert!(
                !visited.get(),
                "unadmitted rules never reach annotation evaluation"
            );
        }
    }
}

#[test]
fn literal_question_mark_is_never_a_native_variable() {
    let literal = purrdf::RdfLiteral {
        lexical_form: "?x".to_owned(),
        datatype: None,
        language: Some("ar".to_owned()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let axiom = LogicAxiom::ground("urn:s", "urn:p", AtomicTerm::Literal(literal.clone())).unwrap();
    let native = lower_atom(&axiom, false).unwrap();
    assert_eq!(
        native.object,
        EvalTerm::ConstLit(crate::rule_ir::literal_value(&literal))
    );
    let axiom = LogicAxiom::ground("urn:s", "urn:p", AtomicTerm::Var("?x".to_owned())).unwrap();
    assert_eq!(
        lower_atom(&axiom, false).unwrap().object,
        EvalTerm::Var("?x".to_owned())
    );
}
