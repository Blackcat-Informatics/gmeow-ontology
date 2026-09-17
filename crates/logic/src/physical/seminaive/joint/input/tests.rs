// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete-input scheduling and native dimensional feedback, without a corpus.

use super::*;
use crate::physical::{PropertyAtom, PropertyRule};
use crate::query_ir::{QBuiltin, QTerm};
use crate::seam::BudgetStatus;
use purrdf::TermValue;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
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
