// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One native execution boundary for typed forward projection programs. Every
//! required SELECT finishes and admits all matched values before CONSTRUCT runs.

use std::sync::Arc;

use gmeow_errors::{Diag, ResultExt};
use gmeow_logic_compile::projections::{
    grounding_catalog::GroundingCatalog, profile_program::ProjectionProfileProgram,
};
use purrdf::sparql::{NativeSparqlEngine, PreparedQuery, QueryOptions};
use purrdf::{RdfDataset, SparqlResult, TermValue};

/// Prepared native programs retain the complete selected capability/proof input.
#[derive(Debug, Clone)]
pub struct PreparedProjectionProfile {
    program: ProjectionProfileProgram,
    forward: Arc<PreparedQuery>,
    checks: Vec<Arc<PreparedQuery>>,
}

/// Capability token for one immutable input. Only complete native input
/// admission can create it; construction cannot bypass or weaken those checks.
#[derive(Debug)]
pub(crate) struct AdmittedProjection<'a> {
    program: &'a PreparedProjectionProfile,
    source: &'a RdfDataset,
}

impl AdmittedProjection<'_> {
    pub(crate) fn profile(&self) -> &str {
        self.program.profile()
    }
    pub(crate) fn identity(&self) -> &str {
        self.program.identity()
    }

    pub(crate) fn execute(self) -> gmeow_errors::Result<SparqlResult> {
        let result = NativeSparqlEngine::new()
            .query_prepared_view(
                self.source,
                &self.program.forward,
                &[],
                &QueryOptions::EMPTY,
            )
            .with_ctx(|| format!("execute required projection {}", self.program.program.id))?;
        if !matches!(result, SparqlResult::Graph(_)) {
            return Err(Diag::of_kind(crate::error::Projection {
                message: format!(
                    "{}: forward projection did not return a graph",
                    self.program.program.id
                ),
            }));
        }
        Ok(result)
    }
}

impl PreparedProjectionProfile {
    pub fn from_bundle(bytes: &[u8], profile: &str) -> gmeow_errors::Result<Self> {
        prepared_from_bundle(bytes, &[profile])
            .with_ctx(|| format!("prepare required installed projection {profile:?} from {profile}.projection.json"))?.into_iter().next()
            .ok_or_else(|| Diag::of_kind(crate::error::Projection { message: format!("selected projection {profile:?} lost its required typed product") }))
    }

    pub fn prepare(
        program: &ProjectionProfileProgram,
        catalog: &GroundingCatalog,
    ) -> gmeow_errors::Result<Self> {
        program.validate(catalog)?;
        let engine = NativeSparqlEngine::new();
        let forward = engine
            .prepare_query(&program.payload.forward, None)
            .with_ctx(|| format!("prepare required projection {}", program.id))?;
        let checks = program
            .payload
            .checks
            .iter()
            .map(|check| {
                engine
                    .prepare_query(&check.query, None)
                    .with_ctx(|| format!("prepare required native input check {}", check.id))
            })
            .collect::<gmeow_errors::Result<_>>()?;
        Ok(Self {
            program: program.clone(),
            forward,
            checks,
        })
    }

    pub fn profile(&self) -> &str {
        &self.program.payload.profile
    }
    pub fn identity(&self) -> &str {
        &self.program.id
    }

    /// A single malformed matched value refuses the whole selected projection.
    /// No value is filtered, converted, dropped or partially constructed.
    pub fn execute(&self, source: &RdfDataset) -> gmeow_errors::Result<SparqlResult> {
        self.admit(source)?.execute()
    }

    fn admit<'a>(&'a self, source: &'a RdfDataset) -> gmeow_errors::Result<AdmittedProjection<'a>> {
        let engine = NativeSparqlEngine::new();
        for (check, query) in self.program.payload.checks.iter().zip(&self.checks) {
            let result = engine
                .query_prepared_view(source, query, &[], &QueryOptions::EMPTY)
                .with_ctx(|| format!("execute required native input check {}", check.id))?;
            let SparqlResult::Solutions {
                variables, rows, ..
            } = result
            else {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "{}: native input check did not return SELECT solutions",
                        check.id
                    ),
                }));
            };
            let variable = check.checked_variable();
            let columns: Vec<_> = variables
                .iter()
                .enumerate()
                .filter_map(|(index, name)| (name == &variable).then_some(index))
                .collect();
            let [column] = columns.as_slice() else {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "{}: required source variable ?{variable} is absent or ambiguous",
                        check.id
                    ),
                }));
            };
            let subject_variable = check.checked_subject();
            let subjects: Vec<_> = variables
                .iter()
                .enumerate()
                .filter_map(|(index, name)| (name == &subject_variable).then_some(index))
                .collect();
            let [subject_column] = subjects.as_slice() else {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "{}: required source subject ?{subject_variable} is absent or ambiguous",
                        check.source_mapping
                    ),
                }));
            };
            for (row_index, row) in rows.iter().enumerate() {
                if row.len() != variables.len() {
                    return Err(Diag::of_kind(crate::error::Projection {
                        message: format!("{}: malformed native SELECT row {row_index}", check.id),
                    }));
                }
                let Some(Some(TermValue::Literal {
                    datatype,
                    language,
                    direction,
                    ..
                })) = row.get(*column)
                else {
                    let subject = diagnostic_term(row[*subject_column].as_ref())?;
                    let value = diagnostic_term(row[*column].as_ref())?;
                    return Err(Diag::of_kind(crate::error::Projection {
                        message: format!(
                            "profile {} mapping <{}> binding <{}>: subject {} <{}> value {} for ?{} in row {} must be a complete RDF literal; IRI, blank, quoted and unbound values are refused before projection",
                            self.program.payload.profile,
                            check.source_mapping,
                            check.binding_iri,
                            subject,
                            check.selected.support.source_property,
                            value,
                            check.selected.object_variable,
                            row_index
                        ),
                    }));
                };
                purrdf::RdfLiteral::validate_components(datatype, language.as_deref(), *direction)
                    .map_err(|why| {
                        Diag::of_kind(crate::error::Projection {
                            message: format!(
                                "{}: malformed native RDF literal in row {row_index}: {why}",
                                check.id
                            ),
                        })
                    })?;
            }
        }
        Ok(AdmittedProjection {
            program: self,
            source,
        })
    }
}

fn diagnostic_term(value: Option<&TermValue>) -> gmeow_errors::Result<String> {
    match value {
        Some(value) => value
            .to_rdf_term()
            .map(|term| term.to_string())
            .map_err(|why| {
                Diag::of_kind(crate::error::Projection {
                    message: format!("malformed native matched term: {why}"),
                })
            }),
        None => Ok("UNBOUND".to_owned()),
    }
}

/// Admit every selected program before handing out any construction capability.
pub(crate) fn admit_all<'a>(
    programs: &'a [PreparedProjectionProfile],
    source: &'a RdfDataset,
) -> gmeow_errors::Result<Vec<AdmittedProjection<'a>>> {
    programs
        .iter()
        .map(|program| program.admit(source))
        .collect()
}

/// Select exact typed products, never archive-wide `.rq` filename inference.
pub fn programs_from_archive(
    archive: &std::collections::BTreeMap<String, Vec<u8>>,
    profiles: &[&str],
    catalog: &GroundingCatalog,
) -> gmeow_errors::Result<Vec<ProjectionProfileProgram>> {
    profiles
        .iter()
        .map(|profile| {
            let filename = format!("{profile}.projection.json");
            let matches: Vec<_> = archive
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == filename || name.ends_with(&format!("/{filename}"))
                })
                .collect();
            let [(_, bytes)] = matches.as_slice() else {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "selected projection {profile:?} requires exactly one {filename} product"
                    ),
                }));
            };
            let program = ProjectionProfileProgram::from_bytes(bytes, catalog)?;
            if program.payload.profile != *profile {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!("{filename}: product declares a different profile"),
                }));
            }
            Ok(program)
        })
        .collect()
}

/// Installed consumers load exact native catalog evidence and explicitly named
/// typed products from the same authenticated bundle bytes.
pub fn prepared_from_bundle(
    bytes: &[u8],
    profiles: &[&str],
) -> gmeow_errors::Result<Vec<PreparedProjectionProfile>> {
    let graph = purrdf::gts::read_graph(bytes, true)?;
    let catalog = GroundingCatalog::from_graph(&graph)?;
    let archive = gmeow_gts_profile::archive::required_blob(&graph, "queries-archive").map_err(
        |mut diagnostic| {
            let operation = format!(
                "load required selected projection products {}",
                profiles
                    .iter()
                    .map(|profile| format!("{profile}.projection.json"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            diagnostic.inner_mut().message = format!("{operation}: {}", diagnostic.message());
            diagnostic.with_context(operation)
        },
    )?;
    programs_from_required_archive(&archive, profiles, &catalog)?
        .iter()
        .map(|program| PreparedProjectionProfile::prepare(program, &catalog))
        .collect()
}

/// Select exact shipped product occurrences through the existing bounded native
/// archive owner. A map would lose duplicate same-path occurrence evidence.
pub fn programs_from_required_archive(
    archive: &[u8],
    profiles: &[&str],
    catalog: &GroundingCatalog,
) -> gmeow_errors::Result<Vec<ProjectionProfileProgram>> {
    profiles
        .iter()
        .map(|profile| {
            let path = format!("{profile}.projection.json");
            let bytes = gmeow_gts_profile::archive::archive_member(
                archive,
                &path,
                gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES,
            )?;
            let program = ProjectionProfileProgram::from_bytes(bytes, catalog)?;
            if program.payload.profile != *profile {
                return Err(Diag::of_kind(crate::error::Projection {
                    message: format!("{path}: product declares a different profile"),
                }));
            }
            Ok(program)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
