// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS adapter surface for `gmeow-rdf`.
//!
//! The oxigraph-free reader half (`read_graph`, `read_all_segments`,
//! `lookaside_from_graph`, …) lives in the ring-fenced `gmeow-rdf-core` kernel and
//! is re-exported wholesale below (#885 / purrdf P2b). This module keeps only the
//! two `oxigraph`-gated flattening helpers, which depend on oxigraph and therefore
//! cannot live in the oxigraph-free core.

pub use gmeow_rdf_core::gts::*;

#[cfg(feature = "oxigraph")]
use crate::RdfDiagnostic;

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
}
