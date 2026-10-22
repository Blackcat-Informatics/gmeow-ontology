// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native typed lowering rejections; lowering policy and failure classes stay with the enum.
use super::MathLoweringError;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for MathLoweringError {
    fn to_json(&self) -> Value {
        match self {
            Self::NumberLiteralMissingValue { node } => Object::new()
                .with(
                    "NumberLiteralMissingValue",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::UnrecognizedExpressionType { node, types } => Object::new()
                .with(
                    "UnrecognizedExpressionType",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("types", types.to_json()),
                    ),
                )
                .into(),
            Self::ArgumentSlotMissingIndex { slot } => Object::new()
                .with(
                    "ArgumentSlotMissingIndex",
                    Value::from(Object::new().with("slot", slot.to_json())),
                )
                .into(),
            Self::ArgumentSlotMultipleIndexes { slot, count } => Object::new()
                .with(
                    "ArgumentSlotMultipleIndexes",
                    Value::from(
                        Object::new()
                            .with("slot", slot.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::ArgumentSlotIndexNotInteger { slot, lexical } => Object::new()
                .with(
                    "ArgumentSlotIndexNotInteger",
                    Value::from(
                        Object::new()
                            .with("slot", slot.to_json())
                            .with("lexical", lexical.to_json()),
                    ),
                )
                .into(),
            Self::ArgumentSlotMissingExpression { slot } => Object::new()
                .with(
                    "ArgumentSlotMissingExpression",
                    Value::from(Object::new().with("slot", slot.to_json())),
                )
                .into(),
            Self::NonContiguousArgumentSlots {
                node,
                index,
                expected_position,
            } => Object::new()
                .with(
                    "NonContiguousArgumentSlots",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("index", index.to_json())
                            .with("expected_position", expected_position.to_json()),
                    ),
                )
                .into(),
            Self::DuplicateArgumentSlotIndex { node, index } => Object::new()
                .with(
                    "DuplicateArgumentSlotIndex",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("index", index.to_json()),
                    ),
                )
                .into(),
            Self::NegativeArgumentSlotIndex { node, slot, index } => Object::new()
                .with(
                    "NegativeArgumentSlotIndex",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("slot", slot.to_json())
                            .with("index", index.to_json()),
                    ),
                )
                .into(),
            Self::ApplicationMissingOperator { node } => Object::new()
                .with(
                    "ApplicationMissingOperator",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::ApplicationMultipleOperators { node, count } => Object::new()
                .with(
                    "ApplicationMultipleOperators",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::BindingMissingOperator { node } => Object::new()
                .with(
                    "BindingMissingOperator",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::BindingMultipleOperators { node, count } => Object::new()
                .with(
                    "BindingMultipleOperators",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::BindingMissingBoundVariable { node } => Object::new()
                .with(
                    "BindingMissingBoundVariable",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::BindingMissingBody { node } => Object::new()
                .with(
                    "BindingMissingBody",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::BindingMultipleBoundVariables { node, count } => Object::new()
                .with(
                    "BindingMultipleBoundVariables",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::VariableExpressionMissingOccurrence { node } => Object::new()
                .with(
                    "VariableExpressionMissingOccurrence",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::VariableExpressionMultipleOccurrences { node, count } => Object::new()
                .with(
                    "VariableExpressionMultipleOccurrences",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::OccurrenceMissingDeclaredVariable { occurrence } => Object::new()
                .with(
                    "OccurrenceMissingDeclaredVariable",
                    Value::from(Object::new().with("occurrence", occurrence.to_json())),
                )
                .into(),
            Self::OccurrenceMultipleDeclaredVariables { occurrence, count } => Object::new()
                .with(
                    "OccurrenceMultipleDeclaredVariables",
                    Value::from(
                        Object::new()
                            .with("occurrence", occurrence.to_json())
                            .with("count", count.to_json()),
                    ),
                )
                .into(),
            Self::UnscopedOccurrence {
                occurrence,
                declaration,
            } => Object::new()
                .with(
                    "UnscopedOccurrence",
                    Value::from(
                        Object::new()
                            .with("occurrence", occurrence.to_json())
                            .with("declaration", declaration.to_json()),
                    ),
                )
                .into(),
            Self::CyclicExpressionGraph { node } => Object::new()
                .with(
                    "CyclicExpressionGraph",
                    Value::from(Object::new().with("node", node.to_json())),
                )
                .into(),
            Self::ExpressionDepthExceeded { node, depth } => Object::new()
                .with(
                    "ExpressionDepthExceeded",
                    Value::from(
                        Object::new()
                            .with("node", node.to_json())
                            .with("depth", depth.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for MathLoweringError {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "MathLoweringError",
            "one MathLoweringError variant",
        )?;
        use purrdf_lex::json::record::Within;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "NumberLiteralMissingValue" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::NumberLiteralMissingValue",
                    )?;
                    Ok(Self::NumberLiteralMissingValue {
                        node: fields.required("node")?,
                    })
                }
                "UnrecognizedExpressionType" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::UnrecognizedExpressionType",
                    )?;
                    Ok(Self::UnrecognizedExpressionType {
                        node: fields.required("node")?,
                        types: fields.required("types")?,
                    })
                }
                "ArgumentSlotMissingIndex" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ArgumentSlotMissingIndex",
                    )?;
                    Ok(Self::ArgumentSlotMissingIndex {
                        slot: fields.required("slot")?,
                    })
                }
                "ArgumentSlotMultipleIndexes" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ArgumentSlotMultipleIndexes",
                    )?;
                    Ok(Self::ArgumentSlotMultipleIndexes {
                        slot: fields.required("slot")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "ArgumentSlotIndexNotInteger" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ArgumentSlotIndexNotInteger",
                    )?;
                    Ok(Self::ArgumentSlotIndexNotInteger {
                        slot: fields.required("slot")?,
                        lexical: fields.required("lexical")?,
                    })
                }
                "ArgumentSlotMissingExpression" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ArgumentSlotMissingExpression",
                    )?;
                    Ok(Self::ArgumentSlotMissingExpression {
                        slot: fields.required("slot")?,
                    })
                }
                "NonContiguousArgumentSlots" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::NonContiguousArgumentSlots",
                    )?;
                    Ok(Self::NonContiguousArgumentSlots {
                        node: fields.required("node")?,
                        index: fields.required("index")?,
                        expected_position: fields.required_with("expected_position", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "DuplicateArgumentSlotIndex" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::DuplicateArgumentSlotIndex",
                    )?;
                    Ok(Self::DuplicateArgumentSlotIndex {
                        node: fields.required("node")?,
                        index: fields.required("index")?,
                    })
                }
                "NegativeArgumentSlotIndex" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::NegativeArgumentSlotIndex",
                    )?;
                    Ok(Self::NegativeArgumentSlotIndex {
                        node: fields.required("node")?,
                        slot: fields.required("slot")?,
                        index: fields.required("index")?,
                    })
                }
                "ApplicationMissingOperator" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ApplicationMissingOperator",
                    )?;
                    Ok(Self::ApplicationMissingOperator {
                        node: fields.required("node")?,
                    })
                }
                "ApplicationMultipleOperators" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ApplicationMultipleOperators",
                    )?;
                    Ok(Self::ApplicationMultipleOperators {
                        node: fields.required("node")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "BindingMissingOperator" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::BindingMissingOperator",
                    )?;
                    Ok(Self::BindingMissingOperator {
                        node: fields.required("node")?,
                    })
                }
                "BindingMultipleOperators" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::BindingMultipleOperators",
                    )?;
                    Ok(Self::BindingMultipleOperators {
                        node: fields.required("node")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "BindingMissingBoundVariable" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::BindingMissingBoundVariable",
                    )?;
                    Ok(Self::BindingMissingBoundVariable {
                        node: fields.required("node")?,
                    })
                }
                "BindingMissingBody" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::BindingMissingBody",
                    )?;
                    Ok(Self::BindingMissingBody {
                        node: fields.required("node")?,
                    })
                }
                "BindingMultipleBoundVariables" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::BindingMultipleBoundVariables",
                    )?;
                    Ok(Self::BindingMultipleBoundVariables {
                        node: fields.required("node")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "VariableExpressionMissingOccurrence" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::VariableExpressionMissingOccurrence",
                    )?;
                    Ok(Self::VariableExpressionMissingOccurrence {
                        node: fields.required("node")?,
                    })
                }
                "VariableExpressionMultipleOccurrences" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::VariableExpressionMultipleOccurrences",
                    )?;
                    Ok(Self::VariableExpressionMultipleOccurrences {
                        node: fields.required("node")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "OccurrenceMissingDeclaredVariable" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::OccurrenceMissingDeclaredVariable",
                    )?;
                    Ok(Self::OccurrenceMissingDeclaredVariable {
                        occurrence: fields.required("occurrence")?,
                    })
                }
                "OccurrenceMultipleDeclaredVariables" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::OccurrenceMultipleDeclaredVariables",
                    )?;
                    Ok(Self::OccurrenceMultipleDeclaredVariables {
                        occurrence: fields.required("occurrence")?,
                        count: fields.required_with("count", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                "UnscopedOccurrence" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::UnscopedOccurrence",
                    )?;
                    Ok(Self::UnscopedOccurrence {
                        occurrence: fields.required("occurrence")?,
                        declaration: fields.required("declaration")?,
                    })
                }
                "CyclicExpressionGraph" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::CyclicExpressionGraph",
                    )?;
                    Ok(Self::CyclicExpressionGraph {
                        node: fields.required("node")?,
                    })
                }
                "ExpressionDepthExceeded" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant MathLoweringError::ExpressionDepthExceeded",
                    )?;
                    Ok(Self::ExpressionDepthExceeded {
                        node: fields.required("node")?,
                        depth: fields.required_with("depth", |value| {
                            gmeow_errors::json::read_unsigned(value, "usize")
                        })?,
                    })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "NumberLiteralMissingValue",
                        "UnrecognizedExpressionType",
                        "ArgumentSlotMissingIndex",
                        "ArgumentSlotMultipleIndexes",
                        "ArgumentSlotIndexNotInteger",
                        "ArgumentSlotMissingExpression",
                        "NonContiguousArgumentSlots",
                        "DuplicateArgumentSlotIndex",
                        "NegativeArgumentSlotIndex",
                        "ApplicationMissingOperator",
                        "ApplicationMultipleOperators",
                        "BindingMissingOperator",
                        "BindingMultipleOperators",
                        "BindingMissingBoundVariable",
                        "BindingMissingBody",
                        "BindingMultipleBoundVariables",
                        "VariableExpressionMissingOccurrence",
                        "VariableExpressionMultipleOccurrences",
                        "OccurrenceMissingDeclaredVariable",
                        "OccurrenceMultipleDeclaredVariables",
                        "UnscopedOccurrence",
                        "CyclicExpressionGraph",
                        "ExpressionDepthExceeded",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
#[cfg(test)]
mod tests;
