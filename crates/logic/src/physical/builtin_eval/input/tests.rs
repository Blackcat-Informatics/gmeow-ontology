// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native binding modes and the GMEOW exact value algebra, not RDF conformance.

use super::*;
use crate::physical::builtin_eval::{
    BuiltinError, BuiltinOutcome, CellResolver, NoCellResolver, emit_term, eval, eval_native,
};
use crate::query_ir::{ArithOp, CmpOp, QBuiltin};
use gmeow_math::{Rational, dimension::DimVector};

fn variable(name: &str) -> QTerm {
    QTerm::Var(name.to_owned())
}

fn operation(op: ArithOp) -> QBuiltin {
    QBuiltin::Is {
        target: variable("result"),
        lhs: variable("left"),
        op,
        rhs: variable("right"),
    }
}

fn evaluate(builtin: &QBuiltin, bindings: &[(&str, TermValue)]) -> BuiltinOutcome {
    eval_native(
        builtin,
        &|name| {
            bindings
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value)
        },
        &NoCellResolver,
    )
}

#[test]
fn native_generators_and_bound_targets_share_the_exact_value_algebra() {
    let half = Rational::new(1, 2).unwrap();
    let length = DimVector::unit(0).unwrap();
    let cases = [
        (Value::Int(6), Value::Int(4), ArithOp::Div, Value::Int(1)),
        (
            Value::Int(6),
            Value::Int(4),
            ArithOp::ExactDiv,
            Value::Rat(Rational::new(3, 2).unwrap()),
        ),
        (
            Value::Rat(half),
            Value::Int(3),
            ArithOp::Mul,
            Value::Rat(Rational::new(3, 2).unwrap()),
        ),
        (
            Value::Dim(Box::new(length)),
            Value::Dim(Box::new(length)),
            ArithOp::ExactDiv,
            Value::Dim(Box::new(DimVector::zero())),
        ),
        (
            Value::Quantity(half, Box::new(length)),
            Value::Int(4),
            ArithOp::Mul,
            Value::Quantity(Rational::new(2, 1).unwrap(), Box::new(length)),
        ),
    ];
    for (left, right, op, expected) in cases {
        let mut bindings = vec![("left", emit_term(&left)), ("right", emit_term(&right))];
        let builtin = operation(op);
        assert_eq!(
            evaluate(&builtin, &bindings),
            BuiltinOutcome::Generate {
                var: "result".into(),
                value: expected.clone(),
            }
        );
        bindings.push(("result", emit_term(&expected)));
        assert_eq!(evaluate(&builtin, &bindings), BuiltinOutcome::Filter(true));
        // The captured reference remains a separate input adapter to this algebra.
        let surfaces: Vec<_> = bindings
            .iter()
            .map(|(name, value)| (*name, crate::provenance::term_display(value)))
            .collect();
        assert_eq!(
            eval(
                &builtin,
                &|name| surfaces
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| Cow::Borrowed(value.as_str())),
                &NoCellResolver
            ),
            BuiltinOutcome::Filter(true)
        );
    }
    let cross_type = [
        ("left", emit_term(&Value::Int(6))),
        ("right", emit_term(&Value::Int(2))),
        ("result", emit_term(&Value::Int(3))),
    ];
    assert_eq!(
        evaluate(&operation(ArithOp::ExactDiv), &cross_type),
        BuiltinOutcome::Filter(true)
    );
}

#[test]
fn native_binding_failure_modes_preserve_error_and_filter_distinctions() {
    let builtin = operation(ArithOp::Div);
    assert_eq!(evaluate(&builtin, &[]), BuiltinOutcome::Unbound);
    let mut bindings = vec![
        ("left", emit_term(&Value::Int(6))),
        ("right", emit_term(&Value::Int(0))),
    ];
    assert_eq!(
        evaluate(&builtin, &bindings),
        BuiltinOutcome::Error(BuiltinError::ZeroDivisor)
    );
    bindings[0].1 = emit_term(&Value::Int(i64::MIN));
    bindings[1].1 = emit_term(&Value::Int(-1));
    assert_eq!(
        evaluate(&builtin, &bindings),
        BuiltinOutcome::Error(BuiltinError::Overflow)
    );
    bindings[0].1 = emit_term(&Value::Int(6));
    bindings[1].1 = emit_term(&Value::Int(2));
    bindings.push(("result", TermValue::simple_literal("3")));
    assert_eq!(evaluate(&builtin, &bindings), BuiltinOutcome::Filter(false));
    bindings[0].1 = TermValue::iri("urn:6");
    assert_eq!(evaluate(&builtin, &bindings), BuiltinOutcome::Unbound);
}

#[test]
fn native_type_components_govern_numeric_admission() {
    let numeric = emit_term(&Value::Int(3));
    let mut language = numeric.clone();
    let TermValue::Literal { language: tag, .. } = &mut language else {
        unreachable!()
    };
    *tag = Some("en".into());
    let mut directional = numeric;
    let TermValue::Literal { direction, .. } = &mut directional else {
        unreachable!()
    };
    *direction = Some(purrdf::RdfTextDirection::Rtl);
    let builtin = QBuiltin::Compare {
        lhs: variable("left"),
        op: CmpOp::Eq,
        rhs: QTerm::Num(3),
    };
    for value in [
        TermValue::iri("urn:3"),
        TermValue::simple_literal("3"),
        language,
        directional,
    ] {
        assert_eq!(
            evaluate(&builtin, &[("left", value)]),
            BuiltinOutcome::Unbound
        );
    }
}

struct Cells;
impl CellResolver for Cells {
    fn gram(&self, iri: &str) -> Option<Vec<(usize, usize, Rational)>> {
        (iri == "urn:gram").then(|| vec![(0, 0, Rational::one())])
    }
    fn vector(&self, iri: &str) -> Option<Vec<Rational>> {
        match iri {
            "urn:x" => Some(vec![Rational::new(3, 1).unwrap()]),
            "urn:y" => Some(vec![Rational::one()]),
            _ => None,
        }
    }
    fn dimension(&self, iri: &str) -> Option<DimVector> {
        (iri == "urn:dimension").then(DimVector::zero)
    }
}

#[test]
fn native_cell_probes_borrow_iris_and_keep_all_builtin_modes() {
    let bindings = [
        ("gram", TermValue::iri("urn:gram")),
        ("x", TermValue::iri("urn:x")),
        ("y", TermValue::iri("urn:y")),
        ("dim", TermValue::iri("urn:dimension")),
    ];
    let lookup = |name: &str| {
        bindings
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    let iri = variable("gram");
    assert!(matches!(
        resolve_iri_operand(&iri, &|name| lookup(name).map(Binding::Native)),
        Some(Cow::Borrowed("urn:gram"))
    ));
    let constant = QTerm::Const("<urn:gram>".into());
    assert!(matches!(
        resolve_iri_operand(&constant, &|_| None),
        Some(Cow::Borrowed("urn:gram"))
    ));
    let metric = QBuiltin::BilinearSqDist {
        target: variable("result"),
        gram: variable("gram"),
        x: variable("x"),
        y: variable("y"),
    };
    assert_eq!(
        eval_native(&metric, &lookup, &Cells),
        BuiltinOutcome::Generate {
            var: "result".into(),
            value: Value::Rat(Rational::new(4, 1).unwrap())
        }
    );
    let equality = QBuiltin::DimEqual {
        d1: variable("dim"),
        d2: QTerm::Const("<urn:dimension>".into()),
    };
    let product = QBuiltin::DimProduct {
        d_f: variable("dim"),
        d_m: variable("dim"),
        d_r: variable("dim"),
    };
    for builtin in [equality, product] {
        assert_eq!(
            eval_native(&builtin, &lookup, &Cells),
            BuiltinOutcome::Filter(true)
        );
        assert_eq!(
            eval_native(&builtin, &|_| None, &Cells),
            BuiltinOutcome::Unbound
        );
    }
}
