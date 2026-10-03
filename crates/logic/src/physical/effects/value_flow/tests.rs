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
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
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
fn seeded_refinement_reuses_unseeded_fixed_point_without_changing_result() {
    let rule = EvalRule::positive(
        "urn:two-hop",
        atom("?s", "urn:result", "?o"),
        vec![
            atom("?s", "urn:left", "?middle"),
            atom("?middle", "urn:right", "?o"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let facts = vec![
        Fact {
            subject: TermValue::iri("urn:a"),
            predicate: "urn:left".to_owned(),
            object: TermValue::iri("urn:b"),
        },
        Fact {
            subject: TermValue::iri("urn:d"),
            predicate: "urn:left".to_owned(),
            object: TermValue::iri("urn:e"),
        },
        Fact {
            subject: TermValue::iri("urn:b"),
            predicate: "urn:right".to_owned(),
            object: TermValue::iri("urn:c"),
        },
    ];
    let analysis = ValueFlow::new(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
    )
    .with_source_constants(facts.iter(), 128)
    .unwrap();
    let state = analysis.summarize(facts.iter());
    let lowered = &analysis.rules[0];
    let upper = analysis.bindings(lowered, &state).unwrap();
    let subject = *lowered.names.get("?s").unwrap();

    for value in 0..analysis.universe.size() {
        let from_universe = analysis.bindings_seeded(lowered, &state, &[(subject, value)]);
        let from_upper =
            analysis.bindings_seeded_from(lowered, &state, &upper, &[(subject, value)]);
        assert_eq!(from_upper, from_universe, "seed cell {value}");
    }
}

#[test]
fn complete_input_summary_is_order_independent_and_keeps_native_constant_facets() {
    let blank = |scope| TermValue::Blank {
        label: "x".to_owned(),
        scope: purrdf::BlankScope(scope),
    };
    let quoted = |value: TermValue| TermValue::Triple {
        s: TermValue::iri("urn:claim").into(),
        p: TermValue::iri("urn:states").into(),
        o: value.into(),
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

#[test]
fn conditional_supports_preserve_dense_identity_without_dense_storage() {
    let size = 4096;
    let mut values = Domain::empty(size);
    for value in [1, 65, 4095] {
        values.insert(value);
    }
    let mut support = AdaptiveDomain::empty(size);
    assert!(support.union(&values));
    assert!(matches!(support, AdaptiveDomain::Sparse { .. }));

    let digest = |support: &AdaptiveDomain| {
        let mut hash = blake3::Hasher::new();
        support.hash_nonzero_words(&mut hash);
        *hash.finalize().as_bytes()
    };
    assert_eq!(digest(&support), digest(&AdaptiveDomain::Dense(values)));
}

#[test]
fn dense_domain_word_initialization_masks_the_unused_tail() {
    for size in [0, 1, 63, 64, 65, 127, 128, 129] {
        let mut domain = Domain::all(size);
        assert_eq!(
            domain.indices().collect::<Vec<_>>(),
            (0..size).collect::<Vec<_>>()
        );
        if size > 0 {
            let retained = size / 2;
            domain.intersect_singleton(retained);
            assert_eq!(domain.indices().collect::<Vec<_>>(), vec![retained]);
        }
    }
}

#[test]
fn parallel_head_refinement_matches_the_serial_recursive_fixed_point() {
    let rule = EvalRule::positive(
        "urn:transitive",
        atom("?left", "urn:reach", "?right"),
        vec![
            atom("?left", "urn:reach", "?middle"),
            atom("?middle", "urn:reach", "?right"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let mut observations = Vec::new();
    let mut facts = Vec::new();
    for index in 0..PARALLEL_HEAD_REFINEMENT_MIN_CANDIDATES {
        let node = format!("urn:node:{index}");
        observations.push(StatementPattern::subject(TermValue::iri(&node)));
        for (subject, object) in [(node.as_str(), "urn:hub"), ("urn:hub", node.as_str())] {
            facts.push(Fact {
                subject: TermValue::iri(subject),
                predicate: "urn:reach".to_owned(),
                object: TermValue::iri(object),
            });
        }
    }
    observations.push(StatementPattern::subject(TermValue::iri("urn:hub")));
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    let input = analysis.summarize(facts.iter());
    let template = [0x5a; 32];
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| analysis.closure(&input).identity(&template))
    };

    assert_eq!(run(1), run(4));
}

#[test]
fn symmetric_nonrecursive_head_mirrors_exact_value_supports() {
    let rule = EvalRule::positive(
        "urn:symmetric",
        atom("?left", "urn:same", "?right"),
        vec![
            atom("?group", "urn:member", "?left"),
            atom("?group", "urn:member", "?right"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let observations: Vec<_> = ["urn:a", "urn:b", "urn:x", "urn:y"]
        .into_iter()
        .map(|value| StatementPattern::subject(TermValue::iri(value)))
        .collect();
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    assert!(analysis.has_symmetric_value_sides(&analysis.rules[0], &analysis.rules[0].heads[0]));

    let facts = [
        Fact {
            subject: TermValue::iri("urn:x"),
            predicate: "urn:member".to_owned(),
            object: TermValue::iri("urn:a"),
        },
        Fact {
            subject: TermValue::iri("urn:y"),
            predicate: "urn:member".to_owned(),
            object: TermValue::iri("urn:b"),
        },
    ];
    let closure = analysis.closure(&analysis.summarize(facts.iter()));
    let relation = closure
        .relations
        .get(&analysis.universe.iri("urn:same"))
        .expect("symmetric rule is reachable");
    let a = analysis.universe.iri("urn:a");
    let b = analysis.universe.iri("urn:b");
    for supports in [&relation.by_subject, &relation.by_object] {
        let a_support = supports.get(&a).expect("a has exact support").to_domain();
        assert!(a_support.contains(a));
        assert!(!a_support.contains(b));
        let b_support = supports.get(&b).expect("b has exact support").to_domain();
        assert!(b_support.contains(b));
        assert!(!b_support.contains(a));
    }
}

#[test]
fn asymmetric_head_is_not_mirrored() {
    let rule = EvalRule::positive(
        "urn:asymmetric",
        atom("?left", "urn:same", "?right"),
        vec![
            atom("?group", "urn:left-member", "?left"),
            atom("?group", "urn:right-member", "?right"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let analysis = ValueFlow::new(&[flow(&rule)], &[effect], SemanticVocabulary::Exact);
    assert!(!analysis.has_symmetric_value_sides(&analysis.rules[0], &analysis.rules[0].heads[0]));
}

#[test]
fn asymmetric_cardinality_guard_is_not_mirrored() {
    let rule = EvalRule::positive(
        "urn:guarded-symmetric-body",
        atom("?left", "urn:same", "?right"),
        vec![
            atom("?group", "urn:member", "?left"),
            atom("?group", "urn:member", "?right"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let mut flow = flow(&rule);
    flow.cardinality_guards.push(FlowCardinalityGuard {
        term: EvalTerm::var("?left"),
        count: 1,
    });
    let analysis = ValueFlow::new(&[flow], &[effect], SemanticVocabulary::Exact);

    assert!(!analysis.has_symmetric_value_sides(&analysis.rules[0], &analysis.rules[0].heads[0]));
}

#[test]
fn dynamic_predicate_symmetric_head_is_mirrored() {
    let rule = FlowRule {
        body: vec![
            [
                EvalTerm::var("?group"),
                EvalTerm::var("?predicate"),
                EvalTerm::var("?left"),
            ],
            [
                EvalTerm::var("?group"),
                EvalTerm::var("?predicate"),
                EvalTerm::var("?right"),
            ],
        ],
        heads: vec![[
            EvalTerm::var("?left"),
            EvalTerm::named("urn:same"),
            EvalTerm::var("?right"),
        ]],
        native_witnesses: Vec::new(),
        reads: Vec::new(),
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
    };
    let effect = ProducerEffect::new(
        "urn:dynamic-symmetric".to_owned(),
        vec![StatementPattern::relation(Some("urn:same"), None)],
        Vec::new(),
    );
    let analysis = ValueFlow::new(&[rule], &[effect], SemanticVocabulary::Exact);
    assert!(analysis.has_symmetric_value_sides(&analysis.rules[0], &analysis.rules[0].heads[0]));
}

#[test]
fn cardinality_guard_narrows_source_rows_without_collapsing_lexical_forms() {
    const COUNT: &str = "urn:count";
    const VALUE: &str = "urn:value";
    const RESULT: &str = "urn:result";
    const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
    let body = vec![
        [
            EvalTerm::var("?restriction"),
            EvalTerm::named(COUNT),
            EvalTerm::var("?count"),
        ],
        [
            EvalTerm::var("?restriction"),
            EvalTerm::named(VALUE),
            EvalTerm::var("?output"),
        ],
    ];
    let head = [
        EvalTerm::var("?output"),
        EvalTerm::named(RESULT),
        EvalTerm::named("urn:yes"),
    ];
    let rule = FlowRule {
        body: body.clone(),
        heads: vec![head.clone()],
        native_witnesses: Vec::new(),
        reads: body.iter().cloned().map(Some).collect(),
        cardinality_guards: vec![FlowCardinalityGuard {
            term: EvalTerm::var("?count"),
            count: 1,
        }],
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
    };
    let effect = ProducerEffect::new(
        "urn:guarded".to_owned(),
        vec![StatementPattern::statement(&head)],
        body.iter()
            .map(|atom| {
                (
                    StatementPattern::statement(atom),
                    crate::physical::dependency::ReadDependency::Positive,
                )
            })
            .collect(),
    );
    let observations: Vec<_> = ["urn:r1", "urn:r2", "urn:a", "urn:b"]
        .into_iter()
        .map(|value| StatementPattern::subject(TermValue::iri(value)))
        .collect();
    let literal = |lexical_form: &str| TermValue::Literal {
        lexical_form: lexical_form.to_owned(),
        datatype: INTEGER.to_owned(),
        language: None,
        direction: None,
    };
    let facts = [
        Fact {
            subject: TermValue::iri("urn:r1"),
            predicate: COUNT.to_owned(),
            object: literal("01"),
        },
        Fact {
            subject: TermValue::iri("urn:r1"),
            predicate: VALUE.to_owned(),
            object: TermValue::iri("urn:a"),
        },
        Fact {
            subject: TermValue::iri("urn:r2"),
            predicate: COUNT.to_owned(),
            object: literal("2"),
        },
        Fact {
            subject: TermValue::iri("urn:r2"),
            predicate: VALUE.to_owned(),
            object: TermValue::iri("urn:b"),
        },
    ];
    let analysis = ValueFlow::with_observations(
        &[rule],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    )
    .with_source_operators(facts.iter());
    let refined = analysis.refine(
        std::slice::from_ref(&effect),
        &analysis.summarize(facts.iter()),
    );
    let subjects = &refined[0].writes[0]
        .ranges
        .as_ref()
        .expect("guarded producer is reachable")[0];
    assert!(subjects.contains(analysis.universe.iri("urn:a")));
    assert!(!subjects.contains(analysis.universe.iri("urn:b")));
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
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
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
                    cardinality_guards: Vec::new(),
                    list_reads: Vec::new(),
                    selected_reads: Vec::new(),
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

#[test]
fn maximum_one_equality_uses_symmetric_value_refinement() {
    let (rules, effects) = schema_flow();
    let analysis = ValueFlow::new(&rules, &effects, SemanticVocabulary::GroundedLogicV1);
    let maximum_rules: Vec<_> = analysis
        .rules
        .iter()
        .filter(|rule| rule.name == "dl:maximum-one-equality")
        .collect();
    assert!(!maximum_rules.is_empty());
    for rule in maximum_rules {
        assert!(analysis.has_symmetric_value_sides(rule, &rule.heads[0]));
    }
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
fn predicate_identity_alone_does_not_request_tuple_conditioning() {
    let (rule, effect) = subproperty_flow();
    let facts = [
        Fact {
            subject: TermValue::iri("urn:selected-property"),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri("urn:selected-superproperty"),
        },
        Fact {
            subject: TermValue::iri("urn:subject"),
            predicate: "urn:unrelated-predicate".to_owned(),
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
    let selected = analysis.universe.iri("urn:selected-property");
    let unrelated = analysis.universe.iri("urn:unrelated-predicate");

    assert_ne!(selected, analysis.universe.other());
    assert_ne!(unrelated, analysis.universe.other());
    assert!(analysis.conditioned.contains(selected));
    assert!(
        !analysis.conditioned.contains(unrelated),
        "an IRI used only as an RDF predicate needs an exact predicate cell, not subject/object correlation"
    );
}

#[test]
fn large_operator_universe_keeps_unreachable_protected_target_excluded() {
    let (rule, effect) = subproperty_flow();
    let mut facts: Vec<_> = (0..4096)
        .map(|index| Fact {
            subject: TermValue::iri(format!("urn:property:{index}")),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri(format!("urn:superproperty:{index}")),
        })
        .collect();
    facts.push(Fact {
        subject: TermValue::iri("urn:inactive-property"),
        predicate: SUBPROPERTY.to_owned(),
        object: TermValue::iri(PROTECTED_BODY),
    });
    facts.push(Fact {
        subject: TermValue::iri("urn:subject"),
        predicate: "urn:property:0".to_owned(),
        object: TermValue::iri("urn:object"),
    });
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
        None
    );
}

#[test]
fn dynamic_predicate_join_ignores_unrelated_relations_without_losing_reachability() {
    let (rule, effect) = subproperty_flow();
    let mut facts = Vec::new();
    for index in 0..4096 {
        let property = format!("urn:property:{index}");
        facts.push(Fact {
            subject: TermValue::iri(&property),
            predicate: property.clone(),
            object: TermValue::iri(format!("urn:object:{index}")),
        });
        facts.push(Fact {
            subject: TermValue::iri(&property),
            predicate: SUBPROPERTY.to_owned(),
            object: TermValue::iri(if index == 0 {
                PROTECTED_BODY.to_owned()
            } else {
                format!("urn:superproperty:{index}")
            }),
        });
    }
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

#[test]
fn wildcard_output_envelope_retains_one_cartesian_rectangle() {
    let output = StatementPattern::relation(Some("urn:output"), None);
    let mut observations = vec![output.clone()];
    observations.extend(
        (0..4096)
            .map(|index| StatementPattern::subject(TermValue::iri(format!("urn:value:{index}")))),
    );
    let analysis = ValueFlow::with_conditioned_observations(
        &[],
        &[],
        SemanticVocabulary::Exact,
        &observations,
        &observations,
    );
    let mut summary = analysis.summarize(std::iter::empty());
    analysis.seed_patterns(&mut summary, std::iter::once(&output));

    let relation = summary
        .relations
        .get(&analysis.universe.iri("urn:output"))
        .expect("output envelope relation");
    assert!(relation.by_subject.is_empty());
    assert!(relation.by_object.is_empty());
    assert_eq!(relation.rectangles.len(), 1);
    assert!(relation.rectangles[0][0].contains(analysis.universe.other()));
    assert!(relation.rectangles[0][1].contains(analysis.universe.other()));
}

#[test]
fn native_witness_head_retains_one_exact_rectangle() {
    let body = [[
        EvalTerm::var("?subject"),
        EvalTerm::named("urn:seed"),
        EvalTerm::named("urn:yes"),
    ]];
    let head = [[
        EvalTerm::var("?subject"),
        EvalTerm::named("urn:generated"),
        EvalTerm::var("?witness"),
    ]];
    let rule = FlowRule {
        body: body.to_vec(),
        heads: head.to_vec(),
        native_witnesses: vec!["?witness".to_owned()],
        reads: body.iter().cloned().map(Some).collect(),
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
    };
    let effect = ProducerEffect::new(
        "urn:native-witness".to_owned(),
        vec![StatementPattern::statement(&head[0])],
        Vec::new(),
    );
    let mut observations: Vec<_> = (0..4096)
        .map(|index| {
            StatementPattern::subject(TermValue::iri(format!(
                "{}{index}",
                crate::facts::SKOLEM_PREFIX
            )))
        })
        .collect();
    observations.extend([
        StatementPattern::subject(TermValue::iri("urn:subject")),
        StatementPattern::subject(TermValue::iri("urn:excluded")),
    ]);
    let analysis = ValueFlow::with_observations(
        &[rule],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    let facts = [Fact {
        subject: TermValue::iri("urn:subject"),
        predicate: "urn:seed".to_owned(),
        object: TermValue::iri("urn:yes"),
    }];
    let closure = analysis.closure(&analysis.summarize(facts.iter()));
    let relation = closure
        .relations
        .get(&analysis.universe.iri("urn:generated"))
        .expect("native witness head is reachable");

    assert!(relation.by_subject.is_empty());
    assert!(relation.by_object.is_empty());
    assert_eq!(relation.rectangles.len(), 1);
    assert!(
        relation.rectangles[0][0].contains(analysis.universe.iri("urn:subject")),
        "the supported body binding reaches the generated head"
    );
    assert!(
        !relation.rectangles[0][0].contains(analysis.universe.iri("urn:excluded")),
        "the compact rectangle must not invent unsupported known subjects"
    );
    for observation in &observations[..4096] {
        let witness = observation.subject.as_ref().expect("witness observation");
        assert!(relation.rectangles[0][1].contains(analysis.universe.value(witness)));
    }
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
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
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

/// A head support as plain sorted indices, whichever representation holds it.
fn support_indices(support: HeadSupport) -> (Vec<usize>, Vec<usize>) {
    (support.predicates, support.opposite.indices().collect())
}

#[test]
fn value_classes_transpose_each_member_onto_its_exact_representative_support() {
    let rule = EvalRule::positive(
        "urn:two-hop",
        atom("?s", "urn:result", "?o"),
        vec![
            atom("?s", "urn:left", "?middle"),
            atom("?middle", "urn:right", "?o"),
        ],
    );
    let effect = ProducerEffect::rule(&rule);
    let node = |index: usize| format!("urn:node:{index}");
    let fact = |subject: &str, predicate: &str, object: &str| Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    };
    let count = PARALLEL_HEAD_REFINEMENT_MIN_CANDIDATES + 76;
    let mut facts = vec![
        fact("urn:hubA", "urn:right", "urn:c1"),
        fact("urn:hubB", "urn:right", "urn:c2"),
        // A self edge and a mutual edge each give their values a distinct signature.
        fact(&node(7), "urn:left", &node(7)),
        fact(&node(5), "urn:left", &node(9)),
        fact(&node(9), "urn:left", &node(5)),
        fact(&node(11), "urn:right", "urn:c1"),
    ];
    // A clique with self edges is a genuine exchange: both members share a class.
    for (subject, object) in [(20, 20), (20, 22), (22, 20), (22, 22)] {
        facts.push(fact(&node(subject), "urn:left", &node(object)));
    }
    let mut observations: Vec<_> = ["urn:hubA", "urn:hubB", "urn:c1", "urn:c2"]
        .into_iter()
        .map(|value| StatementPattern::subject(TermValue::iri(value)))
        .collect();
    for index in 0..count {
        let hub = if index % 2 == 0 {
            "urn:hubA"
        } else {
            "urn:hubB"
        };
        facts.push(fact(&node(index), "urn:left", hub));
        observations.push(StatementPattern::subject(TermValue::iri(&node(index))));
    }
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    let state = analysis.summarize(facts.iter());
    let lowered = &analysis.rules[0];
    let upper = analysis.bindings(lowered, &state).unwrap();
    let head = &lowered.heads[0];
    let value = |iri: &str| analysis.universe.value(&TermValue::iri(iri));
    let support = |slot: usize, member: usize| {
        let refinement =
            analysis.bindings_for_head_value(lowered, head, slot, member, &state, &upper);
        analysis
            .head_value_support(head, slot, refinement)
            .map(support_indices)
    };
    let transposed = |support: Option<(Vec<usize>, Vec<usize>)>, from: usize, to: usize| {
        support.map(|(predicates, opposite)| {
            let swap = |index: usize| match index {
                index if index == from => to,
                index if index == to => from,
                index => index,
            };
            let mut predicates: Vec<_> = predicates.into_iter().map(swap).collect();
            let mut opposite: Vec<_> = opposite.into_iter().map(swap).collect();
            predicates.sort_unstable();
            opposite.sort_unstable();
            (predicates, opposite)
        })
    };

    for slot in [0, 2] {
        let candidates: Vec<_> = analysis.universe.domains(head, &upper, false)[slot]
            .indices()
            .filter(|value| analysis.conditioned.contains(*value))
            .collect();
        let classes = analysis.value_classes(lowered, &state, &upper, &candidates);
        let mut members: Vec<_> = classes.iter().flatten().copied().collect();
        members.sort_unstable();
        assert_eq!(
            members, candidates,
            "slot {slot} partitions every candidate"
        );
        for class in &classes {
            let representative = class[0];
            for &member in class {
                assert_eq!(
                    support(slot, member),
                    transposed(support(slot, representative), representative, member),
                    "slot {slot}: {member} is exchangeable with {representative}",
                );
            }
        }
        if slot == 0 {
            assert!(
                classes.len() < 16,
                "symmetric fan-in collapses: {}",
                classes.len()
            );
            for broken in [node(7), node(5), node(9), node(11)] {
                assert!(
                    classes.contains(&vec![value(&broken)]),
                    "{broken} has its own class",
                );
            }
            assert!(
                classes.contains(&vec![value(&node(20)), value(&node(22))]),
                "an exchangeable clique shares one class",
            );
        }
    }
}

#[test]
fn value_classes_separate_values_named_only_by_a_one_sided_published_support() {
    let rule = EvalRule::positive(
        "urn:copy",
        atom("?s", "urn:result", "?o"),
        vec![atom("?s", "urn:left", "?o")],
    );
    let effect = ProducerEffect::rule(&rule);
    let node = |index: usize| format!("urn:node:{index}");
    let mut facts = Vec::new();
    let mut observations = vec![StatementPattern::subject(TermValue::iri("urn:hub"))];
    for index in 0..10 {
        facts.push(Fact {
            subject: TermValue::iri(&node(index)),
            predicate: "urn:left".to_owned(),
            object: TermValue::iri("urn:hub"),
        });
        observations.push(StatementPattern::subject(TermValue::iri(&node(index))));
    }
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    let size = analysis.universe.size();
    let value = |index: usize| analysis.universe.value(&TermValue::iri(&node(index)));
    let left = analysis.universe.iri("urn:left");
    let mut state = analysis.summarize(facts.iter());
    // Head refinements publish one side at a time. Nodes 0 and 6 carry the same
    // subject-side support, but only node 0 is named by node 2's object side.
    let two = Domain::one(size, value(2));
    state.publish_columns(
        &[
            Domain::one(size, value(0)),
            Domain::one(size, left),
            two.clone(),
        ],
        &analysis.universe,
    );
    assert!(state.publish_condition(left, 0, value(0), &two, size));
    assert!(state.publish_condition(left, 0, value(6), &two, size));
    assert!(state.publish_condition(left, 1, value(2), &Domain::one(size, value(0)), size));
    let lowered = &analysis.rules[0];
    let upper = analysis.bindings(lowered, &state).unwrap();
    let head = &lowered.heads[0];
    let candidates: Vec<_> = analysis.universe.domains(head, &upper, false)[0]
        .indices()
        .filter(|value| analysis.conditioned.contains(*value))
        .collect();
    let support = |member: usize| {
        let refinement = analysis.bindings_for_head_value(lowered, head, 0, member, &state, &upper);
        analysis
            .head_value_support(head, 0, refinement)
            .map(support_indices)
    };
    assert_ne!(support(value(0)), support(value(6)));

    let classes = analysis.value_classes(lowered, &state, &upper, &candidates);
    let class_of = |member: usize| classes.iter().position(|class| class.contains(&member));
    assert_ne!(
        class_of(value(0)),
        class_of(value(6)),
        "membership in another key's support distinguishes otherwise equal values",
    );
    assert_eq!(
        class_of(value(4)),
        class_of(value(8)),
        "plain fan-in still shares a class"
    );
}

#[test]
fn candidate_partition_matches_brute_force_membership_equivalence() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for trial in 0..64 {
        let len = 1 + (next() % 48) as usize;
        let singletons: Vec<bool> = (0..len).map(|_| next() % 11 == 0).collect();
        // Candidates are spread over a larger universe, with non-candidate values
        // between them, so positions and values never coincide.
        let size = 3 * len + 70;
        let candidates: Vec<usize> = (0..len).map(|position| 3 * position + 1).collect();
        let mut partition = CandidatePartition::new(&singletons, &candidates, size);
        // Each position's observed history: singleton identity, then one entry per split.
        let mut history: Vec<Vec<u64>> = (0..len)
            .map(|position| {
                vec![if singletons[position] {
                    position as u64 + 1
                } else {
                    0
                }]
            })
            .collect();
        // Some trials run past the live-mask refresh interval.
        let splits = if trial % 8 == 0 {
            LIVE_REFRESH_SPLITS as u64 + 44
        } else {
            next() % 6
        };
        for _ in 0..splits {
            // A refresh is valid at any point; between refreshes the mask is stale.
            if next() % 3 == 0 {
                partition.refresh_live();
            }
            if next() % 2 == 0 {
                // Dense draws exercise the omitted-side walk of `split_dense`.
                let dense = next() % 2 == 0;
                let members: Vec<usize> = (0..len)
                    .filter(|_| {
                        if dense {
                            next() % 4 != 0
                        } else {
                            next() % 3 == 0
                        }
                    })
                    .collect();
                for (position, entry) in history.iter_mut().enumerate() {
                    entry.push(u64::from(members.contains(&position)));
                }
                if next() % 2 == 0 {
                    partition.split(members);
                } else {
                    // Non-candidate values in the set are never positions; the live
                    // mask drops them.
                    let mut set = Domain::empty(size);
                    for &member in &members {
                        set.insert(candidates[member]);
                    }
                    for value in (0..size).filter(|value| value % 3 != 1 || *value >= 3 * len) {
                        if next() % 2 == 0 {
                            set.insert(value);
                        }
                    }
                    partition.split_dense(&set);
                }
            } else {
                let labelled: Vec<(usize, u64)> = (0..len)
                    .filter_map(|position| {
                        let draw = next();
                        (draw % 2 == 0).then_some((position, (draw >> 8) % 3))
                    })
                    .collect();
                for (position, entry) in history.iter_mut().enumerate() {
                    let label = labelled.iter().find(|(member, _)| *member == position);
                    entry.push(label.map_or(u64::MAX, |(_, label)| *label));
                }
                partition.split_labelled(labelled);
            }
        }
        let classes = partition.classes(&(0..len).collect::<Vec<_>>());
        let class_of: Vec<usize> = {
            let mut class_of = vec![0; len];
            for (class, members) in classes.iter().enumerate() {
                for &member in members {
                    class_of[member] = class;
                }
            }
            class_of
        };
        for left in 0..len {
            for right in 0..len {
                assert_eq!(
                    class_of[left] == class_of[right],
                    history[left] == history[right],
                    "positions {left} and {right}",
                );
            }
        }
        assert!(classes.windows(2).all(|pair| pair[0][0] < pair[1][0]));
    }
}

#[test]
fn published_member_support_is_the_representative_support_transposed() {
    let rule = EvalRule::positive(
        "urn:copy",
        atom("?s", "urn:result", "?o"),
        vec![atom("?s", "urn:left", "?o")],
    );
    let effect = ProducerEffect::rule(&rule);
    let observations: Vec<_> = (0..8)
        .map(|index| StatementPattern::subject(TermValue::iri(&format!("urn:node:{index}"))))
        .collect();
    let analysis = ValueFlow::with_observations(
        &[flow(&rule)],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &observations,
    );
    let size = analysis.universe.size();
    let node = |index: usize| {
        analysis
            .universe
            .value(&TermValue::iri(&format!("urn:node:{index}")))
    };
    let result = analysis.universe.iri("urn:result");
    let (representative, member) = (node(0), node(1));
    // Opposite columns holding neither, both, or exactly one of the exchanged pair.
    for opposite in [
        vec![node(4), node(5)],
        vec![representative, member, node(4)],
        vec![representative, node(4)],
        vec![member, node(5)],
    ] {
        let mut expected: Vec<_> = opposite
            .iter()
            .map(|&index| match index {
                index if index == representative => member,
                index if index == member => representative,
                index => index,
            })
            .collect();
        expected.sort_unstable();
        let mut dense = Domain::empty(size);
        for &index in &opposite {
            dense.insert(index);
        }
        let sparse = AdaptiveDomain::Sparse {
            size,
            values: dense.indices().collect(),
        };
        for stored in [AdaptiveDomain::Dense(dense.clone()), sparse] {
            let support = HeadSupport {
                predicates: vec![result],
                opposite: stored,
            };
            let mut state = analysis.summarize(std::iter::empty());
            let mut scratch = Domain::empty(size);
            let mut domains = std::array::from_fn(|_| Domain::all(size));
            analysis.publish_head_support(
                &mut state,
                0,
                member,
                Some(&support),
                representative,
                &mut scratch,
                &mut domains,
                false,
                &mut BTreeSet::new(),
            );
            let published: Vec<_> = state.relations[&result].by_subject[&member]
                .indices()
                .collect();
            assert_eq!(published, expected, "opposite {opposite:?}");
        }
    }
}
