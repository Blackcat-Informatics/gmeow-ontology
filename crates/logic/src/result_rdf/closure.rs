// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated, fixed-leaf transport of the original execution's complete RDF closure.

use std::collections::BTreeSet;
use std::sync::Arc;

use purrdf::gts::reader::{BlobPayload, StreamingSink};
use purrdf::{BlankScope, DatasetView, RdfDataset, TermValue};

use super::{NativeReceiptAddress, ReasoningResult, result_err};
use crate::physical::LogicalGraph;

pub const CLOSURE_REPRESENTATION: &str = "reasoned-closure";
pub const CLOSURE_MEDIA_TYPE: &str = gmeow_gts_profile::GMEOW_GTS_MEDIA_TYPE;
pub(super) const MAX_CLOSURE_BYTES: usize = gmeow_gts_profile::transport::MAX_OPAQUE_BYTES;
pub(super) const MAX_CLOSURE_ENCODED_BYTES: usize =
    gmeow_gts_profile::transport::MAX_OPAQUE_ENCODED_BYTES;

/// One isolated closure publication. It owns no copy of the rich typed result.
pub(super) struct ClosurePublication {
    pub address: NativeReceiptAddress,
    pub bytes: Vec<u8>,
}

fn transported_graph(graph: &LogicalGraph) -> Option<TermValue> {
    graph.graph().map(|graph| match graph {
        TermValue::Blank { label, scope } => TermValue::Blank {
            label: scope.qualify_label(label).into_owned(),
            scope: BlankScope(1),
        },
        graph => graph.clone(),
    })
}

pub(super) fn graph_catalog(worlds: &[LogicalGraph]) -> BTreeSet<Option<TermValue>> {
    worlds.iter().map(transported_graph).collect()
}

impl ClosurePublication {
    pub fn publish(result: &ReasoningResult) -> gmeow_errors::Result<Self> {
        let original = crate::reason::native_closure_to_dataset(result)?;
        let worlds = result
            .native_execution()?
            .worlds
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let emission = gmeow_gts_profile::view_to_gmeow_gts(original.as_ref())?;
        for omitted in &emission.ingestion.declarations_omitted {
            if !worlds
                .iter()
                .filter_map(LogicalGraph::graph)
                .any(|graph| match graph {
                    TermValue::Iri(iri) => omitted == iri,
                    TermValue::Blank { label, scope } => {
                        omitted == &format!("_:{}", scope.qualify_label(label))
                    }
                    _ => false,
                })
            {
                return Err(result_err(format!(
                    "closure emitter omitted foreign graph {omitted}"
                )));
            }
        }
        let address = NativeReceiptAddress::of_bytes(&emission.bytes);
        let restored = admit_leaf(&address, &emission.bytes, &worlds)?;
        // This native complete-state identity includes empty declarations and
        // one global blank bijection across every RDF role. Equality relies on
        // BLAKE3 collision resistance, as does the attachment's content address;
        // it is not exact canonical-byte or rich-result equality. Unlike the
        // RDFC overlay comparator it accepts every caller-owned source IRI.
        if super::complete_rdf_state(&original, "authenticate complete RDF closure state")?
            != super::complete_rdf_state(&restored, "authenticate complete RDF closure state")?
        {
            return Err(result_err(
                "complete original RDF closure changed during native GTS publication".into(),
            ));
        }
        Ok(Self {
            address,
            bytes: emission.bytes,
        })
    }
}

#[derive(Default)]
struct LeafAudit {
    application_blob: bool,
}

impl StreamingSink for LeafAudit {
    fn blob(&mut self, _segment: u64, _digest: &str, _metadata: Option<&purrdf_lex::cbor::Value>) {
        // Header dictionaries have their own native mechanism and do not emit
        // application-blob callbacks. Even an external reference is forbidden.
        self.application_blob = true;
    }

    fn blob_payload(&mut self, _payload: BlobPayload<'_>) {
        self.application_blob = true;
    }

    fn frame_decode_limit(&self) -> Option<usize> {
        Some(MAX_CLOSURE_BYTES)
    }

    fn blob_decode_limit(&self) -> Option<usize> {
        Some(MAX_CLOSURE_BYTES)
    }

    fn blob_encoded_limit(&self) -> Option<usize> {
        Some(MAX_CLOSURE_ENCODED_BYTES)
    }
}

/// Authenticate a fixed native leaf and restore its complete transported catalog.
/// Source result-looking RDF is deliberately never interpreted as proof authority.
pub(super) fn admit_leaf(
    address: &NativeReceiptAddress,
    bytes: &[u8],
    worlds: &[LogicalGraph],
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    if bytes.len() > MAX_CLOSURE_BYTES || NativeReceiptAddress::of_bytes(bytes) != *address {
        return Err(result_err(
            "closure payload exceeds its limit or differs from its address".into(),
        ));
    }
    gmeow_gts_profile::validate_mandated_frames(bytes)?;
    let mut audit = LeafAudit::default();
    let read = purrdf::gts::reader::read_to_sink(bytes, true, None, &mut audit);
    if audit.application_blob {
        return Err(result_err(
            "native RDF closure leaf contains an application blob or external blob reference"
                .into(),
        ));
    }
    if !read.diagnostics.is_empty()
        || read.torn.is_some()
        || read.segment_heads.len() != 1
        || read.segment_heads[0].is_empty()
        || read.segment_profiles.len() != 1
    {
        return Err(result_err(
            "native RDF closure leaf requires one complete, diagnostic-free segment".into(),
        ));
    }
    let mut limits = purrdf::GtsBlobLimits::new(MAX_CLOSURE_BYTES, 0);
    limits.max_encoded_bytes = MAX_CLOSURE_ENCODED_BYTES;
    limits.max_frame_decoded_bytes = MAX_CLOSURE_BYTES;
    let imported = purrdf::import_gts_events_with_blobs(bytes, &[], limits)?;
    if imported.bundle.envelope.lookaside.segments.len() != 1
        || imported.bundle.envelope.lookaside.segments[0].index != 0
        || imported.bundle.envelope.lookaside.segments[0]
            .head
            .as_ref()
            .is_none_or(String::is_empty)
    {
        return Err(result_err(
            "native RDF closure leaf has an invalid segment index/head".into(),
        ));
    }
    let dataset = imported.bundle.dataset;
    let catalog = graph_catalog(worlds);
    for row in dataset.quads() {
        let graph = row.g.map(|graph| dataset.term_value(graph)).transpose()?;
        if !catalog.contains(&graph) {
            return Err(result_err(
                "native RDF closure carries a row outside the authenticated world catalog".into(),
            ));
        }
    }
    for graph in dataset.named_graphs() {
        if !catalog.contains(&Some(dataset.term_value(graph)?)) {
            return Err(result_err(
                "native RDF closure declares a foreign graph".into(),
            ));
        }
    }
    // Native mutable import retains all base blank identities; pushing this
    // dataset through a builder would standardize it apart a second time.
    let mut complete = purrdf::MutableDataset::new(dataset);
    let expected_graphs = catalog.into_iter().flatten().collect::<BTreeSet<_>>();
    for graph in &expected_graphs {
        complete.declare_named_graph(graph.clone())?;
    }
    let restored = complete.freeze()?;
    let actual_graphs = restored
        .named_graphs()
        .map(|graph| restored.as_ref().term_value(graph))
        .collect::<BTreeSet<_>>();
    if actual_graphs != expected_graphs {
        return Err(result_err("restored RDF closure graph catalog differs from its exact forward-mapped execution worlds".into()));
    }
    Ok(restored)
}
