// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Direct typed transport for complete native RDF literals in the existing model
//! envelope. No JSON value conversion or lexical RDF parser is involved.

use purrdf::{RdfLiteral, RdfTextDirection};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};

#[derive(Serialize, Deserialize)]
struct LiteralFields {
    lexical_form: String,
    datatype: Option<String>,
    language: Option<String>,
    direction: Option<String>,
}

#[derive(Debug)]
enum LiteralTransportError {
    Datatype(purrdf_core::IriError),
    Components(&'static str),
    Direction(String),
}

impl std::fmt::Display for LiteralTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Datatype(error) => std::fmt::Display::fmt(error, f),
            Self::Components(message) => f.write_str(message),
            Self::Direction(token) => write!(f, "invalid RDF base direction {token:?}"),
        }
    }
}

impl std::error::Error for LiteralTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Datatype(error) => Some(error),
            Self::Components(_) | Self::Direction(_) => None,
        }
    }
}

fn validate(literal: &RdfLiteral) -> gmeow_errors::Result<()> {
    let datatype = literal
        .datatype
        .as_deref()
        .unwrap_or_else(|| literal.datatype_iri());
    purrdf_core::BaseIri::parse(datatype).map_err(LiteralTransportError::Datatype)?;
    RdfLiteral::validate_components(datatype, literal.language.as_deref(), literal.direction)
        .map_err(LiteralTransportError::Components)
        .map_err(Into::into)
}

pub(super) fn serialize<S: Serializer>(
    values: &[RdfLiteral],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for literal in values {
        validate(literal).map_err(<S::Error as ser::Error>::custom)?;
        sequence.serialize_element(&LiteralFields {
            lexical_form: literal.lexical_form.clone(),
            datatype: literal.datatype.clone(),
            language: literal.language.clone(),
            direction: literal
                .direction
                .map(|direction| direction.as_str().to_owned()),
        })?;
    }
    sequence.end()
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RdfLiteral>, D::Error> {
    Vec::<LiteralFields>::deserialize(deserializer)?
        .into_iter()
        .map(|fields| {
            let direction = fields
                .direction
                .as_deref()
                .map(|token| {
                    RdfTextDirection::from_str_token(token).ok_or_else(|| {
                        <D::Error as de::Error>::custom(gmeow_errors::Diag::from(
                            LiteralTransportError::Direction(token.to_owned()),
                        ))
                    })
                })
                .transpose()?;
            let literal = RdfLiteral {
                lexical_form: fields.lexical_form,
                datatype: fields.datatype,
                language: fields.language,
                direction,
            };
            validate(&literal).map_err(<D::Error as de::Error>::custom)?;
            Ok(literal)
        })
        .collect()
}

#[cfg(test)]
mod tests;
