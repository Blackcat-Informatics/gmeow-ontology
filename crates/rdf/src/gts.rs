// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use ciborium::value::Value;
use gmeow_gts::model::{Graph, TermKind};

use crate::ir::gts_resolve::{predicate_from_id, term_from_id, triple_from_ids};
use crate::{
    RdfAnnotation, RdfBlobOrigin, RdfBlobRecord, RdfDiagnostic, RdfLocation, RdfLookaside,
    RdfLookasideKind, RdfLookasideResource, RdfMetadataEntry, RdfMetadataValue,
    RdfOpaqueNodeRecord, RdfQuad, RdfReifier, RdfSegmentRecord, RdfSignatureRecord, RdfStore,
    RdfStoreCapabilities, RdfSuppressionRecord,
};

/// RDF store view over a folded GTS graph.
#[derive(Debug, Clone, Copy)]
pub struct GtsGraphStore<'a> {
    graph: &'a Graph,
}

impl<'a> GtsGraphStore<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Self { graph }
    }

    pub fn graph(&self) -> &'a Graph {
        self.graph
    }
}

impl RdfStore for GtsGraphStore<'_> {
    fn quads(&self) -> Box<dyn Iterator<Item = Result<RdfQuad, RdfDiagnostic>> + '_> {
        Box::new(
            self.graph
                .quads
                .iter()
                .enumerate()
                .map(|(index, &(s, p, o, graph_name))| {
                    quad_from_ids(self.graph, index, s, p, o, graph_name)
                }),
        )
    }

    fn reifiers(&self) -> Box<dyn Iterator<Item = Result<RdfReifier, RdfDiagnostic>> + '_> {
        Box::new(self.graph.reifiers.iter().map(|&(rid, (s, p, o))| {
            let location = RdfLocation::logical("gts:reifier").with_gts_reifier(rid);
            let reifier = term_from_id(self.graph, rid, location.clone())?;
            let statement = triple_from_ids(self.graph, s, p, o, location.clone())?;
            Ok(RdfReifier::new(reifier, statement).with_location(location))
        }))
    }

    fn annotations(&self) -> Box<dyn Iterator<Item = Result<RdfAnnotation, RdfDiagnostic>> + '_> {
        Box::new(self.graph.annotations.iter().map(|&(r, p, v)| {
            let location = RdfLocation::logical("gts:annotation").with_gts_reifier(r);
            let reifier = term_from_id(self.graph, r, location.clone())?;
            let predicate = predicate_from_id(self.graph, p, location.clone())?;
            let object = term_from_id(self.graph, v, location.clone())?;
            Ok(RdfAnnotation::new(reifier, predicate, object).with_location(location))
        }))
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        RdfStoreCapabilities {
            named_graphs: self
                .graph
                .quads
                .iter()
                .any(|(_, _, _, graph)| graph.is_some()),
            quoted_triples: self
                .graph
                .terms
                .iter()
                .any(|term| term.kind == TermKind::Triple),
            reifiers: !self.graph.reifiers.is_empty(),
            annotations: !self.graph.annotations.is_empty(),
            source_locations: true,
            loss_records: !self.graph.opaque.is_empty() || !self.graph.suppressions.is_empty(),
            lookaside: has_lookaside(self.graph),
        }
    }

    fn lookaside(&self) -> RdfLookaside {
        lookaside_from_graph(self.graph)
    }

    fn len_hint(&self) -> Option<usize> {
        Some(self.graph.quads.len())
    }
}

pub fn lookaside_from_graph(graph: &Graph) -> RdfLookaside {
    let metadata = graph
        .meta
        .iter()
        .map(|(key, value)| {
            RdfMetadataEntry::new("gts:file", key.clone(), metadata_value_from_cbor(value))
        })
        .chain(
            graph
                .segment_meta
                .iter()
                .enumerate()
                .flat_map(|(segment_index, entries)| {
                    entries.iter().map(move |(key, value)| {
                        RdfMetadataEntry::new(
                            format!("gts:segment:{segment_index}"),
                            key.clone(),
                            metadata_value_from_cbor(value),
                        )
                    })
                }),
        )
        .collect();

    let segments = segment_records(graph);
    let blobs = blob_records(graph);
    let resources = resource_records(graph);
    let suppressions = graph
        .suppressions
        .iter()
        .map(|suppression| RdfSuppressionRecord {
            reason: suppression.reason.clone(),
            by: suppression.by.map(|term_id| term_display(graph, term_id)),
            targets: suppression
                .targets
                .iter()
                .map(metadata_value_from_cbor)
                .collect(),
        })
        .collect();
    let opaque_nodes = graph
        .opaque
        .iter()
        .map(|opaque| RdfOpaqueNodeRecord {
            id: hex_bytes(&opaque.id),
            frame_type: opaque.frame_type.clone(),
            reason: opaque.reason.clone(),
            signature_status: opaque.sigstat.clone(),
            public_metadata: opaque.pub_meta.as_ref().map(metadata_value_from_cbor),
        })
        .collect();
    let signatures = graph
        .signatures
        .iter()
        .map(|signature| RdfSignatureRecord {
            frame_id: hex_bytes(&signature.frame_id),
            key_id: signature.kid.clone(),
            status: signature.status.clone(),
            has_cose: signature.cose.is_some(),
        })
        .collect();

    RdfLookaside {
        resources,
        metadata,
        segments,
        blobs,
        suppressions,
        opaque_nodes,
        signatures,
    }
}

fn has_lookaside(graph: &Graph) -> bool {
    !graph.meta.is_empty()
        || !graph.segment_heads.is_empty()
        || !graph.segment_profiles.is_empty()
        || !graph.segment_meta.is_empty()
        || !graph.blobs.is_empty()
        || !graph.blob_meta.is_empty()
        || !graph.suppressions.is_empty()
        || !graph.opaque.is_empty()
        || !graph.signatures.is_empty()
}

fn segment_records(graph: &Graph) -> Vec<RdfSegmentRecord> {
    let max_segments = graph
        .segment_heads
        .len()
        .max(graph.segment_profiles.len())
        .max(graph.segment_streamable.len());
    (0..max_segments)
        .map(|index| {
            let streamable = graph.segment_streamable.get(index);
            RdfSegmentRecord {
                index,
                head: graph.segment_heads.get(index).map(|head| hex_bytes(head)),
                profile: graph.segment_profiles.get(index).cloned(),
                claimed_streamable: streamable.is_some_and(|info| info.claimed),
                covered: streamable.map_or(0, |info| info.covered),
                tail: streamable.map_or(0, |info| info.tail),
            }
        })
        .collect()
}

fn blob_records(graph: &Graph) -> Vec<RdfBlobRecord> {
    let blob_meta = blob_metadata_index(graph);
    // Origin file identity (segment heads) shared by every blob read from this
    // folded graph. Computed once; the fold does not retain per-blob frame
    // provenance, so the reference is file-level.
    let origin = blob_origin(graph);
    graph
        .blobs
        .iter()
        .map(|(digest, entry)| {
            let metadata = blob_metadata(&blob_meta, digest);
            RdfBlobRecord {
                digest: digest.clone(),
                media_type: metadata_text(&metadata, "mt"),
                representation: metadata_text(&metadata, "rep"),
                // `cached_bytes` measures only an already-decoded entry; it never
                // forces a lazy decode. A transformed (Lazy) blob — potentially
                // multi-terabyte — therefore reports `None` rather than decoding
                // the whole payload just to learn its length.
                decoded_len: entry.cached_bytes().map(<[u8]>::len),
                metadata,
                origin: origin.clone(),
            }
        })
        .collect()
}

/// The content-addressed origin reference for blobs in this folded graph: the
/// file-level segment-head ids (hex). `None` when the graph declares no segment
/// heads (e.g. a hand-built graph).
fn blob_origin(graph: &Graph) -> Option<RdfBlobOrigin> {
    if graph.segment_heads.is_empty() {
        return None;
    }
    Some(RdfBlobOrigin {
        source_segments: graph
            .segment_heads
            .iter()
            .map(|head| hex_bytes(head))
            .collect(),
    })
}

fn resource_records(graph: &Graph) -> Vec<RdfLookasideResource> {
    let blob_meta = blob_metadata_index(graph);
    graph
        .blobs
        .iter()
        .map(|(digest, _)| {
            let metadata = blob_metadata(&blob_meta, digest);
            let kind = lookaside_kind_from_metadata(&metadata);
            let mut resource = RdfLookasideResource::new(kind).with_digest(digest.clone());
            resource.media_type = metadata_text(&metadata, "mt");
            resource.path = metadata_text(&metadata, "path");
            resource.iri = metadata_text(&metadata, "iri");
            resource.name = metadata_text(&metadata, "name")
                .or_else(|| metadata_text(&metadata, "label"))
                .or_else(|| metadata_text(&metadata, "role"));
            resource.graph_name = metadata_text(&metadata, "graph");
            resource.metadata = metadata;
            resource
        })
        .collect()
}

fn blob_metadata_index(graph: &Graph) -> BTreeMap<&str, &Value> {
    graph
        .blob_meta
        .iter()
        .map(|(digest, value)| (digest.as_str(), value))
        .collect()
}

fn blob_metadata(
    blob_meta: &BTreeMap<&str, &Value>,
    digest: &str,
) -> BTreeMap<String, RdfMetadataValue> {
    blob_meta
        .get(digest)
        .map(|value| match metadata_value_from_cbor(value) {
            RdfMetadataValue::Map(map) => map,
            value => {
                let mut map = BTreeMap::new();
                map.insert("value".to_owned(), value);
                map
            }
        })
        .unwrap_or_default()
}

fn lookaside_kind_from_metadata(metadata: &BTreeMap<String, RdfMetadataValue>) -> RdfLookasideKind {
    for key in ["kind", "role", "domain", "type"] {
        if let Some(value) = metadata_text(metadata, key) {
            return RdfLookasideKind::from_hint(&value);
        }
    }
    if let Some(media_type) = metadata_text(metadata, "mt") {
        let lower = media_type.to_ascii_lowercase();
        if lower.contains("shacl") {
            return RdfLookasideKind::Shacl;
        }
        if lower.contains("shex") {
            return RdfLookasideKind::Shex;
        }
        if lower.contains("sparql") {
            return RdfLookasideKind::Query;
        }
        if lower.contains("json") && lower.contains("schema") {
            return RdfLookasideKind::Schema;
        }
        if lower.contains("markdown") || lower.contains("html") {
            return RdfLookasideKind::Docs;
        }
    }
    RdfLookasideKind::Blob
}

fn metadata_text(metadata: &BTreeMap<String, RdfMetadataValue>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(RdfMetadataValue::as_text)
        .map(str::to_owned)
}

fn metadata_value_from_cbor(value: &Value) -> RdfMetadataValue {
    match value {
        Value::Integer(integer) => RdfMetadataValue::Integer(i128::from(*integer)),
        Value::Bytes(bytes) => RdfMetadataValue::Bytes(bytes.clone()),
        Value::Float(value) => RdfMetadataValue::Float(*value),
        Value::Text(value) => RdfMetadataValue::Text(value.clone()),
        Value::Bool(value) => RdfMetadataValue::Bool(*value),
        Value::Null => RdfMetadataValue::Null,
        Value::Tag(tag, value) => RdfMetadataValue::Tagged {
            tag: *tag,
            value: Box::new(metadata_value_from_cbor(value)),
        },
        Value::Array(values) => {
            RdfMetadataValue::Array(values.iter().map(metadata_value_from_cbor).collect())
        }
        Value::Map(entries) => RdfMetadataValue::Map(
            entries
                .iter()
                .map(|(key, value)| (metadata_key_from_cbor(key), metadata_value_from_cbor(value)))
                .collect(),
        ),
        other => RdfMetadataValue::Opaque(format!("{other:?}")),
    }
}

fn metadata_key_from_cbor(value: &Value) -> String {
    match value {
        Value::Text(value) => value.clone(),
        other => format!("{other:?}"),
    }
}

fn term_display(graph: &Graph, term_id: usize) -> String {
    graph
        .terms
        .get(term_id)
        .and_then(|term| term.value.clone())
        .unwrap_or_else(|| format!("term#{term_id}"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Fold GTS bytes into a graph and fail if the reader produced diagnostics.
pub fn read_graph(bytes: &[u8], allow_segments: bool) -> Result<Graph, RdfDiagnostic> {
    let graph = gmeow_gts::reader::read(bytes, allow_segments, None);
    if graph.diagnostics.is_empty() {
        Ok(graph)
    } else {
        Err(diagnostics_to_error(&graph))
    }
}

/// Fold all GTS segments into a graph and fail on any reader diagnostic.
pub fn read_all_segments(bytes: &[u8]) -> Result<Graph, RdfDiagnostic> {
    read_graph(bytes, true)
}

fn diagnostics_to_error(graph: &Graph) -> RdfDiagnostic {
    let joined = graph
        .diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.code, d.detail))
        .collect::<Vec<_>>()
        .join("; ");
    let mut diagnostic = RdfDiagnostic::error(
        "gts-fold-diagnostic",
        format!(
            "GTS fold reported {} diagnostic(s)",
            graph.diagnostics.len()
        ),
    )
    .with_detail(joined);
    if let Some(frame_index) = graph.diagnostics.iter().find_map(|d| d.frame_index) {
        diagnostic = diagnostic
            .with_location(RdfLocation::logical("gts:reader").with_gts_frame(frame_index));
    }
    diagnostic
}

fn quad_from_ids(
    graph: &Graph,
    index: usize,
    s: usize,
    p: usize,
    o: usize,
    graph_name: Option<usize>,
) -> Result<RdfQuad, RdfDiagnostic> {
    let location = RdfLocation::logical("gts:quad").with_gts_quad(index);
    let subject = term_from_id(graph, s, location.clone())?;
    let predicate = predicate_from_id(graph, p, location.clone())?;
    let object = term_from_id(graph, o, location.clone())?;
    let mut quad = RdfQuad::new(subject, predicate, object).with_location(location.clone());
    if let Some(graph_name) = graph_name {
        quad = quad.in_graph(term_from_id(graph, graph_name, location)?);
    }
    Ok(quad)
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
    graph: &Graph,
) -> Result<::oxigraph::store::Store, RdfDiagnostic> {
    use ::oxigraph::io::{RdfFormat, RdfParser};
    use ::oxigraph::model::{GraphNameRef, Quad};
    use ::oxigraph::store::Store;

    let nquads = gmeow_gts::nquads::to_nquads(graph);
    let store =
        Store::new().map_err(|e| RdfDiagnostic::error("oxigraph-store-create", e.to_string()))?;
    for quad in RdfParser::from_format(RdfFormat::NQuads)
        .lenient()
        .for_reader(nquads.as_bytes())
    {
        let quad = quad.map_err(|e| {
            RdfDiagnostic::error("gts-nquads-parse", "GTS N-Quads projection failed")
                .with_detail(e.to_string())
        })?;
        let flattened_quad = Quad::new(
            quad.subject,
            quad.predicate,
            quad.object,
            GraphNameRef::DefaultGraph,
        );
        store
            .insert(&flattened_quad)
            .map_err(|e| RdfDiagnostic::error("oxigraph-store-insert", e.to_string()))?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RdfTerm;
    use gmeow_gts::model::{Term, TermKind};
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
    fn blob_is_preserved_as_content_addressed_reference() {
        // A blob read from a GTS graph is preserved as a content-addressed
        // reference: the blob_id digest + an origin file id — never the payload
        // bytes (which may be multi-terabyte). This is the by-reference model
        // behind the `blob-bytes-absent` intentional loss.
        let mut graph = Graph::default();
        graph.segment_heads.push(vec![0xab, 0xcd]);
        graph.set_blob("blake3:deadbeef".to_owned(), b"payload".to_vec());

        let store = GtsGraphStore::new(&graph);
        let lookaside = store.lookaside();
        assert_eq!(lookaside.blobs.len(), 1);
        let blob = &lookaside.blobs[0];
        // blob_id reference.
        assert_eq!(blob.digest, "blake3:deadbeef");
        // origin file reference (segment-head hex).
        let origin = blob.origin.as_ref().expect("origin reference present");
        assert_eq!(origin.source_segments, vec!["abcd".to_owned()]);
    }

    #[test]
    fn gts_store_preserves_named_graph_and_private_language_tag() {
        let graph = private_lang_named_graph();
        let store = GtsGraphStore::new(&graph);
        let quads = store
            .quads()
            .collect::<Result<Vec<_>, _>>()
            .expect("GTS graph should adapt cleanly");
        assert_eq!(quads.len(), 1);
        assert!(quads[0].graph_name.is_some());
        let lookaside = store.lookaside();
        assert_eq!(lookaside.metadata.len(), 1);
        assert_eq!(lookaside.segments.len(), 1);
        match &quads[0].object {
            RdfTerm::Literal(literal) => {
                assert_eq!(literal.language.as_deref(), Some("x-gmeow-afrikaans"));
            }
            other => panic!("expected literal object, got {other:?}"),
        }
    }

    #[test]
    fn read_graph_rejects_malformed_bytes() {
        let result = read_all_segments(b"not a valid gts file");
        assert!(result.is_err(), "bad GTS bytes must fail");
        assert_eq!(result.unwrap_err().code, "gts-fold-diagnostic");
    }

    #[test]
    fn cyclic_triple_terms_hit_nesting_limit() {
        let mut graph = Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Triple,
            value: None,
            datatype: None,
            lang: None,
            direction: None,
            reifier: Some(0),
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
            kind: TermKind::Iri,
            value: Some("https://example.org/o".to_owned()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.reifiers.push((0, (0, 1, 2)));

        let err = term_from_id(&graph, 0, RdfLocation::logical("test"))
            .expect_err("cyclic triple term should hit nesting limit");
        assert_eq!(err.code, "gts-term-nesting-limit");
    }

    #[test]
    fn iri_terms_require_non_empty_values() {
        for value in [None, Some("")] {
            let mut graph = Graph::default();
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: value.map(str::to_owned),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });

            let err = term_from_id(&graph, 0, RdfLocation::logical("test"))
                .expect_err("invalid GTS IRI term should fail");
            assert_eq!(err.code, "gts-iri-missing-value");
        }
    }

    #[test]
    #[cfg(feature = "oxigraph")]
    fn flattened_oxigraph_adapter_accepts_private_lang_tag() {
        let graph = private_lang_named_graph();
        let writer =
            Writer::deterministic(&graph, "gmeow-rdf-test").expect("deterministic GTS writer");
        let folded = read_all_segments(&writer.to_bytes()).expect("folded graph");
        let store = flattened_oxigraph_store_from_graph(&folded).expect("oxigraph store");
        assert_eq!(store.len().unwrap(), 1);
    }
}
