// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native positive DL/schema laws whose predicate positions may be data-bound.
//! These are real joins in the shared fixed point, including their schema premises.
//! `GroundedLogicV1` interprets their fixed presentation spellings through the shared
//! canonical operator keys. It does not rename stored facts or variable bindings;
//! every schema premise retains its actual source spelling and asserting world.

use crate::physical::{
    CardinalityPattern, CardinalitySet, DatatypeConstraint, ListOperation, ListPattern,
    MinimumPattern, PreparedPropertyRule, PropertyAtom, PropertyGuard, PropertyOperation,
    PropertyRule, ValueComparison,
};
use crate::rule_ir::EvalTerm;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const DISJOINT_CLASS: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
const SOME: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const ALL: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const SAME: &str = "http://www.w3.org/2002/07/owl#sameAs";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

fn atom(s: &str, p: &str, o: &str) -> PropertyAtom {
    let term = |value: &str| {
        if value.starts_with('?') {
            EvalTerm::var(value)
        } else {
            EvalTerm::named(value)
        }
    };
    PropertyAtom([term(s), term(p), term(o)])
}

fn law(name: &str, head: PropertyAtom, body: Vec<PropertyAtom>) -> PreparedPropertyRule {
    PreparedPropertyRule::new(PropertyRule {
        rule_iri: name.to_owned(),
        head,
        body,
        operation: None,
        guards: Vec::new(),
    })
    .expect("fixed native schema laws are range-restricted")
}

fn list_law(
    name: &str,
    head: PropertyAtom,
    body: Vec<PropertyAtom>,
    operation: ListOperation,
) -> PreparedPropertyRule {
    PreparedPropertyRule::new(PropertyRule {
        rule_iri: name.to_owned(),
        head,
        body,
        operation: Some(PropertyOperation::List(ListPattern {
            head: EvalTerm::var("?list"),
            operation,
        })),
        guards: Vec::new(),
    })
    .expect("fixed native list laws are range-restricted")
}

fn guarded_law(
    name: &str,
    head: PropertyAtom,
    body: Vec<PropertyAtom>,
    comparison: ValueComparison,
    left: &str,
    right: &str,
) -> PreparedPropertyRule {
    PreparedPropertyRule::new(PropertyRule {
        rule_iri: name.to_owned(),
        head,
        body,
        operation: None,
        guards: vec![PropertyGuard {
            comparison,
            left: EvalTerm::var(left),
            right: EvalTerm::var(right),
        }],
    })
    .expect("fixed guarded schema laws bind every compared value")
}

/// One immutable preparation for all native selected programs and datasets.
pub(crate) fn laws() -> &'static [PreparedPropertyRule] {
    static RULES: std::sync::LazyLock<Vec<PreparedPropertyRule>> = std::sync::LazyLock::new(|| {
        let mut rules = vec![
            list_law(
                "dl:union-member",
                atom(
                    "?member",
                    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                    "?class",
                ),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#unionOf",
                    "?list",
                )],
                ListOperation::Member(EvalTerm::var("?member")),
            ),
            list_law(
                "dl:intersection-member",
                atom(
                    "?class",
                    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                    "?member",
                ),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#intersectionOf",
                    "?list",
                )],
                ListOperation::Member(EvalTerm::var("?member")),
            ),
            list_law(
                "dl:intersection-membership",
                atom("?s", RDF_TYPE, "?class"),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#intersectionOf",
                    "?list",
                )],
                ListOperation::AllTypes(EvalTerm::var("?s")),
            ),
            // The empty intersection is the universal resource class. Nonempty
            // intersections use the typed candidate join above; only this exact nil
            // case needs to enumerate resources appearing in ordinary statement roles.
            law(
                "dl:empty-intersection-subject",
                atom("?s", RDF_TYPE, "?class"),
                vec![
                    atom(
                        "?class",
                        "http://www.w3.org/2002/07/owl#intersectionOf",
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
                    ),
                    atom("?s", "?p", "?o"),
                ],
            ),
            law(
                "dl:empty-intersection-object",
                atom("?o", RDF_TYPE, "?class"),
                vec![
                    atom(
                        "?class",
                        "http://www.w3.org/2002/07/owl#intersectionOf",
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
                    ),
                    atom("?s", "?p", "?o"),
                ],
            ),
            law(
                "dl:empty-intersection-predicate",
                atom("?p", RDF_TYPE, "?class"),
                vec![
                    atom(
                        "?class",
                        "http://www.w3.org/2002/07/owl#intersectionOf",
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
                    ),
                    atom("?s", "?p", "?o"),
                ],
            ),
            list_law(
                "dl:oneOf-closure-clash",
                atom("?s", RDF_TYPE, "http://www.w3.org/2002/07/owl#Nothing"),
                vec![
                    atom("?class", "http://www.w3.org/2002/07/owl#oneOf", "?list"),
                    atom("?s", RDF_TYPE, "?class"),
                ],
                ListOperation::DistinctFromAll(EvalTerm::var("?s")),
            ),
            list_law(
                "dl:disjointUnion-member",
                atom(
                    "?member",
                    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                    "?class",
                ),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#disjointUnionOf",
                    "?list",
                )],
                ListOperation::Member(EvalTerm::var("?member")),
            ),
            list_law(
                "dl:oneOf-member",
                atom("?member", RDF_TYPE, "?class"),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#oneOf",
                    "?list",
                )],
                ListOperation::Member(EvalTerm::var("?member")),
            ),
            list_law(
                "dl:disjointUnion-disjoint",
                atom(
                    "?left",
                    "http://www.w3.org/2002/07/owl#disjointWith",
                    "?right",
                ),
                vec![atom(
                    "?class",
                    "http://www.w3.org/2002/07/owl#disjointUnionOf",
                    "?list",
                )],
                ListOperation::Pair(EvalTerm::var("?left"), EvalTerm::var("?right")),
            ),
            list_law(
                "dl:allDisjointClasses-pairwise",
                atom(
                    "?left",
                    "http://www.w3.org/2002/07/owl#disjointWith",
                    "?right",
                ),
                vec![
                    atom(
                        "?axiom",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#AllDisjointClasses",
                    ),
                    atom("?axiom", "http://www.w3.org/2002/07/owl#members", "?list"),
                ],
                ListOperation::Pair(EvalTerm::var("?left"), EvalTerm::var("?right")),
            ),
            list_law(
                "dl:allDisjointProperties-pairwise",
                atom(
                    "?left",
                    "http://www.w3.org/2002/07/owl#propertyDisjointWith",
                    "?right",
                ),
                vec![
                    atom(
                        "?axiom",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#AllDisjointProperties",
                    ),
                    atom("?axiom", "http://www.w3.org/2002/07/owl#members", "?list"),
                ],
                ListOperation::Pair(EvalTerm::var("?left"), EvalTerm::var("?right")),
            ),
            list_law(
                "dl:allDifferent-pairwise",
                atom(
                    "?left",
                    "http://www.w3.org/2002/07/owl#differentFrom",
                    "?right",
                ),
                vec![
                    atom(
                        "?axiom",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#AllDifferent",
                    ),
                    atom("?axiom", "http://www.w3.org/2002/07/owl#members", "?list"),
                ],
                ListOperation::Pair(EvalTerm::var("?left"), EvalTerm::var("?right")),
            ),
            list_law(
                "dl:allDifferent-pairwise",
                atom(
                    "?left",
                    "http://www.w3.org/2002/07/owl#differentFrom",
                    "?right",
                ),
                vec![
                    atom(
                        "?axiom",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#AllDifferent",
                    ),
                    atom(
                        "?axiom",
                        "http://www.w3.org/2002/07/owl#distinctMembers",
                        "?list",
                    ),
                ],
                ListOperation::Pair(EvalTerm::var("?left"), EvalTerm::var("?right")),
            ),
            list_law(
                "dl:property-chain",
                atom("?s", "?p", "?o"),
                vec![atom(
                    "?p",
                    "http://www.w3.org/2002/07/owl#propertyChainAxiom",
                    "?list",
                )],
                ListOperation::Chain(EvalTerm::var("?s"), EvalTerm::var("?o")),
            ),
            law(
                "dl:complement-disjoint",
                atom("?c", "http://www.w3.org/2002/07/owl#disjointWith", "?d"),
                vec![atom(
                    "?c",
                    "http://www.w3.org/2002/07/owl#complementOf",
                    "?d",
                )],
            ),
            law(
                "dl:complement-disjoint",
                atom("?d", "http://www.w3.org/2002/07/owl#disjointWith", "?c"),
                vec![atom(
                    "?c",
                    "http://www.w3.org/2002/07/owl#complementOf",
                    "?d",
                )],
            ),
            law(
                "dl:equivalentProperty-subproperty",
                atom("?p", SUBPROPERTY, "?q"),
                vec![atom(
                    "?p",
                    "http://www.w3.org/2002/07/owl#equivalentProperty",
                    "?q",
                )],
            ),
            law(
                "dl:equivalentProperty-subproperty",
                atom("?q", SUBPROPERTY, "?p"),
                vec![atom(
                    "?p",
                    "http://www.w3.org/2002/07/owl#equivalentProperty",
                    "?q",
                )],
            ),
            law(
                "dl:subPropertyOf-propagation",
                atom("?s", "?q", "?o"),
                vec![atom("?p", SUBPROPERTY, "?q"), atom("?s", "?p", "?o")],
            ),
            law(
                "dl:domain",
                atom("?s", RDF_TYPE, "?c"),
                vec![atom("?p", DOMAIN, "?c"), atom("?s", "?p", "?o")],
            ),
            law(
                "dl:range",
                atom("?o", RDF_TYPE, "?c"),
                vec![atom("?p", RANGE, "?c"), atom("?s", "?p", "?o")],
            ),
            law(
                "dl:symmetric-property",
                atom("?o", "?p", "?s"),
                vec![
                    atom(
                        "?p",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#SymmetricProperty",
                    ),
                    atom("?s", "?p", "?o"),
                ],
            ),
            law(
                "dl:inverseOf",
                atom("?o", "?q", "?s"),
                vec![atom("?p", INVERSE, "?q"), atom("?s", "?p", "?o")],
            ),
            law(
                "dl:inverseOf-symmetry",
                atom("?q", INVERSE, "?p"),
                vec![atom("?p", INVERSE, "?q")],
            ),
            law(
                "dl:transitive-property",
                atom("?s", "?p", "?o"),
                vec![
                    atom(
                        "?p",
                        RDF_TYPE,
                        "http://www.w3.org/2002/07/owl#TransitiveProperty",
                    ),
                    atom("?s", "?p", "?m"),
                    atom("?m", "?p", "?o"),
                ],
            ),
            law(
                "dl:someValuesFrom",
                atom("?s", RDF_TYPE, "?r"),
                vec![
                    atom("?r", ON_PROPERTY, "?p"),
                    atom("?r", SOME, "?c"),
                    atom("?s", "?p", "?o"),
                    atom("?o", RDF_TYPE, "?c"),
                ],
            ),
            law(
                "dl:allValuesFrom",
                atom("?o", RDF_TYPE, "?c"),
                vec![
                    atom("?r", ON_PROPERTY, "?p"),
                    atom("?r", ALL, "?c"),
                    atom("?s", RDF_TYPE, "?r"),
                    atom("?s", "?p", "?o"),
                ],
            ),
            law(
                "dl:hasValue-assertion",
                atom("?s", "?p", "?v"),
                vec![
                    atom("?r", ON_PROPERTY, "?p"),
                    atom("?r", HAS_VALUE, "?v"),
                    atom("?s", RDF_TYPE, "?r"),
                ],
            ),
            law(
                "dl:hasValue-membership",
                atom("?s", RDF_TYPE, "?r"),
                vec![
                    atom("?r", ON_PROPERTY, "?p"),
                    atom("?r", HAS_VALUE, "?v"),
                    atom("?s", "?p", "?v"),
                ],
            ),
        ];
        rules.extend(universal_class_laws());
        rules.extend(empty_class_laws());
        rules.extend(bottom_property_laws());
        rules.extend(equality_laws());
        rules.extend(self_laws());
        rules.extend(clash_laws());
        rules.extend(key_laws());
        rules.extend(cardinality_laws());
        rules.extend(minimum_laws());
        rules.extend(datatype_laws());
        rules
    });
    &RULES
}

/// The universal class has two declared source spellings. A fixed membership
/// marker may recognize either, but a bound class value and a subclass subject
/// retain their exact terms. Match those source subjects explicitly instead of
/// rewriting the class binding or manufacturing an alias statement.
fn universal_class_laws() -> Vec<PreparedPropertyRule> {
    [
        "https://blackcatinformatics.ca/logic/Thing",
        "http://www.w3.org/2002/07/owl#Thing",
    ]
    .into_iter()
    .map(|thing| {
        law(
            "dl:universal-class-subclass",
            atom("?member", RDF_TYPE, "?class"),
            vec![
                atom("?member", RDF_TYPE, "http://www.w3.org/2002/07/owl#Thing"),
                atom(thing, SUBCLASS, "?class"),
            ],
        )
    })
    .collect()
}

/// Positive emptiness consequences retain the actual restriction/class evidence.
/// No reflexive subclass axiom or populated individual is invented to recognize
/// an empty class. Membership, when present, follows in the same joint fixed point.
fn empty_class_laws() -> Vec<PreparedPropertyRule> {
    let mut rules = vec![law(
        "dl:self-disjoint-empty-class",
        atom("?class", SUBCLASS, NOTHING),
        vec![atom("?class", DISJOINT_CLASS, "?class")],
    )];
    for disjoint in [
        atom("?class", DISJOINT_CLASS, "?super"),
        atom("?super", DISJOINT_CLASS, "?class"),
    ] {
        rules.push(law(
            "dl:disjoint-superclass-empty-class",
            atom("?class", SUBCLASS, NOTHING),
            vec![atom("?class", SUBCLASS, "?super"), disjoint],
        ));
    }
    for direct_nothing in [false, true] {
        let filler = if direct_nothing { NOTHING } else { "?filler" };
        let empty_filler = || (!direct_nothing).then(|| atom("?filler", SUBCLASS, NOTHING));
        let mut some_body = vec![atom("?r", ON_PROPERTY, "?p"), atom("?r", SOME, filler)];
        some_body.extend(empty_filler());
        rules.push(law(
            "dl:someValuesFrom-unsat-filler",
            atom("?r", SUBCLASS, NOTHING),
            some_body,
        ));
        for predicate in [
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            "http://www.w3.org/2002/07/owl#qualifiedCardinality",
        ] {
            let mut body = vec![
                atom("?r", ON_PROPERTY, "?p"),
                atom("?r", predicate, "?count"),
                atom("?r", "http://www.w3.org/2002/07/owl#onClass", filler),
            ];
            body.extend(empty_filler());
            rules.push(
                PreparedPropertyRule::new(PropertyRule {
                    rule_iri: "dl:min-cardinality-unsat-filler".to_owned(),
                    head: atom("?r", SUBCLASS, NOTHING),
                    body,
                    operation: None,
                    guards: vec![PropertyGuard {
                        comparison: ValueComparison::CardinalityGreater,
                        left: EvalTerm::var("?count"),
                        right: EvalTerm::ConstLit(purrdf::TermValue::simple_literal("0")),
                    }],
                })
                .expect("empty qualified restrictions bind their source count and class"),
            );
        }
    }
    rules
}

/// An explicitly selected empty property cannot carry any value. Restriction
/// emptiness follows from its actual defining fields even before it is populated;
/// ordinary membership then propagates the contradiction in the joint closure.
fn bottom_property_laws() -> Vec<PreparedPropertyRule> {
    let mut rules = Vec::new();
    for property in [
        "http://www.w3.org/2002/07/owl#bottomObjectProperty",
        "http://www.w3.org/2002/07/owl#bottomDataProperty",
    ] {
        rules.push(law(
            "dl:bottom-property-clash",
            atom("?s", RDF_TYPE, NOTHING),
            vec![atom("?s", property, "?value")],
        ));
        for predicate in [SOME, HAS_VALUE] {
            rules.push(law(
                "dl:bottom-property-empty-class",
                atom("?r", SUBCLASS, NOTHING),
                vec![
                    atom("?r", ON_PROPERTY, property),
                    atom("?r", predicate, "?value"),
                ],
            ));
        }
        for (predicate, qualifier) in [
            ("http://www.w3.org/2002/07/owl#minCardinality", None),
            ("http://www.w3.org/2002/07/owl#cardinality", None),
            (
                "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
                Some("http://www.w3.org/2002/07/owl#onClass"),
            ),
            (
                "http://www.w3.org/2002/07/owl#qualifiedCardinality",
                Some("http://www.w3.org/2002/07/owl#onClass"),
            ),
            (
                "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
                Some("http://www.w3.org/2002/07/owl#onDataRange"),
            ),
            (
                "http://www.w3.org/2002/07/owl#qualifiedCardinality",
                Some("http://www.w3.org/2002/07/owl#onDataRange"),
            ),
        ] {
            let mut body = vec![
                atom("?r", ON_PROPERTY, property),
                atom("?r", predicate, "?count"),
            ];
            if let Some(qualifier) = qualifier {
                body.push(atom("?r", qualifier, "?qualifier"));
            }
            rules.push(
                PreparedPropertyRule::new(PropertyRule {
                    rule_iri: "dl:bottom-property-empty-class".to_owned(),
                    head: atom("?r", SUBCLASS, NOTHING),
                    body,
                    operation: None,
                    guards: vec![PropertyGuard {
                        comparison: ValueComparison::CardinalityGreater,
                        left: EvalTerm::var("?count"),
                        right: EvalTerm::ConstLit(purrdf::TermValue::simple_literal("0")),
                    }],
                })
                .expect("empty property restrictions bind each source field and count"),
            );
        }
    }
    rules
}

/// Both canonical characteristic records and their direct type marker carry
/// the same law; source statements remain separate proof premises.
fn characteristic_schemas(inverse: bool) -> [Vec<PropertyAtom>; 2] {
    let (marker, characteristic) = if inverse {
        (
            "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
            "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
        )
    } else {
        (
            FUNCTIONAL,
            "https://blackcatinformatics.ca/logic/functionalProperty",
        )
    };
    [
        vec![atom("?p", RDF_TYPE, marker)],
        vec![
            atom(
                "?record",
                "https://blackcatinformatics.ca/logic/characterizes",
                "?p",
            ),
            atom(
                "?record",
                "https://blackcatinformatics.ca/logic/characteristicSort",
                characteristic,
            ),
        ],
    ]
}

fn guard(comparison: ValueComparison, left: &str, right: &str) -> PropertyGuard {
    PropertyGuard {
        comparison,
        left: EvalTerm::var(left),
        right: EvalTerm::var(right),
    }
}

fn equality_law(
    name: &str,
    body: Vec<PropertyAtom>,
    mut guards: Vec<PropertyGuard>,
) -> PreparedPropertyRule {
    guards.insert(
        0,
        guard(ValueComparison::DifferentResourceTerms, "?left", "?right"),
    );
    PreparedPropertyRule::new(PropertyRule {
        rule_iri: name.to_owned(),
        head: atom("?left", SAME, "?right"),
        body,
        operation: None,
        guards,
    })
    .expect("fixed equality laws bind both resource terms and every guard")
}

/// Deterministic equality consequences join authored rules in the same world.
/// No representative replaces an asserted term. Bounds above one require a
/// disjunction of possible equalities and cannot select an arbitrary pair.
fn equality_laws() -> Vec<PreparedPropertyRule> {
    let mut rules = Vec::new();
    for inverse in [false, true] {
        for mut body in characteristic_schemas(inverse) {
            let (name, guards) = if inverse {
                body.extend([atom("?left", "?p", "?a"), atom("?right", "?p", "?b")]);
                (
                    "dl:inverse-functional-equality",
                    vec![guard(ValueComparison::Equal, "?a", "?b")],
                )
            } else {
                body.extend([atom("?s", "?p", "?left"), atom("?s", "?p", "?right")]);
                ("dl:functional-equality", Vec::new())
            };
            rules.push(equality_law(name, body, guards));
        }
    }
    for (predicate, qualified) in [
        ("http://www.w3.org/2002/07/owl#maxCardinality", false),
        ("http://www.w3.org/2002/07/owl#cardinality", false),
        (
            "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
            true,
        ),
        ("http://www.w3.org/2002/07/owl#qualifiedCardinality", true),
    ] {
        for universal in [false, true] {
            if !qualified && universal {
                continue;
            }
            let mut body = vec![
                atom("?r", predicate, "?count"),
                atom("?r", ON_PROPERTY, "?p"),
                atom("?s", RDF_TYPE, "?r"),
                atom("?s", "?p", "?left"),
                atom("?s", "?p", "?right"),
            ];
            if qualified {
                body.push(atom(
                    "?r",
                    "http://www.w3.org/2002/07/owl#onClass",
                    if universal {
                        "http://www.w3.org/2002/07/owl#Thing"
                    } else {
                        "?class"
                    },
                ));
                if !universal {
                    body.extend([
                        atom("?left", RDF_TYPE, "?class"),
                        atom("?right", RDF_TYPE, "?class"),
                    ]);
                }
            }
            rules.push(equality_law(
                "dl:maximum-one-equality",
                body,
                vec![PropertyGuard {
                    comparison: ValueComparison::CardinalityEqual,
                    left: EvalTerm::var("?count"),
                    right: EvalTerm::ConstLit(purrdf::TermValue::simple_literal("1")),
                }],
            ));
        }
    }
    rules
}

/// A true local self restriction entails its edge, and a self edge entails
/// membership. The native boolean interpretation accepts both XSD true spellings;
/// a string or a false flag never silently enables local reflexivity.
fn self_laws() -> Vec<PreparedPropertyRule> {
    [false, true]
        .into_iter()
        .map(|membership| {
            let edge = atom("?s", "?p", "?s");
            let member = atom("?s", RDF_TYPE, "?r");
            PreparedPropertyRule::new(PropertyRule {
                rule_iri: if membership {
                    "dl:hasSelf-membership"
                } else {
                    "dl:hasSelf-assertion"
                }
                .to_owned(),
                head: if membership {
                    member.clone()
                } else {
                    edge.clone()
                },
                body: vec![
                    atom("?r", "http://www.w3.org/2002/07/owl#hasSelf", "?flag"),
                    atom("?r", ON_PROPERTY, "?p"),
                    if membership { edge } else { member },
                ],
                operation: None,
                guards: vec![PropertyGuard {
                    comparison: ValueComparison::Equal,
                    left: EvalTerm::var("?flag"),
                    right: EvalTerm::ConstLit(purrdf::TermValue::Literal {
                        lexical_form: "true".to_owned(),
                        datatype: "http://www.w3.org/2001/XMLSchema#boolean".to_owned(),
                        language: None,
                        direction: None,
                    }),
                }],
            })
            .expect("fixed self laws bind their property and truth-valued flag")
        })
        .collect()
}

/// Object witnesses participate in the same fixed point as their authored
/// consumers. `onClass` selects the object-valued qualified fragment; an
/// unqualified count or existential requires the explicit object-property marker.
/// A datatype obligation never acquires a resource-valued surrogate here.
fn minimum_laws() -> Vec<PreparedPropertyRule> {
    const OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
    const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
    let mut rules = Vec::new();
    for (predicate, qualified) in [
        ("http://www.w3.org/2002/07/owl#minCardinality", false),
        ("http://www.w3.org/2002/07/owl#cardinality", false),
        (
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            true,
        ),
        ("http://www.w3.org/2002/07/owl#qualifiedCardinality", true),
        (SOME, false),
    ] {
        let some = predicate == SOME;
        let mut body = vec![
            atom("?r", ON_PROPERTY, "?p"),
            atom("?r", predicate, if some { "?class" } else { "?count" }),
            atom("?s", RDF_TYPE, "?r"),
        ];
        if qualified {
            body.push(atom("?r", ON_CLASS, "?class"));
        } else {
            body.push(atom("?p", RDF_TYPE, OBJECT_PROPERTY));
        }
        rules.push(
            PreparedPropertyRule::new(PropertyRule {
                rule_iri: format!("dl:minimum-witness:{predicate}"),
                head: atom("?s", "?p", "?witness"),
                body,
                operation: Some(PropertyOperation::Minimum(MinimumPattern {
                    subject: EvalTerm::var("?s"),
                    property: EvalTerm::var("?p"),
                    minimum: if some {
                        EvalTerm::ConstLit(purrdf::TermValue::simple_literal("1"))
                    } else {
                        EvalTerm::var("?count")
                    },
                    class: (some || qualified).then(|| EvalTerm::var("?class")),
                    witness: "?witness".to_owned(),
                })),
                guards: Vec::new(),
            })
            .expect("fixed minimum laws bind every request and declare their invented slot"),
        );
    }
    rules
}

fn datatype_law(
    name: &str,
    body: Vec<PropertyAtom>,
    constraint: DatatypeConstraint,
) -> PreparedPropertyRule {
    PreparedPropertyRule::new(PropertyRule {
        rule_iri: name.to_owned(),
        head: atom("?s", RDF_TYPE, NOTHING),
        body,
        operation: Some(PropertyOperation::Datatype(constraint)),
        guards: Vec::new(),
    })
    .expect("fixed datatype laws bind every value, expression and count")
}

/// Positive datatype contradictions share the authored fixed point. Each universal
/// range applies independently; an existential filler never constrains unrelated
/// existing values. One insufficient range already proves a lower-bound clash.
fn datatype_laws() -> Vec<PreparedPropertyRule> {
    const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
    let declared = || atom("?p", RDF_TYPE, DATATYPE_PROPERTY);
    let instance = || atom("?s", RDF_TYPE, "?restriction");
    let mut rules = vec![datatype_law(
        "dl:datatype-property-literal-clash",
        vec![declared(), atom("?s", "?p", "?value")],
        DatatypeConstraint::NonLiteral {
            value: EvalTerm::var("?value"),
        },
    )];
    // Bind schema selectors before scanning instance/value relations. Every
    // premise is retained; the small schema joins make the data probes indexed.
    for local in [false, true] {
        let mut body = if local {
            vec![
                atom("?restriction", ALL, "?datatype"),
                atom("?restriction", ON_PROPERTY, "?p"),
                declared(),
                instance(),
            ]
        } else {
            vec![atom("?p", RANGE, "?datatype"), declared()]
        };
        body.push(atom("?s", "?p", "?value"));
        rules.push(datatype_law(
            "dl:datatype-membership-clash",
            body,
            DatatypeConstraint::Outside {
                datatype: EvalTerm::var("?datatype"),
                value: EvalTerm::var("?value"),
            },
        ));
    }

    let one = EvalTerm::ConstLit(purrdf::TermValue::simple_literal("1"));
    let some = vec![
        atom("?restriction", SOME, "?filler"),
        atom("?restriction", ON_PROPERTY, "?p"),
    ];
    let mut direct_some = some.clone();
    direct_some.push(declared());
    direct_some.push(instance());
    rules.push(datatype_law(
        "dl:datatype-capacity-clash",
        direct_some,
        DatatypeConstraint::Insufficient {
            datatype: EvalTerm::var("?filler"),
            minimum: one.clone(),
        },
    ));
    let mut requests = vec![(some, one)];
    for (predicate, qualified) in [
        ("http://www.w3.org/2002/07/owl#minCardinality", false),
        ("http://www.w3.org/2002/07/owl#cardinality", false),
        (
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            true,
        ),
        ("http://www.w3.org/2002/07/owl#qualifiedCardinality", true),
    ] {
        let mut body = vec![atom("?restriction", predicate, "?minimum")];
        if qualified {
            body.push(atom("?restriction", ON_DATA_RANGE, "?filler"));
        }
        body.push(atom("?restriction", ON_PROPERTY, "?p"));
        if qualified {
            let mut direct = body.clone();
            direct.push(instance());
            rules.push(datatype_law(
                "dl:datatype-capacity-clash",
                direct,
                DatatypeConstraint::Insufficient {
                    datatype: EvalTerm::var("?filler"),
                    minimum: EvalTerm::var("?minimum"),
                },
            ));
        }
        requests.push((body, EvalTerm::var("?minimum")));
    }
    for (request, minimum) in requests {
        for local in [false, true] {
            let mut body = request.clone();
            body.push(declared());
            if local {
                body.extend([
                    atom("?universal", ON_PROPERTY, "?p"),
                    atom("?universal", ALL, "?datatype"),
                    atom("?s", RDF_TYPE, "?universal"),
                ]);
            } else {
                body.push(atom("?p", RANGE, "?datatype"));
            }
            body.push(instance());
            rules.push(datatype_law(
                "dl:datatype-capacity-clash",
                body,
                DatatypeConstraint::Insufficient {
                    datatype: EvalTerm::var("?datatype"),
                    minimum: minimum.clone(),
                },
            ));
        }
    }
    rules
}

/// Key disagreement is witnessed by explicit distinctness, never resource names.
/// The canonical record's finite conjunction is read only after its definition
/// producers complete; RDF list keys use the shared complete-list admission.
fn key_laws() -> Vec<PreparedPropertyRule> {
    const THING: &str = "http://www.w3.org/2002/07/owl#Thing";
    const HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
    const KEY_ASSERTION: &str = "https://blackcatinformatics.ca/logic/KeyAssertion";
    const KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
    let mut rules = Vec::new();
    for canonical in [false, true] {
        for universal in [false, true] {
            let class = if universal { THING } else { "?class" };
            let mut body = if canonical {
                vec![
                    atom("?list", RDF_TYPE, KEY_ASSERTION),
                    atom("?list", KEY_CLASS, class),
                ]
            } else {
                vec![atom(class, HAS_KEY, "?list")]
            };
            body.push(atom("?s", DIFFERENT, "?other"));
            if !universal {
                body.extend([atom("?s", RDF_TYPE, class), atom("?other", RDF_TYPE, class)]);
            }
            let operation = if canonical {
                ListOperation::KeyRecordValues(EvalTerm::var("?s"), EvalTerm::var("?other"))
            } else {
                ListOperation::KeyValues(EvalTerm::var("?s"), EvalTerm::var("?other"))
            };
            rules.push(list_law(
                "dl:has-key-clash",
                atom("?s", RDF_TYPE, NOTHING),
                body,
                operation,
            ));
        }
    }
    rules
}

/// Positive maximum violations. Qualified bounds always require their explicit
/// qualifier; a missing onClass/onDataRange never becomes an unqualified count.
/// Each bound assertion contributes its own conjunction, without last-write wins.
fn cardinality_laws() -> Vec<PreparedPropertyRule> {
    let mut laws = Vec::new();
    for predicate in [
        "http://www.w3.org/2002/07/owl#maxCardinality",
        "http://www.w3.org/2002/07/owl#cardinality",
        "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
        "http://www.w3.org/2002/07/owl#qualifiedCardinality",
    ] {
        let qualifiers =
            if predicate.contains("Qualified") || predicate.ends_with("#qualifiedCardinality") {
                vec![
                    (
                        Some(("http://www.w3.org/2002/07/owl#onClass", "?class")),
                        CardinalitySet::Class(EvalTerm::var("?class")),
                    ),
                    (
                        Some((
                            "http://www.w3.org/2002/07/owl#onClass",
                            "http://www.w3.org/2002/07/owl#Thing",
                        )),
                        CardinalitySet::Resources,
                    ),
                    (
                        Some(("http://www.w3.org/2002/07/owl#onDataRange", "?datatype")),
                        CardinalitySet::Datatype(EvalTerm::var("?datatype")),
                    ),
                ]
            } else {
                vec![(None, CardinalitySet::AllValues)]
            };
        for (qualifier, set) in qualifiers {
            let mut body = vec![
                atom("?s", RDF_TYPE, "?restriction"),
                atom("?restriction", ON_PROPERTY, "?p"),
                atom("?restriction", predicate, "?maximum"),
            ];
            if let Some((predicate, object)) = qualifier {
                body.push(atom("?restriction", predicate, object));
            }
            laws.push(
                PreparedPropertyRule::new(PropertyRule {
                    rule_iri: "dl:max-cardinality-clash".to_owned(),
                    head: atom("?s", RDF_TYPE, NOTHING),
                    body,
                    operation: Some(PropertyOperation::Cardinality(CardinalityPattern {
                        subject: EvalTerm::var("?s"),
                        property: EvalTerm::var("?p"),
                        maximum: EvalTerm::var("?maximum"),
                        set,
                    })),
                    guards: Vec::new(),
                })
                .expect("fixed cardinality laws bind every selector and bound"),
            );
        }
    }
    laws
}

/// Monotone DL contradictions participate in the SAME native fixed point as
/// authored rules. Every output keeps all actual schema/value/identity premises.
/// Datatype guards are pure; they neither consult absence nor invent equality.
fn clash_laws() -> Vec<PreparedPropertyRule> {
    let bottom = || atom("?s", RDF_TYPE, NOTHING);
    let mut rules = vec![
        law(
            "dl:sameAs-symmetry",
            atom("?b", SAME, "?a"),
            vec![atom("?a", SAME, "?b")],
        ),
        law(
            "dl:sameAs-transitive",
            atom("?a", SAME, "?c"),
            vec![atom("?a", SAME, "?b"), atom("?b", SAME, "?c")],
        ),
        law(
            "dl:same-different-clash",
            bottom(),
            vec![atom("?s", DIFFERENT, "?o"), atom("?s", SAME, "?o")],
        ),
        law(
            "dl:same-different-clash",
            bottom(),
            vec![atom("?s", DIFFERENT, "?s")],
        ),
        law(
            "dl:asymmetric-property-clash",
            bottom(),
            vec![
                atom(
                    "?p",
                    RDF_TYPE,
                    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
                ),
                atom("?s", "?p", "?o"),
                atom("?o", "?p", "?s"),
            ],
        ),
        law(
            "dl:irreflexive-property-clash",
            bottom(),
            vec![
                atom(
                    "?p",
                    RDF_TYPE,
                    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
                ),
                atom("?s", "?p", "?s"),
            ],
        ),
        guarded_law(
            "dl:property-disjoint-clash",
            bottom(),
            vec![
                atom(
                    "?p",
                    "http://www.w3.org/2002/07/owl#propertyDisjointWith",
                    "?q",
                ),
                atom("?s", "?p", "?left"),
                atom("?s", "?q", "?right"),
            ],
            ValueComparison::Equal,
            "?left",
            "?right",
        ),
    ];
    for target in [
        "http://www.w3.org/2002/07/owl#targetIndividual",
        "http://www.w3.org/2002/07/owl#targetValue",
    ] {
        // The complete NPA-specific structural record suffices even without an
        // explicit type marker; its original component statements are premises.
        rules.push(guarded_law(
            "dl:negative-property-assertion-clash",
            bottom(),
            vec![
                atom(
                    "?record",
                    "http://www.w3.org/2002/07/owl#sourceIndividual",
                    "?s",
                ),
                atom(
                    "?record",
                    "http://www.w3.org/2002/07/owl#assertionProperty",
                    "?p",
                ),
                atom("?record", target, "?negative"),
                atom("?s", "?p", "?positive"),
            ],
            ValueComparison::Equal,
            "?negative",
            "?positive",
        ));
    }
    for schema in characteristic_schemas(false) {
        let mut body = schema;
        body.extend([atom("?s", "?p", "?left"), atom("?s", "?p", "?right")]);
        rules.push(guarded_law(
            "dl:functional-property-clash",
            bottom(),
            body.clone(),
            ValueComparison::DistinctLiteral,
            "?left",
            "?right",
        ));
        for (left, right) in [("?left", "?right"), ("?right", "?left")] {
            let mut distinct = body.clone();
            distinct.push(atom(left, DIFFERENT, right));
            rules.push(law("dl:functional-property-clash", bottom(), distinct));
        }
    }
    rules
}
