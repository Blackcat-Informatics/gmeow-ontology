// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Oxigraph-free [`RdfQuad`] ⇄ [`RdfDataset`] conversions (EPIC #906).
//!
//! These are the native, `gts`-gated twins of the oxigraph-quad helpers in
//! [`crate::dataset_io`] / [`crate::oxigraph`]: a consumer that already holds (or wants)
//! a flat owned-[`RdfQuad`] stream can fold it into the frozen IR (or un-fold the IR back
//! into the source-faithful quad stream) WITHOUT pulling oxigraph. The fold routes
//! through the SAME shared [`fold_statement_layer`] the text codecs and the oxigraph-quad
//! path use, so the RDF 1.2 statement layer (`rdf:reifies` reifiers + annotations) is
//! reconstructed identically and the three paths can never drift.

use std::sync::Arc;

use crate::native_codecs::parse::{fold_statement_layer, FoldNode, FoldRow};
use crate::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermId};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Freeze already-built native [`RdfQuad`]s into a validated [`RdfDataset`], folding the
/// RDF 1.2 statement layer.
///
/// The oxigraph-free twin of [`crate::dataset_from_oxigraph_quads`]: it routes through
/// the SAME [`fold_statement_layer`] helper (a `rdf:reifies` triple-term object becomes a
/// reifier binding and a reifier subject's other triples become annotations), differing
/// only in mapping each native [`RdfQuad`] into the source-agnostic [`FoldRow`] form.
/// Every term is interned under the default blank scope (already-scope-qualified labels,
/// the same contract the oxigraph-quads twin assumed).
///
/// # Errors
/// Returns the diagnostic string if the folded quads fail dataset validation.
pub fn dataset_from_quads(quads: &[RdfQuad]) -> Result<Arc<RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();
    let mut rows: Vec<FoldRow> = Vec::with_capacity(quads.len());
    for quad in quads {
        let subject = intern_native_term(&mut builder, &quad.subject);
        let predicate_iri = quad.predicate.clone();
        let predicate = builder.intern_iri(predicate_iri.clone());
        let object = match &quad.object {
            RdfTerm::Triple(triple) => {
                let s = intern_native_term(&mut builder, &triple.subject);
                let p = builder.intern_iri(triple.predicate.clone());
                let o = intern_native_term(&mut builder, &triple.object);
                FoldNode::Triple { s, p, o }
            }
            other => FoldNode::Term(intern_native_term(&mut builder, other)),
        };
        let graph = quad
            .graph_name
            .as_ref()
            .map(|g| intern_native_term(&mut builder, g));
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

/// Intern one native [`RdfTerm`] leaf (IRI / blank / literal / quoted triple) into
/// `builder` under the default blank scope, returning its [`TermId`].
fn intern_native_term(builder: &mut RdfDatasetBuilder, term: &RdfTerm) -> TermId {
    match term {
        RdfTerm::Iri(iri) => builder.intern_iri(iri.clone()),
        RdfTerm::BlankNode(label) => builder.intern_blank(label.clone(), BlankScope::DEFAULT),
        RdfTerm::Literal(lit) => builder.intern_literal(lit.clone()),
        RdfTerm::Triple(triple) => {
            let s = intern_native_term(builder, &triple.subject);
            let p = builder.intern_iri(triple.predicate.clone());
            let o = intern_native_term(builder, &triple.object);
            builder.intern_triple(s, p, o)
        }
    }
}

/// Flatten a frozen [`RdfDataset`] into the source-faithful flat [`RdfQuad`] stream —
/// the owned-model twin of [`crate::oxigraph::flat_oxigraph_quads_from_dataset`], for
/// consumers that fold over [`RdfQuad`] rather than oxigraph quads, WITHOUT pulling
/// oxigraph. Base quads first, then the re-materialized `rdf:reifies` reifier rows and
/// the annotation rows. The IR fold + this un-fold are exact inverses.
#[must_use]
pub fn flat_rdf_quads_from_dataset(dataset: &RdfDataset) -> Vec<RdfQuad> {
    let mut quads: Vec<RdfQuad> = dataset.owned_quads().collect();
    for reifier in dataset.owned_reifiers() {
        let statement = RdfTerm::triple(reifier.statement.clone());
        quads.push(RdfQuad::new(
            reifier.reifier.clone(),
            RDF_REIFIES,
            statement,
        ));
    }
    for annotation in dataset.owned_annotations() {
        quads.push(RdfQuad::new(
            annotation.reifier.clone(),
            annotation.predicate.clone(),
            annotation.object.clone(),
        ));
    }
    quads
}

/// Freeze a flat owned-[`RdfQuad`] stream into a dataset WITHOUT folding the RDF 1.2
/// statement layer (every quad — including a `rdf:reifies` triple-term row — stays a
/// plain quad), via [`RdfDatasetBuilder::push_owned_quad`].
///
/// The complement of [`dataset_from_quads`] (which DOES fold): a caller that already
/// holds the un-folded flat stream and wants it canonicalized as a flat triple set (not
/// the folded overlay) re-freezes through here so [`crate::canonicalize`] emits the flat
/// `rdf:reifies` / annotation triples, byte-matching the prior oxigraph-flat canonical
/// path.
///
/// # Errors
/// Returns the diagnostic string if the quads fail dataset validation.
pub fn flat_dataset_from_quads(quads: &[RdfQuad]) -> Result<Arc<RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(quad);
    }
    builder.freeze().map_err(|e| e.to_string())
}

/// The RDFC-1.0 canonical N-Quads document of `dataset`, **flattened**: the RDF 1.2
/// statement overlay (reifier bindings + annotations) is re-materialized to plain
/// `rdf:reifies` / annotation triples BEFORE canonicalizing, with no overlay re-fold.
///
/// Byte-identical to the prior `gmeow_rdf::canonicalize_quads` over a flat oxigraph quad
/// set (both canonicalize the same flat triple set under conformant SHA-256 RDFC-1.0), so
/// every committed digest/comparison keyed on this string is preserved. The native folded
/// [`crate::canonicalize`] would instead emit the reserved overlay sentinels.
///
/// # Errors
/// Returns the diagnostic string if the flattened quads fail dataset validation.
pub fn canonical_flat_nquads(dataset: &RdfDataset) -> Result<String, String> {
    let flat = flat_dataset_from_quads(&flat_rdf_quads_from_dataset(dataset))?;
    Ok(crate::canonicalize(&flat).nquads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_roundtrip_through_dataset() {
        let quads = vec![
            RdfQuad::new(
                RdfTerm::iri("https://e/s"),
                "https://e/p",
                RdfTerm::iri("https://e/o"),
            ),
            RdfQuad::new(
                RdfTerm::iri("https://e/s"),
                "https://e/p2",
                RdfTerm::literal(crate::RdfLiteral::simple("lit")),
            ),
        ];
        let ds = dataset_from_quads(&quads).expect("freeze");
        assert_eq!(ds.quad_count(), 2);
        let flat = flat_rdf_quads_from_dataset(&ds);
        assert_eq!(flat.len(), 2);
    }

    /// AIRTIGHT byte-equality gate (EPIC #906): the native `canonical_flat_nquads`
    /// must produce the EXACT line set the prior oxigraph-flat canonical path emitted
    /// (`canonicalize_quads(flat_oxigraph_quads) → format!("{q} .")`), over an input that
    /// exercises every literal/term shape (simple, typed, lang, blank-node, and an RDF
    /// 1.2 reifier with an annotation). Only compiled when the oxigraph oracle is present.
    #[cfg(feature = "oxigraph")]
    #[test]
    fn canonical_flat_nquads_byte_matches_oxigraph_path() {
        // TriG with BOTH a default graph and a NAMED graph (the carrier composes named
        // graphs), exercising every literal/term shape + an RDF 1.2 reifier+annotation.
        const TRIG: &str = r#"
@prefix ex: <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
ex:s ex:p ex:o .
ex:s ex:label "hello" .
ex:s ex:n "42"^^xsd:integer .
ex:s ex:greeting "bonjour"@fr .
ex:s ex:friend [ ex:name "anon" ] .
ex:r rdf:reifies <<( ex:s ex:p ex:o )>> .
ex:r ex:confidence "0.9"^^xsd:decimal .
ex:g {
  ex:a ex:b ex:c .
  ex:a ex:lbl "named" .
}
"#;
        let ir = crate::parse_dataset(TRIG.as_bytes(), "application/trig", None).expect("parse");

        // Native flat-canonical path.
        let native: std::collections::BTreeSet<String> = super::canonical_flat_nquads(&ir)
            .expect("native flat canon")
            .lines()
            .map(str::to_owned)
            .collect();

        // Legacy oxigraph flat-canonical path.
        let ox_quads = crate::oxigraph::flat_oxigraph_quads_from_dataset(&ir).expect("ox flat");
        let ox_canon = crate::canonicalize_quads(ox_quads).expect("ox canon");
        let legacy: std::collections::BTreeSet<String> =
            ox_canon.iter().map(|q| format!("{q} .")).collect();

        assert_eq!(
            native, legacy,
            "native flat-canonical N-Quads must byte-match the oxigraph flat-canonical path"
        );
    }
}
