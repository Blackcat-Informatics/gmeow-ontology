// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact retained class-source evidence, independent of execution and source validation.

use super::{
    ClassAdmissionObservation, ClassAdmissionSourceWorld, ClassAdmissionWorld, FragmentBoundary,
    FragmentFamily, RefutationPremise,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

::purrdf_lex::json_record!(impl ToJson for ClassAdmissionObservation {
    "contract" => contract,
    "source_worlds" => source_worlds,
    "selected_worlds" => selected_worlds,
});
impl FromJson for ClassAdmissionObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClassAdmissionObservation")?;
        Ok(Self {
            contract: fields.required("contract")?,
            source_worlds: fields.required("source_worlds")?,
            selected_worlds: fields.required("selected_worlds")?,
        })
    }
}
impl ToJson for ClassAdmissionSourceWorld {
    fn to_json(&self) -> Value {
        Object::new()
            .with("graph", crate::term_serde::optional::to_json(&self.graph))
            .with("assertions", self.assertions.to_json())
            .into()
    }
}
impl FromJson for ClassAdmissionSourceWorld {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClassAdmissionSourceWorld")?;
        Ok(Self {
            graph: fields.required_with("graph", crate::term_serde::optional::from_json)?,
            assertions: fields.required_with("assertions", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 source assertion count")
            })?,
        })
    }
}
impl ToJson for ClassAdmissionWorld {
    fn to_json(&self) -> Value {
        Object::new()
            .with("graph", crate::term_serde::optional::to_json(&self.graph))
            .with("definitions", self.definitions.to_json())
            .with("refusal", self.refusal.to_json())
            .into()
    }
}
impl FromJson for ClassAdmissionWorld {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClassAdmissionWorld")?;
        Ok(Self {
            graph: fields.required_with("graph", crate::term_serde::optional::from_json)?,
            definitions: fields.required("definitions")?,
            refusal: fields.optional("refusal")?,
        })
    }
}
impl ToJson for RefutationPremise {
    fn to_json(&self) -> Value {
        Object::new()
            .with("subject", crate::term_serde::to_json(&self.subject))
            .with("predicate", self.predicate.to_json())
            .with("object", crate::term_serde::to_json(&self.object))
            .with("graph", crate::term_serde::optional::to_json(&self.graph))
            .into()
    }
}
impl FromJson for RefutationPremise {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RefutationPremise")?;
        Ok(Self {
            subject: fields.required_with("subject", crate::term_serde::from_json)?,
            predicate: fields.required("predicate")?,
            object: fields.required_with("object", crate::term_serde::from_json)?,
            graph: fields.required_with("graph", crate::term_serde::optional::from_json)?,
        })
    }
}
impl ToJson for FragmentFamily {
    fn to_json(&self) -> Value {
        match self {
            Self::DatatypeValueSpace => "DatatypeValueSpace",
            Self::Counting => "Counting",
            Self::CaseSplit => "CaseSplit",
        }
        .to_json()
    }
}
impl FromJson for FragmentFamily {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "FragmentFamily")? {
            "DatatypeValueSpace" => Ok(Self::DatatypeValueSpace),
            "Counting" => Ok(Self::Counting),
            "CaseSplit" => Ok(Self::CaseSplit),
            name => Err(DecodeError::unknown_variant(
                name,
                &["DatatypeValueSpace", "Counting", "CaseSplit"],
            )),
        }
    }
}
impl ToJson for FragmentBoundary {
    fn to_json(&self) -> Value {
        match self {
            Self::SourceAdmission {
                world,
                premises,
                issue,
            } => Object::new()
                .with(
                    "SourceAdmission",
                    Value::from(
                        Object::new()
                            .with("world", world.to_json())
                            .with("premises", premises.to_json())
                            .with("issue", issue.to_json()),
                    ),
                )
                .into(),
            Self::Uncertified {
                family,
                obstructions,
            } => Object::new()
                .with(
                    "Uncertified",
                    Value::from(
                        Object::new()
                            .with("family", family.to_json())
                            .with("obstructions", obstructions.to_json()),
                    ),
                )
                .into(),
            Self::Combined(boundaries) => {
                Object::new().with("Combined", boundaries.to_json()).into()
            }
        }
    }
}
impl FromJson for FragmentBoundary {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "FragmentBoundary",
            "one FragmentBoundary variant",
        )?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "SourceAdmission" => {
                    let mut fields =
                        Record::new(payload, "struct variant FragmentBoundary::SourceAdmission")?;
                    Ok(Self::SourceAdmission {
                        world: fields.required("world")?,
                        premises: fields.required("premises")?,
                        issue: fields.required("issue")?,
                    })
                }
                "Uncertified" => {
                    let mut fields =
                        Record::new(payload, "struct variant FragmentBoundary::Uncertified")?;
                    Ok(Self::Uncertified {
                        family: fields.required("family")?,
                        obstructions: fields.required("obstructions")?,
                    })
                }
                "Combined" => {
                    std::collections::BTreeSet::<Self>::from_json(payload).map(Self::Combined)
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &["SourceAdmission", "Uncertified", "Combined"],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
