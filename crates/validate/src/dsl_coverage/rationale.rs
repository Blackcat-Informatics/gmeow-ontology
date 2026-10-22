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
    let property = rationale_property(source.as_ref())?;
    let required = rationale_requiredness(source.as_ref())?;
    let program = LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None)
        .with_validation_shapes(vec![property])
        .with_constraints(vec![required]);
    Ok(format!(
        "{residual}\n{}\n{}",
        shapes::project_validation_shapes_shacl(&program),
        shapes::project_procedural_constraints(&program)
    ))
}

fn rationale_requiredness(source: &purrdf::RdfDataset) -> gmeow_errors::Result<ConstraintIr> {
    let required_kind = source.owned_quads().any(|quad| {
        quad.subject == purrdf::RdfTerm::Iri(REQUIRED.to_owned())
            && quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && quad.object
                == purrdf::RdfTerm::Iri(
                    "https://blackcatinformatics.ca/logic/DisjunctiveRequirednessConstraint"
                        .to_owned(),
                )
    });
    if !required_kind {
        return Err(super::io_err(
            "selected rationale law must be a canonical disjunctive requiredness constraint"
                .to_owned(),
        ));
    }
    let required_paths: Vec<_> = source
        .owned_quads()
        .filter(|quad| {
            quad.subject == purrdf::RdfTerm::Iri(REQUIRED.to_owned())
                && quad.predicate == "https://blackcatinformatics.ca/logic/anyOf"
        })
        .map(|quad| quad.object)
        .collect();
    if required_paths != [purrdf::RdfTerm::Iri(PATH.to_owned())] {
        return Err(super::io_err("selected rationale requiredness must require exactly cqRationale, without alternatives".to_owned()));
    }
    let (constraints, diagnostics) = frontend::extract_all_constraints(source);
    for diagnostic in diagnostics {
        if diagnostic.subject.as_deref() == Some(REQUIRED)
            && diagnostic.severity == frontend::Severity::Error
        {
            return Err(super::io_err(format!(
                "required canonical rationale law: {}",
                diagnostic.message
            )));
        }
    }
    let mut required: Vec<_> = constraints
        .into_iter()
        .filter(|constraint| constraint.iri == REQUIRED)
        .collect();
    if required.len() != 1 {
        return Err(super::io_err(
            "requires exactly one canonical rationale requiredness constraint".to_owned(),
        ));
    }
    let required = required.remove(0);
    if required.target != ShapeTarget::Class(CLASS.to_owned()) {
        return Err(super::io_err(
            "rationale requiredness constraint targets the wrong class".to_owned(),
        ));
    }
    if shapes::project_procedural_constraint(&required).is_empty() {
        return Err(super::io_err(
            "canonical rationale requiredness constraint is not projectable".to_owned(),
        ));
    }
    Ok(required)
}

fn rationale_property(source: &purrdf::RdfDataset) -> gmeow_errors::Result<ValidationShapeIr> {
    let mut selected = Vec::new();
    for mut shape in frontend::derive_validation_shapes(source)? {
        if shape.target != ShapeTarget::Class(CLASS.to_owned()) {
            continue;
        }
        shape
            .properties
            .retain(|property| property.path == PATH && !property.inverse);
        if !shape.properties.is_empty() {
            if !shape.node_components.is_empty() {
                return Err(super::io_err(
                    "rationale path has ambiguous node-level obligations".to_owned(),
                ));
            }
            selected.push(shape);
        }
    }
    if selected.len() != 1 || selected[0].properties.len() != 1 {
        return Err(super::io_err(
            "requires exactly one canonical CQ rationale property obligation".to_owned(),
        ));
    }
    if !shapes::shacl_residue(&selected[0]).is_empty() {
        return Err(super::io_err(
            "canonical rationale property obligation is not completely projectable".to_owned(),
        ));
    }
    Ok(selected.remove(0))
}

#[cfg(test)]
mod tests;
