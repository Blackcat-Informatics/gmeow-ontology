// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Cache-boundary adapter for PurRDF's complete owned literal. Native values stay
//! native in the compiler; this tuple is only their serialized representation.

use purrdf::{RdfLiteral, RdfTextDirection};
use serde::{Deserialize, Deserializer, Serializer, ser::SerializeSeq};

/// Check RDF component coherence using the native authority and canonicalize the
/// implied datatype and language case. No datatype lexical value is coerced.
pub(crate) fn normalize(literal: &mut RdfLiteral) -> Result<(), &'static str> {
    let datatype = literal
        .datatype
        .as_deref()
        .unwrap_or_else(|| literal.datatype_iri());
    if datatype.trim().is_empty() {
        return Err("a literal datatype must be a non-empty IRI");
    }
    RdfLiteral::validate_components(datatype, literal.language.as_deref(), literal.direction)?;
    literal.datatype =
        if literal.language.is_some() || datatype == "http://www.w3.org/2001/XMLSchema#string" {
            None
        } else {
            Some(datatype.to_owned())
        };
    if let Some(language) = &mut literal.language {
        language.make_ascii_lowercase();
    }
    Ok(())
}

pub(super) fn serialize<S: Serializer>(
    literals: &[RdfLiteral],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut sequence = serializer.serialize_seq(Some(literals.len()))?;
    for literal in literals {
        sequence.serialize_element(&(
            literal.lexical_form.as_str(),
            literal
                .datatype
                .as_deref()
                .unwrap_or_else(|| literal.datatype_iri()),
            literal.language.as_deref(),
            literal.direction.map(RdfTextDirection::as_str),
        ))?;
    }
    sequence.end()
}

/// Stream the existing literal-vector wire, including each explicit datatype.
/// Unlike the single-literal wire, the collection adapter retains the stored
/// datatype before consulting the native effective datatype. Ordering is unchanged.
///
/// # Errors
/// Returns the sink's original failure without reporting partial success.
pub(crate) fn write_cbor<W: std::io::Write + ?Sized>(
    literals: &[RdfLiteral],
    out: &mut W,
) -> std::io::Result<()> {
    use purrdf_lex::cbor::{self, Value, head};

    head::write_head(out, head::ARRAY, literals.len() as u64)?;
    for literal in literals {
        head::write_head(out, head::ARRAY, 4)?;
        head::write_text(out, &literal.lexical_form)?;
        head::write_text(
            out,
            literal
                .datatype
                .as_deref()
                .unwrap_or_else(|| literal.datatype_iri()),
        )?;
        match literal.language.as_deref() {
            Some(language) => head::write_text(out, language)?,
            None => cbor::write(&Value::Null, out)?,
        }
        match literal.direction {
            Some(direction) => head::write_text(out, direction.as_str())?,
            None => cbor::write(&Value::Null, out)?,
        }
    }
    Ok(())
}

/// Canonical RDF set order, with no lexical rendering or temporary sort strings.
pub(crate) fn sort_key(
    literal: &RdfLiteral,
) -> (&str, &str, Option<&str>, Option<RdfTextDirection>) {
    (
        &literal.lexical_form,
        literal.datatype_iri(),
        literal.language.as_deref(),
        literal.direction,
    )
}

/// Admit complete advisory messages as a canonical RDF set without trimming wording.
pub(crate) fn normalize_messages(literals: &mut Vec<RdfLiteral>) -> Result<(), &'static str> {
    for literal in literals.iter_mut() {
        normalize(literal)?;
        if literal.lexical_form.trim().is_empty() {
            return Err("a message literal must have nonblank lexical text");
        }
    }
    literals.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    literals.dedup();
    Ok(())
}

/// Message collections may be absent, but every supplied literal must be valid.
pub(super) mod message_set {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        literals: &[RdfLiteral],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize(literals, serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<RdfLiteral>, D::Error> {
        let mut literals = super::deserialize_set(deserializer)?;
        normalize_messages(&mut literals).map_err(serde::de::Error::custom)?;
        Ok(literals)
    }

    pub(crate) fn from_json(
        value: &purrdf_lex::json::Value,
    ) -> Result<Vec<RdfLiteral>, purrdf_lex::json::record::DecodeError> {
        let mut literals = super::from_json_set(value)?;
        normalize_messages(&mut literals).map_err(purrdf_lex::json::record::DecodeError::custom)?;
        Ok(literals)
    }
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RdfLiteral>, D::Error> {
    let literals = deserialize_set(deserializer)?;
    if literals.is_empty() {
        return Err(serde::de::Error::custom(
            "a correspondence caveat requires at least one comment",
        ));
    }
    Ok(literals)
}

/// The same fixed four-component literal wire is used by every vector adapter.
/// Collection requiredness belongs to the semantic owner, not the literal codec.
fn deserialize_set<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RdfLiteral>, D::Error> {
    type LiteralWire = (String, String, Option<String>, Option<String>);
    let wire: Vec<LiteralWire> = Deserialize::deserialize(deserializer)?;
    let mut literals = Vec::with_capacity(wire.len());
    for (lexical_form, datatype, language, direction) in wire {
        let direction = direction
            .as_deref()
            .map(|direction| match direction {
                "ltr" => Ok(RdfTextDirection::Ltr),
                "rtl" => Ok(RdfTextDirection::Rtl),
                _ => Err(serde::de::Error::custom(
                    "unknown RDF literal base direction",
                )),
            })
            .transpose()?;
        let mut literal = RdfLiteral {
            lexical_form,
            datatype: Some(datatype),
            language,
            direction,
        };
        normalize(&mut literal).map_err(serde::de::Error::custom)?;
        literals.push(literal);
    }
    literals.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    literals.dedup();
    Ok(literals)
}

/// Loss evidence is a required vector field whose empty value asserts no drop.
/// Keep its length prefix even when empty: positional codecs cannot skip fields.
pub(super) mod loss_set {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        literals: &[RdfLiteral],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize(literals, serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<RdfLiteral>, D::Error> {
        super::deserialize_set(deserializer)
    }
}

/// Complete literal identity with framed components; arbitrary lexical separators
/// cannot turn one literal or enclosing atom into another.
pub(crate) fn key(literal: &RdfLiteral) -> String {
    let mut key = String::new();
    for value in [
        literal.lexical_form.as_str(),
        literal.datatype_iri(),
        literal.language.as_deref().unwrap_or(""),
        literal.direction.map_or("", RdfTextDirection::as_str),
    ] {
        key.push_str(&value.len().to_string());
        key.push(':');
        key.push_str(value);
    }
    key
}

/// The single-value cache boundary used by canonical formula and relational terms.
pub(crate) mod single {
    use super::*;
    use serde::Serialize;

    pub(crate) fn serialize<S: Serializer>(
        literal: &RdfLiteral,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        (
            literal.lexical_form.as_str(),
            literal.datatype_iri(),
            literal.language.as_deref(),
            literal.direction.map(RdfTextDirection::as_str),
        )
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RdfLiteral, D::Error> {
        let (lexical_form, datatype, language, direction): (
            String,
            String,
            Option<String>,
            Option<String>,
        ) = Deserialize::deserialize(deserializer)?;
        let direction = base_direction(direction.as_deref()).map_err(serde::de::Error::custom)?;
        from_components(lexical_form, datatype, language, direction)
            .map_err(serde::de::Error::custom)
    }

    /// The existing single-literal direction vocabulary, shared by both codecs.
    fn base_direction(direction: Option<&str>) -> Result<Option<RdfTextDirection>, &'static str> {
        direction
            .map(|value| match value {
                "ltr" => Ok(RdfTextDirection::Ltr),
                "rtl" => Ok(RdfTextDirection::Rtl),
                _ => Err("unknown RDF literal base direction"),
            })
            .transpose()
    }

    /// Admit the same complete single literal after its typed components are read.
    fn from_components(
        lexical_form: String,
        datatype: String,
        language: Option<String>,
        direction: Option<RdfTextDirection>,
    ) -> Result<RdfLiteral, &'static str> {
        let mut literal = RdfLiteral {
            lexical_form,
            datatype: Some(datatype),
            language,
            direction,
        };
        normalize(&mut literal)?;
        Ok(literal)
    }

    /// Stream the existing four-component single-literal CBOR wire.
    /// The effective datatype and lowercase direction names match this owner's
    /// serialized tuple; writing does not normalize or validate the literal.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: std::io::Write + ?Sized>(
        literal: &RdfLiteral,
        out: &mut W,
    ) -> std::io::Result<()> {
        use purrdf_lex::cbor::{self, Value, head};

        head::write_head(out, head::ARRAY, 4)?;
        head::write_text(out, &literal.lexical_form)?;
        head::write_text(out, literal.datatype_iri())?;
        match literal.language.as_deref() {
            Some(language) => head::write_text(out, language)?,
            None => cbor::write(&Value::Null, out)?,
        }
        match literal.direction {
            Some(direction) => head::write_text(out, direction.as_str())?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }

    /// Encode the exact four-component single-literal wire without RDF rendering.
    pub(crate) fn to_json(literal: &RdfLiteral) -> purrdf_lex::json::Value {
        use purrdf_lex::json::{Value, record::ToJson};
        Value::Array(vec![
            literal.lexical_form.to_json(),
            literal.datatype_iri().to_json(),
            literal.language.to_json(),
            literal.direction.map(RdfTextDirection::as_str).to_json(),
        ])
    }

    /// Decode the same checked single literal from exactly four native JSON cells.
    pub(crate) fn from_json(
        value: &purrdf_lex::json::Value,
    ) -> Result<RdfLiteral, purrdf_lex::json::record::DecodeError> {
        use purrdf_lex::json::record::{DecodeError, FromJson, Within};
        let cells = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "four-component RDF literal"))?;
        if cells.len() != 4 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly four RDF literal components",
            ));
        }
        let lexical_form =
            String::from_json(&cells[0]).map_err(|error| error.within_index_native(0, None))?;
        let datatype =
            String::from_json(&cells[1]).map_err(|error| error.within_index_native(1, None))?;
        let language = Option::<String>::from_json(&cells[2])
            .map_err(|error| error.within_index_native(2, None))?;
        let direction = Option::<String>::from_json(&cells[3])
            .map_err(|error| error.within_index_native(3, None))?;
        let direction = base_direction(direction.as_deref())
            .map_err(|error| DecodeError::custom(error).within_index_native(3, None))?;
        from_components(lexical_form, datatype, language, direction).map_err(DecodeError::custom)
    }
}

/// Structural order matching the native value's derived equality and hash.
/// Identity canonicalization happens at checked construction and cache ingress.
pub(crate) fn structural_cmp(a: &RdfLiteral, b: &RdfLiteral) -> std::cmp::Ordering {
    (&a.lexical_form, &a.datatype, &a.language, a.direction).cmp(&(
        &b.lexical_form,
        &b.datatype,
        &b.language,
        b.direction,
    ))
}

/// Encode the literal collection wire, retaining any explicitly stored datatype.
/// Unlike the single-literal wire, malformed explicit component combinations must
/// remain visible to the reader rather than being repaired while writing.
pub(crate) fn to_json(literals: &[RdfLiteral]) -> purrdf_lex::json::Value {
    use purrdf_lex::json::{Value, record::ToJson};
    Value::Array(
        literals
            .iter()
            .map(|literal| {
                Value::Array(vec![
                    literal.lexical_form.to_json(),
                    literal
                        .datatype
                        .as_deref()
                        .unwrap_or_else(|| literal.datatype_iri())
                        .to_json(),
                    literal.language.to_json(),
                    literal.direction.map(RdfTextDirection::as_str).to_json(),
                ])
            })
            .collect(),
    )
}

/// Admit the existing checked literal set, preserving its canonical set policy.
pub(crate) fn from_json_set(
    value: &purrdf_lex::json::Value,
) -> Result<Vec<RdfLiteral>, purrdf_lex::json::record::DecodeError> {
    use purrdf_lex::json::record::{DecodeError, Within};
    let rows = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, "RDF literal set"))?;
    let mut literals = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            single::from_json(row).map_err(|error| error.within_index_native(index, None))
        })
        .collect::<Result<Vec<_>, _>>()?;
    literals.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    literals.dedup();
    Ok(literals)
}
