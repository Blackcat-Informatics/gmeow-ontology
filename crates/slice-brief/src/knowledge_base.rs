// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native candidate theories selected from the packet's actual authored source.

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_errors::ResultExt;
use purrdf::{RdfDataset, TermValue};

use crate::AuthoringPacket;

/// One canonical packet and the native descriptions of its terms and declared
/// definitional dependencies. Graphs and the complete RDF 1.2 statement layer survive.
pub struct CandidatePacket {
    /// The selected packet, assembled once by the existing authoring authority.
    pub packet: AuthoringPacket,
    knowledge_base: Arc<RdfDataset>,
}

#[path = "knowledge_base.tests.rs"]
#[cfg(test)]
mod tests;

impl CandidatePacket {
    pub(crate) fn from_source(
        packet: AuthoringPacket,
        source: &RdfDataset,
    ) -> gmeow_errors::Result<Self> {
        let mut subjects = BTreeSet::new();
        for term in &packet.terms {
            if source
                .term_id_by_value(&TermValue::iri(&term.iri))
                .is_none()
            {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Partition {
                    detail: format!(
                        "packet term <{}> is absent from its source dataset",
                        term.iri
                    ),
                }));
            }
            subjects.insert(term.iri.as_str());
            subjects.extend(term.closure.iter().map(|entry| entry.iri.as_str()));
        }
        let knowledge_base = purrdf::describe::Describer::new(source)
            .describe_iris(subjects)
            .ctx("describe packet terms and definitional dependencies")?;
        Ok(Self {
            packet,
            knowledge_base,
        })
    }

    /// Borrow the exact selected theory without reconstructing it from guidance.
    pub fn knowledge_base(&self) -> &RdfDataset {
        self.knowledge_base.as_ref()
    }

    /// Serialize at the candidate operation boundary, retaining named graphs.
    ///
    /// # Errors
    /// Returns the original native serialization or UTF-8 failure.
    pub fn knowledge_base_nquads(&self) -> gmeow_errors::Result<String> {
        let bytes = purrdf::serialize_dataset(
            &self.knowledge_base,
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .ctx("serialize packet candidate knowledge base")?;
        String::from_utf8(bytes).ctx("packet candidate N-Quads must be UTF-8")
    }
}
