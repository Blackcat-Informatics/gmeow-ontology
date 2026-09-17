// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer observations for the grounding module's exact Common Logic contract.
//!
//! The catalog supplies the original native document. Its standalone program and
//! canonical fixed point are shared by all three dialect checks. This scope is
//! distinct from the aggregate source program: another module cannot supply a
//! missing declaration. Tests authenticate and grade the small observation only.

use std::collections::BTreeMap;

use gmeow_logic_compile::adapter::assert_ir_isomorphic;
use gmeow_logic_compile::frontend::{Diagnostic, Severity, parse_logic_dataset};
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::projections::{ProjectionResult, rdf};
use gmeow_logic_compile::{cgif, clif, xcl};
use serde::{Deserialize, Serialize};

/// Internal authenticated artifact; it is not a new public serialization.
const CHANNEL: &str = "pipeline/logic-module-roundtrip.json";

#[derive(Debug, Serialize, Deserialize)]
struct Observation {
    source: String,
    dialects: Result<[DialectObservation; 3], gmeow_errors::RecordedDiag>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DialectObservation {
    dialect: String,
    result: Result<(), gmeow_errors::RecordedDiag>,
}

pub(super) fn record(
    raw: &LogicProgram,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observation = Observation {
        source: super::SOURCE_PATH.to_owned(),
        dialects: super::observed(observe(raw)),
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observation)
            .map_err(|error| super::stage_err(format!("encode CL observation: {error}")))?,
    );
    Ok(())
}

fn observe(raw: &LogicProgram) -> gmeow_errors::Result<[DialectObservation; 3]> {
    let source = Some(super::SOURCE_IRI.to_owned());
    // Canonical RDF is the reference projection, whose first pass can normalize
    // serialization-dependent axiom identities. Share that reference across the
    // codecs without rendering and reparsing Turtle to pass native RDF around.
    let canonical = rdf::project_canonical_rdf12_dataset(raw).map_err(gmeow_errors::Diag::from)?;
    let (fixed, _) = parse_logic_dataset(&canonical.dataset, source.clone())
        .map_err(gmeow_errors::Diag::from)?;
    Ok([
        dialect("clif", &fixed, clif::project_clif(&fixed), |text| {
            clif::parse_clif_str(text, source.clone()).map_err(gmeow_errors::Diag::from)
        }),
        dialect("cgif", &fixed, cgif::project_cgif(&fixed), |text| {
            cgif::parse_cgif_str(text, source.clone()).map_err(gmeow_errors::Diag::from)
        }),
        dialect("xcl", &fixed, xcl::project_xcl(&fixed), |text| {
            // The XCL reader requires a well-formed XML document before reading
            // its logical content, preserving the original XML validity check.
            xcl::parse_xcl_str(text, source.clone()).map_err(gmeow_errors::Diag::from)
        }),
    ])
}

fn dialect(
    name: &str,
    fixed: &LogicProgram,
    projected: gmeow_errors::Result<ProjectionResult>,
    parse: impl FnOnce(&str) -> gmeow_errors::Result<(LogicProgram, Vec<Diagnostic>)>,
) -> DialectObservation {
    let result = projected.and_then(|p| {
        let (reconstructed, diagnostics) = parse(&p.content)?;
        if let Some(error) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
            return Err(super::stage_err(format!(
                "{}: {}",
                error.code, error.message
            )));
        }
        exact(fixed, &reconstructed)
    });
    DialectObservation {
        dialect: name.to_owned(),
        result: super::observed(result),
    }
}

fn exact(reference: &LogicProgram, reconstructed: &LogicProgram) -> gmeow_errors::Result<()> {
    // The shared adapter checks complete IR equality before constructing its
    // directional diagnostic, including scope and correspondence collections.
    assert_ir_isomorphic(reference, reconstructed).map_err(gmeow_errors::Diag::from)
}

#[path = "roundtrip.tests.rs"]
#[cfg(test)]
mod tests;
