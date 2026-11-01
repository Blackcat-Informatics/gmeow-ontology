// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete validation-IR transport. Hydration retains fields and collection order.

use super::{
    ConstraintComponent, ConstraintProvenance, NodeKind, PropertyConstraintIr, ShaclNodeKind,
    ShaclSeverity, ShapeTarget, ShapeValue, ValidationShapeIr,
};
use crate::ir::literal_serde;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within, items_with},
};

impl ToJson for NodeKind {
    fn to_json(&self) -> Value {
        match self {
            Self::ObjectLevelFormula => "ObjectLevelFormula",
            Self::MetaLevelFormula => "MetaLevelFormula",
            Self::Constraint => "Constraint",
            Self::DerivationRule => "DerivationRule",
            Self::Query => "Query",
            Self::TransactionProgram => "TransactionProgram",
            Self::ActionSchema => "ActionSchema",
            Self::ValidationShape => "ValidationShape",
            Self::Correspondence => "Correspondence",
            Self::Annotation => "Annotation",
        }
        .to_json()
    }
}
impl FromJson for NodeKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "NodeKind")? {
            "ObjectLevelFormula" => Ok(Self::ObjectLevelFormula),
            "MetaLevelFormula" => Ok(Self::MetaLevelFormula),
            "Constraint" => Ok(Self::Constraint),
            "DerivationRule" => Ok(Self::DerivationRule),
            "Query" => Ok(Self::Query),
            "TransactionProgram" => Ok(Self::TransactionProgram),
            "ActionSchema" => Ok(Self::ActionSchema),
            "ValidationShape" => Ok(Self::ValidationShape),
            "Correspondence" => Ok(Self::Correspondence),
            "Annotation" => Ok(Self::Annotation),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "ObjectLevelFormula",
                    "MetaLevelFormula",
                    "Constraint",
                    "DerivationRule",
                    "Query",
                    "TransactionProgram",
                    "ActionSchema",
                    "ValidationShape",
                    "Correspondence",
                    "Annotation",
                ],
            )),
        }
    }
}

impl ToJson for ShaclNodeKind {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri => "Iri",
            Self::Literal => "Literal",
            Self::BlankNode => "BlankNode",
            Self::IriOrLiteral => "IriOrLiteral",
            Self::BlankNodeOrIri => "BlankNodeOrIri",
            Self::BlankNodeOrLiteral => "BlankNodeOrLiteral",
        }
        .to_json()
    }
}
impl FromJson for ShaclNodeKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "ShaclNodeKind")? {
            "Iri" => Ok(Self::Iri),
            "Literal" => Ok(Self::Literal),
            "BlankNode" => Ok(Self::BlankNode),
            "IriOrLiteral" => Ok(Self::IriOrLiteral),
            "BlankNodeOrIri" => Ok(Self::BlankNodeOrIri),
            "BlankNodeOrLiteral" => Ok(Self::BlankNodeOrLiteral),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Iri",
                    "Literal",
                    "BlankNode",
                    "IriOrLiteral",
                    "BlankNodeOrIri",
                    "BlankNodeOrLiteral",
                ],
            )),
        }
    }
}

impl ToJson for ShaclSeverity {
    fn to_json(&self) -> Value {
        match self {
            Self::Violation => "Violation",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
        .to_json()
    }
}
impl FromJson for ShaclSeverity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "ShaclSeverity")? {
            "Violation" => Ok(Self::Violation),
            "Warning" => Ok(Self::Warning),
            "Info" => Ok(Self::Info),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Violation", "Warning", "Info"],
            )),
        }
    }
}

impl ToJson for ConstraintProvenance {
    fn to_json(&self) -> Value {
        match self {
            Self::OwlRestriction => "OwlRestriction",
            Self::OptNative => "OptNative",
        }
        .to_json()
    }
}
impl FromJson for ConstraintProvenance {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "ConstraintProvenance")? {
            "OwlRestriction" => Ok(Self::OwlRestriction),
            "OptNative" => Ok(Self::OptNative),
            name => Err(DecodeError::unknown_variant(
                name,
                &["OwlRestriction", "OptNative"],
            )),
        }
    }
}

impl ToJson for ShapeTarget {
    fn to_json(&self) -> Value {
        match self {
            Self::Class(value) => Object::new().with("Class", value.to_json()).into(),
            Self::SubjectsOf(value) => Object::new().with("SubjectsOf", value.to_json()).into(),
            Self::ObjectsOf(value) => Object::new().with("ObjectsOf", value.to_json()).into(),
            Self::ValueKeyed { predicate, value } => Object::new()
                .with(
                    "ValueKeyed",
                    Value::from(
                        Object::new()
                            .with("predicate", predicate.to_json())
                            .with("value", value.to_json()),
                    ),
                )
                .into(),
            Self::DirectClass(value) => Object::new().with("DirectClass", value.to_json()).into(),
            Self::Sparql(value) => Object::new().with("Sparql", value.to_json()).into(),
        }
    }
}
impl FromJson for ShapeTarget {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "ShapeTarget", "one ShapeTarget variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Class" => String::from_json(payload).map(Self::Class),
                "SubjectsOf" => String::from_json(payload).map(Self::SubjectsOf),
                "ObjectsOf" => String::from_json(payload).map(Self::ObjectsOf),
                "ValueKeyed" => {
                    let mut fields =
                        Record::new(payload, "struct variant ShapeTarget::ValueKeyed")?;
                    Ok(Self::ValueKeyed {
                        predicate: fields.required("predicate")?,
                        value: fields.required("value")?,
                    })
                }
                "DirectClass" => String::from_json(payload).map(Self::DirectClass),
                "Sparql" => String::from_json(payload).map(Self::Sparql),
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "Class",
                        "SubjectsOf",
                        "ObjectsOf",
                        "ValueKeyed",
                        "DirectClass",
                        "Sparql",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
impl ToJson for ShapeValue {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri(value) => Object::new().with("Iri", value.to_json()).into(),
            Self::Literal(value) => Object::new()
                .with("Literal", literal_serde::single::to_json(value))
                .into(),
        }
    }
}
impl FromJson for ShapeValue {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "ShapeValue", "one ShapeValue variant")?;
        let decoded = match tag {
            "Iri" => String::from_json(payload).map(Self::Iri),
            "Literal" => literal_serde::single::from_json(payload).map(Self::Literal),
            name => Err(DecodeError::unknown_variant(name, &["Iri", "Literal"])),
        };
        decoded.map_err(|error| error.within(tag))
    }
}

impl ToJson for PropertyConstraintIr {
    fn to_json(&self) -> Value {
        Object::new()
            .with("path", self.path.to_json())
            .with("min_count", self.min_count.to_json())
            .with("max_count", self.max_count.to_json())
            .with(
                "cardinality_provenance",
                self.cardinality_provenance.to_json(),
            )
            .with("components", self.components.to_json())
            .with("inverse", self.inverse.to_json())
            .with("reifier_shape", self.reifier_shape.to_json())
            .with("reification_required", self.reification_required.to_json())
            .with("severity", self.severity.to_json())
            .with("messages", literal_serde::to_json(&self.messages))
            .into()
    }
}
impl FromJson for PropertyConstraintIr {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PropertyConstraintIr")?;
        Ok(Self {
            path: fields.required("path")?,
            min_count: fields.optional_with("min_count", |value| {
                gmeow_errors::json::read_unsigned(value, "u32 minimum count")
            })?,
            max_count: fields.optional_with("max_count", |value| {
                gmeow_errors::json::read_unsigned(value, "u32 maximum count")
            })?,
            cardinality_provenance: fields.optional("cardinality_provenance")?,
            components: fields.required("components")?,
            inverse: fields.required("inverse")?,
            reifier_shape: fields.optional("reifier_shape")?,
            reification_required: fields.required("reification_required")?,
            severity: fields.optional("severity")?,
            messages: fields.required_with("messages", literal_serde::message_set::from_json)?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for ValidationShapeIr {
    "iri" => iri,
    "target" => target,
    "properties" => properties,
    "node_kind" => node_kind,
    "standpoint" => standpoint,
    "node_components" => node_components,
    "label" => label,
    "failure_class" => failure_class,
});
impl FromJson for ValidationShapeIr {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ValidationShapeIr")?;
        Ok(Self {
            iri: fields.required("iri")?,
            target: fields.required("target")?,
            properties: fields.required("properties")?,
            node_kind: fields.required("node_kind")?,
            standpoint: fields.optional("standpoint")?,
            node_components: fields.required("node_components")?,
            label: fields.optional("label")?,
            failure_class: fields.optional("failure_class")?,
        })
    }
}

impl ToJson for ConstraintComponent {
    fn to_json(&self) -> Value {
        match self {
            Self::NumericRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => Object::new()
                .with(
                    "NumericRange",
                    Value::from(
                        Object::new()
                            .with("min", min.to_json())
                            .with("max", max.to_json())
                            .with("min_inclusive", min_inclusive.to_json())
                            .with("max_inclusive", max_inclusive.to_json()),
                    ),
                )
                .into(),
            Self::PrecisionRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => Object::new()
                .with(
                    "PrecisionRange",
                    Value::from(
                        Object::new()
                            .with("min", min.to_json())
                            .with("max", max.to_json())
                            .with("min_inclusive", min_inclusive.to_json())
                            .with("max_inclusive", max_inclusive.to_json()),
                    ),
                )
                .into(),
            Self::Datatype(value) => Object::new().with("Datatype", value.to_json()).into(),
            Self::Class(value) => Object::new().with("Class", value.to_json()).into(),
            Self::NodeKindShacl(value) => {
                Object::new().with("NodeKindShacl", value.to_json()).into()
            }
            Self::In(value) => Object::new().with("In", value.to_json()).into(),
            Self::Pattern { regex, flags } => Object::new()
                .with(
                    "Pattern",
                    Value::from(
                        Object::new()
                            .with("regex", regex.to_json())
                            .with("flags", flags.to_json()),
                    ),
                )
                .into(),
            Self::MinLength(value) => Object::new().with("MinLength", value.to_json()).into(),
            Self::MaxLength(value) => Object::new().with("MaxLength", value.to_json()).into(),
            Self::LanguageIn(value) => Object::new().with("LanguageIn", value.to_json()).into(),
            Self::DateTimeRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => Object::new()
                .with(
                    "DateTimeRange",
                    Value::from(
                        Object::new()
                            .with("min", min.to_json())
                            .with("max", max.to_json())
                            .with("min_inclusive", min_inclusive.to_json())
                            .with("max_inclusive", max_inclusive.to_json()),
                    ),
                )
                .into(),
            Self::TerminologyBinding {
                terminology_id,
                codes,
            } => Object::new()
                .with(
                    "TerminologyBinding",
                    Value::from(
                        Object::new()
                            .with("terminology_id", terminology_id.to_json())
                            .with("codes", codes.to_json()),
                    ),
                )
                .into(),
            Self::OrdinalSet { pairs } => Object::new()
                .with(
                    "OrdinalSet",
                    Value::from(Object::new().with("pairs", pairs.to_json())),
                )
                .into(),
            Self::DateTimePattern(value) => Object::new()
                .with("DateTimePattern", value.to_json())
                .into(),
            Self::HasValue(value) => Object::new().with("HasValue", value.to_json()).into(),
            Self::QualifiedValueShape { shape, min, max } => Object::new()
                .with(
                    "QualifiedValueShape",
                    Value::from(
                        Object::new()
                            .with("shape", shape.to_json())
                            .with("min", min.to_json())
                            .with("max", max.to_json()),
                    ),
                )
                .into(),
            Self::Not(value) => Object::new().with("Not", value.to_json()).into(),
            Self::Or(value) => Object::new().with("Or", value.to_json()).into(),
            Self::Xone(value) => Object::new().with("Xone", value.to_json()).into(),
            Self::OrProperties(value) => Object::new().with("OrProperties", value.to_json()).into(),
            Self::UniqueLang => "UniqueLang".to_json(),
        }
    }
}
impl FromJson for ConstraintComponent {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if let Some(name) = value.as_str() {
            return match name {
                "UniqueLang" => Ok(Self::UniqueLang),
                name => Err(DecodeError::unknown_variant(name, COMPONENT_VARIANTS)),
            };
        }
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "ConstraintComponent",
            "one ConstraintComponent variant",
        )?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "NumericRange" => {
                    let mut fields =
                        Record::new(payload, "struct variant ConstraintComponent::NumericRange")?;
                    Ok(Self::NumericRange {
                        min: fields.optional::<f64>("min")?,
                        max: fields.optional::<f64>("max")?,
                        min_inclusive: fields.required("min_inclusive")?,
                        max_inclusive: fields.required("max_inclusive")?,
                    })
                }
                "PrecisionRange" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant ConstraintComponent::PrecisionRange",
                    )?;
                    Ok(Self::PrecisionRange {
                        min: fields.optional::<f64>("min")?,
                        max: fields.optional::<f64>("max")?,
                        min_inclusive: fields.required("min_inclusive")?,
                        max_inclusive: fields.required("max_inclusive")?,
                    })
                }
                "Datatype" => String::from_json(payload).map(Self::Datatype),
                "Class" => String::from_json(payload).map(Self::Class),
                "NodeKindShacl" => ShaclNodeKind::from_json(payload).map(Self::NodeKindShacl),
                "In" => Vec::<ShapeValue>::from_json(payload).map(Self::In),
                "Pattern" => {
                    let mut fields =
                        Record::new(payload, "struct variant ConstraintComponent::Pattern")?;
                    Ok(Self::Pattern {
                        regex: fields.required("regex")?,
                        flags: fields.optional("flags")?,
                    })
                }
                "MinLength" => gmeow_errors::json::read_unsigned(payload, "u32 string length")
                    .map(Self::MinLength),
                "MaxLength" => gmeow_errors::json::read_unsigned(payload, "u32 string length")
                    .map(Self::MaxLength),
                "LanguageIn" => Vec::<String>::from_json(payload).map(Self::LanguageIn),
                "DateTimeRange" => {
                    let mut fields =
                        Record::new(payload, "struct variant ConstraintComponent::DateTimeRange")?;
                    Ok(Self::DateTimeRange {
                        min: fields.optional("min")?,
                        max: fields.optional("max")?,
                        min_inclusive: fields.required("min_inclusive")?,
                        max_inclusive: fields.required("max_inclusive")?,
                    })
                }
                "TerminologyBinding" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant ConstraintComponent::TerminologyBinding",
                    )?;
                    Ok(Self::TerminologyBinding {
                        terminology_id: fields.required("terminology_id")?,
                        codes: fields.required("codes")?,
                    })
                }
                "OrdinalSet" => {
                    let mut fields =
                        Record::new(payload, "struct variant ConstraintComponent::OrdinalSet")?;
                    Ok(Self::OrdinalSet {
                        pairs: fields.required_with("pairs", ordinal_pairs)?,
                    })
                }
                "DateTimePattern" => String::from_json(payload).map(Self::DateTimePattern),
                "HasValue" => ShapeValue::from_json(payload).map(Self::HasValue),
                "QualifiedValueShape" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant ConstraintComponent::QualifiedValueShape",
                    )?;
                    Ok(Self::QualifiedValueShape {
                        shape: fields.required("shape")?,
                        min: fields.optional_with("min", |value| {
                            gmeow_errors::json::read_unsigned(value, "u32 qualified count")
                        })?,
                        max: fields.optional_with("max", |value| {
                            gmeow_errors::json::read_unsigned(value, "u32 qualified count")
                        })?,
                    })
                }
                "Not" => Box::<Self>::from_json(payload).map(Self::Not),
                "Or" => Vec::<Self>::from_json(payload).map(Self::Or),
                "Xone" => Vec::<Self>::from_json(payload).map(Self::Xone),
                "OrProperties" => Vec::<String>::from_json(payload).map(Self::OrProperties),
                "UniqueLang" => gmeow_errors::json::read_null(payload, "null UniqueLang payload")
                    .map(|()| Self::UniqueLang),
                name => Err(DecodeError::unknown_variant(name, COMPONENT_VARIANTS)),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}

const COMPONENT_VARIANTS: &[&str] = &[
    "NumericRange",
    "PrecisionRange",
    "Datatype",
    "Class",
    "NodeKindShacl",
    "In",
    "Pattern",
    "MinLength",
    "MaxLength",
    "LanguageIn",
    "DateTimeRange",
    "TerminologyBinding",
    "OrdinalSet",
    "DateTimePattern",
    "HasValue",
    "QualifiedValueShape",
    "Not",
    "Or",
    "Xone",
    "OrProperties",
    "UniqueLang",
];

fn ordinal_pairs(value: &Value) -> Result<Vec<(i64, String)>, DecodeError> {
    items_with(value, |row| -> Result<_, DecodeError> {
        let cells = row
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(row, "ordinal and symbol row"))?;
        if cells.len() != 2 {
            return Err(DecodeError::invalid_value(
                row,
                "exactly two ordinal and symbol cells",
            ));
        }
        let ordinal = gmeow_errors::json::read_signed::<i64>(&cells[0], "i64 ordinal")
            .map_err(|error| error.within_index_native(0, None))?;
        let symbol =
            String::from_json(&cells[1]).map_err(|error| error.within_index_native(1, None))?;
        Ok((ordinal, symbol))
    })
}

#[cfg(test)]
mod tests;
