// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF text/bytes ingress into the frozen [`RdfDataset`] IR.
//!
//! Oxigraph remains the parser at the edge, but the read model handed to GMEOW
//! consumers is the concrete IR. This module is deliberately PyO3-free so logic,
//! SHACL, and pipeline stages can route oxigraph-parsed inputs through the same
//! `RdfDataset` path as the Python `RdfDataset` handle.

use std::sync::Arc;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{BaseDirection, GraphName, NamedOrBlankNode, Quad, Term as OxTerm};

use crate::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTextDirection, TermId};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Parse RDF bytes and freeze them into a validated [`RdfDataset`].
///
/// The RDF 1.2 statement layer is folded in: a `rdf:reifies` triple-term object
/// becomes a reifier binding, and a reifier subject's other triples become
/// annotations (matching the GTS producer's `add_rdf12` pass structure).
pub fn dataset_from_bytes(bytes: &[u8], format: RdfFormat) -> Result<Arc<RdfDataset>, String> {
    let quads = parse_quads(bytes, format).map_err(|e| format!("parse error: {e}"))?;
    dataset_from_oxigraph_quads(&quads)
}

/// Freeze already-parsed oxigraph quads into a validated [`RdfDataset`].
pub fn dataset_from_oxigraph_quads(quads: &[Quad]) -> Result<Arc<RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();

    // Pass 1: bind reifiers; collect the rest as pending base/annotation rows.
    let mut reifier_ids: std::collections::HashSet<TermId> = std::collections::HashSet::new();
    let mut pending: Vec<(TermId, TermId, TermId, Option<TermId>)> = Vec::new();
    for quad in quads {
        let is_reifies = quad.predicate.as_str() == RDF_REIFIES;
        if let (true, OxTerm::Triple(triple)) = (is_reifies, &quad.object) {
            let rid = intern_subject(&mut builder, &quad.subject);
            let qs = intern_subject(&mut builder, &triple.subject);
            let qp = builder.intern_iri(triple.predicate.as_str().to_owned());
            let qo = intern_term(&mut builder, &triple.object)?;
            let triple_term = builder.intern_triple(qs, qp, qo);
            builder.push_reifier(rid, triple_term);
            reifier_ids.insert(rid);
        } else {
            let sid = intern_subject(&mut builder, &quad.subject);
            let pid = builder.intern_iri(quad.predicate.as_str().to_owned());
            let oid = intern_term(&mut builder, &quad.object)?;
            let gid = intern_graph(&mut builder, &quad.graph_name);
            pending.push((sid, pid, oid, gid));
        }
    }

    // Pass 2: a reifier subject's other triples are annotations; the rest base quads.
    for (sid, pid, oid, gid) in pending {
        if reifier_ids.contains(&sid) {
            builder.push_annotation(sid, pid, oid);
        } else {
            builder.push_quad(sid, pid, oid, gid);
        }
    }

    builder.freeze().map_err(|e| e.to_string())
}

fn parse_quads(bytes: &[u8], format: RdfFormat) -> Result<Vec<Quad>, String> {
    let mut quads = Vec::new();
    for quad in RdfParser::from_format(format).lenient().for_reader(bytes) {
        quads.push(quad.map_err(|e| e.to_string())?);
    }
    Ok(quads)
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
        let ds = dataset_from_bytes(nt.as_bytes(), RdfFormat::NTriples).expect("build");
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
        let ds = dataset_from_bytes(nt.as_bytes(), RdfFormat::NTriples).expect("build");
        assert_eq!(ds.quad_count(), 0, "reifier rows are not base quads");
        assert_eq!(ds.reifiers().count(), 1);
        assert_eq!(ds.annotations().count(), 1);
    }
}
