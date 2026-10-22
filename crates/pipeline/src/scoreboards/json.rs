// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed claim records. Public pretty output separately sorts the native tree.

use super::{ClaimEvidence, ClaimFlags, FlatClaim};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for FlatClaim {
    fn to_json(&self) -> Value {
        Object::new()
            .with("claim", self.claim.to_json())
            .with("text", self.text.to_json())
            .with("model", self.model.to_json())
            .with("method", self.method.to_json())
            .with("confidence", self.confidence.to_json())
            .with("evidence", self.evidence.to_json())
            .with("flags", self.flags.to_json())
            .with("contradicts", self.contradicts.to_json())
            .into()
    }
}
impl FromJson for FlatClaim {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FlatClaim")?;
        Ok(Self {
            claim: fields.required("claim")?,
            text: fields.optional("text")?,
            model: fields.optional("model")?,
            method: fields.required("method")?,
            confidence: fields.optional("confidence")?,
            evidence: fields.required("evidence")?,
            flags: fields.required("flags")?,
            contradicts: fields.required("contradicts")?,
        })
    }
}
impl ToJson for ClaimEvidence {
    fn to_json(&self) -> Value {
        Object::new()
            .with("span", self.span.to_json())
            .with("chunk", self.chunk.to_json())
            .with("source", self.source.to_json())
            .with("start", self.start.to_json())
            .with("end", self.end.to_json())
            .with("polarity", self.polarity.to_json())
            .into()
    }
}

/// Ordinary signed-offset transport rejects the float-like negative-zero token.
/// Native signed integer admission otherwise supplies exact i64 bounds/grammar.
fn evidence_offset(value: &Value) -> Result<i64, DecodeError> {
    if value
        .as_number()
        .is_some_and(|number| number.lexeme() == "-0")
    {
        return Err(DecodeError::invalid_value(value, "i64"));
    }
    i64::from_json(value)
}
impl FromJson for ClaimEvidence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClaimEvidence")?;
        Ok(Self {
            span: fields.required("span")?,
            chunk: fields.optional("chunk")?,
            source: fields.optional("source")?,
            start: fields.optional_with("start", evidence_offset)?,
            end: fields.optional_with("end", evidence_offset)?,
            polarity: fields.optional("polarity")?,
        })
    }
}
impl ToJson for ClaimFlags {
    fn to_json(&self) -> Value {
        Object::new()
            .with("ungrounded", self.ungrounded)
            .with("contradicted", self.contradicted)
            .with("stale", self.stale)
            .into()
    }
}
impl FromJson for ClaimFlags {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClaimFlags")?;
        Ok(Self {
            ungrounded: fields.required("ungrounded")?,
            contradicted: fields.required("contradicted")?,
            stale: fields.required("stale")?,
        })
    }
}

#[cfg(test)]
mod tests;
