// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit producer for the committed Common Logic ingestion fixture.
//! The test runner only reads its output; it never invokes this producer.

use gmeow_logic_compile::frontend::{Severity, parse_logic_path};

fn main() -> gmeow_errors::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [source, output, source_iri] = arguments.as_slice() else {
        return Err(gmeow_errors::Diag::of_kind(
            gmeow_logic_compile::error::Frontend {
                detail: "expected SOURCE_TTL OUTPUT_CLIF SOURCE_IRI".into(),
            },
        ));
    };
    let (program, diagnostics) =
        parse_logic_path(std::path::Path::new(source), Some(source_iri.clone()))?;
    for diagnostic in &diagnostics {
        eprintln!(
            "{} {}: {}",
            diagnostic.severity.as_str(),
            diagnostic.code,
            diagnostic.message
        );
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(gmeow_errors::Diag::of_kind(
            gmeow_logic_compile::error::Frontend {
                detail: "fixture source has compiler errors".into(),
            },
        ));
    }
    let clif = gmeow_logic_compile::clif::writer::project_clif(&program)?;
    std::fs::write(output, clif.content)?;
    Ok(())
}
