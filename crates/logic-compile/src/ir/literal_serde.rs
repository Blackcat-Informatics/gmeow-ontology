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
        let direction = direction
            .as_deref()
            .map(|value| match value {
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
        Ok(literal)
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
