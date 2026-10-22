// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native admission of the existing retained value representation.
use super::{Direction, Value};
use purrdf_lex::json::{
    Object, Value as JsonValue,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};
impl ToJson for Direction {
    fn to_json(&self) -> JsonValue {
        match self {
            Self::Ltr => "Ltr",
            Self::Rtl => "Rtl",
        }
        .to_json()
    }
}
impl FromJson for Direction {
    fn from_json(value: &JsonValue) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Direction")? {
            "Ltr" => Ok(Self::Ltr),
            "Rtl" => Ok(Self::Rtl),
            name => Err(DecodeError::unknown_variant(name, &["Ltr", "Rtl"])),
        }
    }
}
impl ToJson for Value {
    fn to_json(&self) -> JsonValue {
        match self {
            Self::Iri(value) => Object::new().with("Iri", value.to_json()).into(),
            Self::Blank { label, scope } => Object::new()
                .with(
                    "Blank",
                    JsonValue::from(
                        Object::new()
                            .with("label", label.to_json())
                            .with("scope", *scope),
                    ),
                )
                .into(),
            Self::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => Object::new()
                .with(
                    "Literal",
                    JsonValue::from(
                        Object::new()
                            .with("lexical_form", lexical_form.to_json())
                            .with("datatype", datatype.to_json())
                            .with("language", language.to_json())
                            .with("direction", direction.to_json()),
                    ),
                )
                .into(),
            Self::Triple { s, p, o } => Object::new()
                .with(
                    "Triple",
                    JsonValue::from(
                        Object::new()
                            .with("s", s.to_json())
                            .with("p", p.to_json())
                            .with("o", o.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for Value {
    fn from_json(value: &JsonValue) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "retained term", "one retained term variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Iri" => String::from_json(payload).map(Self::Iri),
                "Blank" => {
                    let mut fields = Record::new(payload, "struct variant Value::Blank")?;
                    Ok(Self::Blank {
                        label: fields.required("label")?,
                        scope: fields.required_with("scope", |value| {
                            gmeow_errors::json::read_unsigned(value, "u32 blank scope")
                        })?,
                    })
                }
                "Literal" => {
                    let mut fields = Record::new(payload, "struct variant Value::Literal")?;
                    Ok(Self::Literal {
                        lexical_form: fields.required("lexical_form")?,
                        datatype: fields.required("datatype")?,
                        language: fields.optional("language")?,
                        direction: fields.optional("direction")?,
                    })
                }
                "Triple" => {
                    let mut fields = Record::new(payload, "struct variant Value::Triple")?;
                    Ok(Self::Triple {
                        s: fields.required("s")?,
                        p: fields.required("p")?,
                        o: fields.required("o")?,
                    })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &["Iri", "Blank", "Literal", "Triple"],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
#[cfg(test)]
mod tests;
