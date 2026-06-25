// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF text → frozen [`RdfDataset`] IR ingress (#909 / EPIC #906 S3).
//!
//! Parses Turtle / TriG / N-Triples / N-Quads / RDF/XML through the published
//! `gmeow-gts` codecs (GTS bytes), folds the GTS segments to a
//! [`gmeow_gts::model::Graph`] via the oxigraph-free
//! [`read_all_segments`](crate::gts::read_all_segments), then walks that graph into a
//! [`RdfDatasetBuilder`] applying the RDF 1.2 statement-layer fold.
//!
//! The fold is factored into [`fold_statement_layer`], a source-agnostic two-pass
//! classifier over `(subject, predicate, object, graph)` rows that BOTH this native
//! path and the legacy `dataset_io::dataset_from_oxigraph_quads` feed — one fold, no
//! drift (the must-pass RDF 1.2 fixture parity is the guard).
//!
//! Base IRI is handled per the plan: Turtle/TriG prepend a `@base <iri> .` directive
//! (spec-exact); RDF/XML routes through the `from_rdf_xml_with_base_iri` variant;
//! N-Triples/N-Quads require absolute IRIs and ignore the base (N/A by syntax).

use std::collections::HashSet;
use std::sync::Arc;

use gmeow_gts::model::{Graph as GtsGraph, TermKind as GtsTermKind};

use super::media_type::{classify, NativeRdfFormat};
use crate::{
    BlankScope, RdfDataset, RdfDatasetBuilder, RdfDiagnostic, RdfLiteral, RdfTextDirection, TermId,
};

/// The `rdf:reifies` predicate IRI: a triple-term object under this predicate is the
/// RDF 1.2 reifier binding the statement layer folds out of the base quad table.
pub(crate) const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// A subject/object node presented to [`fold_statement_layer`], already interned into
/// the builder. The predicate IRI string travels alongside (see [`FoldRow`]) so the
/// `rdf:reifies` classification can compare it WITHOUT re-resolving the interned id.
#[derive(Clone, Copy)]
pub(crate) enum FoldNode {
    /// An already-interned leaf or (recursively) interned triple-term object.
    Term(TermId),
    /// A triple term whose components are already interned — folded as a reifier
    /// binding when it appears as the object of an `rdf:reifies` row, otherwise
    /// re-interned as a quoted-triple object.
    Triple { s: TermId, p: TermId, o: TermId },
}

/// One `(subject, predicate, object, graph)` row, source-agnostic over oxigraph quads
/// and folded GTS graphs. Every component id is already interned in the SAME builder
/// the fold pushes into; `predicate_iri` carries the predicate's IRI string for the
/// `rdf:reifies` check (the predicate id is `predicate`).
pub(crate) struct FoldRow {
    pub subject: TermId,
    pub predicate_iri: String,
    pub predicate: TermId,
    pub object: FoldNode,
    pub graph: Option<TermId>,
}

/// The RDF 1.2 statement-layer fold, shared by the native codec path and the legacy
/// oxigraph-quads path so the two can never drift (the parity fixture is the guard).
///
/// Pass 1 binds reifiers: a row whose predicate is `rdf:reifies` with a triple-term
/// object becomes a `push_reifier(subject, triple)` binding and the subject id is
/// recorded as a reifier. Pass 2 classifies the remaining rows: a reifier subject's
/// other triples are annotations (`push_annotation`), everything else a base quad
/// (`push_quad`). This mirrors `dataset_io.rs:34-65` exactly.
pub(crate) fn fold_statement_layer<I>(
    builder: &mut RdfDatasetBuilder,
    rows: I,
) -> Result<(), RdfDiagnostic>
where
    I: IntoIterator<Item = FoldRow>,
{
    // Pass 1: bind reifiers; collect the rest as pending base/annotation rows.
    let mut reifier_ids: HashSet<TermId> = HashSet::new();
    let mut pending: Vec<(TermId, TermId, TermId, Option<TermId>)> = Vec::new();
    for row in rows {
        let FoldRow {
            subject,
            predicate_iri,
            predicate,
            object,
            graph,
        } = row;
        if predicate_iri == RDF_REIFIES {
            if let FoldNode::Triple { s, p, o } = object {
                let triple_term = builder.intern_triple(s, p, o);
                builder.push_reifier(subject, triple_term);
                reifier_ids.insert(subject);
                continue;
            }
        }
        let object_id = match object {
            FoldNode::Term(id) => id,
            FoldNode::Triple { s, p, o } => builder.intern_triple(s, p, o),
        };
        pending.push((subject, predicate, object_id, graph));
    }

    // Pass 2: a reifier subject's other triples are annotations; the rest base quads.
    for (subject, predicate, object, graph) in pending {
        if reifier_ids.contains(&subject) {
            builder.push_annotation(subject, predicate, object);
        } else {
            builder.push_quad(subject, predicate, object, graph);
        }
    }
    Ok(())
}

/// Parse RDF text bytes of `media_type` into a frozen [`RdfDataset`].
///
/// Steps: UTF-8 validate (hard-fail `native-codec-utf8`); for Turtle/TriG with a base
/// prepend a `@base` directive; dispatch to the matching `gmeow-gts` `from_*` codec
/// (RDF/XML uses the base variant when a base is present, N-Quads uses
/// `from_nquads::from_nquads`); fold the GTS segments to a graph; walk that graph
/// through [`fold_statement_layer`] and freeze.
pub fn parse_dataset(
    bytes: &[u8],
    media_type: &str,
    base_iri: Option<&str>,
) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
    let format = classify(media_type)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|e| RdfDiagnostic::error("native-codec-utf8", e.to_string()))?;

    let gts_bytes = encode_to_gts(format, text, base_iri)?;
    let graph = crate::gts::read_all_segments(&gts_bytes)?;
    dataset_from_gts_graph(&graph)
}

/// Drive `text` through the matching `gmeow-gts` `from_*` codec, returning GTS bytes.
///
/// Turtle/TriG prepend a spec-exact `@base <iri> .` directive when a base is supplied
/// (the gmeow-gts Turtle/TriG codecs take no base-IRI argument); RDF/XML threads the
/// base through `from_rdf_xml_with_base_iri`; N-Triples/N-Quads require absolute IRIs,
/// so the base is structurally inapplicable and ignored.
fn encode_to_gts(
    format: NativeRdfFormat,
    text: &str,
    base_iri: Option<&str>,
) -> Result<Vec<u8>, RdfDiagnostic> {
    use gmeow_gts::rdf_codecs::{
        from_ntriples, from_rdf_xml, from_rdf_xml_with_base_iri, from_trig, from_turtle,
    };

    let codec_error = |e: gmeow_gts::rdf_codecs::RdfCodecError| {
        RdfDiagnostic::error("native-codec-parse", e.to_string())
    };
    match format {
        NativeRdfFormat::Turtle => {
            from_turtle(&with_base_prefix(text, base_iri)).map_err(codec_error)
        }
        NativeRdfFormat::TriG => from_trig(&with_base_prefix(text, base_iri)).map_err(codec_error),
        NativeRdfFormat::NTriples => from_ntriples(text).map_err(codec_error),
        NativeRdfFormat::NQuads => gmeow_gts::from_nquads::from_nquads(text)
            .map_err(|e| RdfDiagnostic::error("native-codec-parse", e.to_string())),
        NativeRdfFormat::RdfXml => match base_iri {
            Some(base) => from_rdf_xml_with_base_iri(text, base).map_err(codec_error),
            None => from_rdf_xml(text).map_err(codec_error),
        },
    }
}

/// Prepend a `@base <iri> .` Turtle directive when a base is supplied (spec-exact),
/// otherwise return the text unchanged.
fn with_base_prefix(text: &str, base_iri: Option<&str>) -> String {
    match base_iri {
        Some(base) => format!("@base <{base}> .\n{text}"),
        None => text.to_owned(),
    }
}

/// Walk a folded GTS [`GtsGraph`] into a frozen [`RdfDataset`] through the shared
/// [`fold_statement_layer`].
///
/// The `gmeow-gts` `from_*` codecs already fold `rdf:reifies` triples into the graph's
/// `reifiers` table (they never appear in `quads`), but they do NOT classify
/// annotations. To feed the SAME two-pass fold the oxigraph path uses — and reach the
/// SAME IR byte-for-byte — this re-materializes each reifier binding as a synthetic
/// `<reifier> rdf:reifies <<( s p o )>>` row alongside the plain quads, so pass 1
/// re-binds reifiers and pass 2 reclassifies the reifier subjects' other triples as
/// annotations. Term interning is shared across all rows, so identical terms collapse
/// to one id exactly as on the oxigraph path.
pub(crate) fn dataset_from_gts_graph(graph: &GtsGraph) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
    let mut builder = RdfDatasetBuilder::new();
    let interner = GtsInterner { graph };

    let mut rows: Vec<FoldRow> = Vec::with_capacity(graph.quads.len() + graph.reifiers.len());

    // Synthetic `rdf:reifies` rows reconstructed from the GTS reifier table, so the
    // shared fold re-binds them identically to the oxigraph path (pass 1). A
    // self-reifier sentinel — a `Triple` term whose `reifier` is its OWN id — is the
    // binding of an inline quoted-triple term used as a quad object, NOT a statement-
    // layer reifier; it carries no `<reifier> rdf:reifies <<…>>` row (the N-Quads
    // serializer skips it identically) and is resolved when its parent quad interns the
    // object. Emitting a synthetic row for it would make a quoted triple the subject of
    // `rdf:reifies`, which the IR rejects.
    for &(reifier_id, (s, p, o)) in &graph.reifiers {
        if graph.terms.get(reifier_id).is_some_and(|term| {
            term.kind == GtsTermKind::Triple && term.reifier == Some(reifier_id)
        }) {
            continue;
        }
        let subject = interner.intern(&mut builder, reifier_id)?;
        let predicate = builder.intern_iri(RDF_REIFIES.to_owned());
        let s = interner.intern(&mut builder, s)?;
        let p = interner.intern(&mut builder, p)?;
        let o = interner.intern(&mut builder, o)?;
        rows.push(FoldRow {
            subject,
            predicate_iri: RDF_REIFIES.to_owned(),
            predicate,
            object: FoldNode::Triple { s, p, o },
            graph: None,
        });
    }

    // Base quad rows (annotations are still plain quads here; pass 2 reclassifies them).
    for &(s, p, o, g) in &graph.quads {
        let subject = interner.intern(&mut builder, s)?;
        let predicate_iri = interner.iri_string(p)?;
        let predicate = interner.intern(&mut builder, p)?;
        let object = interner.intern_node(&mut builder, o)?;
        let graph = match g {
            Some(g) => Some(interner.intern(&mut builder, g)?),
            None => None,
        };
        rows.push(FoldRow {
            subject,
            predicate_iri,
            predicate,
            object,
            graph,
        });
    }

    fold_statement_layer(&mut builder, rows)?;
    builder.freeze()
}

/// Intern GTS terms into a builder, resolving quoted-triple terms through their
/// reifier binding. Every folded blank node lands in [`BlankScope::DEFAULT`] (the
/// folded graph has already collapsed per-segment scope — the documented
/// `bnode-scope-flatten` loss, identical to `import_gts_graph`).
struct GtsInterner<'a> {
    graph: &'a GtsGraph,
}

impl GtsInterner<'_> {
    /// Intern a GTS term id into the builder, returning its [`TermId`]. A quoted-triple
    /// term resolves its `(s, p, o)` through the reifier table and interns as a triple.
    fn intern(
        &self,
        builder: &mut RdfDatasetBuilder,
        gts_id: usize,
    ) -> Result<TermId, RdfDiagnostic> {
        match self.intern_node(builder, gts_id)? {
            FoldNode::Term(id) => Ok(id),
            FoldNode::Triple { s, p, o } => Ok(builder.intern_triple(s, p, o)),
        }
    }

    /// Intern a GTS term id, returning a [`FoldNode`]: a leaf becomes `Term`, a
    /// quoted-triple term becomes `Triple` (its components already interned) so a
    /// caller can fold it as a reifier binding rather than re-interning it.
    fn intern_node(
        &self,
        builder: &mut RdfDatasetBuilder,
        gts_id: usize,
    ) -> Result<FoldNode, RdfDiagnostic> {
        let term = self.graph.terms.get(gts_id).ok_or_else(|| {
            RdfDiagnostic::error(
                "native-codec-term-out-of-range",
                format!("GTS term id {gts_id} is out of range"),
            )
        })?;
        match term.kind {
            GtsTermKind::Iri => {
                let iri = term
                    .value
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        RdfDiagnostic::error(
                            "native-codec-iri-missing-value",
                            "GTS IRI term requires a non-empty value",
                        )
                    })?;
                Ok(FoldNode::Term(builder.intern_iri(iri.to_owned())))
            }
            GtsTermKind::Bnode => {
                let label = term
                    .value
                    .clone()
                    .unwrap_or_else(|| format!("gts_bnode_{gts_id}"));
                Ok(FoldNode::Term(
                    builder.intern_blank(label, BlankScope::DEFAULT),
                ))
            }
            GtsTermKind::Literal => {
                let datatype = match term.datatype {
                    Some(dt_id) => Some(self.iri_string(dt_id)?),
                    None => None,
                };
                let direction =
                    parse_gts_direction(term.direction.as_deref(), term.lang.as_deref())?;
                Ok(FoldNode::Term(builder.intern_literal(RdfLiteral {
                    lexical_form: term.value.clone().unwrap_or_default(),
                    datatype,
                    language: term.lang.clone(),
                    direction,
                })))
            }
            GtsTermKind::Triple => {
                let reifier_id = term.reifier.ok_or_else(|| {
                    RdfDiagnostic::error(
                        "native-codec-unbound-triple-term",
                        "GTS triple term has no reifier binding",
                    )
                })?;
                let (s, p, o) = self.graph.reifier(reifier_id).ok_or_else(|| {
                    RdfDiagnostic::error(
                        "native-codec-missing-reifier-binding",
                        format!("GTS triple term references missing reifier {reifier_id}"),
                    )
                })?;
                let s = self.intern(builder, s)?;
                let p = self.intern(builder, p)?;
                let o = self.intern(builder, o)?;
                Ok(FoldNode::Triple { s, p, o })
            }
        }
    }

    /// Intern a GTS term id known to occupy an IRI position (predicate / datatype),
    /// returning its IRI string for the `rdf:reifies` check and literal datatype.
    fn iri_string(&self, gts_id: usize) -> Result<String, RdfDiagnostic> {
        let term = self.graph.terms.get(gts_id).ok_or_else(|| {
            RdfDiagnostic::error(
                "native-codec-term-out-of-range",
                format!("GTS term id {gts_id} is out of range"),
            )
        })?;
        match term.kind {
            GtsTermKind::Iri => term.value.clone().filter(|v| !v.is_empty()).ok_or_else(|| {
                RdfDiagnostic::error(
                    "native-codec-iri-missing-value",
                    "GTS IRI term requires a non-empty value",
                )
            }),
            other => Err(RdfDiagnostic::error(
                "native-codec-predicate-not-iri",
                format!("GTS term in an IRI position must be an IRI, got {other:?}"),
            )),
        }
    }
}

/// Parse a GTS literal base-direction string (`"ltr"`/`"rtl"`) into the IR's
/// [`RdfTextDirection`], mirroring `gmeow_rdf_core`'s `parse_gts_direction`: `None` is
/// legitimate absence, an unrecognized non-empty value hard-fails, and RDF 1.2 admits
/// a direction ONLY on a language-tagged string (a direction without a language is a
/// hard error rather than a silently ill-formed literal).
fn parse_gts_direction(
    value: Option<&str>,
    language: Option<&str>,
) -> Result<Option<RdfTextDirection>, RdfDiagnostic> {
    let direction = match value {
        None => return Ok(None),
        Some("ltr") => RdfTextDirection::Ltr,
        Some("rtl") => RdfTextDirection::Rtl,
        Some(other) => {
            return Err(RdfDiagnostic::error(
                "native-codec-invalid-direction",
                format!("unrecognized GTS literal base direction {other:?}"),
            ))
        }
    };
    if language.is_none_or(str::is_empty) {
        return Err(RdfDiagnostic::error(
            "native-codec-direction-without-language",
            "an RDF 1.2 literal base direction requires a non-empty language tag",
        ));
    }
    Ok(Some(direction))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TermValue;

    #[test]
    fn parses_basic_ntriples() {
        let nt = "<https://e/s> <https://e/p> <https://e/o> .\n\
                  <https://e/s> <https://e/p2> \"lit\" .\n";
        let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None).expect("parse");
        assert_eq!(ds.quad_count(), 2);
        assert!(ds.term_count() >= 4);
    }

    #[test]
    fn folds_rdf12_statement_layer_to_parity() {
        // The exact RDF 1.2 fixture from dataset_io.rs:131: a reifier binding + an
        // annotation. The base quad table is empty; the reifier and annotation land in
        // their own tables (parity gate R1).
        let nt = concat!(
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            "<https://e/r> <https://e/confidence> \"0.9\" .\n",
        );
        let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None).expect("parse");
        assert_eq!(ds.quad_count(), 0, "reifier rows are not base quads");
        assert_eq!(ds.reifiers().count(), 1);
        assert_eq!(ds.annotations().count(), 1);
    }

    #[test]
    fn turtle_base_resolves_relative_iri() {
        // A relative-IRI Turtle doc parsed with a base resolves the subject against it.
        let ttl = "<rel> <https://e/p> <https://e/o> .\n";
        let ds = parse_dataset(ttl.as_bytes(), "text/turtle", Some("https://example.org/"))
            .expect("parse with base");
        assert_eq!(ds.quad_count(), 1);
        assert!(
            ds.term_id_by_value(&TermValue::Iri("https://example.org/rel".to_owned()))
                .is_some(),
            "relative <rel> must resolve against the base IRI"
        );
    }

    #[test]
    fn literal_direction_survives_parse() {
        let nt = concat!(
            "<https://e/s> <https://e/p> ",
            "\"\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}\"@ar--rtl .\n",
        );
        let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None).expect("parse");
        // The IR expands a language-tagged literal's datatype to rdf:langString (C0.1)
        // and keeps the base direction in the literal identity key (NOT dirLangString).
        assert!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}".to_owned(),
                datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".to_owned(),
                language: Some("ar".to_owned()),
                direction: Some(RdfTextDirection::Rtl),
            })
            .is_some(),
            "an rtl directional literal must survive the parse"
        );
    }

    #[test]
    fn invalid_utf8_hard_fails() {
        let err =
            parse_dataset(&[0xff, 0xfe], "text/turtle", None).expect_err("invalid utf-8 must fail");
        assert_eq!(err.code, "native-codec-utf8");
    }

    #[test]
    fn unknown_media_type_hard_fails() {
        let err =
            parse_dataset(b"", "application/json", None).expect_err("unknown media type must fail");
        assert_eq!(err.code, "native-codec-unsupported-format");
    }
}
