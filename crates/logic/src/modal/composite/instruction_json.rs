// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact key projections for already-admitted modal DAG instructions.

use super::{Accessibility, AtomTerm, Instruction, NodeId, Quantifier, TemporalOperator};
use purrdf_lex::json::{Object, Value, record::ToJson};

impl ToJson for NodeId {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}

impl ToJson for Accessibility {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}

impl ToJson for Quantifier {
    fn to_json(&self) -> Value {
        match self {
            Self::Every => "Every",
            Self::Some => "Some",
        }
        .to_json()
    }
}

impl ToJson for TemporalOperator {
    fn to_json(&self) -> Value {
        match self {
            Self::Next => "Next",
            Self::Eventually => "Eventually",
            Self::Globally => "Globally",
        }
        .to_json()
    }
}

impl ToJson for AtomTerm {
    fn to_json(&self) -> Value {
        match self {
            Self::Ground(term) => Object::new().with("Ground", term.to_json()).into(),
            Self::Context => "Context".to_json(),
        }
    }
}

impl ToJson for Instruction {
    fn to_json(&self) -> Value {
        match self {
            Self::AtContext { context, body } => Object::new()
                .with(
                    "AtContext",
                    Value::from(
                        Object::new()
                            .with("context", context.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
            Self::Atom {
                relation,
                subject,
                object,
            } => Object::new()
                .with(
                    "Atom",
                    Value::from(
                        Object::new()
                            .with("relation", relation.to_json())
                            .with("subject", subject.to_json())
                            .with("object", object.to_json()),
                    ),
                )
                .into(),
            Self::Not(body) => Object::new().with("Not", body.to_json()).into(),
            Self::And(children) => Object::new().with("And", children.to_json()).into(),
            Self::Or(children) => Object::new().with("Or", children.to_json()).into(),
            Self::Implies(left, right) => Object::new()
                .with("Implies", (left, right).to_json())
                .into(),
            Self::Iff(left, right) => Object::new().with("Iff", (left, right).to_json()).into(),
            Self::Modal {
                quantifier,
                axis,
                body,
            } => Object::new()
                .with(
                    "Modal",
                    Value::from(
                        Object::new()
                            .with("quantifier", quantifier.to_json())
                            .with("axis", axis.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
            Self::Temporal { operator, body } => Object::new()
                .with(
                    "Temporal",
                    Value::from(
                        Object::new()
                            .with("operator", operator.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
            Self::Until { left, right } => Object::new()
                .with(
                    "Until",
                    Value::from(
                        Object::new()
                            .with("left", left.to_json())
                            .with("right", right.to_json()),
                    ),
                )
                .into(),
        }
    }
}
