// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete-input scheduling and native dimensional feedback, without a corpus.

use super::*;
use crate::physical::{PropertyAtom, PropertyRule};
use crate::query_ir::{QBuiltin, QTerm};
use crate::seam::BudgetStatus;
use purrdf::TermValue;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const PROPERTY_CHAIN: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const WORLD: &str = "urn:world";

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
fn property(values: [&str; 3]) -> PropertyAtom {
    PropertyAtom(values.map(term))
}

fn dependency_cycle(input: &JointInput) -> crate::physical::dependency::DependencyCycle {
    let observations: Vec<_> = input
        .facts
        .keys()
        .flat_map(|world| {
            input.template.family_reads.iter().map(move |read| {
                crate::physical::effects::WorldStatementObservation {
                    world: world.clone(),
                    pattern: input.flow.ranged_pattern(
                        crate::physical::effects::StatementPattern::relation(
                            read.predicate.as_deref(),
                            read.marker.as_deref(),
                        ),
                    ),
                }
            })
        })
        .collect();
    let scoped: Vec<_> = input
        .world_effects
        .iter()
        .flat_map(|(world, effects)| {
            effects.iter().cloned().map(move |effect| {
                crate::physical::effects::WorldProducerEffect::local(world, effect)
            })
        })
        .collect();
    let predicates = input
        .facts
        .iter()
        .map(|(world, facts)| {
            (
                world.clone(),
                facts.iter().map(|fact| fact.predicate.clone()).collect(),
            )
        })
        .collect();
    crate::physical::effects::schedule_worlds(
        &scoped,
        input.template.semantics,
        &predicates,
        &observations,
    )
    .err()
    .expect("input has a dependency cycle")
}

#[test]
fn maximum_one_schema_exports_its_native_count_guard_to_value_flow() {
    let laws: Vec<_> = crate::reason::schema_laws()
        .iter()
        .filter(|law| law.source.rule_iri == "dl:maximum-one-equality")
        .collect();
    assert!(!laws.is_empty());
    for law in laws {
        let guards = flow_cardinality_guards(law);
        assert_eq!(guards.len(), 1);
        assert_eq!(guards[0].count, 1);
        assert_eq!(guards[0].term, EvalTerm::var("?count"));
    }
}

#[test]
fn minimum_schema_exports_every_generated_analysis_witness_to_value_flow() {
    let laws: Vec<_> = crate::reason::schema_laws()
        .iter()
        .filter(|law| law.source.rule_iri.starts_with("dl:minimum-witness:"))
        .collect();
    assert!(!laws.is_empty());
    for law in laws {
        let body_variables: BTreeSet<_> = law
            .analysis_body
            .iter()
            .flat_map(|atom| &atom.0)
            .filter_map(|term| match term {
                EvalTerm::Var(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let generated: BTreeSet<_> = law
            .analysis_heads
            .iter()
            .flat_map(|atom| &atom.0)
            .filter_map(|term| match term {
                EvalTerm::Var(name) if !body_variables.contains(name) => Some(name.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(generated.len(), 2, "{}", law.source.rule_iri);
        assert_eq!(
            flow_native_witnesses(law)
                .into_iter()
                .collect::<BTreeSet<_>>(),
            generated,
            "{}",
            law.source.rule_iri,
        );
    }
}

fn template(operator: &str) -> Arc<JointTemplate> {
    let mut violation = EvalRule::positive(
        "urn:dimension-check",
        atom("?x", TYPE, "urn:FailureClass"),
        vec![atom("?x", "urn:seed", "?y")],
    );
    violation.constraint_tag = Some("urn:dimension-check".to_owned());
    violation.builtins.push(QBuiltin::DimEqual {
        d1: QTerm::Const(format!("<{MATH}dimensionless>")),
        d2: QTerm::Const("<urn:dimension>".to_owned()),
    });
    let schema = PreparedPropertyRule::new(PropertyRule {
        rule_iri: "urn:classify-failure".to_owned(),
        head: property(["?x", operator, "?class"]),
        body: vec![
            property(["?x", operator, "urn:FailureClass"]),
            property(["urn:selector", "urn:class", "?class"]),
        ],
        operation: None,
        guards: Vec::new(),
    })
    .unwrap();
    Arc::new(
        JointTemplate::new(
            &[violation],
            &[],
            &[schema],
            SemanticVocabulary::GroundedLogicV1,
        )
        .unwrap(),
    )
}

fn source(class: &str, operator: &str) -> BTreeMap<String, Vec<Fact>> {
    let mut facts: Vec<_> = [
        ("urn:dimension", "urn:seed", "urn:value".to_owned()),
        ("urn:selector", "urn:class", class.to_owned()),
        ("urn:dimension", operator, format!("{MATH}DerivedDimension")),
        (
            "urn:dimension",
            "https://blackcatinformatics.ca/math/baseDimensionExponent",
            "urn:exponent".to_owned(),
        ),
        (
            "urn:exponent",
            "https://blackcatinformatics.ca/math/exponentOfDimension",
            format!("{MATH}massDimension"),
        ),
    ]
    .into_iter()
    .map(|(s, p, o)| Fact {
        subject: TermValue::iri(s),
        predicate: p.to_owned(),
        object: TermValue::iri(o),
    })
    .collect();
    for name in ["exponentNumerator", "exponentDenominator"] {
        facts.push(Fact {
            subject: TermValue::iri("urn:exponent"),
            predicate: format!("{MATH}{name}"),
            object: TermValue::Literal {
                lexical_form: "1".to_owned(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_owned(),
                language: None,
                direction: None,
            },
        });
    }
    BTreeMap::from([(WORLD.to_owned(), facts)])
}

#[test]
fn schema_class_value_flow_admits_unrelated_failure_types_and_refuses_feedback() {
    for operator in [TYPE, INSTANCE] {
        let template = template(operator);
        let data = source("urn:OtherClass", operator);
        let input = template
            .input(&data, std::sync::Arc::from([]), &[])
            .unwrap();
        let NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
            panic!("unrelated class effects must not create a dimension cycle")
        };
        let NativeOutcome::Decided(result) = plan.materialize_input(&input, None).unwrap() else {
            panic!("admitted finite producer must execute")
        };
        assert_eq!(result.result.status, BudgetStatus::Ok);
        assert!(
            result.result.rows.iter().any(
                |row| row.predicate == TYPE && row.object == TermValue::iri("urn:FailureClass")
            )
        );
        let classified = result
            .result
            .rows
            .iter()
            .find(|row| row.predicate == operator && row.object == TermValue::iri("urn:OtherClass"))
            .unwrap();
        assert_eq!(classified.subject, TermValue::iri("urn:dimension"));
        assert_eq!(classified.graph, WORLD);
        assert!(
            classified
                .antecedents
                .iter()
                .any(|fact| fact.predicate == "urn:class"
                    && fact.object == TermValue::iri("urn:OtherClass"))
        );
        let cyclic = source(&format!("{MATH}Dimensionless"), operator);
        let changed = template
            .input(&cyclic, std::sync::Arc::from([]), &[])
            .unwrap();
        assert_ne!(
            input.identity(),
            changed.identity(),
            "changing only class values must invalidate the schedule"
        );
        assert!(matches!(
            changed.prepare().unwrap(),
            NativeOutcome::Unsupported(super::super::UnsupportedKind::NonStratifiable)
        ));
        assert!(plan.materialize_input(&changed, None).is_err());
        assert!(plan.materialize_facts(&data, None).is_err());
    }
}

#[test]
fn range_selected_class_declaration_does_not_turn_type_into_the_ranged_property() {
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
    const PROPERTY: &str = "https://blackcatinformatics.ca/logic/variableSort";
    let template = Arc::new(
        JointTemplate::build(
            &[],
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    );
    let data = BTreeMap::from([(
        WORLD.to_owned(),
        vec![
            Fact {
                subject: TermValue::iri(PROPERTY),
                predicate: RANGE.to_owned(),
                object: TermValue::iri(CLASS),
            },
            Fact {
                subject: TermValue::iri("urn:term"),
                predicate: PROPERTY.to_owned(),
                object: TermValue::iri("urn:declared-class"),
            },
            Fact {
                subject: TermValue::iri("urn:individual"),
                predicate: TYPE.to_owned(),
                object: TermValue::iri("urn:ordinary-class"),
            },
        ],
    )]);

    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    assert!(
        matches!(input.prepare().unwrap(), NativeOutcome::Decided(_)),
        "rdf:type output cannot satisfy an unrelated data-selected predicate"
    );
}

#[test]
fn minimum_witness_selector_does_not_accept_unrelated_class_output() {
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
    const MINIMUM: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
    const CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
    const PROPERTY: &str = "https://blackcatinformatics.ca/logic/variableSort";
    let template = Arc::new(
        JointTemplate::build(
            &[],
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    );
    let mut rows = vec![
        Fact {
            subject: TermValue::iri(PROPERTY),
            predicate: RANGE.to_owned(),
            object: TermValue::iri(CLASS),
        },
        Fact {
            subject: TermValue::iri("urn:restriction"),
            predicate: ON_PROPERTY.to_owned(),
            object: TermValue::iri(PROPERTY),
        },
        Fact {
            subject: TermValue::iri("urn:restriction"),
            predicate: MINIMUM.to_owned(),
            object: TermValue::simple_literal("1"),
        },
        Fact {
            subject: TermValue::iri("urn:restriction"),
            predicate: ON_CLASS.to_owned(),
            object: TermValue::iri("urn:witness-class"),
        },
        Fact {
            subject: TermValue::iri("urn:individual"),
            predicate: TYPE.to_owned(),
            object: TermValue::iri("urn:restriction"),
        },
    ];
    rows.extend((0..600).map(|index| Fact {
        subject: TermValue::iri(format!("urn:unrelated-individual:{index}")),
        predicate: TYPE.to_owned(),
        object: TermValue::iri(format!("urn:unrelated-class:{index}")),
    }));
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);

    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    match input.prepare().unwrap() {
        NativeOutcome::Decided(plan) => {
            assert!(
                plan.admission.admits_native(),
                "the finite restriction selectors must certify termination: {:?}",
                plan.admission,
            );
        }
        NativeOutcome::Unsupported(kind) => panic!(
            "class-family output cannot satisfy a different exact restriction carrier: {kind:?}: {:?}",
            dependency_cycle(&input),
        ),
    }
}

fn property_chain_input(
    member: &str,
    unrelated_type_list: bool,
) -> (Arc<JointTemplate>, BTreeMap<String, Vec<Fact>>) {
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
    const PROPERTY: &str = "https://blackcatinformatics.ca/logic/variableSort";
    let template = Arc::new(
        JointTemplate::build(
            &[],
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    );
    let mut facts = vec![
        Fact {
            subject: TermValue::iri(PROPERTY),
            predicate: RANGE.to_owned(),
            object: TermValue::iri(CLASS),
        },
        Fact {
            subject: TermValue::iri(PROPERTY),
            predicate: PROPERTY_CHAIN.to_owned(),
            object: TermValue::iri("urn:selected-list"),
        },
        Fact {
            subject: TermValue::iri("urn:selected-list"),
            predicate: FIRST.to_owned(),
            object: TermValue::iri(member),
        },
        Fact {
            subject: TermValue::iri("urn:selected-list"),
            predicate: REST.to_owned(),
            object: TermValue::iri(NIL),
        },
        Fact {
            subject: TermValue::iri("urn:individual"),
            predicate: TYPE.to_owned(),
            object: TermValue::iri("urn:ordinary-class"),
        },
        Fact {
            subject: TermValue::iri("urn:individual"),
            predicate: "urn:edge".to_owned(),
            object: TermValue::iri("urn:object"),
        },
    ];
    if unrelated_type_list {
        facts.extend([
            Fact {
                subject: TermValue::iri("urn:unrelated-list"),
                predicate: FIRST.to_owned(),
                object: TermValue::iri(TYPE),
            },
            Fact {
                subject: TermValue::iri("urn:unrelated-list"),
                predicate: REST.to_owned(),
                object: TermValue::iri(NIL),
            },
        ]);
    }
    (template, BTreeMap::from([(WORLD.to_owned(), facts)]))
}

#[test]
fn unrelated_rdf_list_member_cannot_become_a_property_chain_read() {
    let (template, data) = property_chain_input("urn:edge", true);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    assert!(
        matches!(input.prepare().unwrap(), NativeOutcome::Decided(_)),
        "rdf:type in an unrelated RDF list must not create a property-chain cycle"
    );
}

#[test]
fn selected_property_chain_member_retains_its_real_dependency_cycle() {
    let (template, data) = property_chain_input(TYPE, false);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    assert!(matches!(
        input.prepare().unwrap(),
        NativeOutcome::Unsupported(super::super::UnsupportedKind::NonStratifiable)
    ));
}

#[test]
fn intersection_membership_uses_only_its_selected_list_classes() {
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
    const INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
    const CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
    const PROPERTY: &str = "https://blackcatinformatics.ca/logic/variableSort";
    let template = Arc::new(
        JointTemplate::build(
            &[],
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    );
    let data = BTreeMap::from([(
        WORLD.to_owned(),
        vec![
            Fact {
                subject: TermValue::iri(PROPERTY),
                predicate: RANGE.to_owned(),
                object: TermValue::iri(CLASS),
            },
            Fact {
                subject: TermValue::iri("urn:restriction"),
                predicate: ON_PROPERTY.to_owned(),
                object: TermValue::iri(PROPERTY),
            },
            Fact {
                subject: TermValue::iri("urn:restriction"),
                predicate: HAS_VALUE.to_owned(),
                object: TermValue::iri("urn:value"),
            },
            Fact {
                subject: TermValue::iri("urn:intersection"),
                predicate: INTERSECTION.to_owned(),
                object: TermValue::iri("urn:intersection-list"),
            },
            Fact {
                subject: TermValue::iri("urn:intersection-list"),
                predicate: FIRST.to_owned(),
                object: TermValue::iri("urn:member-class"),
            },
            Fact {
                subject: TermValue::iri("urn:intersection-list"),
                predicate: REST.to_owned(),
                object: TermValue::iri(NIL),
            },
            Fact {
                subject: TermValue::iri("urn:intersection"),
                predicate: SUBCLASS.to_owned(),
                object: TermValue::iri("urn:restriction"),
            },
            Fact {
                subject: TermValue::iri("urn:individual"),
                predicate: TYPE.to_owned(),
                object: TermValue::iri("urn:member-class"),
            },
        ],
    )]);

    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    match input.prepare().unwrap() {
        NativeOutcome::Decided(_) => {}
        NativeOutcome::Unsupported(kind) => panic!(
            "class-family output cannot satisfy an unselected intersection member: {kind:?}: {:?}",
            dependency_cycle(&input),
        ),
    }
}

#[test]
fn equivalent_abstract_shapes_reuse_a_plan_without_reusing_concrete_values() {
    let template = template(TYPE);
    let first = source("urn:OtherClassA", TYPE);
    let second = source("urn:OtherClassB", TYPE);
    let left = template
        .input(&first, std::sync::Arc::from([]), &[])
        .unwrap();
    let right = template
        .input(&second, std::sync::Arc::from([]), &[])
        .unwrap();
    assert!(Arc::ptr_eq(&left.template, &right.template));
    assert!(Arc::ptr_eq(&left.effects, &right.effects));
    assert_eq!(
        left.identity(),
        right.identity(),
        "both untracked values occupy Other in this complete abstraction"
    );
    let NativeOutcome::Decided(plan) = left.prepare().unwrap() else {
        panic!("finite plan")
    };
    let NativeOutcome::Decided(result) = plan.materialize_input(&right, None).unwrap() else {
        panic!("same complete abstract shape")
    };
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.object == TermValue::iri("urn:OtherClassB"))
    );
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|row| row.object == TermValue::iri("urn:OtherClassA"))
    );
}

/// The same rules can occupy different strata as native selector values change.
/// Compare execution to conservative all-input admission while checking that the
/// regrouping retains the exact join, schema and witness allocations.
#[test]
fn changing_strata_share_layouts_and_preserve_witnesses_and_absence() {
    let absent = |predicate: &str, object: &str| {
        let mut result = atom("?x", predicate, object);
        result.negated = true;
        result
    };
    let rules = vec![
        EvalRule::positive(
            "urn:layout:copy",
            atom("?x", "urn:layout:copy", "?y"),
            vec![
                atom("?x", "urn:layout:seed", "?y"),
                absent("urn:layout:absent", "urn:value"),
            ],
        ),
        EvalRule::positive(
            "urn:layout:ready",
            atom("?x", "urn:layout:ready", "?y"),
            vec![
                atom("?x", "urn:layout:seed", "?y"),
                absent("urn:layout:blocked", "urn:watched"),
            ],
        ),
    ];
    let producers = vec![ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:layout:witness".to_owned(),
        body: vec![atom("?x", "urn:layout:copy", "?y")],
        head: vec![atom("?x", "urn:layout:witness", "?w")],
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: crate::physical::chase::WitnessPolicy::FrontierSkolem,
    }];
    let properties = vec![
        PreparedPropertyRule::new(PropertyRule {
            rule_iri: "urn:layout:selector".to_owned(),
            head: property(["?x", "urn:layout:blocked", "?class"]),
            body: vec![
                property(["?x", "urn:layout:copy", "?y"]),
                property(["urn:selector", "urn:layout:class", "?class"]),
            ],
            operation: None,
            guards: Vec::new(),
        })
        .unwrap(),
    ];
    let template = Arc::new(
        JointTemplate::new(&rules, &producers, &properties, SemanticVocabulary::Exact).unwrap(),
    );
    let NativeOutcome::Decided(conservative) =
        JointProgram::prepare_with_properties(&rules, &producers, &properties, &Default::default())
            .unwrap()
    else {
        panic!("the full source-independent program is stratifiable")
    };
    let mut plans = Vec::new();
    let mut witnesses = Vec::new();
    for class in ["urn:unwatched", "urn:watched"] {
        let data = BTreeMap::from([(
            WORLD.to_owned(),
            vec![
                Fact {
                    subject: TermValue::iri("urn:subject"),
                    predicate: "urn:layout:seed".to_owned(),
                    object: TermValue::iri("urn:value"),
                },
                Fact {
                    subject: TermValue::iri("urn:selector"),
                    predicate: "urn:layout:class".to_owned(),
                    object: TermValue::iri(class),
                },
            ],
        )]);
        let input = template
            .input(&data, std::sync::Arc::from([]), &[])
            .unwrap();
        let NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
            panic!("each input shape has a valid signed schedule")
        };
        let NativeOutcome::Decided(result) = plan.materialize_input(&input, None).unwrap() else {
            panic!("finite native execution must decide")
        };
        let NativeOutcome::Decided(reference) =
            conservative.materialize_facts(&data, None).unwrap()
        else {
            panic!("conservative execution must decide")
        };
        assert_eq!(
            format!("{:?}", result.result.rows),
            format!("{:?}", reference.result.rows),
        );
        assert_eq!(result.result.status, BudgetStatus::Ok);
        assert_eq!(
            result
                .result
                .rows
                .iter()
                .any(|row| row.predicate == "urn:layout:ready"),
            class == "urn:unwatched",
        );
        for stratum in &plan.strata {
            for producer in &stratum.producers {
                assert!(Arc::ptr_eq(producer, &template.producers[0]));
            }
            for law in &stratum.properties {
                assert!(std::ptr::eq(&law.source, &properties[0].source));
            }
            if let Some(executable) = &stratum.ordinary {
                let selected: std::collections::BTreeSet<_> = executable
                    .stratum_rule_indices(0)
                    .iter()
                    .map(|&index| executable.rule_entry(index).0.head.predicate.clone())
                    .collect();
                assert_eq!(executable.head_predicates(), &selected);
            }
        }
        witnesses.push(result.witness_derivations);
        plans.push(plan);
    }
    assert_ne!(plans[0].strata.len(), plans[1].strata.len());
    assert!(!witnesses[0].is_empty());
    assert_eq!(witnesses[0], witnesses[1]);
    fn entry<'a>(
        program: &'a JointProgram,
        name: &str,
    ) -> (&'a EvalRule, &'a crate::physical::plan::RulePlan) {
        program
            .strata
            .iter()
            .find_map(|stratum| {
                let executable = stratum.ordinary.as_ref()?;
                executable
                    .stratum_rule_indices(0)
                    .iter()
                    .find_map(|&index| {
                        let entry = executable.rule_entry(index);
                        (entry.0.rule_iri == name).then_some(entry)
                    })
            })
            .expect("selected ordinary rule")
    }
    for rule in &rules {
        let first = entry(&plans[0], &rule.rule_iri);
        let second = entry(&plans[1], &rule.rule_iri);
        assert!(std::ptr::eq(first.0, second.0));
        assert!(std::ptr::eq(first.1, second.1));
    }
}

/// The shipped schema laws as one forward joint template.
fn schema_template() -> Arc<JointTemplate> {
    Arc::new(
        JointTemplate::build(
            &[],
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    )
}

/// One minimum restriction with a member, so the minimum-witness law fires.
fn minimum_restriction_rows() -> Vec<Fact> {
    let fact = |s: &str, p: &str, o: TermValue| Fact {
        subject: TermValue::iri(s),
        predicate: p.to_owned(),
        object: o,
    };
    vec![
        fact(
            "urn:restriction",
            "http://www.w3.org/2002/07/owl#onProperty",
            TermValue::iri("urn:property"),
        ),
        fact(
            "urn:restriction",
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            TermValue::simple_literal("1"),
        ),
        fact(
            "urn:restriction",
            "http://www.w3.org/2002/07/owl#onClass",
            TermValue::iri("urn:witness-class"),
        ),
        fact("urn:individual", TYPE, TermValue::iri("urn:restriction")),
    ]
}

/// `rdf:first`/`rdf:rest` rows of one list `prefix:0 .. prefix:(len - 1)`.
fn list_rows(prefix: &str, members: &[String]) -> Vec<Fact> {
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let cell = |index: usize| TermValue::iri(format!("{prefix}:{index}"));
    members
        .iter()
        .enumerate()
        .flat_map(|(index, member)| {
            let next = if index + 1 == members.len() {
                TermValue::iri(NIL)
            } else {
                cell(index + 1)
            };
            [
                Fact {
                    subject: cell(index),
                    predicate: FIRST.to_owned(),
                    object: TermValue::iri(member),
                },
                Fact {
                    subject: cell(index),
                    predicate: REST.to_owned(),
                    object: next,
                },
            ]
        })
        .collect()
}

fn assert_admitted(template: &Arc<JointTemplate>, rows: Vec<Fact>, why: &str) {
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    match input.prepare().unwrap() {
        NativeOutcome::Decided(plan) => {
            assert!(
                plan.admission.admits_native(),
                "{why}: {:?}",
                plan.admission
            )
        }
        NativeOutcome::Unsupported(kind) => panic!("{why}: {kind:?}"),
    }
}

#[test]
fn a_pairwise_list_law_past_the_analysis_bound_is_witnessed_once() {
    // A 600-member disjoint union names 360,000 ordered member pairs, past the 2^18
    // analysis bound. Every variable of the pairwise law is bound by immutable list
    // rows, so each specialization is ground: none carries a witness, all fire on
    // the same predicates, and one represents them.
    let mut rows = minimum_restriction_rows();
    rows.push(Fact {
        subject: TermValue::iri("urn:union"),
        predicate: "http://www.w3.org/2002/07/owl#disjointUnionOf".to_owned(),
        object: TermValue::iri("urn:union-cell:0"),
    });
    let members: Vec<_> = (0..600)
        .map(|index| format!("urn:union-member:{index}"))
        .collect();
    rows.extend(list_rows("urn:union-cell", &members));
    assert_admitted(
        &schema_template(),
        rows,
        "a ground pairwise law cannot exhaust the input certificate",
    );
}

#[test]
fn a_list_operator_reads_only_the_cells_of_its_own_list() {
    // One property chain among 600 unrelated two-member lists. A member cell that
    // could be any cell of any list pairs 1,200 x 1,200 cells (1.44M matches, past
    // the analysis bound); bound to its own list, the chain reads only its own two.
    let mut rows = minimum_restriction_rows();
    rows.push(Fact {
        subject: TermValue::iri("urn:chained"),
        predicate: "http://www.w3.org/2002/07/owl#propertyChainAxiom".to_owned(),
        object: TermValue::iri("urn:chain-cell:0"),
    });
    rows.extend(list_rows(
        "urn:chain-cell",
        &["urn:link-a".to_owned(), "urn:link-b".to_owned()],
    ));
    for index in 0..600 {
        rows.extend(list_rows(
            &format!("urn:unrelated-list:{index}"),
            &[
                format!("urn:unrelated-a:{index}"),
                format!("urn:unrelated-b:{index}"),
            ],
        ));
    }
    assert_admitted(
        &schema_template(),
        rows,
        "unrelated lists cannot multiply a list operator's members",
    );
}

// ── #1795: production-scale witness programs certify through closed relations ──

fn fact_iri(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    }
}

const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";

/// `members` individuals of `class`.
fn population(class: &str, members: usize) -> Vec<Fact> {
    (0..members)
        .map(|index| fact_iri(&format!("urn:member:{index}"), TYPE, class))
        .collect()
}

/// The first, settled-relation pass's certificate. On a production corpus the
/// source-enumerated second pass cannot complete (its 512 value cells and binding
/// bound are exhausted) and this pass decides alone; a small input is enumerated by
/// the second pass, so the decisive first pass is checked directly.
fn settled_admission(template: &Arc<JointTemplate>, rows: Vec<Fact>) -> ChaseAdmission {
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    let Some(Ok(evidence)) = &input.evidence else {
        panic!("no input-specific evidence was observed");
    };
    template
        .termination
        .as_ref()
        .expect("a native termination template")
        .certify_settled(
            evidence,
            input.facts,
            &input.possible,
            &input.contextual_effects,
            template.semantics,
        )
        .unwrap()
        .unwrap()
        .admission
}

/// Both passes and the first pass alone must certify.
fn assert_settled(template: &Arc<JointTemplate>, rows: Vec<Fact>, why: &str) {
    let settled = settled_admission(template, rows.clone());
    assert!(settled.admits_native(), "{why}: {settled:?}");
    assert_admitted(template, rows, why);
}

/// One minimum restriction over a production-sized population.
fn restricted_population() -> Vec<Fact> {
    let mut rows = minimum_restriction_rows();
    rows.extend(population("urn:restriction", 600));
    rows
}

/// The EL calculus (subclass/subproperty closure, type propagation) joined with the
/// shipped schema laws, as the production reasoner composes them.
fn el_schema_template() -> Arc<JointTemplate> {
    Arc::new(
        JointTemplate::build(
            &crate::reason::el::structured_el_rules(),
            &[],
            crate::reason::schema_laws(),
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            super::super::JointOperation::Forward,
        )
        .unwrap(),
    )
}

fn admission(template: &Arc<JointTemplate>, rows: Vec<Fact>) -> ChaseAdmission {
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    match input.prepare().unwrap() {
        NativeOutcome::Decided(plan) => plan.admission.clone(),
        NativeOutcome::Unsupported(kind) => panic!("unsupported: {kind:?}"),
    }
}

/// Neither the first pass alone nor the whole input-specific admission certifies.
fn assert_refused(template: &Arc<JointTemplate>, rows: Vec<Fact>, why: &str) {
    let settled = settled_admission(template, rows.clone());
    assert!(!settled.admits_native(), "{why}: {settled:?}");
    let admission = admission(template, rows);
    assert!(!admission.admits_native(), "{why}: {admission:?}");
}

#[test]
fn a_witness_restriction_with_an_unrelated_transitive_property_certifies() {
    // The transitive law takes its predicate from `instanceOf TransitiveProperty`
    // markers, which `instanceOf` writers could in principle produce. No writer can
    // produce THAT marker here, so the pair is closed, the law binds `urn:chain`, and
    // the restriction's witness stays in its own binary relations.
    let mut rows = restricted_population();
    rows.push(fact_iri("urn:chain", TYPE, TRANSITIVE));
    rows.push(fact_iri("urn:a", "urn:chain", "urn:b"));
    rows.push(fact_iri("urn:b", "urn:chain", "urn:c"));
    assert_settled(
        &schema_template(),
        rows,
        "an unrelated transitive property cannot refuse a terminating witness",
    );
}

#[test]
fn a_witness_restriction_with_an_unrelated_subproperty_axiom_certifies() {
    // `subPropertyOf` is written (by equivalence and by its own EL transitivity),
    // but only by witness-free laws over settled relations: its closure is exact,
    // so property propagation binds both of its predicates.
    let mut rows = restricted_population();
    rows.push(fact_iri("urn:narrow", SUBPROPERTY, "urn:broad"));
    rows.push(fact_iri("urn:broad", SUBPROPERTY, "urn:broadest"));
    rows.push(fact_iri("urn:a", "urn:narrow", "urn:b"));
    assert_settled(
        &el_schema_template(),
        rows,
        "an unrelated subproperty axiom cannot refuse a terminating witness",
    );
}

#[test]
fn type_propagation_over_real_subclass_rows_certifies() {
    // The witness class has superclasses, and the EL calculus propagates its type
    // along the subclass closure. None reaches the restriction, so the program
    // terminates; the closure binds both classes of the propagation law.
    let mut rows = restricted_population();
    rows.push(fact_iri("urn:witness-class", SUBCLASS, "urn:middle-class"));
    rows.push(fact_iri("urn:middle-class", SUBCLASS, "urn:top-class"));
    assert_settled(
        &el_schema_template(),
        rows,
        "type propagation over unrelated superclasses cannot refuse a terminating witness",
    );
}

#[test]
fn type_propagation_into_the_restriction_is_still_refused() {
    // `witness-class ⊑ middle-class ⊑ restriction`: every witness is itself
    // restricted and mints another witness forever. Only the subclass CLOSURE
    // connects the witness class to the restriction, so it must be exact.
    let mut rows = restricted_population();
    rows.push(fact_iri("urn:witness-class", SUBCLASS, "urn:middle-class"));
    rows.push(fact_iri("urn:middle-class", SUBCLASS, "urn:restriction"));
    assert_refused(
        &el_schema_template(),
        rows,
        "a subclass path back into the restriction must be refused",
    );
}

#[test]
fn a_range_back_into_its_own_restriction_is_still_refused() {
    // `p rdfs:range R`, `R ⊑ ≥1 p`: each witness is typed `R` by the range and
    // mints another `p` witness forever.
    let mut rows = restricted_population();
    rows.push(fact_iri("urn:property", RANGE, "urn:restriction"));
    assert_refused(
        &schema_template(),
        rows,
        "a genuinely cyclic range restriction must be refused",
    );
}

#[test]
fn an_inverse_pair_cycle_stays_refused_by_the_skolem_certificate() {
    // Finance shape: `A ⊑ ≥2 p.B`, `B ⊑ ∃q.A`, `q inverseOf p`. The restricted chase
    // terminates (the inverse edge already satisfies `B`'s restriction), but every
    // rung bounds the Skolem chase, which does not: this stays refused (#1796).
    let fact = |s: &str, p: &str, o: TermValue| Fact {
        subject: TermValue::iri(s),
        predicate: p.to_owned(),
        object: o,
    };
    let owl = |local: &str| format!("http://www.w3.org/2002/07/owl#{local}");
    let mut rows = vec![
        fact("urn:a-min", &owl("onProperty"), TermValue::iri("urn:p")),
        fact(
            "urn:a-min",
            &owl("minQualifiedCardinality"),
            TermValue::simple_literal("2"),
        ),
        fact("urn:a-min", &owl("onClass"), TermValue::iri("urn:B")),
        fact_iri("urn:A", SUBCLASS, "urn:a-min"),
        fact("urn:b-some", &owl("onProperty"), TermValue::iri("urn:q")),
        fact(
            "urn:b-some",
            &owl("someValuesFrom"),
            TermValue::iri("urn:A"),
        ),
        fact_iri("urn:B", SUBCLASS, "urn:b-some"),
        fact_iri("urn:q", INVERSE, "urn:p"),
        fact_iri("urn:p", TYPE, &owl("ObjectProperty")),
        fact_iri("urn:q", TYPE, &owl("ObjectProperty")),
    ];
    rows.extend(population("urn:A", 600));
    assert_refused(
        &el_schema_template(),
        rows,
        "the inverse-pair existential cycle is #1796's restricted-chase question",
    );
}
