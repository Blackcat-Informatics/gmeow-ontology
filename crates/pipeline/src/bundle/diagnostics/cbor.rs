// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete diagnostics publication CBOR and its intentionally closed admission.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Arc;

use gmeow_errors::Report;
use gmeow_errors::cbor::{DiagnosticCborError, MAX_DIAGNOSTIC_CBOR_DEPTH};
use purrdf_lex::cbor::{self, Value, head};

use super::{DiagnosticReportOwner, DiagnosticsPublication};

impl DiagnosticsPublication {
    /// Stream the exact report-owner map and complete original report records.
    /// Intrinsic and stage/graph validation remain with the existing admission.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 1)?;
        head::write_text(out, "reports")?;
        head::write_head(out, head::MAP, self.reports.len() as u64)?;
        for (owner, report) in &self.reports {
            head::write_text(
                out,
                match owner {
                    DiagnosticReportOwner::CompileLogic => "CompileLogic",
                    DiagnosticReportOwner::Validate => "Validate",
                },
            )?;
            report.write_cbor(out)?;
        }
        Ok(())
    }

    /// Decode one bounded complete publication with a closed typed record schema.
    /// Unknown/duplicate fields and unsupported metadata are intentionally refused
    /// rather than discarded. Existing ownership validation follows decoding.
    ///
    /// # Errors
    /// Refuses malformed CBOR, trailing bytes, missing reports and foreign owners.
    pub(crate) fn read_cbor(bytes: &[u8]) -> Result<Self, DiagnosticCborError> {
        let value = cbor::decode(
            bytes,
            cbor::Limits {
                max_depth: MAX_DIAGNOSTIC_CBOR_DEPTH,
            },
        )
        .map_err(DiagnosticCborError::from)?;
        let Value::Map(fields) = &value else {
            return Err(DiagnosticCborError::invalid(
                "expected DiagnosticsPublication map",
            ));
        };
        let mut reports = None;
        for (key, value) in fields {
            match key {
                Value::Text(key) if key == "reports" => {
                    if reports.is_some() {
                        return Err(DiagnosticCborError::invalid(
                            "duplicate DiagnosticsPublication.reports",
                        ));
                    }
                    reports = Some(read_reports(value)?);
                }
                _ => {
                    return Err(DiagnosticCborError::invalid(
                        "unknown DiagnosticsPublication field",
                    ));
                }
            }
        }
        Ok(Self {
            reports: reports.ok_or_else(|| {
                DiagnosticCborError::invalid("missing DiagnosticsPublication.reports")
            })?,
        })
    }
}

/// Admit at most the two distinct closed owners without replacing an earlier report.
fn read_reports(
    value: &Value,
) -> Result<BTreeMap<DiagnosticReportOwner, Arc<Report>>, DiagnosticCborError> {
    let Value::Map(entries) = value else {
        return Err(DiagnosticCborError::invalid(
            "expected native diagnostic report-owner map",
        ));
    };
    let mut reports = BTreeMap::new();
    for (key, value) in entries {
        let owner = match key {
            Value::Text(key) if key == "CompileLogic" => DiagnosticReportOwner::CompileLogic,
            Value::Text(key) if key == "Validate" => DiagnosticReportOwner::Validate,
            _ => {
                return Err(DiagnosticCborError::invalid(
                    "unknown native diagnostic report owner",
                ));
            }
        };
        if reports.contains_key(&owner) {
            return Err(DiagnosticCborError::invalid(
                "duplicate native diagnostic report owner",
            ));
        }
        reports.insert(owner, Arc::new(Report::read_cbor(value)?));
    }
    Ok(reports)
}
