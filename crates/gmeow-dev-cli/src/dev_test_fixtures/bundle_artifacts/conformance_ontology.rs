// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned default-ontology views for repository conformance assertions.

use std::sync::{Arc, OnceLock};

use purrdf::{PackBuilder, RdfDataset, RdfDatasetBuilder, SerializeGraph};

use super::{FixtureResult, fail};

pub(super) const AUTHORED_PACK_ARTIFACT: &str = "validate-authored-ontology.purrpack";
pub(super) const PACK_ARTIFACT: &str = "validate-conformance-ontology.purrpack";
pub(super) const TEXT_ARTIFACT: &str = "validate-conformance-ontology.nt";

/// One terminal default-graph selection shared by both artifact misses. The
/// native reader surface has ordinary statement assertions, matching the SHACL
/// and graph-query consumers. Source graph and statement tables stay untouched.
pub(super) struct ConformanceOntology {
    default: Arc<RdfDataset>,
    reader: OnceLock<Arc<RdfDataset>>,
}

impl ConformanceOntology {
    pub(super) fn new(dataset: &RdfDataset) -> FixtureResult<Self> {
        let mut builder = RdfDatasetBuilder::new();
        for (index, quad) in dataset.quads().enumerate() {
            if quad.g.is_none() {
                // Owned terms preserve their qualified blank identities. A native
                // push carries triple terms without a text codec or source parse.
                builder.push_owned_quad(&dataset.to_owned_quad(index, quad))?;
            }
        }
        let reifies = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")?;
        for (reifier, triple, graph) in dataset.reifiers_with_graph() {
            if graph.is_none() {
                let subject = builder.intern_owned_term(&dataset.to_owned_term(reifier))?;
                let object = builder.intern_owned_term(&dataset.to_owned_term(triple))?;
                builder.push_quad(subject, reifies, object, None)?;
            }
        }
        for (reifier, predicate, object, graph) in dataset.annotations_with_graph() {
            if graph.is_none() {
                let subject = builder.intern_owned_term(&dataset.to_owned_term(reifier))?;
                let predicate = builder.intern_owned_term(&dataset.to_owned_term(predicate))?;
                let object = builder.intern_owned_term(&dataset.to_owned_term(object))?;
                builder.push_quad(subject, predicate, object, None)?;
            }
        }
        let default = builder.freeze()?;
        if default.quad_count() == 0 {
            return Err(fail("terminal bundle omitted its default ontology graph"));
        }
        Ok(Self {
            default,
            reader: OnceLock::new(),
        })
    }

    pub(super) fn reader(&self) -> FixtureResult<&Arc<RdfDataset>> {
        if let Some(reader) = self.reader.get() {
            return Ok(reader);
        }
        // Publish only a fully constructed reader view. Failure remains retryable
        // and never installs a partial dataset in the shared producer cache.
        let projected =
            gmeow_logic_compile::projections::reader_view::with_owl_rdfs_projection(&self.default)?;
        Ok(self.reader.get_or_init(|| projected))
    }

    pub(super) fn authored_pack(&self) -> FixtureResult<Vec<u8>> {
        PackBuilder::build_bytes(&self.default).map_err(fail)
    }

    pub(super) fn pack(&self) -> FixtureResult<Vec<u8>> {
        PackBuilder::build_bytes(self.reader()?).map_err(fail)
    }

    pub(super) fn text(&self) -> FixtureResult<Vec<u8>> {
        purrdf::serialize_dataset(
            self.default.as_ref(),
            "application/n-quads",
            SerializeGraph::DefaultGraph,
        )
        .map_err(fail)
    }
}

#[path = "conformance_ontology.tests.rs"]
#[cfg(test)]
mod tests;
