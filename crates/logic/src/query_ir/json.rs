// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Persistent query operands; arena-local compound handles are never a wire value.
use super::{ArithOp, CmpOp, CompletionFrontier, QBuiltin, QTerm};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

::purrdf_lex::json_record!(impl ToJson for CompletionFrontier {
    "completed" => completed,
    "total" => total,
    "saturated_preds" => saturated_preds,
    "consumed_steps" => consumed_steps,
});

impl ToJson for ArithOp {
    fn to_json(&self) -> Value {
        match self {
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::ExactDiv => "ExactDiv",
        }
        .to_json()
    }
}
impl FromJson for ArithOp {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "ArithOp")? {
            "Add" => Ok(Self::Add),
            "Sub" => Ok(Self::Sub),
            "Mul" => Ok(Self::Mul),
            "Div" => Ok(Self::Div),
            "ExactDiv" => Ok(Self::ExactDiv),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Add", "Sub", "Mul", "Div", "ExactDiv"],
            )),
        }
    }
}

impl ToJson for CmpOp {
    fn to_json(&self) -> Value {
        match self {
            Self::Gt => "Gt",
            Self::Lt => "Lt",
            Self::Ge => "Ge",
            Self::Le => "Le",
            Self::Eq => "Eq",
        }
        .to_json()
    }
}
impl FromJson for CmpOp {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "CmpOp")? {
            "Gt" => Ok(Self::Gt),
            "Lt" => Ok(Self::Lt),
            "Ge" => Ok(Self::Ge),
            "Le" => Ok(Self::Le),
            "Eq" => Ok(Self::Eq),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Gt", "Lt", "Ge", "Le", "Eq"],
            )),
        }
    }
}

impl QTerm {
    pub(crate) fn json_value(&self) -> gmeow_errors::Result<Value> {
        Ok(match self {
            Self::Const(value) => Object::new().with("Const", value.to_json()).into(),
            Self::Var(value) => Object::new().with("Var", value.to_json()).into(),
            Self::Num(value) => Object::new().with("Num", value.to_json()).into(),
            Self::Struct(_) => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Verify {
                    detail: "prepared verification laws cannot persist arena-local QTerm::Struct"
                        .to_owned(),
                }));
            }
            Self::Triple { s, p, o } => Object::new()
                .with(
                    "Triple",
                    Object::new()
                        .with(
                            "s",
                            s.json_value()
                                .map_err(|error| error.with_context("Triple.s"))?,
                        )
                        .with(
                            "p",
                            p.json_value()
                                .map_err(|error| error.with_context("Triple.p"))?,
                        )
                        .with(
                            "o",
                            o.json_value()
                                .map_err(|error| error.with_context("Triple.o"))?,
                        ),
                )
                .into(),
        })
    }
}
impl FromJson for QTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(value, "QTerm", "one QTerm variant")?;
        let decode = || -> Result<Self, DecodeError> {
            Ok(match tag {
                "Const" => Self::Const(String::from_json(payload)?),
                "Var" => Self::Var(String::from_json(payload)?),
                "Num" => Self::Num(gmeow_errors::json::read_signed(
                    payload,
                    "i64 query number",
                )?),
                "Triple" => {
                    let mut fields = Record::new(payload, "struct variant QTerm::Triple")?;
                    Self::Triple {
                        s: fields.required("s")?,
                        p: fields.required("p")?,
                        o: fields.required("o")?,
                    }
                }
                name => {
                    return Err(DecodeError::unknown_variant(
                        name,
                        &["Const", "Var", "Num", "Triple"],
                    ));
                }
            })
        };
        decode().map_err(|error| error.within(tag))
    }
}
impl QBuiltin {
    pub(crate) fn json_value(&self) -> gmeow_errors::Result<Value> {
        Ok(match self {
            Self::TermCompare {
                left,
                right,
                comparison,
                expected,
            } => Object::new()
                .with(
                    "TermCompare",
                    Object::new()
                        .with("left", left.to_json())
                        .with("right", right.to_json())
                        .with("comparison", comparison.to_json())
                        .with("expected", expected.to_json()),
                )
                .into(),
            Self::TermIdentity {
                left,
                left_component,
                right,
                right_component,
                expected,
            } => Object::new()
                .with(
                    "TermIdentity",
                    Object::new()
                        .with("left", left.to_json())
                        .with("left_component", left_component.to_json())
                        .with("right", right.to_json())
                        .with("right_component", right_component.to_json())
                        .with("expected", expected.to_json()),
                )
                .into(),
            Self::TermEqual {
                left,
                right,
                expected,
            } => Object::new()
                .with(
                    "TermEqual",
                    Object::new()
                        .with("left", left.to_json())
                        .with("right", right.to_json())
                        .with("expected", expected.to_json()),
                )
                .into(),
            Self::TermKind {
                term,
                predicate,
                expected,
            } => Object::new()
                .with(
                    "TermKind",
                    Object::new()
                        .with("term", term.to_json())
                        .with("predicate", predicate.to_json())
                        .with("expected", expected.to_json()),
                )
                .into(),
            Self::Is {
                target,
                lhs,
                op,
                rhs,
            } => Object::new()
                .with(
                    "Is",
                    Object::new()
                        .with(
                            "target",
                            target
                                .json_value()
                                .map_err(|error| error.with_context("Is.target"))?,
                        )
                        .with(
                            "lhs",
                            lhs.json_value()
                                .map_err(|error| error.with_context("Is.lhs"))?,
                        )
                        .with("op", op.to_json())
                        .with(
                            "rhs",
                            rhs.json_value()
                                .map_err(|error| error.with_context("Is.rhs"))?,
                        ),
                )
                .into(),
            Self::Compare { lhs, op, rhs } => Object::new()
                .with(
                    "Compare",
                    Object::new()
                        .with(
                            "lhs",
                            lhs.json_value()
                                .map_err(|error| error.with_context("Compare.lhs"))?,
                        )
                        .with("op", op.to_json())
                        .with(
                            "rhs",
                            rhs.json_value()
                                .map_err(|error| error.with_context("Compare.rhs"))?,
                        ),
                )
                .into(),
            Self::BilinearSqDist { target, gram, x, y } => Object::new()
                .with(
                    "BilinearSqDist",
                    Object::new()
                        .with(
                            "target",
                            target
                                .json_value()
                                .map_err(|error| error.with_context("BilinearSqDist.target"))?,
                        )
                        .with(
                            "gram",
                            gram.json_value()
                                .map_err(|error| error.with_context("BilinearSqDist.gram"))?,
                        )
                        .with(
                            "x",
                            x.json_value()
                                .map_err(|error| error.with_context("BilinearSqDist.x"))?,
                        )
                        .with(
                            "y",
                            y.json_value()
                                .map_err(|error| error.with_context("BilinearSqDist.y"))?,
                        ),
                )
                .into(),
            Self::DimEqual { d1, d2 } => Object::new()
                .with(
                    "DimEqual",
                    Object::new()
                        .with(
                            "d1",
                            d1.json_value()
                                .map_err(|error| error.with_context("DimEqual.d1"))?,
                        )
                        .with(
                            "d2",
                            d2.json_value()
                                .map_err(|error| error.with_context("DimEqual.d2"))?,
                        ),
                )
                .into(),
            Self::DimProduct { d_f, d_m, d_r } => Object::new()
                .with(
                    "DimProduct",
                    Object::new()
                        .with(
                            "d_f",
                            d_f.json_value()
                                .map_err(|error| error.with_context("DimProduct.d_f"))?,
                        )
                        .with(
                            "d_m",
                            d_m.json_value()
                                .map_err(|error| error.with_context("DimProduct.d_m"))?,
                        )
                        .with(
                            "d_r",
                            d_r.json_value()
                                .map_err(|error| error.with_context("DimProduct.d_r"))?,
                        ),
                )
                .into(),
        })
    }
}
impl FromJson for QBuiltin {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "QBuiltin", "one QBuiltin variant")?;
        let decode = || -> Result<Self, DecodeError> {
            Ok(match tag {
                "TermCompare" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::TermCompare")?;
                    Self::TermCompare {
                        left: fields.required("left")?,
                        right: fields.required("right")?,
                        comparison: fields.required("comparison")?,
                        expected: fields.required("expected")?,
                    }
                }
                "TermIdentity" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::TermIdentity")?;
                    Self::TermIdentity {
                        left: fields.required("left")?,
                        left_component: fields.required("left_component")?,
                        right: fields.required("right")?,
                        right_component: fields.required("right_component")?,
                        expected: fields.required("expected")?,
                    }
                }
                "TermEqual" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::TermEqual")?;
                    Self::TermEqual {
                        left: fields.required("left")?,
                        right: fields.required("right")?,
                        expected: fields.required("expected")?,
                    }
                }
                "TermKind" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::TermKind")?;
                    Self::TermKind {
                        term: fields.required("term")?,
                        predicate: fields.required("predicate")?,
                        expected: fields.required("expected")?,
                    }
                }
                "Is" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::Is")?;
                    Self::Is {
                        target: fields.required("target")?,
                        lhs: fields.required("lhs")?,
                        op: fields.required("op")?,
                        rhs: fields.required("rhs")?,
                    }
                }
                "Compare" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::Compare")?;
                    Self::Compare {
                        lhs: fields.required("lhs")?,
                        op: fields.required("op")?,
                        rhs: fields.required("rhs")?,
                    }
                }
                "BilinearSqDist" => {
                    let mut fields =
                        Record::new(payload, "struct variant QBuiltin::BilinearSqDist")?;
                    Self::BilinearSqDist {
                        target: fields.required("target")?,
                        gram: fields.required("gram")?,
                        x: fields.required("x")?,
                        y: fields.required("y")?,
                    }
                }
                "DimEqual" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::DimEqual")?;
                    Self::DimEqual {
                        d1: fields.required("d1")?,
                        d2: fields.required("d2")?,
                    }
                }
                "DimProduct" => {
                    let mut fields = Record::new(payload, "struct variant QBuiltin::DimProduct")?;
                    Self::DimProduct {
                        d_f: fields.required("d_f")?,
                        d_m: fields.required("d_m")?,
                        d_r: fields.required("d_r")?,
                    }
                }
                name => {
                    return Err(DecodeError::unknown_variant(
                        name,
                        &[
                            "TermCompare",
                            "TermIdentity",
                            "TermEqual",
                            "TermKind",
                            "Is",
                            "Compare",
                            "BilinearSqDist",
                            "DimEqual",
                            "DimProduct",
                        ],
                    ));
                }
            })
        };
        decode().map_err(|error| error.within(tag))
    }
}
