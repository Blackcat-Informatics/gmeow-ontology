// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny source-ownership and feedback contracts; no corpus or upstream conformance.

use super::*;
use crate::native_semantics::SemanticVocabulary;
use crate::physical::{ChaseAdmission, JointTemplate, NativeOutcome, StepGovernor};
use crate::rule_ir::EvalRule;
use crate::termination_demonstrators::termination_ladder_demonstrators;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const RULE: &str = "urn:source:rule";
const X: &str = "urn:source:x";
const SEED: &str = "urn:source:seed";
const EDGE: &str = "urn:source:edge";
const DONE: &str = "urn:source:done";
const A: &str = "urn:source:world-a";
const B: &str = "urn:source:world-b";

fn term(value: &str) -> RdfTerm {
    if value.starts_with('?') {
        RdfTerm::literal(RdfLiteral::simple(value))
    } else {
        RdfTerm::iri(value)
    }
}
fn row(world: Option<&str>, subject: &str, predicate: &str, object: RdfTerm) -> RdfQuad {
    let mut row = RdfQuad::new(RdfTerm::iri(subject), predicate, object);
    row.graph_name = world.map(RdfTerm::iri);
    row
}
fn rule(
    world: Option<&str>,
    operator: &str,
    body: [RdfTerm; 3],
    heads: &[[RdfTerm; 3]],
) -> Vec<RdfQuad> {
    let mut rows = vec![row(
        world,
        RULE,
        operator,
        term(&vocabulary("ExistentialRule")),
    )];
    for (side, atoms) in [("body", std::slice::from_ref(&body)), ("head", heads)] {
        for (index, atom) in atoms.iter().enumerate() {
            let node = format!("{RULE}/{side}/{index}");
            rows.push(row(world, RULE, &vocabulary(side), term(&node)));
            for (local, value) in ["s", "p", "o"].into_iter().zip(atom) {
                rows.push(row(world, &node, &vocabulary(local), value.clone()));
            }
        }
    }
    rows
}
fn usual(world: Option<&str>) -> Vec<RdfQuad> {
    rule(
        world,
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[
            [term("?x"), term(EDGE), term("?new")],
            [term("?new"), term(DONE), term("?value")],
        ],
    )
}
fn dataset(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}
fn prepare(rows: &[RdfQuad]) -> PreparedSources {
    super::super::program::prepare_reasoning_input(&dataset(rows))
        .unwrap()
        .sources
        .prepare()
        .unwrap()
}
fn template(sources: &PreparedSources, rules: &[EvalRule]) -> Arc<JointTemplate> {
    Arc::new(
        JointTemplate::with_sources(
            rules,
            &[],
            &[],
            SemanticVocabulary::GroundedLogicV1,
            &sources.rules,
            Some(sources.contract()),
            &crate::physical::SelectedDomains::new([]).unwrap(),
        )
        .unwrap(),
    )
}
fn execute(
    rows: &[RdfQuad],
    rules: &[EvalRule],
) -> (crate::physical::JointMaterialization, PreparedSources) {
    let input = super::super::program::prepare_reasoning_input(&dataset(rows)).unwrap();
    let sources = input.sources.prepare().unwrap();
    let template = template(&sources, rules);
    let admitted = template
        .input(&input.facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(program) = admitted.prepare().unwrap() else {
        panic!("finite source program")
    };
    let NativeOutcome::Decided(result) =
        materialize(&program, &admitted, &input.occurrences, &input.graphs, None).unwrap()
    else {
        panic!("admitted execution")
    };
    (result, sources)
}
fn materialize(
    program: &crate::physical::JointProgram,
    input: &crate::physical::JointInput<'_>,
    sources: &std::collections::BTreeMap<String, Arc<[crate::reason::refute::RefutationPremise]>>,
    graphs: &std::collections::BTreeMap<String, Option<TermValue>>,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<NativeOutcome<crate::physical::JointMaterialization>> {
    let binding = input.bind_native(
        sources,
        graphs,
        Some(crate::modal::native::NativeModalProgram::prepare(sources).unwrap()),
        Some(Arc::new(
            crate::contextual::native::NativeContextualProgram::prepare(sources).unwrap(),
        )),
    )?;
    program.materialize_input_governed(
        input,
        binding,
        &mut StepGovernor::new(max_steps),
        &mut crate::physical::SkolemRegistry::new(),
    )
}

fn positive(s: &str, p: &str, o: &str) -> EvalAtom {
    let term = |value: &str| {
        if value.starts_with('?') {
            EvalTerm::var(value)
        } else {
            EvalTerm::named(value)
        }
    };
    EvalAtom::positive(term(s), p, term(o))
}

#[test]
fn source_worlds_do_not_become_templates_and_definition_premises_are_real() {
    let mut rows = usual(Some(A));
    rows.push(row(Some(A), X, SEED, term("urn:a")));
    rows.push(row(Some(B), X, SEED, term("urn:b")));
    let (result, sources) = execute(&rows, &[]);
    let outputs: Vec<_> = result
        .result
        .rows
        .iter()
        .filter(|row| row.predicate == EDGE || row.predicate == DONE)
        .collect();
    assert_eq!(outputs.len(), 2);
    assert!(outputs.iter().all(|row| row.graph == A));
    let witness = &result.witness_derivations[0];
    assert_eq!(witness.scope.world, A);
    assert_eq!(witness.heads.len(), 2);
    for output in outputs {
        for premise in &sources.rules[0].premises {
            assert!(output.antecedents.contains(premise));
        }
        assert!(output.antecedents.iter().all(
            |premise| sources.rules[0].premises.contains(premise)
                || premise
                    == &Fact {
                        subject: TermValue::iri(X),
                        predicate: SEED.to_owned(),
                        object: TermValue::iri("urn:a")
                    }
        ));
    }
}

#[test]
fn same_source_iri_in_distinct_worlds_has_distinct_content_bound_witnesses() {
    let mut rows = usual(Some(A));
    rows.extend(usual(Some(B)));
    for world in [A, B] {
        rows.push(row(Some(world), X, SEED, term("urn:value")));
    }
    let (result, sources) = execute(&rows, &[]);
    assert_eq!(sources.rules.len(), 2);
    assert_eq!(result.witness_derivations.len(), 2);
    assert_ne!(
        result.witness_derivations[0].witness,
        result.witness_derivations[1].witness
    );
    let mut reordered = rows.clone();
    reordered.reverse();
    let (again, _) = execute(&reordered, &[]);
    assert_eq!(result.witness_derivations, again.witness_derivations);
}

#[test]
fn actual_literals_and_canonical_data_constants_are_not_namespace_rewritten() {
    let literal = RdfTerm::literal(RdfLiteral::language_tagged("?literal", "fr"));
    let canonical = "https://blackcatinformatics.ca/logic/Thing";
    let mut rows = rule(
        None,
        TYPE,
        [term("?x"), term(SEED), literal.clone()],
        &[
            [term("?x"), term(DONE), literal.clone()],
            [term("?x"), term(EDGE), term(canonical)],
        ],
    );
    rows.push(row(None, X, SEED, literal));
    let (result, sources) = execute(&rows, &[]);
    assert!(
        matches!(&sources.rules[0].rule.body[0].object, EvalTerm::ConstLit(TermValue::Literal { language: Some(language), .. }) if language == "fr")
    );
    assert_eq!(
        sources.rules[0].rule.head[1].object,
        EvalTerm::named(canonical)
    );
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == EDGE && row.object == TermValue::iri(canonical))
    );
    assert!(result.result.rows.iter().any(|row| row.predicate == DONE && matches!(&row.object, TermValue::Literal { lexical_form, language: Some(language), .. } if lexical_form == "?literal" && language == "fr")));
}

#[test]
fn malformed_or_foreign_world_atom_cannot_drop_a_conjunct() {
    for change in ["missing", "foreign", "conflicting"] {
        let mut rows = usual(Some(A));
        let position = rows
            .iter()
            .position(|row| {
                row.subject == term(&format!("{RULE}/body/0")) && row.predicate == vocabulary("o")
            })
            .unwrap();
        match change {
            "missing" => {
                rows.remove(position);
            }
            "foreign" => {
                rows[position].graph_name = Some(term(B));
            }
            _ => {
                let mut other = rows[position].clone();
                other.object = term("urn:conflict");
                rows.push(other);
            }
        }
        let error = super::super::program::prepare_reasoning_input(&dataset(&rows))
            .unwrap()
            .sources
            .prepare()
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains(A) && message.contains("logicx:o"),
            "{message}"
        );
    }
}

#[test]
fn source_preparation_cache_reuses_native_ir_and_binds_field_content_and_graph() {
    let rows = usual(Some(A));
    let first = prepare(&rows);
    let same = prepare(&rows);
    assert!(Arc::ptr_eq(&first.rules, &same.rules));
    let mut changed = rows.clone();
    changed
        .iter_mut()
        .find(|row| {
            row.subject == term(&format!("{RULE}/head/0")) && row.predicate == vocabulary("p")
        })
        .unwrap()
        .object = term("urn:changed");
    let changed = prepare(&changed);
    assert_ne!(first.identity(), changed.identity());
    assert!(!Arc::ptr_eq(&first.rules, &changed.rules));
    assert_ne!(first.identity(), prepare(&usual(Some(B))).identity());
    let mut oversized_rows = rows.clone();
    oversized_rows
        .iter_mut()
        .find(|row| {
            row.subject == term(&format!("{RULE}/body/0")) && row.predicate == vocabulary("o")
        })
        .unwrap()
        .object = RdfTerm::literal(RdfLiteral::simple("x".repeat(1024 * 1024 + 1)));
    let oversized = prepare(&oversized_rows);
    assert!(!oversized.cacheable());
    assert!(!template(&oversized, &[]).cacheable());
    assert!(!Arc::ptr_eq(
        &oversized.rules,
        &prepare(&oversized_rows).rules
    ));
}

#[test]
fn named_compiler_graph_is_not_a_silent_source_blacklist() {
    let world = crate::reasoning_graphs::GRAPH_LOGIC;
    let mut rows = usual(Some(world));
    rows.push(row(Some(world), X, SEED, term("urn:value")));
    let (result, sources) = execute(&rows, &[]);
    assert_eq!(sources.rules[0].world, world);
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == DONE && row.graph == world)
    );
}

#[test]
fn grammar_writers_refuse_but_unrelated_type_inference_is_admitted() {
    let mut rows = usual(Some(A));
    rows.push(row(Some(A), X, SEED, term("urn:value")));
    let input = super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap();
    let sources = input.sources.prepare().unwrap();
    for (predicate, object, admitted) in [
        (TYPE.to_owned(), "urn:OrdinaryClass".to_owned(), true),
        (INSTANCE.to_owned(), vocabulary("ExistentialRule"), false),
        (vocabulary("o"), "urn:changed".to_owned(), false),
    ] {
        let rule = EvalRule::positive(
            "urn:writer",
            positive("?x", &predicate, &object),
            vec![positive("?x", SEED, "?value")],
        );
        let template = template(&sources, &[rule]);
        let result = template.input(&input.facts, std::sync::Arc::from([]), &[]);
        if admitted {
            assert!(result.is_ok());
        } else {
            let message = result
                .err()
                .expect("grammar writer must refuse")
                .to_string();
            assert!(
                message.contains(PROFILE)
                    && message.contains(RULE)
                    && message.contains(A)
                    && message.contains("urn:writer"),
                "{message}"
            );
        }
    }
}

#[test]
fn source_schema_target_aliases_refuse_without_renaming_data() {
    let mut rows = usual(Some(A));
    rows.push(row(
        Some(A),
        "urn:alias",
        "https://blackcatinformatics.ca/logic/subPropertyOf",
        term(&vocabulary("head")),
    ));
    rows.push(row(Some(A), RULE, "urn:alias", term("urn:new-head")));
    let source = dataset(&rows);
    let input = super::super::program::prepare_reasoning_input(&source).unwrap();
    assert!(
        input.facts[A]
            .iter()
            .any(|fact| fact.predicate == "urn:alias")
    );
    let empty = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None);
    let prepared = crate::program_analysis::prepare_program(&empty).unwrap();
    let error = super::super::program::execute(
        &prepared,
        input,
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("reachable native schema writer mutates admitted source grammar");
    let detail = error.to_string();
    assert!(
        detail.contains(A) && detail.contains("immutable"),
        "{detail}"
    );
}

#[test]
fn positive_native_consumer_observes_source_multihead_output_in_same_fixed_point() {
    let mut rows = usual(None);
    rows.push(row(None, X, SEED, term("urn:value")));
    let consumer = EvalRule::positive(
        "urn:consumer",
        positive("?x", "urn:seen", "?value"),
        vec![
            positive("?x", EDGE, "?new"),
            positive("?new", DONE, "?value"),
        ],
    );
    let (result, _) = execute(&rows, &[consumer]);
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:seen" && row.object == TermValue::iri("urn:value"))
    );
    assert_eq!(result.result.status, crate::seam::BudgetStatus::Ok);
}

#[test]
fn dl_source_execution_keeps_literal_results_and_one_governor_budget() {
    let literal = RdfTerm::literal(RdfLiteral::language_tagged("value", "fr"));
    let mut rows = rule(
        None,
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[[term("?x"), term(DONE), term("?value")]],
    );
    rows.push(row(None, X, SEED, literal));
    let source = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None);
    let prepared = crate::program_analysis::prepare_program(&source).unwrap();
    let result = super::super::program::execute(
        &prepared,
        super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(1),
    )
    .unwrap();
    assert_eq!(result.consumed_steps, 1);
    assert!(result.inferred.iter().any(|row| row.predicate == DONE
        && matches!(&row.object,
        TermValue::Literal { language: Some(language), .. } if language == "fr")));
    assert_eq!(result.certificates.len(), result.graphs.len());
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.input_contract == result.input_contract)
    );
}

#[test]
fn source_governor_backstop_restores_caller_limit_on_error_and_refusal() {
    let mut governor = StepGovernor::new(Some(5));
    let error: Result<(), &str> = governor.with_backstop(1, |governor| {
        governor.charge();
        assert!(governor.spent());
        Err("synthetic refusal")
    });
    assert!(error.is_err());
    assert_eq!(governor.consumed, 1);
    assert_eq!(governor.remaining(), Some(4));
    let refusal = governor.with_backstop(0, |governor| {
        assert!(governor.spent());
        NativeOutcome::<()>::Unsupported(
            crate::physical::UnsupportedKind::NonTerminatingExistential,
        )
    });
    assert!(matches!(refusal, NativeOutcome::Unsupported(_)));
    assert_eq!(governor.remaining(), Some(4));
}

#[test]
fn annotation_owned_definitions_execute_once_with_original_source_evidence() {
    let rows = usual(Some(A));
    let mut builder = RdfDatasetBuilder::new();
    let world = builder.intern_iri(A);
    let quoted_subject = builder.intern_iri("urn:quoted:s");
    let quoted_predicate = builder.intern_iri("urn:quoted:p");
    let quoted_object = builder.intern_iri("urn:quoted:o");
    let quoted = builder.intern_triple(quoted_subject, quoted_predicate, quoted_object);
    let mut reifiers = BTreeSet::new();
    for row in &rows {
        let subject = builder.intern_owned_term(&row.subject);
        if reifiers.insert(subject) {
            builder.push_reifier_in_graph(subject, quoted, Some(world));
        }
        let predicate = builder.intern_iri(&row.predicate);
        let object = builder.intern_owned_term(&row.object);
        builder.push_annotation_in_graph(subject, predicate, object, Some(world));
    }
    // The same defining statement in two physical tables is still one source slot.
    builder.push_owned_quad(&rows[2]);
    builder.push_owned_quad(&row(Some(A), X, SEED, term("urn:value")));
    let input = super::super::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap();
    let sources = input.sources.prepare().unwrap();
    assert_eq!(sources.rules.len(), 1);
    assert_eq!(sources.rules[0].evidence.len(), rows.len());
    let template = template(&sources, &[]);
    let admitted = template
        .input(&input.facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(program) = admitted.prepare().unwrap() else {
        panic!("finite source")
    };
    let NativeOutcome::Decided(result) =
        materialize(&program, &admitted, &input.occurrences, &input.graphs, None).unwrap()
    else {
        panic!("native source")
    };
    assert_eq!(result.witness_derivations.len(), 1);
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:quoted:p")
    );
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == DONE && row.graph == A)
    );
}

#[test]
fn budget_prefix_only_publishes_committed_source_heads() {
    let mut rows = usual(Some(A));
    rows.push(row(Some(A), X, SEED, term("urn:value")));
    let native = super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap();
    let sources = native.sources.prepare().unwrap();
    let template = template(&sources, &[]);
    let input = template
        .input(&native.facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(program) = input.prepare().unwrap() else {
        panic!("finite source")
    };
    for budget in [0, 1] {
        let NativeOutcome::Decided(result) = materialize(
            &program,
            &input,
            &native.occurrences,
            &native.graphs,
            Some(budget),
        )
        .unwrap() else {
            panic!("budgeted source")
        };
        let committed: Vec<_> = result
            .result
            .rows
            .iter()
            .filter(|row| row.rule_iri != crate::provenance::ASSERT_RULE_IRI)
            .collect();
        assert_eq!(committed.len(), budget as usize);
        assert_eq!(result.result.status, crate::seam::BudgetStatus::Exhausted);
        if budget == 0 {
            assert!(result.witness_derivations.is_empty());
        }
        for witness in &result.witness_derivations {
            assert_eq!(witness.heads.len(), committed.len());
            witness.validate().unwrap();
            for head in &witness.heads {
                assert!(
                    committed
                        .iter()
                        .any(|row| row.subject == head.statement.subject
                            && row.predicate == head.statement.predicate
                            && row.object == head.statement.object)
                );
            }
        }
    }
}

#[test]
fn changed_source_content_rekeys_witness_without_reinterpreting_the_source_iri() {
    let mut rows = usual(Some(A));
    rows.push(row(Some(A), X, SEED, term("urn:value")));
    let (first, _) = execute(&rows, &[]);
    rows.iter_mut()
        .find(|row| {
            row.subject == term(&format!("{RULE}/head/1")) && row.predicate == vocabulary("p")
        })
        .unwrap()
        .object = term("urn:changed");
    let (changed, _) = execute(&rows, &[]);
    assert_eq!(
        first.witness_derivations[0].rule_iri,
        changed.witness_derivations[0].rule_iri
    );
    assert_ne!(
        first.witness_derivations[0].scope.source_rule,
        changed.witness_derivations[0].scope.source_rule
    );
    assert_ne!(
        first.witness_derivations[0].witness,
        changed.witness_derivations[0].witness
    );
}

#[test]
fn nominal_joint_clash_precedes_its_absence_consumer() {
    use gmeow_logic_compile::ir::{
        AtomicTerm, ContextualScope, LogicAxiom, LogicProgram, LogicRule,
    };
    let class = "urn:nominal:one-a";
    let class2 = "urn:nominal:one-a2";
    let nothing = "http://www.w3.org/2002/07/owl#Nothing";
    let mut rows = usual(None); // A valid inert source definition selects source feedback.
    for (s, p, o) in [
        (X, TYPE, class),
        (class, "http://www.w3.org/2002/07/owl#oneOf", "urn:list:a"),
        (
            "urn:list:a",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "urn:nominal:a",
        ),
        (
            "urn:list:a",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ),
        ("urn:nominal:y", TYPE, class2),
        (class2, "http://www.w3.org/2002/07/owl#oneOf", "urn:list:b"),
        (
            "urn:list:b",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "urn:nominal:a",
        ),
        (
            "urn:list:b",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ),
        (
            X,
            "http://www.w3.org/2002/07/owl#differentFrom",
            "urn:nominal:y",
        ),
    ] {
        rows.push(row(None, s, p, term(o)));
    }
    let axiom = |predicate: &str, object: &str, negated: bool| {
        LogicAxiom::new(
            "?x",
            predicate,
            AtomicTerm::Iri(object.to_owned()),
            negated,
            ContextualScope::default(),
        )
        .unwrap()
    };
    // The body sign selects stratified absence. Formula::Not is strong logical
    // negation and cannot stand in for a completion-dependent NAF consumer.
    let program = LogicProgram::new(
        vec![],
        vec![LogicRule::new(
            axiom("urn:absence-result", "urn:marker", false),
            vec![axiom(TYPE, class, false), axiom(TYPE, nothing, true)],
            vec![],
            ContextualScope::default(),
        )],
        vec![],
        None,
    );
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    assert!(prepared.preservation.unsupported_constructs.is_empty());
    assert!(prepared.existential_rules.is_empty());
    assert_eq!(prepared.rules.len(), 1);
    let reader = &prepared.rules[0];
    assert_eq!(reader.head.predicate, "urn:absence-result");
    assert_eq!(reader.body.len(), 2);
    assert_eq!(reader.body.iter().filter(|atom| atom.negated).count(), 1);
    assert!(reader.body.iter().any(|atom| {
        atom.negated
            && atom.subject == EvalTerm::var("?x")
            && atom.predicate == TYPE
            && atom.object == EvalTerm::named(nothing)
    }));
    assert!(reader.body.iter().any(|atom| {
        !atom.negated
            && atom.subject == EvalTerm::var("?x")
            && atom.predicate == TYPE
            && atom.object == EvalTerm::named(class)
    }));
    let result = super::super::program::execute(
        &prepared,
        super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(result.inferred.iter().any(|row| !row.is_edb
        && row.object == TermValue::iri("https://blackcatinformatics.ca/logic/Nothing")));
    assert!(
        !result
            .inferred
            .iter()
            .any(|row| !row.is_edb && row.predicate == "urn:absence-result")
    );
    assert!(
        result
            .classes
            .iter()
            .any(|world| !world.contextual_conflicts.is_empty())
    );
}

#[test]
fn initially_unreachable_consumer_waits_for_joint_source_heads() {
    let mut absent = positive("?x", "urn:late-target", "urn:value");
    absent.negated = true;
    let reader = EvalRule::positive(
        "urn:late-negative-reader",
        positive("?x", "urn:absence", "urn:value"),
        vec![positive("?x", "urn:late-positive", "urn:value"), absent],
    );
    let mut rows = rule(
        None,
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[
            [term("?x"), term("urn:late-positive"), term("urn:value")],
            [term("?x"), term("urn:late-target"), term("urn:value")],
        ],
    );
    rows.push(row(None, X, SEED, term("urn:value")));
    let (result, _) = execute(&rows, &[reader]);
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:late-positive")
    );
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:late-target")
    );
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:absence")
    );
}

#[test]
fn source_and_ordinary_feedback_share_the_termination_admission() {
    let mut rows = rule(
        None,
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[[term("?new"), term(EDGE), term("?x")]],
    );
    rows.push(row(None, X, SEED, term("urn:value")));
    let ordinary = EvalRule::positive(
        "urn:feedback",
        positive("?x", SEED, "?value"),
        vec![positive("?x", EDGE, "?value")],
    );
    let native = super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap();
    let sources = native.sources.prepare().unwrap();
    let template = template(&sources, &[ordinary]);
    let input = template
        .input(&native.facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
        panic!("positive dependency schedule")
    };
    assert!(!plan.admission.admits_native());
    assert!(matches!(
        materialize(&plan, &input, &native.occurrences, &native.graphs, None).unwrap(),
        NativeOutcome::Unsupported(crate::physical::UnsupportedKind::NonTerminatingExistential)
    ));
    let NativeOutcome::Decided(prefix) =
        materialize(&plan, &input, &native.occurrences, &native.graphs, Some(3)).unwrap()
    else {
        panic!("explicit bounded prefix")
    };
    assert_eq!(prefix.result.status, crate::seam::BudgetStatus::Exhausted);
    assert_eq!(prefix.result.consumed_steps, 3);
}

#[test]
fn duplicate_authored_rule_names_do_not_share_termination_symbols() {
    let mut rows = rule(
        Some(A),
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[[term("?new"), term(EDGE), term("?x")]],
    );
    rows.extend(rule(
        Some(B),
        INSTANCE,
        [term("?x"), term(EDGE), term("?value")],
        &[[term("?new"), term(SEED), term("?x")]],
    ));
    rows.push(row(Some(A), X, SEED, term("urn:value")));
    let native = super::super::program::prepare_reasoning_input(&dataset(&rows)).unwrap();
    let sources = native.sources.prepare().unwrap();
    assert_eq!(
        sources.rules[0].rule.rule_iri,
        sources.rules[1].rule.rule_iri
    );
    let named: Vec<_> = sources
        .rules
        .iter()
        .map(|source| source.rule.clone())
        .collect();
    let mut renamed = named.clone();
    for (index, rule) in renamed.iter_mut().enumerate() {
        rule.rule_iri = format!("urn:analysis-control:{index}");
    }
    let duplicate = crate::physical::ChaseAdmission::certify(&named);
    let distinct = crate::physical::ChaseAdmission::certify(&renamed);
    assert_eq!(duplicate.admits_native(), distinct.admits_native());
    assert!(
        !duplicate.admits_native(),
        "the combined over-approximation has an invention cycle"
    );
    let template = template(&sources, &[]);
    let input = template
        .input(&native.facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
        panic!("positive schedule")
    };
    assert!(matches!(
        materialize(&plan, &input, &native.occurrences, &native.graphs, None).unwrap(),
        NativeOutcome::Unsupported(crate::physical::UnsupportedKind::NonTerminatingExistential)
    ));
    let NativeOutcome::Decided(bounded) =
        materialize(&plan, &input, &native.occurrences, &native.graphs, Some(8)).unwrap()
    else {
        panic!("selected finite budget")
    };
    assert_eq!(bounded.result.status, crate::seam::BudgetStatus::Ok);
    assert!(
        bounded
            .result
            .rows
            .iter()
            .filter(|row| row.rule_iri == RULE)
            .all(|row| row.graph == A)
    );
}

#[test]
fn each_demonstrator_parses_and_certifies_to_its_class() {
    // Freeze each demonstrator turtle against its intended termination class — a typo
    // or a drifted witness would hard-fail `stage-reason` at `make check`, so catch it
    // here on the fast path.
    for (i, (graph, ttl)) in termination_ladder_demonstrators().iter().enumerate() {
        let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("{graph}: demonstrator turtle must parse: {e}"));
        let sources = super::super::program::prepare_reasoning_input(dataset.as_ref())
            .unwrap_or_else(|e| panic!("{graph}: demonstrator input must assemble: {e}"))
            .sources
            .prepare()
            .unwrap_or_else(|e| panic!("{graph}: demonstrator rules must assemble: {e}"));
        let rules: Vec<_> = sources
            .rules
            .iter()
            .map(|source| source.rule.clone())
            .collect();
        assert!(!rules.is_empty(), "{graph}: demonstrator rules must parse");
        let cert = ChaseAdmission::certify(&rules);
        let ok = match i {
            0 => matches!(cert, ChaseAdmission::JointlyAcyclic { .. }),
            1 => matches!(cert, ChaseAdmission::SuperWeaklyAcyclic { .. }),
            2 => matches!(cert, ChaseAdmission::ModelSummarizingAcyclic { .. }),
            _ => unreachable!(),
        };
        assert!(
            ok,
            "{graph}: demonstrator must certify to its class, got {cert:?}"
        );
    }
}

#[test]
fn source_owned_effects_do_not_create_a_foreign_negative_cycle() {
    let mut rows = rule(
        Some(A),
        INSTANCE,
        [term("?x"), term(SEED), term("?value")],
        &[[term("?x"), term(EDGE), term("?value")]],
    );
    rows.extend(rule(
        Some(B),
        INSTANCE,
        [term("?x"), term(DONE), term("?value")],
        &[[term("?x"), term(SEED), term("?value")]],
    ));
    rows.push(row(Some(A), X, SEED, term("urn:value")));
    let mut absent = positive("?x", SEED, "?value");
    absent.negated = true;
    let consumer = EvalRule::positive(
        "urn:source:absence-consumer",
        positive("?x", DONE, "?value"),
        vec![positive("?x", EDGE, "?value"), absent],
    );
    let (result, _) = execute(&rows, &[consumer]);
    assert_eq!(
        result.terminal,
        crate::reason::refute::native::NativeClosureStatus::Completed
    );
    let edges: Vec<_> = result
        .result
        .rows
        .iter()
        .filter(|row| row.predicate == EDGE)
        .map(|row| row.graph.as_str())
        .collect();
    assert_eq!(edges, [A]);
    assert!(!result.result.rows.iter().any(|row| row.predicate == DONE));
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|row| row.graph == B && row.predicate == SEED)
    );
}
