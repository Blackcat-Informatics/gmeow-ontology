// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Direct-authored persistence evidence; no sources, preparation or execution are used.
use super::{MathCompilation, MathConstraint, PreparedReasonedGates};
use crate::query_ir::{ArithOp, CmpOp, QBuiltin, QTerm};
use crate::relational_core::ViolationLowering;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Reduction};
use gmeow_logic_compile::frontend::{Diagnostic, Severity};
use gmeow_logic_compile::ir::{AtomicTerm, TermKindPredicate};
use gmeow_logic_compile::relational_core::{
    NumericOperator, RcComparison, RcNumeric, RcViolationGap, RcViolationResidue,
};
use gmeow_term_arena::TermArena;
use purrdf::{BlankScope, RdfLiteral, RdfTextDirection, TermBox, TermValue};
use purrdf_lex::json::record::{DecodeError, ToJson, from_slice};
use std::collections::BTreeMap;

fn quoted_value() -> TermValue {
    TermValue::Triple {
        s: TermBox::new(TermValue::Blank {
            label: "same-label".to_owned(),
            scope: BlankScope(u32::MAX),
        }),
        p: TermBox::new(TermValue::Iri("urn:test:outer-predicate".to_owned())),
        o: TermBox::new(TermValue::Triple {
            s: TermBox::new(TermValue::Blank {
                label: "same-label".to_owned(),
                scope: BlankScope(7),
            }),
            p: TermBox::new(TermValue::Iri("urn:test:inner-predicate".to_owned())),
            o: TermBox::new(TermValue::Literal {
                lexical_form: "مرحبا\0 中😀".to_owned(),
                datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned(),
                language: Some("ar".to_owned()),
                direction: Some(RdfTextDirection::Rtl),
            }),
        }),
    }
}

fn query_triple() -> QTerm {
    QTerm::Triple {
        s: Box::new(QTerm::Var("raw variable".to_owned())),
        p: Box::new(QTerm::Num(i64::MIN)),
        o: Box::new(QTerm::Triple {
            s: Box::new(QTerm::Const("raw\0 constant 中😀".to_owned())),
            p: Box::new(QTerm::Var("raw predicate".to_owned())),
            o: Box::new(QTerm::Num(i64::MAX)),
        }),
    }
}

fn authored_builtins() -> Vec<QBuiltin> {
    let mut builtins = Vec::new();
    for comparison in [
        RcComparison::GreaterEqual,
        RcComparison::Less,
        RcComparison::Greater,
        RcComparison::LessEqual,
    ] {
        builtins.push(QBuiltin::TermCompare {
            left: AtomicTerm::Var("raw left".to_owned()),
            right: AtomicTerm::Literal(RdfLiteral {
                lexical_form: "01".to_owned(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
                language: None,
                direction: None,
            }),
            comparison,
            expected: false,
        });
    }
    builtins.push(QBuiltin::TermEqual {
        left: AtomicTerm::Blank("raw blank".to_owned()),
        right: AtomicTerm::Iri("raw IRI".to_owned()),
        expected: true,
    });
    for predicate in [
        TermKindPredicate::LanguageTagged,
        TermKindPredicate::Iri,
        TermKindPredicate::FiniteNumeric,
        TermKindPredicate::BlankOrIri,
        TermKindPredicate::Literal,
    ] {
        builtins.push(QBuiltin::TermKind {
            term: AtomicTerm::Var("raw kind operand".to_owned()),
            predicate,
            expected: false,
        });
    }
    for op in [
        ArithOp::ExactDiv,
        ArithOp::Add,
        ArithOp::Sub,
        ArithOp::Div,
        ArithOp::Mul,
    ] {
        builtins.push(QBuiltin::Is {
            target: QTerm::Var("raw target".to_owned()),
            lhs: QTerm::Num(i64::MIN),
            op,
            rhs: QTerm::Num(i64::MAX),
        });
    }
    for op in [CmpOp::Eq, CmpOp::Gt, CmpOp::Le, CmpOp::Lt, CmpOp::Ge] {
        builtins.push(QBuiltin::Compare {
            lhs: query_triple(),
            op,
            rhs: QTerm::Const("raw\0 right 中😀".to_owned()),
        });
    }
    builtins.push(QBuiltin::BilinearSqDist {
        target: QTerm::Var("raw distance".to_owned()),
        gram: QTerm::Const("raw gram".to_owned()),
        x: QTerm::Num(i64::MIN),
        y: query_triple(),
    });
    builtins.push(QBuiltin::DimEqual {
        d1: query_triple(),
        d2: QTerm::Var("raw dimension".to_owned()),
    });
    builtins.push(QBuiltin::DimProduct {
        d_f: QTerm::Const("raw integrand".to_owned()),
        d_m: query_triple(),
        d_r: QTerm::Num(i64::MAX),
    });
    builtins
}

fn authored_rule(function: &str) -> EvalRule {
    // Native hydration retains the private reduction fields without running Reduction::lower.
    let reduction: Reduction = from_slice(
        format!(
            r#"{{"function":"{function}","input":"raw","result":"raw","groups":["z","a","z"]}}"#
        )
        .as_bytes(),
    )
    .expect("authored private reduction must hydrate as raw persistence data");
    let body = EvalAtom {
        subject: EvalTerm::Var("raw variable".to_owned()),
        predicate: "raw\0 predicate 中😀".to_owned(),
        object: EvalTerm::ConstNamed("raw named operand".to_owned()),
        negated: true,
    };
    EvalRule {
        numeric: vec![
            RcNumeric {
                operator: NumericOperator::Add,
                left: AtomicTerm::Var("raw left".to_owned()),
                right: AtomicTerm::Literal(RdfLiteral {
                    lexical_form: "01".to_owned(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
                    language: None,
                    direction: None,
                }),
                result: Some(AtomicTerm::Var("raw result".to_owned())),
            },
            RcNumeric {
                operator: NumericOperator::Equal,
                left: AtomicTerm::Iri("raw IRI".to_owned()),
                right: AtomicTerm::Blank("raw blank".to_owned()),
                result: None,
            },
        ],
        reduction: Some(reduction),
        head: EvalAtom {
            subject: EvalTerm::ConstLit(TermValue::Blank {
                label: "same-label".to_owned(),
                scope: BlankScope(7),
            }),
            predicate: "raw head predicate".to_owned(),
            object: EvalTerm::ConstLit(quoted_value()),
            negated: true,
        },
        body: vec![
            body.clone(),
            EvalAtom {
                subject: EvalTerm::ConstNamed("raw named operand".to_owned()),
                predicate: "raw literal predicate".to_owned(),
                object: EvalTerm::ConstLit(TermValue::Literal {
                    lexical_form: "01".to_owned(),
                    datatype: "http://www.w3.org/2001/XMLSchema#integer".to_owned(),
                    language: None,
                    direction: None,
                }),
                negated: false,
            },
            body,
        ],
        rule_iri: format!("raw\0 rule {function} 中😀"),
        distinct_pairs: vec![
            ("z".to_owned(), "a".to_owned()),
            ("a".to_owned(), "z".to_owned()),
            ("z".to_owned(), "a".to_owned()),
        ],
        builtins: Vec::new(),
        constraint_tag: Some("raw constraint".to_owned()),
    }
}

fn authored_prepared_laws() -> PreparedReasonedGates {
    let mut math_rules: Vec<_> = ["Max", "Count", "Sum", "Min", "Avg"]
        .into_iter()
        .map(authored_rule)
        .collect();
    math_rules[1].builtins = authored_builtins();
    let mut ungrouped = math_rules[0].clone();
    ungrouped.reduction = None;
    ungrouped.constraint_tag = None;
    math_rules.push(ungrouped.clone());
    math_rules.push(ungrouped.clone());
    let gaps = [
        RcViolationGap::UnsafeUniversalVariable,
        RcViolationGap::NotUniversalImplication,
        RcViolationGap::NoFailureClass,
        RcViolationGap::BuiltinBoundConsequent,
        RcViolationGap::UnsupportedAntecedent,
        RcViolationGap::UnsupportedConsequent,
        RcViolationGap::UnsafeFocusVariable,
        RcViolationGap::UnsafeTermKindVariable,
        RcViolationGap::UnsafeEqualityVariable,
        RcViolationGap::UnsafeUniversalVariable,
    ];
    let diagnostic = Diagnostic {
        severity: Severity::Warning,
        code: "raw\0 diagnostic 中😀".to_owned(),
        message: "raw diagnostic".to_owned(),
        subject: Some("raw subject".to_owned()),
    };
    PreparedReasonedGates {
        schema_version: u32::MAX,
        source_digests: BTreeMap::from([
            ("z".to_owned(), "raw\0 digest 中😀".to_owned()),
            ("a".to_owned(), "raw digest".to_owned()),
        ]),
        math_rules,
        enactment: ViolationLowering {
            rules: vec![ungrouped.clone(), authored_rule("Count"), ungrouped],
            residue: gaps
                .into_iter()
                .map(|gap| RcViolationResidue {
                    constraint_iri: "raw constraint".to_owned(),
                    gap,
                })
                .collect(),
            failure_classes: BTreeMap::from([
                ("z".to_owned(), "raw class z".to_owned()),
                ("a".to_owned(), "raw class a".to_owned()),
            ]),
        },
        math_compilation: MathCompilation {
            diagnostics: vec![
                diagnostic.clone(),
                Diagnostic {
                    severity: Severity::Info,
                    code: "raw info".to_owned(),
                    message: "raw info".to_owned(),
                    subject: None,
                },
                diagnostic,
            ],
            constraints: vec![
                MathConstraint {
                    iri: "z".to_owned(),
                    target: "raw target".to_owned(),
                    failure_class: Some("raw class".to_owned()),
                    projected_sparql: "raw\0 projection 中😀".to_owned(),
                },
                MathConstraint {
                    iri: "a".to_owned(),
                    target: "raw target".to_owned(),
                    failure_class: None,
                    projected_sparql: "raw projection".to_owned(),
                },
            ],
        },
    }
}

#[test]
fn native_prepared_laws_wire_preserves_complete_original_artifact() {
    let laws = authored_prepared_laws();
    let bytes = laws
        .to_json_bytes()
        .expect("complete native prepared laws must serialize");
    let original = include_bytes!("fixtures/channel-v1.json")
        .strip_suffix(b"\n")
        .expect("reference stores one terminal LF outside the original JSON");
    assert_eq!(bytes, original, "complete original prepared-law encoding");
    assert!(!bytes.ends_with(b"\n"));
    let decoded = PreparedReasonedGates::from_json_bytes(&bytes)
        .expect("native prepared-law bytes must hydrate");
    assert_eq!(decoded, laws);
    assert_eq!(
        decoded
            .to_json_bytes()
            .expect("hydrated laws must serialize"),
        bytes
    );
    assert_eq!(decoded.schema_version, u32::MAX);
    assert_eq!(
        decoded.math_rules[1].distinct_pairs,
        [
            ("z".to_owned(), "a".to_owned()),
            ("a".to_owned(), "z".to_owned()),
            ("z".to_owned(), "a".to_owned())
        ]
    );
    assert_eq!(decoded.math_rules[5], decoded.math_rules[6]);
    assert_eq!(decoded.enactment.residue[0], decoded.enactment.residue[9]);
}

#[test]
fn native_prepared_laws_admission_preserves_raw_fields_and_refuses_invalid_wire() {
    let laws = authored_prepared_laws();
    let wire = String::from_utf8(laws.to_json_bytes().expect("authored laws must encode"))
        .expect("native JSON is UTF-8");
    let future = wire
        .replacen("{", r#"{"future":[],"#, 1)
        .replace(r#"{"numeric":"#, r#"{"future":null,"numeric":"#)
        .replace(r#"{"subject":"#, r#"{"future":false,"subject":"#)
        .replace(r#""reduction":{"#, r#""reduction":{"future":{},"#)
        .replace(r#""Compare":{"#, r#""Compare":{"future":0,"#)
        .replace(r#""Triple":{"#, r#""Triple":{"future":"raw","#)
        .replace(r#""enactment":{"#, r#""enactment":{"future":[],"#)
        .replace(
            r#"{"constraint_iri":"#,
            r#"{"future":null,"constraint_iri":"#,
        )
        .replace(
            r#""math_compilation":{"#,
            r#""math_compilation":{"future":false,"#,
        )
        .replace(r#"{"iri":"#, r#"{"future":[],"iri":"#);
    assert_eq!(
        PreparedReasonedGates::from_json_bytes(future.as_bytes())
            .expect("unknown record fields are ignored"),
        laws
    );
    for schema in ["-0", "-1", "1.0", "1e0", "null", "4294967296"] {
        let bad = wire.replacen(
            r#""schema_version":4294967295"#,
            &format!(r#""schema_version":{schema}"#),
            1,
        );
        assert_ne!(bad, wire, "schema mutation reaches its actual field");
        assert!(
            PreparedReasonedGates::from_json_bytes(bad.as_bytes()).is_err(),
            "{schema}"
        );
    }
    for number in [
        "-0",
        "1.0",
        "1e0",
        "null",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        let bad = wire.replacen(
            r#""Num":-9223372036854775808"#,
            &format!(r#""Num":{number}"#),
            1,
        );
        assert_ne!(
            bad, wire,
            "query-number mutation reaches its actual operand"
        );
        assert!(
            PreparedReasonedGates::from_json_bytes(bad.as_bytes()).is_err(),
            "{number}"
        );
    }
    let bad = wire.replacen(r#""Num":-9223372036854775808"#, r#""Num":-0"#, 1);
    let error = PreparedReasonedGates::from_json_bytes(bad.as_bytes())
        .expect_err("signed zero must fail natively");
    let source = error
        .downcast_ref::<DecodeError>()
        .expect("native decoding error remains the typed source");
    assert_eq!(source.pointer(), "/math_rules/1/builtins/10/Is/lhs/Num");
    for bad in [
        wire.replacen(r#""schema_version":4294967295,"#, "", 1),
        wire.replacen(
            r#""source_digests":{"#,
            r#""source_digests":null,"unused":{"#,
            1,
        ),
        wire.replacen(r#""numeric":["#, r#""numeric":null,"unused":["#, 1),
        wire.replacen(r#""negated":true"#, r#""negated":null"#, 1),
        wire.replacen(r#""function":"Max""#, r#""function":"MAX""#, 1),
        wire.replacen(r#""op":"ExactDiv""#, r#""op":"/""#, 1),
        wire.replacen(
            r#""comparison":"GreaterEqual""#,
            r#""comparison":"termGreaterEqual""#,
            1,
        ),
        wire.replacen(
            r#""predicate":"LanguageTagged""#,
            r#""predicate":"termHasLang""#,
            1,
        ),
        wire.replacen(
            r#""gap":"UnsafeUniversalVariable""#,
            r#""gap":"unsafe-universal-variable""#,
            1,
        ),
        wire.replacen(r#""Num":-9223372036854775808"#, r#""Struct":null"#, 1),
        wire.replacen(
            r#"{"Num":-9223372036854775808}"#,
            r#"{"Num":0,"Var":"raw"}"#,
            1,
        ),
        "[]".to_owned(),
    ] {
        assert_ne!(
            bad, wire,
            "admission mutation reaches its intended boundary"
        );
        assert!(
            PreparedReasonedGates::from_json_bytes(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    for bad in [
        wire.replacen(
            r#""schema_version":4294967295"#,
            r#""schema_version":0,"schema_version":4294967295"#,
            1,
        ),
        wire.replacen(r#""negated":true"#, r#""negated":false,"negated":true"#, 1),
        wire.replacen(
            r#""function":"Max""#,
            r#""function":"Count","function":"Max""#,
            1,
        ),
        wire.replacen(
            r#""expected":false"#,
            r#""expected":true,"expected":false"#,
            1,
        ),
    ] {
        let error = PreparedReasonedGates::from_json_bytes(bad.as_bytes())
            .expect_err("duplicate known field must fail");
        assert!(error.to_string().contains("duplicate field"), "{error:#}");
    }
    let singleton = wire.replacen(r#""function":"Max""#, r#""function":{"Max":null}"#, 1);
    assert_eq!(
        PreparedReasonedGates::from_json_bytes(singleton.as_bytes())
            .expect("unit enum accepts its existing singleton-null shape"),
        laws
    );
    let last_wins = wire.replacen(
        r#""source_digests":{"#,
        r#""source_digests":{"a":"first","#,
        1,
    );
    assert_eq!(
        PreparedReasonedGates::from_json_bytes(last_wins.as_bytes())
            .expect("typed map retains the last valid duplicate"),
        laws
    );
    let shadowed = wire.replacen(r#""source_digests":{"#, r#""source_digests":{"a":null,"#, 1);
    assert!(PreparedReasonedGates::from_json_bytes(shadowed.as_bytes()).is_err());
    let reduction = format!(
        r#""reduction":{},"#,
        purrdf_lex::json::write_compact(&laws.math_rules[0].reduction.to_json())
    );
    for optional in [
        wire.replacen(&reduction, "", 1),
        wire.replacen(&reduction, r#""reduction":null,"#, 1),
    ] {
        assert!(
            PreparedReasonedGates::from_json_bytes(optional.as_bytes())
                .expect("missing/null reduction is absent")
                .math_rules[0]
                .reduction
                .is_none()
        );
    }
    for optional in [
        wire.replacen(
            r#""constraint_tag":"raw constraint""#,
            r#""constraint_tag":null"#,
            1,
        ),
        wire.replacen(r#""failure_class":"raw class","#, "", 1),
    ] {
        PreparedReasonedGates::from_json_bytes(optional.as_bytes())
            .expect("plain optional metadata permits null or omission");
    }
    let pairs = purrdf_lex::json::write_compact(&laws.math_rules[0].distinct_pairs.to_json());
    for rows in [
        "null",
        "[[]]",
        r#"[["left"]]"#,
        r#"[["left","right","extra"]]"#,
    ] {
        let bad = wire.replacen(
            &format!(r#""distinct_pairs":{pairs}"#),
            &format!(r#""distinct_pairs":{rows}"#),
            1,
        );
        assert!(
            PreparedReasonedGates::from_json_bytes(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = wire.replacen(
        &format!(r#""distinct_pairs":{pairs}"#),
        r#""distinct_pairs":[[false,"right"]]"#,
        1,
    );
    let error = PreparedReasonedGates::from_json_bytes(bad.as_bytes())
        .expect_err("nonstring distinct row cell must fail");
    assert_eq!(
        error
            .downcast_ref::<DecodeError>()
            .expect("row refusal retains its native source")
            .pointer(),
        "/math_rules/0/distinct_pairs/0/0"
    );
}

#[test]
fn native_prepared_laws_refuse_arena_terms_before_publication() {
    let mut arena = TermArena::new();
    let symbol = arena.intern_leaf(TermValue::Iri("urn:test:function".to_owned()));
    let argument = arena.intern_leaf(TermValue::Iri("urn:test:argument".to_owned()));
    let handle = arena
        .intern_app(symbol, &[argument])
        .expect("authored compound uses nodes from one arena");
    let nested = QTerm::Triple {
        s: Box::new(QTerm::Const("raw subject".to_owned())),
        p: Box::new(QTerm::Num(0)),
        o: Box::new(QTerm::Triple {
            s: Box::new(QTerm::Var("raw subject".to_owned())),
            p: Box::new(QTerm::Const("raw predicate".to_owned())),
            o: Box::new(QTerm::Struct(handle)),
        }),
    };
    let failures = [
        (
            QBuiltin::Compare {
                lhs: nested.clone(),
                op: CmpOp::Eq,
                rhs: QTerm::Num(0),
            },
            "Compare.lhs",
            true,
        ),
        (
            QBuiltin::Is {
                target: QTerm::Var("raw".to_owned()),
                lhs: QTerm::Num(0),
                op: ArithOp::Add,
                rhs: QTerm::Struct(handle),
            },
            "Is.rhs",
            false,
        ),
        (
            QBuiltin::DimProduct {
                d_f: QTerm::Num(0),
                d_m: QTerm::Var("raw".to_owned()),
                d_r: nested,
            },
            "DimProduct.d_r",
            true,
        ),
    ];
    for (builtin, operand, recursive) in failures {
        for in_math in [true, false] {
            let mut laws = authored_prepared_laws();
            let rules = if in_math {
                &mut laws.math_rules
            } else {
                &mut laws.enactment.rules
            };
            rules[0].builtins = vec![builtin.clone()];
            let rule = format!("{:?}", rules[0].rule_iri);
            let error = laws
                .to_json_bytes()
                .expect_err("arena-local handles must prevent artifact publication");
            assert!(
                error.downcast_ref::<crate::error::Verify>().is_some(),
                "existing verification error owns this refusal"
            );
            let message = format!("{error:#}");
            assert!(message.contains("QTerm::Struct"), "{message}");
            assert!(message.contains(&rule), "{message}");
            assert!(message.contains(operand), "{message}");
            assert!(
                message.contains(if in_math {
                    "math_rules[0]"
                } else {
                    "enactment"
                }),
                "{message}"
            );
            if recursive {
                assert!(message.contains("Triple.o"), "{message}");
            }
        }
    }
}
