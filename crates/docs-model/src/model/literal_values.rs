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
    Components(purrdf_core::LiteralComponentError),
    Direction(String),
}

impl std::fmt::Display for LiteralTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Datatype(error) => std::fmt::Display::fmt(error, f),
            Self::Components(error) => std::fmt::Display::fmt(error, f),
            Self::Direction(token) => write!(f, "invalid RDF base direction {token:?}"),
        }
    }
}

impl std::error::Error for LiteralTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Datatype(error) => Some(error),
            Self::Components(error) => Some(error),
            Self::Direction(_) => None,
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

impl LiteralFields {
    fn encode(literal: &RdfLiteral) -> gmeow_errors::Result<Self> {
        validate(literal)?;
        Ok(Self {
            lexical_form: literal.lexical_form.clone(),
            datatype: literal.datatype.clone(),
            language: literal.language.clone(),
            direction: literal
                .direction
                .map(|direction| direction.as_str().to_owned()),
        })
    }

    fn decode(self) -> gmeow_errors::Result<RdfLiteral> {
        let direction = self
            .direction
            .as_deref()
            .map(|token| {
                RdfTextDirection::from_str_token(token)
                    .ok_or_else(|| LiteralTransportError::Direction(token.to_owned()))
            })
            .transpose()?;
        let literal = RdfLiteral {
            lexical_form: self.lexical_form,
            datatype: self.datatype,
            language: self.language,
            direction,
        };
        validate(&literal)?;
        Ok(literal)
    }
}

pub(super) fn serialize<S: Serializer>(
    values: &[RdfLiteral],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for literal in values {
        let fields = LiteralFields::encode(literal).map_err(<S::Error as ser::Error>::custom)?;
        sequence.serialize_element(&fields)?;
    }
    sequence.end()
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RdfLiteral>, D::Error> {
    Vec::<LiteralFields>::deserialize(deserializer)?
        .into_iter()
        .map(|fields| fields.decode().map_err(<D::Error as de::Error>::custom))
        .collect()
}

pub(super) mod single {
    use super::{LiteralFields, RdfLiteral};
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};

    pub(in crate::model) fn serialize<S: Serializer>(
        literal: &RdfLiteral,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        LiteralFields::encode(literal)
            .map_err(<S::Error as ser::Error>::custom)?
            .serialize(serializer)
    }

    pub(in crate::model) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RdfLiteral, D::Error> {
        LiteralFields::deserialize(deserializer)?
            .decode()
            .map_err(<D::Error as de::Error>::custom)
    }
}

#[cfg(test)]
mod tests;
