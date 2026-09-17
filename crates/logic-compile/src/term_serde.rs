// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared value-only codec for retained native reasoning terms and presentation
//! metadata. No dataset-local term IDs and no execution certificates.

use purrdf::{BlankScope, RdfTextDirection, TermValue};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize)]
enum Value {
    Iri(String),
    Blank {
        label: String,
        scope: u32,
    },
    Literal {
        lexical_form: String,
        datatype: String,
        language: Option<String>,
        direction: Option<Direction>,
    },
    Triple {
        s: Box<Value>,
        p: Box<Value>,
        o: Box<Value>,
    },
}

#[derive(Serialize, Deserialize)]
enum Direction {
    Ltr,
    Rtl,
}

impl From<&TermValue> for Value {
    fn from(value: &TermValue) -> Self {
        match value {
            TermValue::Iri(iri) => Self::Iri(iri.clone()),
            TermValue::Blank { label, scope } => Self::Blank {
                label: label.clone(),
                scope: scope.0,
            },
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => Self::Literal {
                lexical_form: lexical_form.clone(),
                datatype: datatype.clone(),
                language: language.clone(),
                direction: direction.map(|direction| match direction {
                    RdfTextDirection::Ltr => Direction::Ltr,
                    RdfTextDirection::Rtl => Direction::Rtl,
                }),
            },
            TermValue::Triple { s, p, o } => Self::Triple {
                s: Box::new(Self::from(s.as_ref())),
                p: Box::new(Self::from(p.as_ref())),
                o: Box::new(Self::from(o.as_ref())),
            },
        }
    }
}

impl From<Value> for TermValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Iri(iri) => Self::Iri(iri),
            Value::Blank { label, scope } => Self::Blank {
                label,
                scope: BlankScope(scope),
            },
            Value::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => Self::Literal {
                lexical_form,
                datatype,
                language,
                direction: direction.map(|direction| match direction {
                    Direction::Ltr => RdfTextDirection::Ltr,
                    Direction::Rtl => RdfTextDirection::Rtl,
                }),
            },
            Value::Triple { s, p, o } => Self::Triple {
                s: Box::new((*s).into()),
                p: Box::new((*p).into()),
                o: Box::new((*o).into()),
            },
        }
    }
}

/// Serialize a complete native RDF 1.2 term without dataset-local identifiers.
pub fn serialize<S: Serializer>(value: &TermValue, serializer: S) -> Result<S::Ok, S::Error> {
    Value::from(value).serialize(serializer)
}

/// Deserialize a retained term with blank scope and nested statement identity.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<TermValue, D::Error> {
    Value::deserialize(deserializer).map(Into::into)
}

/// The same native-term codec for an explicitly absent default graph or a named graph.
pub mod optional {
    use super::{Deserialize, Deserializer, Serialize, Serializer, TermValue, Value};

    /// Serialize an optional complete native term; absence remains explicit.
    pub fn serialize<S: Serializer>(
        value: &Option<TermValue>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(Value::from).serialize(serializer)
    }

    /// Deserialize the explicit graph choice without accepting a missing field.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<TermValue>, D::Error> {
        Option::<Value>::deserialize(deserializer).map(|value| value.map(Into::into))
    }
}

/// The same native-term codec for an ordered collection of retained operands.
pub mod vec {
    use super::{Deserialize, Deserializer, Serializer, TermValue, Value};

    /// Serialize each native value directly into the enclosing sequence.
    pub fn serialize<S: Serializer>(
        values: &[TermValue],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(values.iter().map(Value::from))
    }

    /// Retain collection order, blank scope and complete nested value identity.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<TermValue>, D::Error> {
        Vec::<Value>::deserialize(deserializer)
            .map(|values| values.into_iter().map(Into::into).collect())
    }
}
