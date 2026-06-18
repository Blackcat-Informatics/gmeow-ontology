// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{RdfAnnotation, RdfDiagnostic, RdfLookaside, RdfQuad, RdfReifier};

/// Capability flags exposed by an RDF store adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdfStoreCapabilities {
    pub named_graphs: bool,
    pub quoted_triples: bool,
    pub reifiers: bool,
    pub annotations: bool,
    pub source_locations: bool,
    pub loss_records: bool,
    pub lookaside: bool,
}

impl RdfStoreCapabilities {
    pub const fn plain_rdf() -> Self {
        Self {
            named_graphs: false,
            quoted_triples: false,
            reifiers: false,
            annotations: false,
            source_locations: false,
            loss_records: false,
            lookaside: false,
        }
    }
}

impl Default for RdfStoreCapabilities {
    fn default() -> Self {
        Self::plain_rdf()
    }
}

/// Shared RDF store abstraction used by SHACL, validate, LOGIC, and adapters.
pub trait RdfStore {
    fn quads(&self) -> Box<dyn Iterator<Item = Result<RdfQuad, RdfDiagnostic>> + '_>;

    fn reifiers(&self) -> Box<dyn Iterator<Item = Result<RdfReifier, RdfDiagnostic>> + '_> {
        Box::new(std::iter::empty())
    }

    fn annotations(&self) -> Box<dyn Iterator<Item = Result<RdfAnnotation, RdfDiagnostic>> + '_> {
        Box::new(std::iter::empty())
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        RdfStoreCapabilities::default()
    }

    fn lookaside(&self) -> RdfLookaside {
        RdfLookaside::default()
    }

    fn len_hint(&self) -> Option<usize> {
        None
    }
}

/// Simple owned in-memory RDF store for tests and small generated projections.
#[derive(Debug, Clone, Default)]
pub struct VecRdfStore {
    pub quads: Vec<RdfQuad>,
    pub reifiers: Vec<RdfReifier>,
    pub annotations: Vec<RdfAnnotation>,
    pub lookaside: RdfLookaside,
    pub capabilities: RdfStoreCapabilities,
}

impl VecRdfStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_quads(quads: Vec<RdfQuad>) -> Self {
        Self {
            quads,
            ..Self::default()
        }
    }
}

impl RdfStore for VecRdfStore {
    fn quads(&self) -> Box<dyn Iterator<Item = Result<RdfQuad, RdfDiagnostic>> + '_> {
        Box::new(self.quads.iter().cloned().map(Ok))
    }

    fn reifiers(&self) -> Box<dyn Iterator<Item = Result<RdfReifier, RdfDiagnostic>> + '_> {
        Box::new(self.reifiers.iter().cloned().map(Ok))
    }

    fn annotations(&self) -> Box<dyn Iterator<Item = Result<RdfAnnotation, RdfDiagnostic>> + '_> {
        Box::new(self.annotations.iter().cloned().map(Ok))
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        self.capabilities
    }

    fn lookaside(&self) -> RdfLookaside {
        self.lookaside.clone()
    }

    fn len_hint(&self) -> Option<usize> {
        Some(self.quads.len())
    }
}
