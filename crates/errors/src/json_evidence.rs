// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Immutable JSON evidence. Lexical text is the sole retained authority; parsed
//! trees are temporary projections, not a second mutable metadata model.

use std::borrow::Cow;

use purrdf_lex::json::{
    self, Event, Limits, Reader, Value,
    record::{DecodeError, FromJson, sorted_last_wins},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Maximum container depth of one independently owned metadata value.
pub const METADATA_JSON_DEPTH: usize = 128;

/// Immutable canonical JSON evidence for a diagnostic metadata entry.
///
/// Canonical means compact native escaping and recursively sorted last-wins
/// object members. Numeric lexemes remain exact, including values outside
/// the finite floating point range. CBOR carries this lexical evidence as text; JSON presentations
/// project the original value. Construction and binary hydration enforce the
/// same native grammar and explicit depth profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonEvidence(String);

impl JsonEvidence {
    /// Borrow the canonical lexical evidence without allocating or parsing.
    pub fn canonical_json(&self) -> &str {
        &self.0
    }

    /// Project a temporary native JSON value from this immutable evidence.
    ///
    /// # Errors
    /// Preserves native lexical, depth and physical allocation refusals.
    pub fn value(&self) -> Result<Value, DecodeError> {
        Ok(json::read_slice(
            self.0.as_bytes(),
            Limits::with_depth(METADATA_JSON_DEPTH),
        )?)
    }

    /// Read a string scalar, borrowing unescaped content where possible.
    /// Other valid JSON kinds yield `None`; native read failures remain errors.
    ///
    /// # Errors
    /// Preserves native parsing and string allocation refusals.
    pub fn string(&self) -> Result<Option<Cow<'_, str>>, DecodeError> {
        let mut reader = Reader::new(&self.0, Limits::with_depth(METADATA_JSON_DEPTH));
        match reader.next_event()? {
            Event::String(value) => {
                let decoded = value.decode()?;
                reader.finish()?;
                Ok(Some(decoded))
            }
            // Every stored document was completely admitted at construction.
            // Inspecting its root kind does not need to materialize a subtree.
            _ => Ok(None),
        }
    }
}

impl FromJson for JsonEvidence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut normalized = value.clone();
        sorted_last_wins(&mut normalized);
        let lexical = json::write_compact(&normalized);
        let mut reader = Reader::new(&lexical, Limits::with_depth(METADATA_JSON_DEPTH));
        reader.check_value()?;
        reader.finish()?;
        Ok(Self(lexical))
    }
}

impl From<String> for JsonEvidence {
    fn from(value: String) -> Self {
        // Native writing of a typed scalar is valid at depth zero. String
        // length is unbounded in the selected native profile.
        Self(json::write_compact(&Value::String(value)))
    }
}

impl From<&str> for JsonEvidence {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<bool> for JsonEvidence {
    fn from(value: bool) -> Self {
        Self(json::write_compact(&Value::Bool(value)))
    }
}

impl From<usize> for JsonEvidence {
    fn from(value: usize) -> Self {
        Self(json::write_compact(&Value::from(value)))
    }
}

impl Serialize for JsonEvidence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            return Err(serde::ser::Error::custom(
                "JSON evidence requires native JSON projection",
            ));
        }
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for JsonEvidence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            return Err(serde::de::Error::custom(
                "JSON evidence requires native JSON admission",
            ));
        }
        let lexical = String::deserialize(deserializer)?;
        let value = json::read_slice(lexical.as_bytes(), Limits::with_depth(METADATA_JSON_DEPTH))
            .map_err(serde::de::Error::custom)?;
        let admitted = Self::from_json(&value).map_err(serde::de::Error::custom)?;
        if admitted.0 != lexical {
            return Err(serde::de::Error::custom(
                "diagnostic metadata requires canonical JSON lexical evidence",
            ));
        }
        Ok(admitted)
    }
}
