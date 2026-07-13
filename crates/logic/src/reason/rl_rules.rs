// @generated once from the reviewed fixed RL calculus; now canonical structured Rust.

use crate::physical::{GenericAtom, GenericRule};
use crate::rule_ir::EvalTerm;

pub(crate) fn structured_rl_rules() -> Vec<GenericRule> {
    vec![
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cax-sco".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                    ),
                    EvalTerm::Var("?c3".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c3".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-sco".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                    ),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
                    ),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:scm-eqc1-fwd".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                    ),
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
                    ),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:scm-eqc1-bwd".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?c1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
                    ),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-eqc2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                    ),
                    EvalTerm::Var("?p3".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                        ),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                        ),
                        EvalTerm::Var("?p3".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-spo".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                        ),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-spo1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                    ),
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2002/07/owl#equivalentProperty".to_owned(),
                    ),
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:prp-eqp1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                    ),
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2002/07/owl#equivalentProperty".to_owned(),
                    ),
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:prp-eqp2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#domain".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-dom".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#range".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-rng".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2000/01/rdf-schema#domain".to_owned()),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#domain".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                        ),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-dom2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2000/01/rdf-schema#domain".to_owned()),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#domain".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-dom1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2000/01/rdf-schema#range".to_owned()),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#range".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".to_owned(),
                        ),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-rng2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2000/01/rdf-schema#range".to_owned()),
                    EvalTerm::Var("?c2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#range".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:scm-rng1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?z".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#TransitiveProperty".to_owned(),
                        ),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?z".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-trp".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#SymmetricProperty".to_owned(),
                        ),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-symp".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::Var("?p2".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#inverseOf".to_owned()),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-inv1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::Var("?p1".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#inverseOf".to_owned()),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-inv2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?u1".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?u3".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#propertyChainAxiom".to_owned(),
                        ),
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".to_owned(),
                        ),
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".to_owned(),
                        ),
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".to_owned(),
                        ),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".to_owned(),
                        ),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil".to_owned(),
                        ),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?u1".to_owned()),
                        EvalTerm::Var("?p1".to_owned()),
                        EvalTerm::Var("?u2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?u2".to_owned()),
                        EvalTerm::Var("?p2".to_owned()),
                        EvalTerm::Var("?u3".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:prp-spo2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?r".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#onProperty".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#someValuesFrom".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-svf1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#onProperty".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#allValuesFrom".to_owned(),
                        ),
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-avf".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?v".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#onProperty".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#hasValue".to_owned()),
                        EvalTerm::Var("?v".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-hv1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?r".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#onProperty".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#hasValue".to_owned()),
                        EvalTerm::Var("?v".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?v".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-hv2".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "list_member".to_owned(),
                args: vec![
                    EvalTerm::Var("?l".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?l".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".to_owned(),
                    ),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:list-member-head".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "list_member".to_owned(),
                args: vec![
                    EvalTerm::Var("?l".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".to_owned(),
                        ),
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "list_member".to_owned(),
                    args: vec![
                        EvalTerm::Var("?r".to_owned()),
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:list-member-tail".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#oneOf".to_owned()),
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "list_member".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-oneOf".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?m".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#unionOf".to_owned()),
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "list_member".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?m".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-union-member".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?m".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#disjointUnionOf".to_owned(),
                        ),
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "list_member".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l".to_owned()),
                        EvalTerm::Var("?m".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-disjointUnion-member".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    ),
                    EvalTerm::Var("?c".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?c".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/2002/07/owl#intersectionOf".to_owned(),
                        ),
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l0".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".to_owned(),
                        ),
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?l1".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".to_owned(),
                        ),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil".to_owned(),
                        ),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?c1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                        ),
                        EvalTerm::Var("?c2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:cls-int1".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                    EvalTerm::Var("?y".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            }],
            rule_iri: "rl:eq-sym".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x".to_owned()),
                    EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                    EvalTerm::Var("?z".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?y".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                        EvalTerm::Var("?z".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:eq-trans".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?x2".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?o".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x1".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                        EvalTerm::Var("?x2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?x1".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?o".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:eq-rep-s".to_owned(),
        },
        GenericRule {
            head: GenericAtom {
                relation: "triple".to_owned(),
                args: vec![
                    EvalTerm::Var("?s".to_owned()),
                    EvalTerm::Var("?p".to_owned()),
                    EvalTerm::Var("?o2".to_owned()),
                    EvalTerm::Var("?w".to_owned()),
                ],
            },
            body: vec![
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?o1".to_owned()),
                        EvalTerm::ConstNamed("http://www.w3.org/2002/07/owl#sameAs".to_owned()),
                        EvalTerm::Var("?o2".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
                GenericAtom {
                    relation: "triple".to_owned(),
                    args: vec![
                        EvalTerm::Var("?s".to_owned()),
                        EvalTerm::Var("?p".to_owned()),
                        EvalTerm::Var("?o1".to_owned()),
                        EvalTerm::Var("?w".to_owned()),
                    ],
                },
            ],
            rule_iri: "rl:eq-rep-o".to_owned(),
        },
    ]
}
