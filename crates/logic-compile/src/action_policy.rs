// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native preparation of the canonical MCP action theory. Source parsing belongs to
//! the producer; consumers hydrate the prepared rows and add only actual call state.

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTextDirection};
use serde::{Deserialize, Serialize};

/// Canonical authored action-policy document.
pub const SOURCE_PATH: &str = "slices/core/agentic/examples/mcp-action-policy.ttl";
/// Exact source identity this implementation's action semantics admit.
pub const SOURCE_SHA256: &str = "d4cb9681847cbbf9023775d6f69847ad002295020229ef983acd4b552d35542b";
/// Producer-only source observation channel.
pub const SOURCE_ARTIFACT: &str = "pipeline/mcp-action-policy.json";
/// The prepared policy member inside the bundle's reasoning archive.
pub const BUNDLE_MEMBER: &str = "reason/mcp-action-policy.json";
/// Name of the exact bundle-derived compact artifact selected for corpus tests.
pub const CORPUS_ARTIFACT: &str = "mcp-action-policy.json";
/// Maximum serialized native policy size accepted by consumers.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Context in which transaction execution reads the projected action theory.
pub const WORLD: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec";
/// The authored policy namespace used by the source-versus-bundle correspondence check.
pub const POLICY_NAMESPACE: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
/// The one literal-valued predicate retained in the executable policy projection.
pub const TOOL_NAME: &str = "https://blackcatinformatics.ca/logic/mcpToolName";
const ANNOTATIONS: [&str; 2] = [
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2000/01/rdf-schema#comment",
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Object {
    Iri(String),
    Literal {
        lexical: String,
        datatype: Option<String>,
        language: Option<String>,
        direction: Option<bool>,
    },
}

impl Object {
    fn from_native(term: &RdfTerm) -> Option<Self> {
        match term {
            RdfTerm::Iri(iri) => Some(Self::Iri(iri.clone())),
            RdfTerm::Literal(value) => Some(Self::Literal {
                lexical: value.lexical_form.clone(),
                datatype: value.datatype.clone(),
                language: value.language.clone(),
                direction: value
                    .direction
                    .map(|direction| direction == RdfTextDirection::Rtl),
            }),
            RdfTerm::BlankNode(_) | RdfTerm::Triple(_) => None,
        }
    }

    fn native(&self) -> RdfTerm {
        match self {
            Self::Iri(iri) => RdfTerm::Iri(iri.clone()),
            Self::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => RdfTerm::Literal(RdfLiteral {
                lexical_form: lexical.clone(),
                datatype: datatype.clone(),
                language: language.clone(),
                direction: direction.map(|rtl| {
                    if rtl {
                        RdfTextDirection::Rtl
                    } else {
                        RdfTextDirection::Ltr
                    }
                }),
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    subject: String,
    predicate: String,
    object: Object,
}

impl Row {
    fn native(&self) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::Iri(self.subject.clone()),
            &self.predicate,
            self.object.native(),
        )
        .in_graph(RdfTerm::Iri(WORLD.to_owned()))
    }

    fn line(&self) -> String {
        format!(
            "<{}> <{}> {} <{WORLD}> .",
            self.subject,
            self.predicate,
            self.object.native()
        )
    }
}

/// Complete source identity, projected native rows and presentation bytes for one policy.
/// The source statements retain annotations independently of the executable projection.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActionPolicy {
    schema_version: u32,
    source_path: String,
    source_sha256: String,
    source_statements: BTreeSet<String>,
    rows: Vec<Row>,
    omitted_annotations: BTreeSet<String>,
    nquads: String,
    #[serde(skip)]
    dataset: OnceLock<Arc<RdfDataset>>,
}

fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir {
        detail: detail.into(),
    })
}

/// Comparable original policy statements, retaining all object kinds and annotations.
#[must_use]
pub fn policy_statements(dataset: &RdfDataset) -> BTreeSet<String> {
    purrdf::flat_rdf_quads_from_dataset(dataset)
        .into_iter()
        .filter(
            |quad| matches!(&quad.subject, RdfTerm::Iri(iri) if iri.starts_with(POLICY_NAMESPACE)),
        )
        .map(|quad| format!("{} <{}> {}", quad.subject, quad.predicate, quad.object))
        .collect()
}

/// Project native source rows into the execution context and canonical presentation.
/// This is shared by real production and producer-run negative observations; it never
/// parses a document or authorizes a transaction.
///
/// # Errors
/// Rejects source growth outside the declared IRI/tool-name projection and its
/// retained label/comment complement.
pub fn project_nquads(dataset: &RdfDataset) -> gmeow_errors::Result<String> {
    let (rows, _) = projected_rows(dataset)?;
    let mut lines: Vec<String> = rows.iter().map(Row::line).collect();
    lines.sort();
    Ok(lines.join("\n"))
}

fn projected_rows(dataset: &RdfDataset) -> gmeow_errors::Result<(Vec<Row>, BTreeSet<String>)> {
    let mut rows = Vec::new();
    let mut omitted = BTreeSet::new();
    for quad in purrdf::flat_rdf_quads_from_dataset(dataset) {
        let RdfTerm::Iri(subject) = &quad.subject else {
            return Err(fail(
                "MCP action policy contains an unaccounted non-IRI subject",
            ));
        };
        if matches!(&quad.object, RdfTerm::Literal(_)) && quad.predicate != TOOL_NAME {
            if !ANNOTATIONS.contains(&quad.predicate.as_str()) {
                return Err(fail(format!(
                    "MCP action policy has an unaccounted literal predicate {}",
                    quad.predicate
                )));
            }
            omitted.insert(format!(
                "{} <{}> {}",
                quad.subject, quad.predicate, quad.object
            ));
            continue;
        }
        let object = Object::from_native(&quad.object)
            .ok_or_else(|| fail("MCP action policy contains an unaccounted object term kind"))?;
        rows.push(Row {
            subject: subject.clone(),
            predicate: quad.predicate,
            object,
        });
    }
    Ok((rows, omitted))
}

impl PreparedActionPolicy {
    /// Prepare from the producer's already parsed original document.
    ///
    /// # Errors
    /// Rejects a source identity mismatch or malformed/empty native projection.
    pub fn from_dataset(dataset: &RdfDataset, source_sha256: &str) -> gmeow_errors::Result<Self> {
        let (mut rows, omitted_annotations) = projected_rows(dataset)?;
        rows.sort_by_cached_key(Row::line);
        let nquads = rows.iter().map(Row::line).collect::<Vec<_>>().join("\n");
        let prepared = Self {
            schema_version: 1,
            source_path: SOURCE_PATH.to_owned(),
            source_sha256: source_sha256.to_owned(),
            source_statements: policy_statements(dataset),
            rows,
            omitted_annotations,
            nquads,
            dataset: OnceLock::new(),
        };
        prepared.validate()?;
        Ok(prepared)
    }

    /// Exact original source statements, including the annotations the execution view omits.
    #[must_use]
    pub fn source_statements(&self) -> &BTreeSet<String> {
        &self.source_statements
    }

    /// Exact label/comment statements deliberately omitted from the execution view.
    #[must_use]
    pub fn omitted_annotations(&self) -> &BTreeSet<String> {
        &self.omitted_annotations
    }

    /// Producer-rendered N-Quads served verbatim by the MCP tool and resource.
    #[must_use]
    pub fn nquads(&self) -> &str {
        &self.nquads
    }

    /// Authenticate the native preparation's schema, exact source identity and representation.
    /// The enclosing bundle/action receipt authenticates provenance; this is not a signature.
    ///
    /// # Errors
    /// Rejects incompatible, empty, excessive or inconsistent native policy representations.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        if self.schema_version != 1
            || self.source_path != SOURCE_PATH
            || self.source_sha256 != SOURCE_SHA256
            || self.rows.is_empty()
            || self.rows.len() > 16_384
            || self.source_statements.is_empty()
            || self.source_statements.len() > 16_384
            || self.omitted_annotations.len() > 16_384
            || self.nquads.len() > MAX_BYTES
        {
            return Err(fail(
                "prepared MCP action policy has an incompatible source identity or size",
            ));
        }
        if self
            .rows
            .iter()
            .any(|row| matches!(row.object, Object::Literal { .. }) && row.predicate != TOOL_NAME)
        {
            return Err(fail(
                "prepared MCP action policy contains a non-tool-name literal",
            ));
        }
        let lines: Vec<String> = self.rows.iter().map(Row::line).collect();
        if lines.windows(2).any(|pair| pair[0] > pair[1]) || lines.join("\n") != self.nquads {
            return Err(fail(
                "prepared MCP action policy native rows disagree with its served projection",
            ));
        }
        Ok(())
    }

    /// Hydrate the prepared native rows once, preserving their transaction context.
    /// No RDF text is reparsed and no authored source is loaded or compiled.
    ///
    /// # Errors
    /// Rejects incompatible preparation or malformed native terms.
    pub fn dataset(&self) -> gmeow_errors::Result<&RdfDataset> {
        if let Some(dataset) = self.dataset.get() {
            return Ok(dataset);
        }
        self.validate()?;
        let mut builder = RdfDatasetBuilder::new();
        for row in &self.rows {
            builder.push_owned_quad(&row.native());
        }
        let built = builder.freeze().map_err(|error| fail(error.to_string()))?;
        Ok(self.dataset.get_or_init(|| built))
    }
}

#[path = "action_policy.tests.rs"]
#[cfg(test)]
mod tests;
