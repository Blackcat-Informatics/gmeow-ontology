// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete grammar channel transport, retaining raw preparation and emitted evidence.
use super::{Emission, GrammarObservation, Observations};
use gmeow_lang_bridge::{Grammar, IngestDiagnostic};
use gmeow_logic_compile::ir::LegPath;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

struct GrammarOutcome(Result<Grammar, IngestDiagnostic>);
impl FromJson for GrammarOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "grammar outcome",
            "one grammar outcome variant",
        )?;
        let result = match tag {
            "Ok" => Grammar::from_json(payload).map(Ok),
            "Err" => IngestDiagnostic::from_json(payload).map(Err),
            name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
        };
        result.map(Self).map_err(|error| error.within(tag))
    }
}
fn outcome_to_json(result: &Result<Grammar, IngestDiagnostic>) -> Value {
    match result {
        Ok(value) => Object::new().with("Ok", value.to_json()).into(),
        Err(value) => Object::new().with("Err", value.to_json()).into(),
    }
}
fn read_leg_pair(value: &Value) -> Result<(LegPath, LegPath), DecodeError> {
    let cells = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, "two leg paths"))?;
    if cells.len() != 2 {
        return Err(DecodeError::invalid_value(value, "exactly two leg paths"));
    }
    Ok((
        LegPath::from_json(&cells[0]).map_err(|error| error.within_index_native(0, None))?,
        LegPath::from_json(&cells[1]).map_err(|error| error.within_index_native(1, None))?,
    ))
}
::purrdf_lex::json_record!(impl ToJson for Observations { "grammars" => grammars });
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            grammars: fields.required("grammars")?,
        })
    }
}
impl ToJson for GrammarObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_iri", self.source_iri.to_json())
            .with("source_rule_count", self.source_rule_count.to_json())
            .with("canonical", self.canonical.to_json())
            .with("raw_reparsed", outcome_to_json(&self.raw_reparsed))
            .with("emissions", self.emissions.to_json())
            .into()
    }
}
impl FromJson for GrammarObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GrammarObservation")?;
        Ok(Self {
            source_iri: fields.required("source_iri")?,
            source_rule_count: fields.required_with("source_rule_count", |value| {
                gmeow_errors::json::read_unsigned(value, "usize source rule count")
            })?,
            canonical: fields.required("canonical")?,
            raw_reparsed: fields.required::<GrammarOutcome>("raw_reparsed")?.0,
            emissions: fields.required("emissions")?,
        })
    }
}
impl ToJson for Emission {
    fn to_json(&self) -> Value {
        Object::new()
            .with("correspondence", self.correspondence.to_json())
            .with("leg_pair", self.leg_pair.to_json())
            .with("round_trip_holds", self.round_trip_holds.to_json())
            .with("lossy_kind", self.lossy_kind.to_json())
            .with("unsupported", self.unsupported.to_json())
            .with("paths", self.paths.to_json())
            .with(
                "ebnf_reparsed",
                Value::Array(self.ebnf_reparsed.iter().map(outcome_to_json).collect()),
            )
            .into()
    }
}
impl FromJson for Emission {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Emission")?;
        Ok(Self {
            correspondence: fields.required("correspondence")?,
            leg_pair: fields.optional_with("leg_pair", read_leg_pair)?,
            round_trip_holds: fields.required("round_trip_holds")?,
            lossy_kind: fields.required("lossy_kind")?,
            unsupported: fields.required("unsupported")?,
            paths: fields.required("paths")?,
            ebnf_reparsed: fields
                .required::<Vec<GrammarOutcome>>("ebnf_reparsed")?
                .into_iter()
                .map(|outcome| outcome.0)
                .collect(),
        })
    }
}
