// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Finite temporal standard translation through the shared source reader.
//!
//! These guards require an explicitly selected finite journal or observed path
//! at evaluation. Producing FOL structure does not admit infinite-trace semantics.

use super::formula_reader::{ReadResult, Translation, formula_err, one_child_subject};
use super::{ACTUAL_WORLD_IRI, Formula, FormulaReader, Subject, Term};

/// Immediate observed finite successor, preserving the selected context axes.
pub const FINITE_NEXT: &str = "https://blackcatinformatics.ca/logic/finiteNext";
/// Current-or-later observed position in the selected finite context trajectory.
pub const FINITE_AT_OR_AFTER: &str = "https://blackcatinformatics.ca/logic/finiteAtOrAfter";
/// Strict position order inside an already selected finite interval.
pub const FINITE_STRICTLY_BEFORE: &str =
    "https://blackcatinformatics.ca/logic/finiteStrictlyBefore";

fn atom(node: &Subject, relation: &str, args: Vec<Term>) -> ReadResult<Formula> {
    let relation = Term::iri(relation).map_err(|error| formula_err(node, error.message()))?;
    Formula::atom(relation, args).map_err(|error| formula_err(node, error.message()))
}

pub(super) fn expand<D: purrdf::DatasetView + ?Sized>(
    reader: &mut FormulaReader<'_, D>,
    node: &Subject,
    constructor: &str,
    context: &Translation,
    active: &mut Vec<D::Id>,
) -> ReadResult<Formula> {
    let (world, depth) = match context {
        Translation::Plain => (Term::Iri(ACTUAL_WORLD_IRI.to_owned()), 0),
        Translation::AtWorld { world, depth } => (world.clone(), *depth),
    };
    let body = one_child_subject(&reader.source(), node, constructor)?;
    let variable = reader.temporal_variable(false, depth);
    let future = Term::Var(variable.clone());
    let guard = atom(
        node,
        if constructor == "next" {
            FINITE_NEXT
        } else {
            FINITE_AT_OR_AFTER
        },
        vec![world.clone(), future.clone()],
    )?;
    let body = reader.expand(
        &body,
        &Translation::AtWorld {
            world: future.clone(),
            depth: depth + 1,
        },
        active,
    )?;
    if constructor == "globally" {
        return Ok(Formula::Forall {
            vars: vec![variable],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(body))),
        });
    }
    let body = if constructor == "until" {
        let maintained = one_child_subject(&reader.source(), node, "untilLeft")?;
        let intermediate_variable = reader.temporal_variable(true, depth);
        let intermediate = Term::Var(intermediate_variable.clone());
        let interval = Formula::And(vec![
            atom(node, FINITE_AT_OR_AFTER, vec![world, intermediate.clone()])?,
            atom(
                node,
                FINITE_STRICTLY_BEFORE,
                vec![intermediate.clone(), future],
            )?,
        ]);
        let maintained = reader.expand(
            &maintained,
            &Translation::AtWorld {
                world: intermediate,
                depth: depth + 1,
            },
            active,
        )?;
        Formula::And(vec![
            body,
            Formula::Forall {
                vars: vec![intermediate_variable],
                body: Box::new(Formula::Implies(Box::new(interval), Box::new(maintained))),
            },
        ])
    } else {
        body
    };
    Ok(Formula::Exists {
        vars: vec![variable],
        body: Box::new(Formula::And(vec![guard, body])),
    })
}
