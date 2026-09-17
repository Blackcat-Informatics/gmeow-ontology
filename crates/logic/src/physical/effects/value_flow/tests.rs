// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Abstract coverage checked against independent concrete native execution.

use super::*;
use crate::rule_ir::{EvalAtom, EvalRule, FactStore, least_model_of_reduct};

fn term(value: &str) -> EvalTerm {
    if value.starts_with('?') {
        EvalTerm::var(value)
    } else {
        EvalTerm::named(value)
    }
}
fn atom(s: &str, p: &str, o: &str) -> EvalAtom {
    EvalAtom::positive(term(s), p, term(o))
}
fn flow(rule: &EvalRule) -> FlowRule {
    let statement = |a: &EvalAtom| {
        [
            a.subject.clone(),
            EvalTerm::named(&a.predicate),
            a.object.clone(),
        ]
    };
    FlowRule {
        body: rule
            .body
            .iter()
            .filter(|a| !a.negated)
            .map(statement)
            .collect(),
        heads: vec![statement(&rule.head)],
        native_witnesses: Vec::new(),
        reads: rule.body.iter().map(|a| Some(statement(a))).collect(),
    }
}

#[test]
fn column_fixed_point_covers_every_concrete_positive_derivation() {
    let rules = [
        EvalRule::positive(
            "urn:copy",
            atom("?s", "urn:reach", "?o"),
            vec![atom("?s", "urn:edge", "?o")],
        ),
        EvalRule::positive(
            "urn:transitive",
            atom("?s", "urn:reach", "?o"),
            vec![atom("?s", "urn:reach", "?m"), atom("?m", "urn:reach", "?o")],
        ),
        EvalRule::positive(
            "urn:classify",
            atom("?s", "urn:type", "urn:Class"),
            vec![atom("?s", "urn:reach", "urn:watched")],
        ),
    ];
    let effects: Vec<_> = rules.iter().map(ProducerEffect::rule).collect();
    let analysis = ValueFlow::new(
        &rules.iter().map(flow).collect::<Vec<_>>(),
        &effects,
        SemanticVocabulary::Exact,
    );
    let candidates: Vec<_> = [
        ("urn:a", "urn:b"),
        ("urn:b", "urn:watched"),
        ("urn:watched", "urn:a"),
        ("urn:a", "urn:watched"),
    ]
    .into_iter()
    .map(|(s, o)| Fact {
        subject: TermValue::iri(s),
        predicate: "urn:edge".to_owned(),
        object: TermValue::iri(o),
    })
    .collect();
    for subset in 0u32..16 {
        let mut source = FactStore::new();
        for (index, fact) in candidates.iter().enumerate() {
            if subset & (1 << index) != 0 {
                source.insert(fact.clone());
            }
        }
        let input = analysis.summarize(source.facts().iter());
        let refined = analysis.refine(&effects, &input);
        let concrete = least_model_of_reduct(&source, &rules, &FactStore::new()).unwrap();
        for row in concrete.derivations {
            let rule = rules
                .iter()
                .position(|rule| rule.rule_iri == row.rule_iri)
                .unwrap();
            let range = refined[rule].writes[0]
                .ranges
                .as_ref()
                .expect("reachable producer");
            assert!(range[0].contains(analysis.universe.value(&row.subject)));
            assert!(range[1].contains(analysis.universe.iri(&row.predicate)));
            assert!(range[2].contains(analysis.universe.value(&row.object)));
        }
    }
}

#[test]
fn complete_input_summary_is_order_independent_and_keeps_native_constant_facets() {
    let blank = |scope| TermValue::Blank {
        label: "x".to_owned(),
        scope: purrdf::BlankScope(scope),
    };
    let quoted = |value| TermValue::Triple {
        s: Box::new(TermValue::iri("urn:claim")),
        p: Box::new(TermValue::iri("urn:states")),
        o: Box::new(value),
    };
    let values = [
        blank(1),
        blank(2),
        quoted(blank(1)),
        quoted(blank(2)),
        TermValue::simple_literal("<urn:value>"),
        TermValue::iri("urn:value"),
    ];
    let rules: Vec<_> = values
        .iter()
        .map(|value| {
            EvalRule::positive(
                "urn:observe",
                atom("?s", "urn:seen", "urn:yes"),
                vec![EvalAtom::positive(
                    EvalTerm::var("?s"),
                    "urn:source",
                    EvalTerm::ConstLit(value.clone()),
                )],
            )
        })
        .collect();
    let effects: Vec<_> = rules.iter().map(ProducerEffect::rule).collect();
    let analysis = ValueFlow::new(
        &rules.iter().map(flow).collect::<Vec<_>>(),
        &effects,
        SemanticVocabulary::Exact,
    );
    let facts: Vec<_> = values
        .iter()
        .map(|value| Fact {
            subject: TermValue::iri("urn:x"),
            predicate: "urn:source".to_owned(),
            object: value.clone(),
        })
        .collect();
    let identity = |facts: &[Fact]| analysis.summarize(facts.iter()).identity(&[0; 32]);
    let expected = identity(&facts);
    let mut reversed = facts.clone();
    reversed.reverse();
    assert_eq!(expected, identity(&reversed));
    for left in 0..facts.len() {
        for right in 0..facts.len() {
            assert_eq!(
                identity(&facts[left..=left]) == identity(&facts[right..=right]),
                left == right
            );
        }
    }
}

const GENERATED_WORLD: &str = "urn:generated-domain:world";
const GENERATED_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const GENERATED_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const GENERATED_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const PROTECTED_BODY: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#body";
const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

fn subproperty_flow() -> (FlowRule, ProducerEffect) {
    let law = crate::reason::schema_laws()
        .iter()
        .find(|law| law.source.rule_iri == "dl:subPropertyOf-propagation")
        .expect("fixed schema law");
    let flow = FlowRule {
        body: law
            .analysis_body
            .iter()
            .map(|atom| atom.0.clone())
            .collect(),
        heads: law
            .analysis_heads
            .iter()
            .map(|atom| atom.0.clone())
            .collect(),
        native_witnesses: Vec::new(),
        reads: law
            .source
            .body
            .iter()
            .map(|atom| Some(atom.0.clone()))
            .collect(),
    };
    let effect = ProducerEffect::new(
        law.source.rule_iri.clone(),
        law.analysis_heads
            .iter()
            .map(|atom| StatementPattern::statement(&atom.0))
            .collect(),
        law.source
            .body
            .iter()
            .map(|atom| {
                (
                    StatementPattern::statement(&atom.0),
                    crate::physical::dependency::ReadDependency::Positive,
                )
            })
            .collect(),
    );
    (flow, effect)
}

fn schema_flow() -> (Vec<FlowRule>, Vec<ProducerEffect>) {
    crate::reason::schema_laws()
        .iter()
        .map(|law| {
            (
                FlowRule {
                    body: law
                        .analysis_body
                        .iter()
                        .map(|atom| atom.0.clone())
                        .collect(),
                    heads: law
                        .analysis_heads
                        .iter()
                        .map(|atom| atom.0.clone())
                        .collect(),
                    native_witnesses: Vec::new(),
                    reads: law
                        .source
                        .body
                        .iter()
                        .map(|atom| Some(atom.0.clone()))
                        .collect(),
                },
                ProducerEffect::new(
                    law.source.rule_iri.clone(),
                    law.analysis_heads
                        .iter()
                        .map(|atom| StatementPattern::statement(&atom.0))
                        .collect(),
                    law.source
                        .body
                        .iter()
                        .map(|atom| {
                            (
                                StatementPattern::statement(&atom.0),
                                crate::physical::dependency::ReadDependency::Positive,
                            )
                        })
                        .collect(),
                ),
            )
        })
        .unzip()
}

fn generated_source() -> BTreeMap<String, Vec<Fact>> {
    BTreeMap::from([(
        GENERATED_WORLD.to_owned(),
        vec![Fact {
            subject: TermValue::iri("urn:source"),
            predicate: "urn:seed".to_owned(),
            object: TermValue::iri("urn:unit"),
        }],
    )])
}

#[test]
fn unrelated_subproperty_target_cannot_mutate_protected_source_grammar() {
    let (rule, effect) = subproperty_flow();
    let analysis = ValueFlow::with_observations(
        &[rule],
        std::slice::from_ref(&effect),
        SemanticVocabulary::GroundedLogicV1,
        &[StatementPattern::relation(Some(PROTECTED_BODY), None)],
    );
    let facts = [
        Fact {
            subject: TermValue::iri("urn:ordinary-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri("urn:ordinary-superproperty"),
        },
        Fact {
            subject: TermValue::iri("urn:subject"),
            predicate: "urn:ordinary-property".to_owned(),
            object: TermValue::iri("urn:object"),
        },
        Fact {
            subject: TermValue::iri("urn:rule"),
            predicate: PROTECTED_BODY.to_owned(),
            object: TermValue::iri("urn:atom"),
        },
    ];
    let refined = analysis.refine(
        std::slice::from_ref(&effect),
        &analysis.summarize(facts.iter()),
    );
    assert_eq!(
        analysis.overlapping_writer(
            &StatementPattern::relation(Some(PROTECTED_BODY), None),
            &refined,
        ),
        None
    );
}

#[test]
fn unreachable_subproperty_target_cannot_mutate_protected_source_grammar() {
    let (rule, effect) = subproperty_flow();
    let facts = [
        Fact {
            subject: TermValue::iri("urn:inactive-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri(PROTECTED_BODY),
        },
        Fact {
            subject: TermValue::iri("urn:active-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri("urn:ordinary-superproperty"),
        },
        Fact {
            subject: TermValue::iri("urn:subject"),
            predicate: "urn:active-property".to_owned(),
            object: TermValue::iri("urn:object"),
        },
    ];
    let analysis = ValueFlow::with_observations(
        &[rule],
        std::slice::from_ref(&effect),
        SemanticVocabulary::GroundedLogicV1,
        &[StatementPattern::relation(Some(PROTECTED_BODY), None)],
    )
    .with_source_operators(facts.iter());
    assert_ne!(
        analysis.universe.iri("urn:inactive-property"),
        analysis.universe.other(),
        "a schema selector that can become a predicate needs an exact cell"
    );
    assert_eq!(
        analysis.universe.iri("urn:subject"),
        analysis.universe.other(),
        "ordinary resources must not widen every value-flow bitset"
    );
    let refined = analysis.refine(
        std::slice::from_ref(&effect),
        &analysis.summarize(facts.iter()),
    );
    assert_eq!(
        analysis.overlapping_writer(
            &StatementPattern::relation(Some(PROTECTED_BODY), None),
            &refined,
        ),
        None
    );
}

#[test]
fn reachable_subproperty_target_still_refuses_protected_source_grammar_mutation() {
    let (rule, effect) = subproperty_flow();
    let facts = [
        Fact {
            subject: TermValue::iri("urn:ordinary-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri(PROTECTED_BODY),
        },
        Fact {
            subject: TermValue::iri("urn:subject"),
            predicate: "urn:ordinary-property".to_owned(),
            object: TermValue::iri("urn:object"),
        },
    ];
    let analysis = ValueFlow::with_observations(
        &[rule],
        std::slice::from_ref(&effect),
        SemanticVocabulary::GroundedLogicV1,
        &[StatementPattern::relation(Some(PROTECTED_BODY), None)],
    )
    .with_source_operators(facts.iter());
    let refined = analysis.refine(
        std::slice::from_ref(&effect),
        &analysis.summarize(facts.iter()),
    );
    assert_eq!(
        analysis.overlapping_writer(
            &StatementPattern::relation(Some(PROTECTED_BODY), None),
            &refined,
        ),
        Some("dl:subPropertyOf-propagation")
    );
}

#[test]
fn unrelated_transitive_marker_cannot_mutate_protected_source_grammar() {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
    let (rules, effects) = schema_flow();
    let facts = [
        Fact {
            subject: TermValue::iri("urn:ordinary-property"),
            predicate: RDF_TYPE.to_owned(),
            object: TermValue::iri(TRANSITIVE),
        },
        Fact {
            subject: TermValue::iri(PROTECTED_BODY),
            predicate: RDF_TYPE.to_owned(),
            object: TermValue::iri("urn:grammar-property"),
        },
        Fact {
            subject: TermValue::iri("urn:left"),
            predicate: "urn:ordinary-property".to_owned(),
            object: TermValue::iri("urn:middle"),
        },
        Fact {
            subject: TermValue::iri("urn:middle"),
            predicate: "urn:ordinary-property".to_owned(),
            object: TermValue::iri("urn:right"),
        },
        Fact {
            subject: TermValue::iri("urn:rule"),
            predicate: PROTECTED_BODY.to_owned(),
            object: TermValue::iri("urn:atom"),
        },
        Fact {
            subject: TermValue::iri("urn:atom"),
            predicate: PROTECTED_BODY.to_owned(),
            object: TermValue::iri("urn:tail"),
        },
    ];
    let analysis = ValueFlow::with_observations(
        &rules,
        &effects,
        SemanticVocabulary::GroundedLogicV1,
        &[StatementPattern::relation(Some(PROTECTED_BODY), None)],
    )
    .with_source_operators(facts.iter());
    let refined = analysis.refine(&effects, &analysis.summarize(facts.iter()));
    assert_eq!(
        analysis.overlapping_writer(
            &StatementPattern::relation(Some(PROTECTED_BODY), None),
            &refined,
        ),
        None
    );
}

#[test]
fn contextual_result_envelopes_do_not_invent_protected_schema_predicates() {
    let (rules, effects) = schema_flow();
    let outputs = crate::result_rdf::contextual_projection_effects();
    let facts = [
        Fact {
            subject: TermValue::iri("urn:inactive-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri(PROTECTED_BODY),
        },
        Fact {
            subject: TermValue::iri("urn:active-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri("urn:ordinary-superproperty"),
        },
        Fact {
            subject: TermValue::iri("urn:subject"),
            predicate: "urn:active-property".to_owned(),
            object: TermValue::iri("urn:object"),
        },
    ];
    let analysis = ValueFlow::with_observations(
        &rules,
        &effects,
        SemanticVocabulary::GroundedLogicV1,
        &[StatementPattern::relation(Some(PROTECTED_BODY), None)],
    )
    .with_additional_observations(outputs.iter())
    .with_source_operators(facts.iter());
    let mut summary = analysis.summarize(facts.iter());
    analysis.seed_patterns(&mut summary, outputs.iter());
    let refined = analysis.refine(&effects, &summary);
    assert_eq!(
        analysis.overlapping_writer(
            &StatementPattern::relation(Some(PROTECTED_BODY), None),
            &refined,
        ),
        None
    );
}

fn generated_list_rule() -> crate::physical::ExistentialRule {
    crate::physical::ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:generated-domain:producer".to_owned(),
        body: vec![atom("?x", "urn:seed", "?y")],
        head: vec![
            atom("?fresh", GENERATED_FIRST, "?fresh"),
            atom("?fresh", GENERATED_REST, GENERATED_NIL),
        ],
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: crate::physical::WitnessPolicy::FrontierSkolem,
    }
}

fn generated_template(
    rules: &[EvalRule],
    producers: &[crate::physical::ExistentialRule],
) -> std::sync::Arc<crate::physical::JointTemplate> {
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    std::sync::Arc::new(
        crate::physical::JointTemplate::with_sources(
            rules,
            producers,
            &[],
            SemanticVocabulary::GroundedLogicV1,
            &sources.rules,
            Some(sources.contract()),
            &crate::physical::SelectedDomains::new([]).unwrap(),
        )
        .unwrap(),
    )
}

fn actual_generated_values() -> [TermValue; 2] {
    let rule = generated_list_rule();
    let contract =
        crate::physical::store::WitnessContract::native(SemanticVocabulary::GroundedLogicV1);
    let scope = contract.scope(
        GENERATED_WORLD,
        crate::physical::metadata_identity("gmeow-existential-source-v1", &rule),
    );
    let skolem = crate::physical::SkolemRegistry::new().mint(crate::physical::store::SkolemTerm {
        scope,
        rule_iri: rule.rule_iri,
        ordinal: 0,
        frontier: Vec::new(),
    });
    let tuple = TermValue::iri(
        crate::provenance::mint_nary_reifier(
            "urn:tuple",
            &[TermValue::iri("urn:source"), TermValue::iri("urn:unit")],
        )
        .unwrap(),
    );
    [skolem, tuple]
}

#[test]
fn native_generated_domain_covers_real_constructors_and_known_namespace_values() {
    let values = actual_generated_values();
    let supplied = [
        TermValue::iri(format!("{}authored", crate::facts::SKOLEM_PREFIX)),
        TermValue::iri(format!(
            "{}authored",
            crate::provenance::NARY_REIFIER_PREFIX
        )),
    ];
    let mut universe = Universe::new(SemanticVocabulary::GroundedLogicV1);
    for value in values.iter().chain(&supplied) {
        universe.register(value.clone());
    }
    universe.register(TermValue::iri(PROTECTED_BODY));
    universe.register(TermValue::simple_literal(crate::facts::SKOLEM_PREFIX));
    let domain = universe.native_witness_domain();
    assert!(
        domain.contains(universe.other()),
        "future generated values remain unbounded"
    );
    for value in values.iter().chain(&supplied) {
        assert!(
            domain.contains(universe.value(value)),
            "known witness-shaped source values cannot be excluded"
        );
    }
    assert!(!domain.contains(universe.iri(PROTECTED_BODY)));
    assert!(
        !domain.contains(universe.value(&TermValue::simple_literal(crate::facts::SKOLEM_PREFIX)))
    );
}

#[test]
fn source_immutable_profile_admits_native_fresh_list_heads_with_actual_witness_evidence() {
    use crate::physical::NativeOutcome;
    let template = generated_template(&[], &[generated_list_rule()]);
    let facts = generated_source();
    let input = template
        .input(&facts, std::sync::Arc::from([]), &[])
        .expect("fresh list addresses cannot be source grammar symbols");
    let NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
        panic!("finite native producer")
    };
    assert!(plan.admission.admits_native());
    let graph = TermValue::iri(GENERATED_WORLD);
    let graphs = BTreeMap::from([(GENERATED_WORLD.to_owned(), Some(graph.clone()))]);
    let sources = BTreeMap::from([(
        GENERATED_WORLD.to_owned(),
        facts[GENERATED_WORLD]
            .iter()
            .map(|fact| crate::reason::refute::RefutationPremise {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
                graph: Some(graph.clone()),
            })
            .collect::<Vec<_>>()
            .into(),
    )]);
    let binding = input
        .bind_native(
            &sources,
            &graphs,
            Some(crate::modal::native::NativeModalProgram::prepare(&sources).unwrap()),
            Some(std::sync::Arc::new(
                crate::contextual::native::NativeContextualProgram::prepare(&sources).unwrap(),
            )),
        )
        .unwrap();
    let NativeOutcome::Decided(result) = plan
        .materialize_input_governed(
            &input,
            binding,
            &mut crate::physical::StepGovernor::new(None),
            &mut crate::physical::SkolemRegistry::new(),
        )
        .unwrap()
    else {
        panic!("complete native witness")
    };
    assert_eq!(result.witness_derivations.len(), 1);
    let witness = &result.witness_derivations[0];
    assert_eq!(witness.scope.world, GENERATED_WORLD);
    assert_eq!(witness.heads.len(), 2);
    assert_eq!(
        witness
            .heads
            .iter()
            .map(|head| head.statement.predicate.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([GENERATED_FIRST, GENERATED_REST])
    );
    witness.validate().unwrap();
    assert!(
        witness
            .heads
            .iter()
            .all(|head| head.statement.subject == TermValue::iri(&witness.witness))
    );
    assert!(result.result.rows.iter().filter(|row| row.predicate == GENERATED_FIRST)
        .all(|row| row.subject == TermValue::iri(&witness.witness) && row.object == row.subject));
    assert!(!result.result.rows.iter().any(
        |row| row.predicate == PROTECTED_BODY || row.subject == TermValue::iri(PROTECTED_BODY)
    ));
}

#[test]
fn explicit_source_definition_writes_remain_refused_after_native_range_refinement() {
    for rule in [
        EvalRule::positive(
            "urn:direct-grammar-writer",
            atom("?x", PROTECTED_BODY, "?y"),
            vec![atom("?x", "urn:seed", "?y")],
        ),
        EvalRule::positive(
            "urn:direct-declaration-writer",
            atom(
                "?x",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "https://blackcatinformatics.ca/gmeow/logic/existential#ExistentialRule",
            ),
            vec![atom("?x", "urn:seed", "?y")],
        ),
    ] {
        let name = rule.rule_iri.clone();
        let template = generated_template(&[rule], &[generated_list_rule()]);
        let facts = generated_source();
        let error = template
            .input(&facts, std::sync::Arc::from([]), &[])
            .err()
            .expect("actual definition mutation must refuse before execution");
        assert!(
            error.message().contains(&name) && error.message().contains("immutable definitions"),
            "{error}"
        );
    }
}

#[test]
fn known_generated_values_can_flow_through_equality_to_a_protected_target() {
    let same_as = "http://www.w3.org/2002/07/owl#sameAs";
    let rule_class = "https://blackcatinformatics.ca/gmeow/logic/existential#ExistentialRule";
    let rule = EvalRule::positive(
        "urn:subject-congruence",
        atom(
            "?value",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "?target",
        ),
        vec![
            atom("?source", GENERATED_FIRST, "?value"),
            atom("?source", same_as, "?target"),
        ],
    );
    let template = generated_template(&[rule], &[generated_list_rule()]);
    let mut facts = generated_source();
    facts.get_mut(GENERATED_WORLD).unwrap().push(Fact {
        subject: actual_generated_values()[0].clone(),
        predicate: same_as.to_owned(),
        object: TermValue::iri(rule_class),
    });
    let error = template
        .input(&facts, std::sync::Arc::from([]), &[])
        .err()
        .expect("explicit equality can carry a generated value into a protected role");
    assert!(
        error.message().contains("urn:subject-congruence") && error.message().contains(rule_class),
        "{error}"
    );
}

#[test]
fn unmodeled_generated_outputs_are_not_assigned_native_witness_domains() {
    // Ordinary arithmetic/reduction output ranges may be unknown. A head-only
    // variable has no native witness claim merely because it is not body-bound.
    let rule = EvalRule::positive(
        "urn:unmodeled-output",
        atom("?calculated", GENERATED_FIRST, "?x"),
        vec![atom("?x", "urn:seed", "?y")],
    );
    let effect = ProducerEffect::rule(&rule);
    let protected = StatementPattern::statement(&[
        EvalTerm::named(PROTECTED_BODY),
        EvalTerm::named(GENERATED_FIRST),
        EvalTerm::var("?value"),
    ]);
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::GroundedLogicV1,
        std::slice::from_ref(&protected),
    );
    let facts = generated_source();
    let summary = analysis.summarize(facts.values().flatten());
    let effects = analysis.refine(&[effect], &summary);
    assert_eq!(
        analysis.overlapping_writer(&protected, &effects),
        Some("urn:unmodeled-output")
    );
}

#[test]
fn source_constant_enrichment_preserves_unbounded_native_generation_and_known_aliases() {
    let rule = generated_list_rule();
    let statement = |atom: &EvalAtom| {
        [
            atom.subject.clone(),
            EvalTerm::named(&atom.predicate),
            atom.object.clone(),
        ]
    };
    let flow = FlowRule {
        body: rule.body.iter().map(statement).collect(),
        heads: rule.head.iter().map(statement).collect(),
        native_witnesses: rule.existentials(),
        reads: rule.body.iter().map(|atom| Some(statement(atom))).collect(),
    };
    let effect = ProducerEffect::new(
        rule.rule_iri.clone(),
        rule.head.iter().map(StatementPattern::atom).collect(),
        rule.body
            .iter()
            .map(|atom| {
                (
                    StatementPattern::atom(atom),
                    crate::physical::dependency::ReadDependency::Positive,
                )
            })
            .collect(),
    );
    let mut facts = generated_source();
    for value in actual_generated_values() {
        facts.get_mut(GENERATED_WORLD).unwrap().push(Fact {
            subject: value,
            predicate: "urn:known-value".to_owned(),
            object: TermValue::iri(PROTECTED_BODY),
        });
    }
    let analysis = ValueFlow::new(&[flow], &[effect], SemanticVocabulary::GroundedLogicV1)
        .with_source_constants(facts.values().flatten(), 128)
        .unwrap();
    let domain = analysis.universe.native_witness_domain();
    for value in actual_generated_values() {
        assert!(domain.contains(analysis.universe.value(&value)));
    }
    assert!(!domain.contains(analysis.universe.iri(PROTECTED_BODY)));
    let bounds = analysis.finite_bindings(facts.values().flatten());
    assert!(
        !bounds[0].as_ref().unwrap().contains_key("?fresh"),
        "the new range must not turn an unbounded witness family into finite constants"
    );
}
