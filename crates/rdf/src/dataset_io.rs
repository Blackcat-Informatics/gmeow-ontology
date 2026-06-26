// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF text/bytes ingress into the frozen [`RdfDataset`] IR.
//!
//! The text codec is now the oxigraph-free native [`parse_dataset`](crate::parse_dataset)
//! path (#909 / EPIC #906 S3); the read model handed to GMEOW consumers is the
//! concrete IR. This module is deliberately PyO3-free so logic, SHACL, and pipeline
//! stages can route parsed inputs through the same `RdfDataset` path as the Python
//! `RdfDataset` handle. The `dataset_from_oxigraph_quads` helper stays as the
//! snapshot stage's already-parsed-quads → IR fold (it reuses the SHARED
//! [`fold_statement_layer`] so the oxigraph-quads path and the native path can never
//! drift).

use std::sync::Arc;

use oxigraph::model::{BaseDirection, GraphName, NamedOrBlankNode, Quad, Term as OxTerm};

use crate::native_codecs::parse::{fold_statement_layer, FoldNode, FoldRow};
use crate::{
    parse_dataset, BlankScope, NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfLiteral,
    RdfTextDirection, TermId,
};

/// Parse RDF bytes and freeze them into a validated [`RdfDataset`] via the native
/// codec path.
///
/// `format` is a [`NativeRdfFormat`] codec selector — the workspace-wide #909 sweep
/// (Tasks 2–6) routed every call site onto the native enum, so the temporary
/// `oxigraph::io::RdfFormat` `From` shim is gone. The RDF 1.2 statement layer is
/// folded in: a `rdf:reifies` triple-term object becomes a reifier binding, and a
/// reifier subject's other triples become annotations (matching the GTS producer's
/// `add_rdf12` pass structure).
pub fn dataset_from_bytes(
    bytes: &[u8],
    format: NativeRdfFormat,
) -> Result<Arc<RdfDataset>, String> {
    let media_type = format.media_type();
    parse_dataset(bytes, media_type, None).map_err(|e| format!("parse error: {e}"))
}

/// Freeze already-parsed oxigraph quads into a validated [`RdfDataset`].
///
/// The RDF 1.2 statement-layer fold is delegated to the SHARED
/// [`fold_statement_layer`] helper (#909 Task 1) so the oxigraph path and the native
/// codec path apply ONE fold and cannot drift. This function's only job is to map each
/// oxigraph quad into the source-agnostic [`FoldRow`] form (interning leaf terms into
/// the builder), leaving the two-pass reifier/annotation classification to the helper.
pub fn dataset_from_oxigraph_quads(quads: &[Quad]) -> Result<Arc<RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();

    let mut rows: Vec<FoldRow> = Vec::with_capacity(quads.len());
    for quad in quads {
        let subject = intern_subject(&mut builder, &quad.subject);
        let predicate_iri = quad.predicate.as_str().to_owned();
        let predicate = builder.intern_iri(predicate_iri.clone());
        // A triple-term object is presented as a `FoldNode::Triple` (components
        // interned) so the helper can fold it as a reifier binding under `rdf:reifies`
        // or re-intern it as a quoted-triple object otherwise — matching the old
        // single-function behavior exactly.
        let object = match &quad.object {
            OxTerm::Triple(triple) => {
                let s = intern_subject(&mut builder, &triple.subject);
                let p = builder.intern_iri(triple.predicate.as_str().to_owned());
                let o = intern_term(&mut builder, &triple.object)?;
                FoldNode::Triple { s, p, o }
            }
            other => FoldNode::Term(intern_term(&mut builder, other)?),
        };
        let graph = intern_graph(&mut builder, &quad.graph_name);
        rows.push(FoldRow {
            subject,
            predicate_iri,
            predicate,
            object,
            graph,
        });
    }

    fold_statement_layer(&mut builder, rows).map_err(|e| e.to_string())?;
    builder.freeze().map_err(|e| e.to_string())
}

fn intern_subject(builder: &mut RdfDatasetBuilder, subject: &NamedOrBlankNode) -> TermId {
    match subject {
        NamedOrBlankNode::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(b) => {
            builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT)
        }
    }
}

fn intern_graph(builder: &mut RdfDatasetBuilder, graph: &GraphName) -> Option<TermId> {
    match graph {
        GraphName::DefaultGraph => None,
        GraphName::NamedNode(n) => Some(builder.intern_iri(n.as_str().to_owned())),
        GraphName::BlankNode(b) => {
            Some(builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT))
        }
    }
}

fn intern_term(builder: &mut RdfDatasetBuilder, term: &OxTerm) -> Result<TermId, String> {
    Ok(match term {
        OxTerm::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        OxTerm::BlankNode(b) => builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT),
        OxTerm::Literal(l) => builder.intern_literal(RdfLiteral {
            lexical_form: l.value().to_owned(),
            datatype: Some(l.datatype().as_str().to_owned()),
            language: l.language().map(str::to_owned),
            direction: l.direction().map(|direction| match direction {
                BaseDirection::Ltr => RdfTextDirection::Ltr,
                BaseDirection::Rtl => RdfTextDirection::Rtl,
            }),
        }),
        OxTerm::Triple(triple) => {
            let s = intern_subject(builder, &triple.subject);
            let p = builder.intern_iri(triple.predicate.as_str().to_owned());
            let o = intern_term(builder, &triple.object)?;
            builder.intern_triple(s, p, o)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_from_bytes_counts_quads() {
        let nt = "<https://e/s> <https://e/p> <https://e/o> .\n\
                  <https://e/s> <https://e/p2> \"lit\" .\n";
        let ds = dataset_from_bytes(nt.as_bytes(), NativeRdfFormat::NTriples).expect("build");
        assert_eq!(ds.quad_count(), 2);
        assert!(ds.term_count() >= 4);
    }

    #[test]
    fn dataset_from_bytes_classifies_rdf12_statement_layer() {
        // A reifier's reifies binding + an annotation: the base quad table is
        // empty, the reifier binding and annotation land in their own tables.
        let nt = concat!(
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            "<https://e/r> <https://e/confidence> \"0.9\" .\n",
        );
        let ds = dataset_from_bytes(nt.as_bytes(), NativeRdfFormat::NTriples).expect("build");
        assert_eq!(ds.quad_count(), 0, "reifier rows are not base quads");
        assert_eq!(ds.reifiers().count(), 1);
        assert_eq!(ds.annotations().count(), 1);
    }

    #[test]
    fn dataset_from_bytes_routes_each_native_format() {
        // The codec selector is the native enum across every format — the #909 sweep
        // removed the temporary oxigraph::io::RdfFormat From shim entirely.
        let nq = "<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n";
        let ds = dataset_from_bytes(nq.as_bytes(), NativeRdfFormat::NQuads).expect("build nquads");
        assert_eq!(ds.quad_count(), 1);
    }
}
