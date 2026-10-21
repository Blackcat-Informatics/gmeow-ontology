// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named source-span records preserve path-table order and per-slot interning.

use super::{SourceSpan, SpanIndex};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug)]
struct SpanEntryWire {
    subject: String,
    path_id: u32,
    line: u64,
    column: u32,
    byte_offset: u64,
}

struct SpanIndexWire {
    paths: Vec<String>,
    entries: Vec<SpanEntryWire>,
}

impl ToJson for SpanEntryWire {
    fn to_json(&self) -> Value {
        Object::new()
            .with("subject", self.subject.to_json())
            .with("path_id", self.path_id)
            .with("line", self.line)
            .with("column", self.column)
            .with("byte_offset", self.byte_offset)
            .into()
    }
}

impl ToJson for SpanIndexWire {
    fn to_json(&self) -> Value {
        Object::new()
            .with("paths", self.paths.to_json())
            .with("entries", self.entries.to_json())
            .into()
    }
}

/// Admit u32 path references and columns through the native strict unsigned token.
fn span_u32(value: &Value) -> Result<u32, DecodeError> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| DecodeError::invalid_value(value, "u32"))
}

/// Admit full-width line and byte-offset values without decimal/-0 coercion.
fn span_u64(value: &Value) -> Result<u64, DecodeError> {
    value
        .as_u64()
        .ok_or_else(|| DecodeError::invalid_value(value, "u64"))
}

impl FromJson for SpanEntryWire {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SpanEntryWire")?;
        Ok(Self {
            subject: record.required("subject")?,
            path_id: record.required_with("path_id", span_u32)?,
            line: record.required_with("line", span_u64)?,
            column: record.required_with("column", span_u32)?,
            byte_offset: record.required_with("byte_offset", span_u64)?,
        })
    }
}

impl FromJson for SpanIndexWire {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SpanIndexWire")?;
        Ok(Self {
            paths: record.required("paths")?,
            entries: record.required("entries")?,
        })
    }
}

/// The wire's path reference has a fixed u32 width even on a wider host.
fn encode_path_id(index: usize) -> gmeow_errors::Result<u32> {
    u32::try_from(index).map_err(|cause| {
        gmeow_errors::Diag::of_kind(crate::error::SpanPathIdOverflow { index, cause })
    })
}

impl SpanIndex {
    /// Encode the complete source-span table as compact native JSON without a LF.
    /// Subjects sort lexically; path IDs follow their first encounter in that order.
    /// A path-table index exceeding the wire's u32 width is an actionable error.
    pub fn to_json_bytes(&self) -> gmeow_errors::Result<Vec<u8>> {
        let mut paths: Vec<String> = Vec::new();
        let mut entries = Vec::with_capacity(self.by_subject.len());
        for (subject, span) in &self.by_subject {
            let path = span.path.as_ref();
            let index = match paths.iter().position(|existing| existing == path) {
                Some(index) => index,
                None => {
                    paths.push(path.to_owned());
                    paths.len() - 1
                }
            };
            entries.push(SpanEntryWire {
                subject: subject.clone(),
                path_id: encode_path_id(index)?,
                line: span.line,
                column: span.column,
                byte_offset: span.byte_offset,
            });
        }
        Ok(purrdf_lex::json::record::to_vec(&SpanIndexWire {
            paths,
            entries,
        }))
    }
}

impl FromJson for SpanIndex {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let wire = SpanIndexWire::from_json(value)?;
        // A separate Arc for each table slot preserves repeated-path slot identity.
        let interned: Vec<Arc<str>> = wire
            .paths
            .iter()
            .map(|path| Arc::from(path.as_str()))
            .collect();
        let mut by_subject = BTreeMap::new();
        for (index, entry) in wire.entries.into_iter().enumerate() {
            let path = usize::try_from(entry.path_id)
                .ok()
                .and_then(|path_id| interned.get(path_id))
                .ok_or_else(|| {
                    DecodeError::custom(format!("span path_id {} out of range", entry.path_id))
                        .within("path_id")
                        .within_index_native(index, None)
                        .within("entries")
                })?
                .clone();
            by_subject.insert(
                entry.subject,
                SourceSpan {
                    path,
                    line: entry.line,
                    column: entry.column,
                    byte_offset: entry.byte_offset,
                },
            );
        }
        Ok(SpanIndex { by_subject })
    }
}

#[cfg(test)]
mod tests;
