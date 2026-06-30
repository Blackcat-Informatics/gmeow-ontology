// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS adapter surface for `gmeow-rdf`.
//!
//! The oxigraph-free reader half (`read_graph`, `read_all_segments`,
//! `lookaside_from_graph`, …) lives in the ring-fenced `gmeow-rdf-core` kernel and
//! is re-exported wholesale below (#885 / purrdf P2b). The oxigraph-FREE
//! [`flattened_dataset_from_bytes`] (EPIC #906 Task 4) is the load path the native
//! SPARQL conformance gate replays against the frozen goldens.

pub use gmeow_rdf_core::gts::*;

use crate::RdfDiagnostic;

/// Load a GTS bundle into a frozen [`RdfDataset`](crate::RdfDataset) with **every**
/// named graph folded into the default graph. This is the load path the EPIC #906
/// Task-4 native SPARQL conformance gate (`crates/sparql-conformance`) replays
/// against the frozen goldens, which were captured over the same
/// flatten-to-default-graph view. Implemented entirely on the oxigraph-free `gts`
/// reader path: `read_all_segments` (re-exported from the `gmeow-rdf-core` kernel) →
/// the native statement-layer fold (`flattened_dataset_from_gts_graph`), which
/// re-homes each base quad's graph component to the default graph (`None`) before
/// `freeze()`.
pub fn flattened_dataset_from_bytes(
    bytes: &[u8],
) -> Result<std::sync::Arc<crate::RdfDataset>, RdfDiagnostic> {
    let graph = read_all_segments(bytes)?;
    crate::native_codecs::parse::flattened_dataset_from_gts_graph(&graph)
}

#[cfg(test)]
#[cfg(feature = "gts")]
mod tests {
    use super::*;
    use ciborium::value::Value;
    use gmeow_gts::model::{Graph, Term, TermKind};
    use gmeow_gts::writer::Writer;

    fn private_lang_named_graph() -> Graph {
        let mut graph = Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/s".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some("hallo".to_owned()),
            datatype: None,
            lang: Some("x-gmeow-afrikaans".to_owned()),
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/graph".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.meta.push((
            "producer".to_owned(),
            Value::Text("gmeow-rdf-test".to_owned()),
        ));
        graph.segment_profiles.push("rdf12".to_owned());
        graph.quads.push((0, 1, 2, Some(3)));
        graph
    }

    /// The oxigraph-free [`flattened_dataset_from_bytes`] folds the one named-graph
    /// quad into the DEFAULT graph (graph component `None`). This is the load contract
    /// the EPIC #906 Task-4 native conformance gate relies on, and it accepts a
    /// private (`x-gmeow-…`) language tag.
    #[test]
    fn flattened_dataset_from_bytes_folds_named_graph_into_default() {
        let graph = private_lang_named_graph();
        let writer =
            Writer::deterministic(&graph, "gmeow-rdf-test").expect("deterministic GTS writer");
        let bytes = writer.to_bytes();

        let dataset = flattened_dataset_from_bytes(&bytes).expect("native flattened dataset");
        let quads: Vec<_> = dataset.quads().collect();
        assert_eq!(quads.len(), 1, "the single source quad survives the fold");
        assert!(
            quads[0].g.is_none(),
            "the named graph was re-homed to the default graph (None)"
        );
    }
}
