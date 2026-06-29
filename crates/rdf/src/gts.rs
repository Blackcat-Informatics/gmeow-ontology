// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS adapter surface for `gmeow-rdf`.
//!
//! The oxigraph-free reader half (`read_graph`, `read_all_segments`,
//! `lookaside_from_graph`, …) lives in the ring-fenced `gmeow-rdf-core` kernel and
//! is re-exported wholesale below (#885 / purrdf P2b). The two `oxigraph`-gated
//! flattening helpers (`flattened_oxigraph_store_from_*`) depend on oxigraph and so
//! live here behind the feature; the oxigraph-FREE [`flattened_dataset_from_bytes`]
//! (EPIC #906 Task 4) is always available — it is the load path the native SPARQL
//! conformance gate replays against the frozen oxigraph goldens.

pub use gmeow_rdf_core::gts::*;

use crate::RdfDiagnostic;

/// Load a GTS bundle into a frozen [`RdfDataset`](crate::RdfDataset) with **every**
/// named graph folded into the default graph — the OXIGRAPH-FREE twin of
/// [`flattened_oxigraph_store_from_bytes`]. This is the load path the EPIC #906
/// Task-4 native SPARQL conformance gate (`crates/sparql-conformance`) replays
/// against the frozen oxigraph goldens, which were captured over a flattened store
/// (`GraphPolicy::FlattenToDefaultGraph`). Implemented entirely on the oxigraph-free
/// `gts` reader path: `read_all_segments` (re-exported from the `gmeow-rdf-core`
/// kernel) → the native statement-layer fold (`flattened_dataset_from_gts_graph`),
/// which re-homes each base quad's graph component to the default graph (`None`)
/// before `freeze()`. Carries no oxigraph dependency, so the conformance crate can
/// call it without the `oxigraph` feature.
pub fn flattened_dataset_from_bytes(
    bytes: &[u8],
) -> Result<std::sync::Arc<crate::RdfDataset>, RdfDiagnostic> {
    let graph = read_all_segments(bytes)?;
    crate::native_codecs::parse::flattened_dataset_from_gts_graph(&graph)
}

#[cfg(feature = "oxigraph")]
pub fn flattened_oxigraph_store_from_bytes(
    bytes: &[u8],
) -> Result<::oxigraph::store::Store, RdfDiagnostic> {
    let graph = read_all_segments(bytes)?;
    flattened_oxigraph_store_from_graph(&graph)
}

#[cfg(feature = "oxigraph")]
pub fn flattened_oxigraph_store_from_graph(
    graph: &gmeow_gts::model::Graph,
) -> Result<::oxigraph::store::Store, RdfDiagnostic> {
    // Text-free path (#909): fold the gmeow-gts graph straight into the IR (the same
    // statement-layer fold the native parser uses) and materialize it into the
    // oxigraph Store, flattening every named graph into the default graph. The
    // previous implementation round-tripped through N-Quads TEXT (`to_nquads` → an
    // oxigraph text parser); that text codec is retired.
    let dataset = crate::native_codecs::parse::dataset_from_gts_graph(graph)?;
    crate::oxigraph::store_from_dataset(
        &dataset,
        crate::oxigraph::GraphPolicy::FlattenToDefaultGraph,
    )
}

#[cfg(test)]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
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

    #[test]
    fn flattened_oxigraph_adapter_accepts_private_lang_tag() {
        let graph = private_lang_named_graph();
        let writer =
            Writer::deterministic(&graph, "gmeow-rdf-test").expect("deterministic GTS writer");
        let folded = read_all_segments(&writer.to_bytes()).expect("folded graph");
        let store = flattened_oxigraph_store_from_graph(&folded).expect("oxigraph store");
        assert_eq!(store.len().unwrap(), 1);
    }

    /// The oxigraph-free [`flattened_dataset_from_bytes`] folds the one named-graph
    /// quad into the DEFAULT graph (graph component `None`) — the same flatten the
    /// oxigraph store path applies (the store above has 1 default-graph quad). This is
    /// the load contract the EPIC #906 Task-4 native conformance gate relies on.
    #[test]
    fn flattened_dataset_from_bytes_folds_named_graph_into_default() {
        let graph = private_lang_named_graph();
        let writer =
            Writer::deterministic(&graph, "gmeow-rdf-test").expect("deterministic GTS writer");
        let bytes = writer.to_bytes();

        let dataset =
            flattened_dataset_from_bytes(&bytes).expect("oxigraph-free flattened dataset");
        let quads: Vec<_> = dataset.quads().collect();
        assert_eq!(quads.len(), 1, "the single source quad survives the fold");
        assert!(
            quads[0].g.is_none(),
            "the named graph was re-homed to the default graph (None)"
        );

        // Cross-check: same quad count as the oxigraph flattened store.
        let store = flattened_oxigraph_store_from_bytes(&bytes).expect("oxigraph store");
        assert_eq!(
            quads.len(),
            store.len().unwrap(),
            "native flattened dataset and oxigraph flattened store agree on quad count"
        );
    }
}
