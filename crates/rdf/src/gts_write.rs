// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Write any [`RdfStore`] into a deterministic GTS byte stream.
//!
//! This is the inverse direction of [`crate::gts::GtsGraphStore`]: instead of
//! viewing a folded GTS graph as an RDF store, we materialise an RDF store into
//! a [`gmeow_gts::model::Graph`] and ask [`gmeow_gts::writer::Writer`] to
//! canonicalise it. All interning, term remapping, and frame authoring is
//! delegated to `gmeow-gts`.

use std::collections::HashMap;

use ciborium::value::Value;
use gmeow_gts::codec::CodecError;
use gmeow_gts::model::{Graph, Suppression, Term, TermKind, Triple3};
use gmeow_gts::writer::Writer;

use crate::{
    RdfAnnotation, RdfDiagnostic, RdfLiteral, RdfLookaside, RdfMetadataValue, RdfQuad, RdfReifier,
    RdfStore, RdfTerm,
};

const MAX_TERM_NESTING_DEPTH: usize = 16;

/// Convert any [`RdfStore`] into a canonical GTS [`Writer`].
///
/// `profile` is passed through to the GTS header (e.g. `"gmeow-rdf"`). The
/// resulting writer can be further configured (signing, indexes) or emitted
/// directly with [`Writer::to_bytes`].
pub fn to_writer(store: &impl RdfStore, profile: &str) -> Result<Writer, RdfDiagnostic> {
    let mut graph = Graph::default();

    // First pass: collect the logical rows so we can intern terms in a stable
    // order and resolve reifier bindings before quad terms reference them.
    let quads: Vec<RdfQuad> = collect(store.quads(), "quad")?;
    let reifiers: Vec<RdfReifier> = collect(store.reifiers(), "reifier")?;
    let annotations: Vec<RdfAnnotation> = collect(store.annotations(), "annotation")?;

    let mut state = InternState::new();

    // Explicit reifiers take precedence over auto-generated blank-node
    // reifiers for the same triple content.
    for reifier in &reifiers {
        bind_explicit_reifier(&mut state, reifier)?;
    }

    for quad in &quads {
        let s = intern_term(&mut state, &quad.subject)?;
        let p = intern_iri(&mut state, &quad.predicate)?;
        let o = intern_term(&mut state, &quad.object)?;
        let g = quad
            .graph_name
            .as_ref()
            .map(|g| intern_graph_name(&mut state, g))
            .transpose()?;
        graph.quads.push((s, p, o, g));
    }

    for reifier in &reifiers {
        let rid = intern_term(&mut state, &reifier.reifier)?;
        let s = intern_term(&mut state, &reifier.statement.subject)?;
        let p = intern_iri(&mut state, &reifier.statement.predicate)?;
        let o = intern_term(&mut state, &reifier.statement.object)?;
        graph.reifiers.push((rid, (s, p, o)));
    }

    for annotation in &annotations {
        let r = intern_term(&mut state, &annotation.reifier)?;
        let p = intern_iri(&mut state, &annotation.predicate)?;
        let v = intern_term(&mut state, &annotation.object)?;
        graph.annotations.push((r, p, v));
    }

    apply_lookaside(&state, &mut graph, store.lookaside());
    graph.terms = state.terms;

    Writer::deterministic(&graph, profile).map_err(codec_error_to_diagnostic)
}

/// Convert any [`RdfStore`] directly into canonical GTS bytes.
pub fn to_gts(store: &impl RdfStore, profile: &str) -> Result<Vec<u8>, RdfDiagnostic> {
    to_writer(store, profile).map(|writer| writer.to_bytes())
}

struct InternState {
    terms: Vec<Term>,
    index: HashMap<RdfTerm, usize>,
    /// Triple component ids → reifier term ids. RDF 1.2 permits several distinct
    /// explicit reifiers for one (s,p,o) (gmeow-gts#213); they are all retained.
    /// A nested triple term reuses the first-bound reifier for its single
    /// `Term.reifier` slot. Reifiers are IRI/blank-node terms already in `terms`.
    reifier_map: HashMap<Triple3, Vec<usize>>,
}

impl InternState {
    fn new() -> Self {
        Self {
            terms: Vec::new(),
            index: HashMap::new(),
            reifier_map: HashMap::new(),
        }
    }
}

fn collect<T>(
    iter: Box<dyn Iterator<Item = Result<T, RdfDiagnostic>> + '_>,
    kind: &str,
) -> Result<Vec<T>, RdfDiagnostic> {
    iter.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.with_detail(format!("failed to read {kind} from RDF store")))
}

fn bind_explicit_reifier(
    state: &mut InternState,
    reifier: &RdfReifier,
) -> Result<(), RdfDiagnostic> {
    let rid = intern_term(state, &reifier.reifier)?;
    if !is_iri_or_bnode(&state.terms[rid]) {
        return Err(RdfDiagnostic::error(
            "rdf-reifier-not-node",
            "RDF 1.2 reifier must be an IRI or blank node",
        ));
    }
    let s = intern_term(state, &reifier.statement.subject)?;
    let p = intern_iri(state, &reifier.statement.predicate)?;
    let o = intern_term(state, &reifier.statement.object)?;

    // RDF 1.2 allows several distinct explicit reifiers for the same triple
    // content (gmeow-gts#213); record each one, deduplicating only an identical
    // (rid, (s,p,o)) pair. Every distinct reifier is emitted as its own
    // `graph.reifiers` row below, so no binding is collapsed.
    let bound = state.reifier_map.entry((s, p, o)).or_default();
    if !bound.contains(&rid) {
        bound.push(rid);
    }

    Ok(())
}

fn intern_iri(state: &mut InternState, iri: &str) -> Result<usize, RdfDiagnostic> {
    intern_term(state, &RdfTerm::Iri(iri.to_owned()))
}

fn intern_graph_name(state: &mut InternState, term: &RdfTerm) -> Result<usize, RdfDiagnostic> {
    if !is_iri_or_bnode_term(term) {
        return Err(RdfDiagnostic::error(
            "rdf-graph-name-not-node",
            format!(
                "named graph name must be an IRI or blank node, got {:?}",
                term.kind()
            ),
        ));
    }
    intern_term(state, term)
}

fn intern_term(state: &mut InternState, term: &RdfTerm) -> Result<usize, RdfDiagnostic> {
    intern_term_depth(state, term, 0)
}

fn intern_term_depth(
    state: &mut InternState,
    term: &RdfTerm,
    depth: usize,
) -> Result<usize, RdfDiagnostic> {
    if depth > MAX_TERM_NESTING_DEPTH {
        return Err(RdfDiagnostic::error(
            "rdf-term-nesting-limit",
            "RDF term nesting depth limit exceeded while building GTS graph",
        ));
    }
    if let Some(id) = state.index.get(term) {
        return Ok(*id);
    }

    match term {
        RdfTerm::Iri(iri) => {
            let id = push_term(
                state,
                term,
                Term {
                    kind: TermKind::Iri,
                    value: Some(iri.clone()),
                    datatype: None,
                    lang: None,
                    direction: None,
                    reifier: None,
                },
            );
            Ok(id)
        }
        RdfTerm::BlankNode(label) => {
            let id = push_term(
                state,
                term,
                Term {
                    kind: TermKind::Bnode,
                    value: Some(label.clone()),
                    datatype: None,
                    lang: None,
                    direction: None,
                    reifier: None,
                },
            );
            Ok(id)
        }
        RdfTerm::Literal(literal) => intern_literal(state, literal, depth),
        RdfTerm::Triple(triple) => intern_triple_term(state, triple, depth),
    }
}

fn intern_literal(
    state: &mut InternState,
    literal: &RdfLiteral,
    depth: usize,
) -> Result<usize, RdfDiagnostic> {
    let datatype = if let Some(dt) = &literal.datatype {
        Some(intern_term_depth(
            state,
            &RdfTerm::Iri(dt.clone()),
            depth + 1,
        )?)
    } else {
        None
    };

    // RDF 1.2 literal base direction now round-trips through GTS (gmeow-gts#212):
    // map the IR's RdfTextDirection onto the GTS Term.direction string. Lexical
    // form, datatype, language tag, and direction are all preserved.
    let lang = literal.language.clone();
    let direction = literal.direction.map(|d| d.as_str().to_string());

    let id = push_term(
        state,
        &RdfTerm::Literal(literal.clone()),
        Term {
            kind: TermKind::Literal,
            value: Some(literal.lexical_form.clone()),
            datatype,
            lang,
            direction,
            reifier: None,
        },
    );
    Ok(id)
}

fn intern_triple_term(
    state: &mut InternState,
    triple: &crate::RdfTriple,
    depth: usize,
) -> Result<usize, RdfDiagnostic> {
    let s = intern_term_depth(state, &triple.subject, depth + 1)?;
    let p = intern_iri(state, &triple.predicate)?;
    let o = intern_term_depth(state, &triple.object, depth + 1)?;

    let reifier_id = if let Some(rid) = state
        .reifier_map
        .get(&(s, p, o))
        .and_then(|rids| rids.first())
        .copied()
    {
        rid
    } else {
        let rid = create_anonymous_reifier(state);
        state.reifier_map.entry((s, p, o)).or_default().push(rid);
        rid
    };

    let id = push_term(
        state,
        &RdfTerm::Triple(Box::new(triple.clone())),
        Term {
            kind: TermKind::Triple,
            value: None,
            datatype: None,
            lang: None,
            direction: None,
            reifier: Some(reifier_id),
        },
    );
    Ok(id)
}

fn create_anonymous_reifier(state: &mut InternState) -> usize {
    let label = format!("gmeow_rdf_auto_{}", state.terms.len());
    let id = state.terms.len();
    state.terms.push(Term {
        kind: TermKind::Bnode,
        value: Some(label.clone()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    });
    state.index.insert(RdfTerm::BlankNode(label), id);
    id
}

fn push_term(state: &mut InternState, key: &RdfTerm, term: Term) -> usize {
    let id = state.terms.len();
    state.terms.push(term);
    state.index.insert(key.clone(), id);
    id
}

fn is_iri_or_bnode(term: &Term) -> bool {
    matches!(term.kind, TermKind::Iri | TermKind::Bnode)
}

fn is_iri_or_bnode_term(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
}

fn apply_lookaside(state: &InternState, graph: &mut Graph, lookaside: RdfLookaside) {
    for entry in lookaside.metadata {
        let key = entry.key;
        let value = metadata_value_to_cbor(&entry.value);
        graph.set_meta(key, value);
    }

    for suppression in lookaside.suppressions {
        let by = suppression
            .by
            .as_deref()
            .and_then(|label| term_id_by_display(state, label));
        graph.suppressions.push(Suppression {
            targets: suppression
                .targets
                .iter()
                .map(metadata_value_to_cbor)
                .collect(),
            reason: suppression.reason,
            by,
        });
    }

    // Blobs travel by content-addressed reference, not by value. The RDF IR
    // never holds payload bytes (a blob may be a multi-terabyte data dump), so
    // the destination is not re-inlined here: the `RdfBlobRecord` carries the
    // blob_id digest + origin, and a streaming materializer copies the bytes
    // origin→destination on demand (deferred — see the `blob-bytes-absent`
    // intentional loss in `crate::loss`).
}

fn term_id_by_display(state: &InternState, label: &str) -> Option<usize> {
    state
        .terms
        .iter()
        .position(|term| term.value.as_deref() == Some(label) && is_iri_or_bnode(term))
}

fn metadata_value_to_cbor(value: &RdfMetadataValue) -> Value {
    match value {
        RdfMetadataValue::Null => Value::Null,
        RdfMetadataValue::Bool(b) => Value::Bool(*b),
        RdfMetadataValue::Integer(i) => match ciborium::value::Integer::try_from(*i) {
            Ok(integer) => Value::Integer(integer),
            Err(_) => Value::Integer(ciborium::value::Integer::from(if *i < 0 {
                i64::MIN
            } else {
                i64::MAX
            })),
        },
        RdfMetadataValue::Float(f) => Value::Float(*f),
        RdfMetadataValue::Text(t) => Value::Text(t.clone()),
        RdfMetadataValue::Bytes(b) => Value::Bytes(b.clone()),
        RdfMetadataValue::Array(a) => Value::Array(a.iter().map(metadata_value_to_cbor).collect()),
        RdfMetadataValue::Map(m) => Value::Map(
            m.iter()
                .map(|(k, v)| (Value::Text(k.clone()), metadata_value_to_cbor(v)))
                .collect(),
        ),
        RdfMetadataValue::Tagged { tag, value } => {
            Value::Tag(*tag, Box::new(metadata_value_to_cbor(value)))
        }
        RdfMetadataValue::Opaque(s) => Value::Text(s.clone()),
    }
}

fn codec_error_to_diagnostic(err: CodecError) -> RdfDiagnostic {
    RdfDiagnostic::error("gts-writer-codec", err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RdfAnnotation, RdfLiteral, RdfMetadataEntry, RdfMetadataValue, RdfQuad, RdfReifier,
        RdfSuppressionRecord, RdfTerm, RdfTextDirection, RdfTriple, VecRdfStore,
    };

    fn roundtrip_store(store: &VecRdfStore, profile: &str) -> Graph {
        let bytes = to_gts(store, profile).expect("to_gts should succeed");
        let graph = gmeow_gts::reader::read(&bytes, false, None);
        assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
        graph
    }

    fn assert_nquads_eq(store: &VecRdfStore, profile: &str, expected: &str) {
        let graph = roundtrip_store(store, profile);
        let nquads = gmeow_gts::nquads::to_nquads(&graph);
        assert_eq!(nquads.trim(), expected.trim());
    }

    #[test]
    fn simple_quad_roundtrips_through_gts() {
        let store = VecRdfStore::with_quads(vec![RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        )]);
        assert_nquads_eq(
            &store,
            "gmeow-rdf-test",
            "<https://example.org/s> <https://example.org/p> <https://example.org/o> .",
        );
    }

    #[test]
    fn direction_roundtrips_through_gts() {
        // RDF 1.2 directional language-tagged literal (gmeow-gts#212): the base
        // direction must survive RDF IR -> GTS -> read. This proves the retired
        // `direction-dropped` loss is genuinely gone, not merely undocumented.
        let mut lit = RdfLiteral::language_tagged("\u{645}\u{631}\u{62d}\u{628}\u{627}", "ar");
        lit.direction = Some(RdfTextDirection::Rtl);
        let store = VecRdfStore::with_quads(vec![RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(lit),
        )]);
        let graph = roundtrip_store(&store, "gmeow-rdf-test");
        let lit_term = graph
            .terms
            .iter()
            .find(|t| t.kind == TermKind::Literal)
            .expect("literal term present after read");
        assert_eq!(lit_term.direction.as_deref(), Some("rtl"));
        assert_eq!(lit_term.lang.as_deref(), Some("ar"));
    }

    #[test]
    fn named_graph_roundtrips() {
        let quad = RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::language_tagged("hello", "en")),
        )
        .in_graph(RdfTerm::iri("https://example.org/g"));
        let store = VecRdfStore::with_quads(vec![quad]);
        assert_nquads_eq(
            &store,
            "gmeow-rdf-test",
            "<https://example.org/s> <https://example.org/p> \"hello\"@en <https://example.org/g> .",
        );
    }

    #[test]
    fn reifiers_and_annotations_roundtrip() {
        let statement = RdfTriple::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        );
        let reifier = RdfTerm::blank_node("r1");
        let store = VecRdfStore {
            quads: vec![RdfQuad::new(
                reifier.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
                RdfTerm::triple(statement.clone()),
            )],
            reifiers: vec![RdfReifier::new(reifier.clone(), statement)],
            annotations: vec![RdfAnnotation::new(
                reifier.clone(),
                "https://example.org/confidence",
                RdfTerm::literal(RdfLiteral::typed(
                    "0.9",
                    "http://www.w3.org/2001/XMLSchema#decimal",
                )),
            )],
            ..VecRdfStore::default()
        };

        let graph = roundtrip_store(&store, "gmeow-rdf-test");
        let nquads = gmeow_gts::nquads::to_nquads(&graph);
        assert!(nquads.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"));
        assert!(nquads.contains("https://example.org/confidence"));
        assert!(nquads.contains("0.9"));
    }

    #[test]
    fn two_reifiers_same_triple_both_survive() {
        // RDF 1.2 permits several distinct explicit reifiers for one (s,p,o).
        // gmeow-gts#213 lets the writer keep both, so `multi-reifier-collapsed`
        // is no longer a loss: both bindings must survive the round-trip.
        let statement = RdfTriple::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        );
        let store = VecRdfStore {
            reifiers: vec![
                RdfReifier::new(RdfTerm::blank_node("r1"), statement.clone()),
                RdfReifier::new(RdfTerm::blank_node("r2"), statement.clone()),
            ],
            ..VecRdfStore::default()
        };
        let graph = roundtrip_store(&store, "gmeow-rdf-test");
        // Two distinct reifier rows over the same triple content survive.
        assert_eq!(graph.reifiers.len(), 2);
        let rids: std::collections::BTreeSet<usize> =
            graph.reifiers.iter().map(|(rid, _)| *rid).collect();
        assert_eq!(rids.len(), 2, "the two reifiers must be distinct");
        let triples: std::collections::BTreeSet<Triple3> =
            graph.reifiers.iter().map(|(_, t)| *t).collect();
        assert_eq!(triples.len(), 1, "both reify the same (s,p,o)");
    }

    #[test]
    fn determinism_produces_identical_bytes() {
        let store = VecRdfStore::with_quads(vec![
            RdfQuad::new(
                RdfTerm::iri("https://example.org/s"),
                "https://example.org/p",
                RdfTerm::iri("https://example.org/o"),
            ),
            RdfQuad::new(
                RdfTerm::blank_node("b1"),
                "https://example.org/p2",
                RdfTerm::literal(RdfLiteral::simple("literal value")),
            ),
        ]);
        let first = to_gts(&store, "gmeow-rdf-test").expect("first write");
        let second = to_gts(&store, "gmeow-rdf-test").expect("second write");
        assert_eq!(first, second);
    }

    #[test]
    fn lookaside_metadata_and_suppressions_are_preserved() {
        let mut store = VecRdfStore::with_quads(vec![RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        )]);
        store.lookaside.metadata.push(RdfMetadataEntry::new(
            "gts:file",
            "producer",
            RdfMetadataValue::Text("gmeow-rdf-test".to_owned()),
        ));
        store.lookaside.suppressions.push(RdfSuppressionRecord {
            reason: Some("test suppression".to_owned()),
            by: None,
            targets: vec![RdfMetadataValue::Map(
                [("kind".to_owned(), RdfMetadataValue::Text("quad".to_owned()))]
                    .into_iter()
                    .collect(),
            )],
        });

        let graph = roundtrip_store(&store, "gmeow-rdf-test");
        assert_eq!(graph.meta.len(), 1);
        assert_eq!(graph.suppressions.len(), 1);
    }

    #[test]
    fn deeply_nested_triple_terms_hit_nesting_limit() {
        let mut term = RdfTerm::iri("https://example.org/leaf");
        for _ in 0..MAX_TERM_NESTING_DEPTH + 2 {
            term = RdfTerm::triple(RdfTriple::new(
                RdfTerm::iri("https://example.org/s"),
                "https://example.org/p",
                term,
            ));
        }
        let store = VecRdfStore::with_quads(vec![RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            term,
        )]);
        let err = to_gts(&store, "gmeow-rdf-test").expect_err("nested triple should fail");
        assert_eq!(err.code, "rdf-term-nesting-limit");
    }
}
