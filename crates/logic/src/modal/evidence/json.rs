// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named-record transport for observational modal evaluation evidence.

use super::{ModalEvaluation, ModalFrontier, ModalOp, ModalWorldEvidence};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

impl ToJson for ModalOp {
    fn to_json(&self) -> Value {
        match self {
            Self::Box => "Box",
            Self::Diamond => "Diamond",
        }
        .to_json()
    }
}

impl FromJson for ModalOp {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "ModalOp")? {
            "Box" => Ok(Self::Box),
            "Diamond" => Ok(Self::Diamond),
            name => Err(DecodeError::unknown_variant(name, &["Box", "Diamond"])),
        }
    }
}

::purrdf_lex::json_record!(ModalWorldEvidence as "struct ModalWorldEvidence" {
    "world" => world: required::<String>,
    "atom_present" => atom_present: required::<bool>,
});

impl ToJson for ModalFrontier {
    fn to_json(&self) -> Value {
        match self {
            Self::CompletedFinitePredecessor { worlds } => Object::new()
                .with(
                    "CompletedFinitePredecessor",
                    Value::from(Object::new().with("worlds", worlds.to_json())),
                )
                .into(),
        }
    }
}

impl FromJson for ModalFrontier {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (name, payload) =
            gmeow_errors::json::read_variant(value, "ModalFrontier", "one ModalFrontier variant")?;
        match name {
            "CompletedFinitePredecessor" => {
                let mut fields = Record::new(payload, "completed finite predecessor")
                    .map_err(|error| error.within(name))?;
                let worlds = fields
                    .required("worlds")
                    .map_err(|error| error.within(name))?;
                fields.deny_unknown().map_err(|error| error.within(name))?;
                Ok(Self::CompletedFinitePredecessor { worlds })
            }
            name => Err(DecodeError::unknown_variant(
                name,
                &["CompletedFinitePredecessor"],
            )),
        }
    }
}

::purrdf_lex::json_record!(ModalEvaluation as "struct ModalEvaluation" {
    "context" => context: required::<String>,
    "formula" => formula: required::<String>,
    "operator" => operator: required::<ModalOp>,
    "body" => body: required::<String>,
    "evaluation_world" => evaluation_world: required::<String>,
    "accessibility_relation" => accessibility_relation: required::<String>,
    "atom_subject" => atom_subject: required::<String>,
    "atom_predicate" => atom_predicate: required::<String>,
    "atom_object" => atom_object: required::<String>,
    "frontier" => frontier: required::<ModalFrontier>,
    "conclusion_predicate" => conclusion_predicate: required::<String>,
    "conclusion_object" => conclusion_object: required::<String>,
});
