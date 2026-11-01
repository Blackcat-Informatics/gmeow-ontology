// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete named-record scoped-witness transport. Intrinsic address and proof
//! validation stays on WitnessDerivation, outside data-only field hydration.

use super::{
    WitnessDerivation, WitnessHead, WitnessOrigin, WitnessPosition, WitnessScope, WitnessStatement,
};
use crate::physical::SelectedLogicalWorld;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within, items_with},
};

impl ToJson for WitnessOrigin {
    fn to_json(&self) -> Value {
        match self {
            Self::Rule => "Rule".to_json(),
            Self::NonemptyDomain(selected) => Object::new()
                .with("NonemptyDomain", selected.to_json())
                .into(),
        }
    }
}

impl FromJson for WitnessOrigin {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if value
            .as_object()
            .and_then(|object| object.get("NonemptyDomain"))
            .is_some()
        {
            let (name, payload) = gmeow_errors::json::read_variant(
                value,
                "WitnessOrigin",
                "one WitnessOrigin variant",
            )?;
            return SelectedLogicalWorld::from_json(payload)
                .map(Self::NonemptyDomain)
                .map_err(|error| error.within(name));
        }
        match gmeow_errors::json::read_unit_name(value, "WitnessOrigin")? {
            "Rule" => Ok(Self::Rule),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Rule", "NonemptyDomain"],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for WitnessScope {
    "world" => world,
    "contract" => contract,
    "source_rule" => source_rule,
    "origin" => origin,
});

impl FromJson for WitnessScope {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct WitnessScope")?;
        let scope = Self {
            world: fields.required("world")?,
            contract: fields
                .required_with("contract", gmeow_errors::json::read_byte_array::<32>)?,
            source_rule: fields
                .required_with("source_rule", gmeow_errors::json::read_byte_array::<32>)?,
            origin: fields.required("origin")?,
        };
        fields.deny_unknown()?;
        Ok(scope)
    }
}

impl ToJson for WitnessStatement {
    fn to_json(&self) -> Value {
        Object::new()
            .with("subject", crate::term_serde::to_json(&self.subject))
            .with("predicate", self.predicate.to_json())
            .with("object", crate::term_serde::to_json(&self.object))
            .into()
    }
}

impl FromJson for WitnessStatement {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct WitnessStatement")?;
        let statement = Self {
            subject: fields.required_with("subject", crate::term_serde::from_json)?,
            predicate: fields.required("predicate")?,
            object: fields.required_with("object", crate::term_serde::from_json)?,
        };
        fields.deny_unknown()?;
        Ok(statement)
    }
}

impl ToJson for WitnessPosition {
    fn to_json(&self) -> Value {
        match self {
            Self::Subject => "Subject",
            Self::Object => "Object",
        }
        .to_json()
    }
}

impl FromJson for WitnessPosition {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "WitnessPosition")? {
            "Subject" => Ok(Self::Subject),
            "Object" => Ok(Self::Object),
            name => Err(DecodeError::unknown_variant(name, &["Subject", "Object"])),
        }
    }
}

::purrdf_lex::json_record!(WitnessHead as "struct WitnessHead" {
    "statement" => statement: required::<WitnessStatement>,
    "positions" => positions: required::<Vec<WitnessPosition>>,
    "premises" => premises: required::<Vec<WitnessStatement>>,
    "derivation_id" => derivation_id: required::<String>,
});

impl ToJson for WitnessDerivation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("witness", self.witness.to_json())
            .with("rule_iri", self.rule_iri.to_json())
            .with("scope", self.scope.to_json())
            .with("ordinal", self.ordinal.to_json())
            .with(
                "frontier",
                Value::Array(
                    self.frontier
                        .iter()
                        .map(crate::term_serde::to_json)
                        .collect(),
                ),
            )
            .with("heads", self.heads.to_json())
            .into()
    }
}

impl FromJson for WitnessDerivation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct WitnessDerivation")?;
        let witness = Self {
            witness: fields.required("witness")?,
            rule_iri: fields.required("rule_iri")?,
            scope: fields.required("scope")?,
            ordinal: fields.required_with("ordinal", |value| {
                gmeow_errors::json::read_unsigned::<usize>(value, "an unsigned witness ordinal")
            })?,
            frontier: fields.required_with("frontier", |value| -> Result<_, DecodeError> {
                items_with(value, crate::term_serde::from_json)
            })?,
            heads: fields.required("heads")?,
        };
        fields.deny_unknown()?;
        Ok(witness)
    }
}
