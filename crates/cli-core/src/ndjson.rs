// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Event-first, line-framed native diagnostic records.

use gmeow_errors::Finding;
use purrdf_lex::json::{Object, Value, record::ToJson};
use std::io::{self, Write};

pub(super) enum NdjsonEvent<'a> {
    StageStart {
        stage: &'a str,
    },
    StageEnd {
        stage: &'a str,
        elapsed_ms: u128,
    },
    Finding {
        tool: &'a str,
        finding: &'a Finding,
    },
    Summary {
        tool: &'a str,
        ok: bool,
        errors: usize,
        warnings: usize,
    },
}

impl ToJson for NdjsonEvent<'_> {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        match self {
            Self::StageStart { stage } => {
                object.insert("event", "stage_start");
                object.insert("stage", stage.to_json());
            }
            Self::StageEnd { stage, elapsed_ms } => {
                object.insert("event", "stage_end");
                object.insert("stage", stage.to_json());
                object.insert("elapsed_ms", Value::from(*elapsed_ms));
            }
            Self::Finding { tool, finding } => {
                object.insert("event", "finding");
                object.insert("tool", tool.to_json());
                object.insert("finding", finding.to_json());
            }
            Self::Summary {
                tool,
                ok,
                errors,
                warnings,
            } => {
                object.insert("event", "summary");
                object.insert("tool", tool.to_json());
                object.insert("ok", ok.to_json());
                object.insert("errors", errors.to_json());
                object.insert("warnings", warnings.to_json());
            }
        }
        Value::Object(object)
    }
}

impl NdjsonEvent<'_> {
    /// Write the complete compact record and exactly one line separator.
    /// Encoding is infallible; sink failures remain errors for the caller.
    pub(super) fn write_to(&self, output: &mut impl Write) -> io::Result<()> {
        output.write_all(&purrdf_lex::json::record::to_vec(self))?;
        output.write_all(b"\n")
    }
}

#[cfg(test)]
mod tests;
