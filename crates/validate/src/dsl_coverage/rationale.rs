// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The selected canonical rationale obligations, composed with residual DSL shapes.
//! Selection occurs on typed IR; untouched legacy paths are not projected twice.

use gmeow_logic_compile::{
    frontend,
    ir::{ConstraintIr, LogicProgram, ShapeTarget, ValidationShapeIr},
    projections::shapes,
};

const CLASS: &str = "https://blackcatinformatics.ca/gmeow/CompetencyQuestion";
const PATH: &str = "https://blackcatinformatics.ca/gmeow/cqRationale";
const REQUIRED: &str =
    "https://blackcatinformatics.ca/gmeow/CompetencyQuestionRationaleRequiredConstraint";

pub(super) fn compose(vocabulary: &[u8], residual: &str) -> gmeow_errors::Result<String> {
    let source = purrdf::parse_dataset(vocabulary, "text/turtle", None)?;
    let properties = vec![
        canonical_property(source.as_ref(), CLASS, PATH)?,
        canonical_property(
            source.as_ref(),
            "https://blackcatinformatics.ca/gmeow/StructuralAssertion",
            "https://blackcatinformatics.ca/gmeow/saRationale",
        )?,
        canonical_property(
            source.as_ref(),
            "https://blackcatinformatics.ca/gmeow/ExampleConformance",
            "https://blackcatinformatics.ca/gmeow/conformanceRationale",
        )?,
    ];
    let required = canonical_requiredness(source.as_ref(), CLASS, PATH, REQUIRED)?;
    let program = LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None)
        .with_validation_shapes(properties)
        .with_constraints(vec![required]);
    Ok(format!(
        "{residual}\n{}\n{}",
        shapes::project_validation_shapes_shacl(&program),
        shapes::project_procedural_constraints(&program)
    ))
}

pub(super) fn canonical_requiredness(
    source: &purrdf::RdfDataset,
    class: &str,
    path: &str,
    law: &str,
) -> gmeow_errors::Result<ConstraintIr> {
    let required_kind = source.owned_quads().any(|quad| {
        quad.subject == purrdf::RdfTerm::Iri(law.to_owned())
            && quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && quad.object
                == purrdf::RdfTerm::Iri(
                    "https://blackcatinformatics.ca/logic/DisjunctiveRequirednessConstraint"
                        .to_owned(),
                )
    });
    if !required_kind {
        return Err(super::io_err(format!(
            "selected law <{law}> must be a canonical disjunctive requiredness constraint"
        )));
    }
    let required_paths: Vec<_> = source
        .owned_quads()
        .filter(|quad| {
            quad.subject == purrdf::RdfTerm::Iri(law.to_owned())
                && quad.predicate == "https://blackcatinformatics.ca/logic/anyOf"
        })
        .map(|quad| quad.object)
        .collect();
    if required_paths != [purrdf::RdfTerm::Iri(path.to_owned())] {
        return Err(super::io_err(format!(
            "selected requiredness <{law}> must require exactly <{path}>, without alternatives"
        )));
    }
    let (constraints, diagnostics) = frontend::extract_all_constraints(source);
    for diagnostic in diagnostics {
        if diagnostic.subject.as_deref() == Some(law)
            && diagnostic.severity == frontend::Severity::Error
        {
            return Err(super::io_err(format!(
                "required canonical law <{law}>: {}",
                diagnostic.message
            )));
        }
    }
    let mut required: Vec<_> = constraints
        .into_iter()
        .filter(|constraint| constraint.iri == law)
        .collect();
    if required.len() != 1 {
        return Err(super::io_err(format!(
            "requires exactly one canonical requiredness constraint <{law}>"
        )));
    }
    let required = required.remove(0);
    if required.target != ShapeTarget::Class(class.to_owned()) {
        return Err(super::io_err(format!(
            "requiredness constraint <{law}> must target <{class}>"
        )));
    }
    if shapes::project_procedural_constraint(&required).is_empty() {
        return Err(super::io_err(format!(
            "canonical requiredness constraint <{law}> is not projectable"
        )));
    }
    Ok(required)
}

pub(super) fn canonical_property(
    source: &purrdf::RdfDataset,
    class: &str,
    path: &str,
) -> gmeow_errors::Result<ValidationShapeIr> {
    let mut selected = Vec::new();
    for mut shape in frontend::derive_validation_shapes(source)? {
        if shape.target != ShapeTarget::Class(class.to_owned()) {
            continue;
        }
        shape
            .properties
            .retain(|property| property.path == path && !property.inverse);
        if !shape.properties.is_empty() {
            if !shape.node_components.is_empty() {
                return Err(super::io_err(format!(
                    "selected path <{path}> has ambiguous node-level obligations"
                )));
            }
            selected.push(shape);
        }
    }
    if selected.len() != 1 || selected[0].properties.len() != 1 {
        return Err(super::io_err(format!(
            "requires exactly one canonical property obligation for <{class}> <{path}>"
        )));
    }
    if !shapes::shacl_residue(&selected[0]).is_empty() {
        return Err(super::io_err(format!(
            "canonical property obligation <{class}> <{path}> is not completely projectable"
        )));
    }
    Ok(selected.remove(0))
}

#[cfg(test)]
mod tests;
