// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete selected-dictionary observations. Hydration never reruns the dictionary
//! or grammar producers; all observed values and typed refusals remain data.

use super::{
    GlyphSource, Observations,
    cases::{Process, Records, Scoped, Written},
    costs::{CostRow, Costs},
    notation::Notation,
    stamps::Stamps,
};
use gmeow_lang_bridge::{Gmn1Error, IngestDiagnostic, gmn1_witness::StandaloneReport};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within as _},
};
use std::collections::BTreeMap;

fn outcome<T, E: ToJson>(result: &Result<T, E>, write: impl FnOnce(&T) -> Value) -> Value {
    match result {
        Ok(value) => Object::new().with("Ok", write(value)).into(),
        Err(error) => Object::new().with("Err", error.to_json()).into(),
    }
}
fn read_outcome<T, E: FromJson>(
    value: &Value,
    read: impl FnOnce(&Value) -> Result<T, DecodeError>,
) -> Result<Result<T, E>, DecodeError> {
    let (tag, payload) = gmeow_errors::json::read_variant(
        value,
        "dictionary outcome",
        "one dictionary outcome variant",
    )?;
    match tag {
        "Ok" => read(payload).map(Ok),
        "Err" => E::from_json(payload).map(Err),
        name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
    }
    .map_err(|error| error.within(tag))
}
fn unit(value: &Value) -> Result<(), DecodeError> {
    gmeow_errors::json::read_null(value, "null unit witness")
}
fn acceptance(value: &Value) -> Result<(u32, u32), DecodeError> {
    let items = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, "two u32 acceptance coordinates"))?;
    if items.len() != 2 {
        return Err(DecodeError::invalid_value(
            value,
            "two u32 acceptance coordinates",
        ));
    }
    let read = |index: usize| {
        gmeow_errors::json::read_unsigned(&items[index], "u32 acceptance coordinate")
            .map_err(|error| error.within(&index.to_string()))
    };
    Ok((read(0)?, read(1)?))
}
struct Count(usize);
impl FromJson for Count {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_unsigned(value, "usize measured token cost").map(Self)
    }
}
struct GmnOutcome<T>(Result<T, Gmn1Error>);
impl<T: FromJson> FromJson for GmnOutcome<T> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_outcome(value, T::from_json).map(Self)
    }
}
fn emitted(value: &Value) -> Result<BTreeMap<String, Result<String, Gmn1Error>>, DecodeError> {
    BTreeMap::<String, GmnOutcome<String>>::from_json(value).map(|rows| {
        rows.into_iter()
            .map(|(name, value)| (name, value.0))
            .collect()
    })
}
struct GrammarOutcome(Result<Vec<gmeow_lang_bridge::GrammarRule>, IngestDiagnostic>);
impl FromJson for GrammarOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_outcome(value, Vec::from_json).map(Self)
    }
}

::purrdf_lex::json_record!(impl ToJson for GlyphSource { "codepoints" => codepoints, "scopes" => scopes });
impl FromJson for GlyphSource {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GlyphSource")?;
        Ok(Self {
            codepoints: fields.required("codepoints")?,
            scopes: fields.required("scopes")?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for Written { "text" => text, "canonical_equal" => canonical_equal });
impl FromJson for Written {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Written")?;
        Ok(Self {
            text: fields.required("text")?,
            canonical_equal: fields.required("canonical_equal")?,
        })
    }
}
impl ToJson for Scoped {
    fn to_json(&self) -> Value {
        Object::new()
            .with("written", self.written.to_json())
            .with("fallback", outcome(&self.fallback, ToJson::to_json))
            .with("wrong_scope", outcome(&self.wrong_scope, |_| Value::Null))
            .with(
                "wrong_fallback_scope",
                outcome(&self.wrong_fallback_scope, |_| Value::Null),
            )
            .into()
    }
}
impl FromJson for Scoped {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Scoped")?;
        Ok(Self {
            written: fields.required("written")?,
            fallback: fields
                .required_with("fallback", |value| read_outcome(value, bool::from_json))?,
            wrong_scope: fields.required_with("wrong_scope", |value| read_outcome(value, unit))?,
            wrong_fallback_scope: fields
                .required_with("wrong_fallback_scope", |value| read_outcome(value, unit))?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for Process { "written" => written, "primary" => primary, "glyph_count" => glyph_count });
impl FromJson for Process {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Process")?;
        Ok(Self {
            written: fields.required("written")?,
            primary: fields.optional("primary")?,
            glyph_count: fields.required_with("glyph_count", |value| {
                gmeow_errors::json::read_unsigned(value, "usize glyph count")
            })?,
        })
    }
}
impl ToJson for Records {
    fn to_json(&self) -> Value {
        Object::new()
            .with("sigils", outcome(&self.sigils, ToJson::to_json))
            .with("scoped", outcome(&self.scoped, ToJson::to_json))
            .with("process", outcome(&self.process, ToJson::to_json))
            .into()
    }
}
impl FromJson for Records {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Records")?;
        Ok(Self {
            sigils: fields
                .required_with("sigils", |value| read_outcome(value, Written::from_json))?,
            scoped: fields
                .required_with("scoped", |value| read_outcome(value, Scoped::from_json))?,
            process: fields
                .required_with("process", |value| read_outcome(value, Process::from_json))?,
        })
    }
}
impl ToJson for CostRow {
    fn to_json(&self) -> Value {
        Object::new()
            .with("codepoints", self.codepoints.to_json())
            .with("values", self.values.to_json())
            .with(
                "measured",
                gmeow_errors::json::recorded_result_to_json(&self.measured),
            )
            .into()
    }
}
impl FromJson for CostRow {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CostRow")?;
        Ok(Self {
            codepoints: fields.required("codepoints")?,
            values: fields.required("values")?,
            measured: fields.required_with("measured", |value| {
                gmeow_errors::json::read_recorded_result::<Count>(
                    value,
                    "measured cost outcome",
                    "one measured cost outcome variant",
                )
                .map(|value| value.map(|count| count.0))
            })?,
        })
    }
}
impl ToJson for Costs {
    fn to_json(&self) -> Value {
        let emitted: Object = self
            .emitted
            .iter()
            .map(|(name, value)| (name.as_str(), outcome(value, ToJson::to_json)))
            .collect();
        Object::new()
            .with("rows", self.rows.to_json())
            .with("spellings", self.spellings.to_json())
            .with("denotation_form_present", self.denotation_form_present)
            .with("aliases", self.aliases.to_json())
            .with("emitted", emitted)
            .into()
    }
}
impl FromJson for Costs {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Costs")?;
        Ok(Self {
            rows: fields.required("rows")?,
            spellings: fields.required("spellings")?,
            denotation_form_present: fields.required("denotation_form_present")?,
            aliases: fields.required("aliases")?,
            emitted: fields.required_with("emitted", emitted)?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for Stamps {
    "resolved" => resolved, "dictionary_major" => dictionary_major, "acceptance_major" => acceptance_major,
    "values" => values, "repeated_quad_equal" => repeated_quad_equal,
});
impl FromJson for Stamps {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Stamps")?;
        Ok(Self {
            resolved: fields.required("resolved")?,
            dictionary_major: fields.required("dictionary_major")?,
            acceptance_major: fields.required("acceptance_major")?,
            values: fields.required("values")?,
            repeated_quad_equal: fields.required("repeated_quad_equal")?,
        })
    }
}
impl ToJson for Notation {
    fn to_json(&self) -> Value {
        let views: Object = self
            .views
            .iter()
            .map(|(name, value)| (name.as_str(), outcome(value, ToJson::to_json)))
            .collect();
        Object::new()
            .with("production", self.production.to_json())
            .with("canonical", self.canonical.to_json())
            .with("views", views)
            .into()
    }
}
impl FromJson for Notation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Notation")?;
        let production = fields.required("production")?;
        let canonical = fields.required("canonical")?;
        let views: BTreeMap<String, GrammarOutcome> = fields.required("views")?;
        Ok(Self {
            production,
            canonical,
            views: views
                .into_iter()
                .map(|(name, value)| (name, value.0))
                .collect(),
        })
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("aliases", self.aliases.to_json())
            .with("nec_reverse", self.nec_reverse.to_json())
            .with("acceptance", self.acceptance.to_json())
            .with("resolved_acceptance", self.resolved_acceptance.to_json())
            .with(
                "current_header",
                outcome(&self.current_header, |_| Value::Null),
            )
            .with(
                "future_header",
                outcome(&self.future_header, |_| Value::Null),
            )
            .with("per_claim", outcome(&self.per_claim, |_| Value::Null))
            .with("standalone", outcome(&self.standalone, ToJson::to_json))
            .with("glyphs", self.glyphs.to_json())
            .with("records", self.records.to_json())
            .with("costs", self.costs.to_json())
            .with("stamps", self.stamps.to_json())
            .with("notation", outcome(&self.notation, ToJson::to_json))
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            aliases: fields.required("aliases")?,
            nec_reverse: fields.optional("nec_reverse")?,
            acceptance: fields.required_with("acceptance", acceptance)?,
            resolved_acceptance: fields.required_with("resolved_acceptance", acceptance)?,
            current_header: fields
                .required_with("current_header", |value| read_outcome(value, unit))?,
            future_header: fields
                .required_with("future_header", |value| read_outcome(value, unit))?,
            per_claim: fields.required_with("per_claim", |value| read_outcome(value, unit))?,
            standalone: fields.required_with("standalone", |value| {
                read_outcome(value, StandaloneReport::from_json)
            })?,
            glyphs: fields.required("glyphs")?,
            records: fields.required("records")?,
            costs: fields.required("costs")?,
            stamps: fields.required("stamps")?,
            notation: fields
                .required_with("notation", |value| read_outcome(value, Notation::from_json))?,
        })
    }
}
#[cfg(test)]
mod tests;
