// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed inverse-ingest lowering. Recovery asserts its declared source atoms;
//! candidate preimages retain inert claims and deterministic import provenance.
//! Required SPARQL is an output of these same native templates and patterns.

use crate::projections::get_leg::{Atom, ProfileBinding, ProjectionCell};
use crate::projections::put_derivation::classify_put;
use crate::projections::reified_claim::{
    AssertionPolarity, ClaimAnnotation, ClaimObject, GM_MAPPED_FROM, ReifiedClaim,
    reified_claim_template,
};
use crate::projections::sparql::{GENERATED_BANNER, HelperNames, LoweredLeg, native, templates_of};
use purrdf::sparql::{Literal, Query, TermPattern};
use std::collections::BTreeMap;

const IMPORTER_AGENT_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/agent/put-projection-importer";

fn claim_of_atom(
    atom: &Atom,
    target: &str,
    import: &str,
    index: usize,
) -> gmeow_errors::Result<ReifiedClaim<TermPattern>> {
    if atom.path.is_some() || !atom.path_alts.is_empty() || atom.predicate_var.is_some() {
        return Err(native::error(format!(
            "put emitter: source atom on ?{} has no single qPredicate IRI to reify honestly",
            atom.subject_var
        )));
    }
    let predicate = atom
        .predicate
        .clone()
        .ok_or_else(|| native::error("put source atom has no predicate to reify"))?;
    let value = if let Some(value) = &atom.object_value {
        ClaimObject::Iri(native::named(value)?)
    } else if let Some(literal) = &atom.object_literal {
        // Keep the actual datatype/language/direction. A datatype IRI is not a
        // language tag, and an inert candidate claim does not erase that evidence.
        ClaimObject::Literal(native::literal_term(literal)?)
    } else if let Some(variable) = &atom.object_var {
        ClaimObject::Iri(native::var(variable))
    } else {
        return Err(native::error("put source atom has no object to reify"));
    };
    Ok(ReifiedClaim {
        cell_label: format!("cell{index}"),
        subject: native::var(&atom.subject_var),
        predicate,
        object: value,
        annotations: vec![ClaimAnnotation {
            label: format!("mapann{index}"),
            property: GM_MAPPED_FROM.into(),
            value: native::named(target)?,
        }],
        generated_by: Some(native::named(import)?),
    })
}

pub(super) fn lower_binding(
    cell: &ProjectionCell,
    binding: &ProfileBinding,
    claim_index: &mut usize,
    names: &HelperNames,
) -> gmeow_errors::Result<Option<LoweredLeg>> {
    let class = classify_put(
        binding.mnemomorphic,
        binding.lattice().1,
        binding.ingest_claim.as_slice(),
    );
    let polarity = AssertionPolarity::of(class);
    if polarity == AssertionPolarity::Withhold {
        return Ok(None);
    }
    let mut template = Vec::new();
    let mut residue = Vec::new();
    match polarity {
        AssertionPolarity::AssertBase => {
            for atom in cell.pattern.flat_atoms() {
                template.push(native::atom_triple(&atom, &BTreeMap::new())?);
            }
        }
        AssertionPolarity::ReifyClaim => {
            let target = binding
                .to_class
                .as_ref()
                .or(binding.to_predicate.as_ref())
                .ok_or_else(|| {
                    native::error(format!(
                        "put binding <{}> for {} has no nameable forward target",
                        cell.iri, binding.profile
                    ))
                })?;
            let import = format!(
                "https://blackcatinformatics.ca/gmeow/import/{}",
                binding.profile
            );
            for atom in cell.pattern.flat_atoms() {
                template.extend(reified_claim_template(&claim_of_atom(
                    &atom,
                    target,
                    &import,
                    *claim_index,
                )?)?);
                *claim_index += 1;
            }
            template.extend([
                native::triple(
                    native::named(&import)?,
                    super::get_leg::RDF_TYPE,
                    native::named("https://blackcatinformatics.ca/gmeow/ImportActivity")?,
                )?,
                native::triple(
                    native::named(&import)?,
                    "http://www.w3.org/2000/01/rdf-schema#label",
                    TermPattern::Literal(Literal::new_simple(format!(
                        "inverse-ingest of {} into GMEOW",
                        binding.profile
                    ))),
                )?,
                native::triple(
                    native::named(&import)?,
                    "https://blackcatinformatics.ca/gmeow/wasAssociatedWith",
                    native::named(IMPORTER_AGENT_IRI)?,
                )?,
                native::triple(
                    native::named(IMPORTER_AGENT_IRI)?,
                    super::get_leg::RDF_TYPE,
                    native::named("https://blackcatinformatics.ca/gmeow/SoftwareAgent")?,
                )?,
            ]);
            residue.clone_from(&binding.ingest_residue);
        }
        AssertionPolarity::Withhold => unreachable!(),
    }
    // Ingest matches the external carrier without the get leg's suppression or retag.
    let pattern = native::bgp(templates_of(cell, binding, &BTreeMap::new(), names)?);
    Ok(Some(LoweredLeg {
        template,
        pattern,
        residue,
        claim: polarity == AssertionPolarity::ReifyClaim,
    }))
}

pub(super) fn emit_profile(
    profile: &str,
    claim: bool,
    query: Query,
) -> gmeow_errors::Result<String> {
    let header = if claim {
        format!(
            "# Inverse ingest: {profile} → GMEOW. Mint-with-claim, validation-only — import-derived claim, not extracted fact; subject/tenure not synthesized (residue). {GENERATED_BANNER}\n"
        )
    } else {
        format!(
            "# Inverse ingest: pure {profile} → GMEOW. CompleteOver up-lift — identity on the displayable image of get. {GENERATED_BANNER}\n"
        )
    };
    Ok(format!("{header}{}", native::emit(query)?))
}

#[path = "sparql_put.tests.rs"]
#[cfg(test)]
mod tests;
