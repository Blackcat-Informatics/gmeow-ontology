// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Creator obligations selected from the slice vocabulary, beside residual checks.

use gmeow_logic_compile::{ir::LogicProgram, projections::shapes};

use super::rationale::{canonical_property, canonical_requiredness};

pub(super) fn compose(vocabulary: &[u8], residual: &str) -> gmeow_errors::Result<String> {
    const CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";
    const PATH: &str = "http://purl.org/dc/terms/creator";
    const REQUIRED: &str =
        "https://blackcatinformatics.ca/gmeow/SliceDependsOnCreatorRequiredConstraint";
    let source = purrdf::parse_dataset(vocabulary, "text/turtle", None)?;
    let property = canonical_property(source.as_ref(), CLASS, PATH)?;
    let required = canonical_requiredness(source.as_ref(), CLASS, PATH, REQUIRED)?;
    let program = LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None)
        .with_validation_shapes(vec![property])
        .with_constraints(vec![required]);
    Ok(format!(
        "{residual}\n{}\n{}",
        shapes::project_validation_shapes_shacl(&program),
        shapes::project_procedural_constraints(&program)
    ))
}
