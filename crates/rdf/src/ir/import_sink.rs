// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **authoritative** GTS ingestion path: a [`StreamingSink`] that preserves
//! per-segment blank-node scope while folding into the immutable IR (#819 C2.a).
//!
//! `gmeow_gts::reader::read()` folds every segment into one append-order term
//! table, which destroys per-segment blank-node scope (the same `_:b1` label in
//! two different segments names two *different* nodes, but the folded table loses
//! that). The only place segment identity survives is the streaming sink
//! callbacks, each of which carries a `segment_index`. This importer therefore
//! drives [`gmeow_gts::reader::read_to_sink`] and interns each segment's blank
//! nodes under a per-segment [`BlankScope`] — making this the correctness-bearing
//! ingestion path for blank-node scope (see `docs/design/819-rdf-ir-dataflow.md`,
//! *Appendix C0.2* and the `bnode-scope-flatten` loss code).
//!
//! Every `StreamingSink` method returns `()` (it cannot signal failure inline), so
//! a referential error during a `quad`/`reifier`/`annotation` callback is recorded
//! as a deferred [`RdfDiagnostic`] in `self.error` and surfaced AFTER the reader
//! returns. Per the no-optionality / hard-fail doctrine, a dangling term reference
//! is an `Err`, never a silent skip.

use std::collections::HashMap;

use ciborium::value::Value;
use gmeow_gts::model::{OpaqueNode, Quad, Signature, StreamableInfo, Suppression, Term, TermKind};
use gmeow_gts::reader::StreamingSink;

use super::builder::RdfDatasetBuilder;
use super::bundle::{RdfBundle, RdfEnvelope};
use super::term::{BlankScope, TermId};
use crate::{
    RdfDiagnostic, RdfLiteral, RdfLocation, RdfLookaside, RdfMetadataValue, RdfOpaqueNodeRecord,
    RdfSegmentRecord, RdfSignatureRecord, RdfSuppressionRecord,
};

/// Depth bound for resolving nested quoted-triple terms, mirroring the
/// `MAX_GTS_TERM_NESTING_DEPTH` guard in [`crate::gts`]. A cyclic or absurdly
/// nested triple term hard-fails rather than recursing without bound.
const MAX_GTS_TERM_NESTING_DEPTH: usize = 16;

/// A [`StreamingSink`] that folds GTS events into the immutable IR with per-segment
/// blank-node scope isolation.
struct SinkImporter {
    /// The fallible IR builder we intern terms and push structure into.
    builder: RdfDatasetBuilder,
    /// Per-segment map from GTS segment-local term id → our [`TermId`]. The outer
    /// index IS the `segment_index`; the map is grown on demand so a sparse or
    /// out-of-order segment stream still resolves correctly.
    remaps: Vec<HashMap<usize, TermId>>,
    /// Per-segment reifier bindings (`(segment_index, reifier gts-id) → (s, p, o)
    /// gts-ids`), so a Triple term delivered later can recover its components
    /// THROUGH the segment's remap.
    reifier_bindings: HashMap<(usize, usize), gmeow_gts::model::Triple3>,
    /// Out-of-band material accumulated from blob / signature / suppression /
    /// segment-head / opaque events.
    lookaside: RdfLookaside,
    /// First deferred error. `StreamingSink` methods return `()`, so a referential
    /// failure is parked here and surfaced after the reader returns.
    error: Option<RdfDiagnostic>,
}

impl SinkImporter {
    fn new() -> Self {
        Self {
            builder: RdfDatasetBuilder::new(),
            remaps: Vec::new(),
            reifier_bindings: HashMap::new(),
            lookaside: RdfLookaside::default(),
            error: None,
        }
    }

    /// Record the first deferred error; later errors do not overwrite it.
    fn fail(&mut self, diagnostic: RdfDiagnostic) {
        if self.error.is_none() {
            self.error = Some(diagnostic);
        }
    }

    /// Ensure `remaps[segment_index]` exists.
    fn segment_map(&mut self, segment_index: usize) -> &mut HashMap<usize, TermId> {
        if self.remaps.len() <= segment_index {
            self.remaps.resize_with(segment_index + 1, HashMap::new);
        }
        &mut self.remaps[segment_index]
    }

    /// Resolve a GTS segment-local term id THROUGH the segment's remap, failing if
    /// the id was never introduced by a `term` callback (a dangling reference).
    fn resolve(
        &self,
        segment_index: usize,
        gts_id: usize,
        role: &str,
    ) -> Result<TermId, RdfDiagnostic> {
        self.remaps
            .get(segment_index)
            .and_then(|map| map.get(&gts_id).copied())
            .ok_or_else(|| {
                RdfDiagnostic::error(
                    "rdf-ir-dangling-term-ref",
                    format!(
                        "GTS {role} references segment-{segment_index} term id {gts_id}, \
                         which no `term` event introduced"
                    ),
                )
                .with_location(
                    RdfLocation::logical("gts:sink")
                        .with_gts_segment(segment_index)
                        .with_gts_term(gts_id),
                )
            })
    }
}

/// Build an [`RdfLiteral`] from a GTS literal term, resolving its datatype id
/// THROUGH the current segment's already-interned terms.
///
/// GTS cannot carry RDF 1.2 base direction (its `Term` has no direction slot), so
/// `direction` is always `None` on this path — a known, ledgered projection limit.
fn literal_from_term(
    importer: &SinkImporter,
    segment_index: usize,
    term: &Term,
) -> Result<RdfLiteral, RdfDiagnostic> {
    let datatype = match term.datatype {
        Some(dt_gts_id) => {
            let dt_id = importer.resolve(segment_index, dt_gts_id, "literal datatype")?;
            match importer.builder.term(dt_id) {
                super::term::InternedTerm::Iri(iri) => Some(iri.to_string()),
                other => {
                    return Err(RdfDiagnostic::error(
                        "rdf-ir-literal-datatype-not-iri",
                        format!("GTS literal datatype must resolve to an IRI, got {other:?}"),
                    )
                    .with_location(
                        RdfLocation::logical("gts:sink")
                            .with_gts_segment(segment_index)
                            .with_gts_term(dt_gts_id),
                    ));
                }
            }
        }
        None => None,
    };
    Ok(RdfLiteral {
        lexical_form: term.value.clone().unwrap_or_default(),
        datatype,
        language: term.lang.clone(),
        direction: None,
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Convert a CBOR [`Value`] into the crate's [`RdfMetadataValue`].
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
                .map(|(key, value)| {
                    let key = match key {
                        Value::Text(text) => text.clone(),
                        other => format!("{other:?}"),
                    };
                    (key, metadata_value_from_cbor(value))
                })
                .collect(),
        ),
        other => RdfMetadataValue::Opaque(format!("{other:?}")),
    }
}

impl StreamingSink for SinkImporter {
    fn term(&mut self, segment_index: usize, gts_term_id: usize, term: &Term) {
        if self.error.is_some() {
            return;
        }
        let our_id = match term.kind {
            TermKind::Iri => {
                let Some(iri) = term.value.clone().filter(|value| !value.is_empty()) else {
                    self.fail(
                        RdfDiagnostic::error(
                            "rdf-ir-iri-missing-value",
                            "GTS IRI term requires a non-empty value",
                        )
                        .with_location(
                            RdfLocation::logical("gts:sink")
                                .with_gts_segment(segment_index)
                                .with_gts_term(gts_term_id),
                        ),
                    );
                    return;
                };
                self.builder.intern_iri(iri)
            }
            // Per-segment scope isolation (C0.2): scope = segment_index + 1 so the
            // SAME blank label in different segments interns to DISTINCT ids, while
            // scope 0 stays reserved for the default/global scope.
            TermKind::Bnode => {
                let label = term
                    .value
                    .clone()
                    .unwrap_or_else(|| format!("gts_bnode_{segment_index}_{gts_term_id}"));
                let scope = BlankScope(segment_index as u32 + 1);
                self.builder.intern_blank(label, scope)
            }
            TermKind::Literal => match literal_from_term(self, segment_index, term) {
                Ok(literal) => self.builder.intern_literal(literal),
                Err(diagnostic) => {
                    self.fail(diagnostic);
                    return;
                }
            },
            TermKind::Triple => {
                let Some(reifier_gts_id) = term.reifier else {
                    self.fail(
                        RdfDiagnostic::error(
                            "rdf-ir-unbound-triple-term",
                            "GTS triple term has no reifier binding",
                        )
                        .with_location(
                            RdfLocation::logical("gts:sink")
                                .with_gts_segment(segment_index)
                                .with_gts_term(gts_term_id),
                        ),
                    );
                    return;
                };
                // The reifier names the (s, p, o) of this quoted triple. By the
                // time a `term` of kind Triple is delivered, its reifier and the
                // triple's components have already been delivered as terms in this
                // segment, so they are present in the segment remap. We resolve the
                // (s, p, o) THROUGH the reifier event recorded earlier.
                match self.resolve_triple_components(segment_index, reifier_gts_id, 0) {
                    Ok((s, p, o)) => self.builder.intern_triple(s, p, o),
                    Err(diagnostic) => {
                        self.fail(diagnostic);
                        return;
                    }
                }
            }
        };
        self.segment_map(segment_index).insert(gts_term_id, our_id);
    }

    fn quad(&mut self, segment_index: usize, quad: Quad) {
        if self.error.is_some() {
            return;
        }
        let (s, p, o, g) = quad;
        let resolved = (|| {
            let s = self.resolve(segment_index, s, "quad subject")?;
            let p = self.resolve(segment_index, p, "quad predicate")?;
            let o = self.resolve(segment_index, o, "quad object")?;
            let g = match g {
                Some(g) => Some(self.resolve(segment_index, g, "quad graph name")?),
                None => None,
            };
            Ok::<_, RdfDiagnostic>((s, p, o, g))
        })();
        match resolved {
            Ok((s, p, o, g)) => self.builder.push_quad(s, p, o, g),
            Err(diagnostic) => self.fail(diagnostic),
        }
    }

    fn reifier(&mut self, segment_index: usize, reifier: usize, triple: gmeow_gts::model::Triple3) {
        if self.error.is_some() {
            return;
        }
        // Record the reifier → (s, p, o) binding for this segment so a later
        // Triple term can resolve its components, and bind the reifier resource to
        // the interned triple term in the IR.
        self.reifier_bindings_insert(segment_index, reifier, triple);

        let resolved = (|| {
            let reifier_id = self.resolve(segment_index, reifier, "reifier")?;
            let (s, p, o) = self.resolve_triple_components(segment_index, reifier, 0)?;
            Ok::<_, RdfDiagnostic>((reifier_id, s, p, o))
        })();
        match resolved {
            Ok((reifier_id, s, p, o)) => {
                let triple_term = self.builder.intern_triple(s, p, o);
                self.builder.push_reifier(reifier_id, triple_term);
            }
            Err(diagnostic) => self.fail(diagnostic),
        }
    }

    fn annotation(&mut self, segment_index: usize, annotation: gmeow_gts::model::Triple3) {
        if self.error.is_some() {
            return;
        }
        let (r, p, v) = annotation;
        let resolved = (|| {
            let r = self.resolve(segment_index, r, "annotation reifier")?;
            let p = self.resolve(segment_index, p, "annotation predicate")?;
            let v = self.resolve(segment_index, v, "annotation object")?;
            Ok::<_, RdfDiagnostic>((r, p, v))
        })();
        match resolved {
            Ok((r, p, v)) => self.builder.push_annotation(r, p, v),
            Err(diagnostic) => self.fail(diagnostic),
        }
    }

    fn suppression(&mut self, _segment_index: usize, suppression: &Suppression) {
        self.lookaside.suppressions.push(RdfSuppressionRecord {
            reason: suppression.reason.clone(),
            // `by` is a segment-local term id; we record it as a display hint only,
            // never as a cross-dataset id (C0.8).
            by: suppression.by.map(|term_id| format!("term#{term_id}")),
            targets: suppression
                .targets
                .iter()
                .map(metadata_value_from_cbor)
                .collect(),
        });
    }

    fn blob(&mut self, _segment_index: usize, digest: &str, meta: Option<&Value>) {
        let metadata = match meta.map(metadata_value_from_cbor) {
            Some(RdfMetadataValue::Map(map)) => map,
            Some(value) => {
                let mut map = std::collections::BTreeMap::new();
                map.insert("value".to_owned(), value);
                map
            }
            None => std::collections::BTreeMap::new(),
        };
        let media_type = metadata
            .get("mt")
            .and_then(RdfMetadataValue::as_text)
            .map(str::to_owned);
        let representation = metadata
            .get("rep")
            .and_then(RdfMetadataValue::as_text)
            .map(str::to_owned);
        self.lookaside.blobs.push(crate::RdfBlobRecord {
            digest: digest.to_owned(),
            media_type,
            representation,
            decoded_len: None,
            metadata,
        });
    }

    fn opaque(&mut self, _segment_index: usize, opaque: &OpaqueNode) {
        self.lookaside.opaque_nodes.push(RdfOpaqueNodeRecord {
            id: hex_bytes(&opaque.id),
            frame_type: opaque.frame_type.clone(),
            reason: opaque.reason.clone(),
            signature_status: opaque.sigstat.clone(),
            public_metadata: opaque.pub_meta.as_ref().map(metadata_value_from_cbor),
        });
    }

    fn signature(&mut self, _segment_index: usize, signature: &Signature) {
        self.lookaside.signatures.push(RdfSignatureRecord {
            frame_id: hex_bytes(&signature.frame_id),
            key_id: signature.kid.clone(),
            status: signature.status.clone(),
            has_cose: signature.cose.is_some(),
        });
    }

    fn segment_head(&mut self, segment_index: usize, head: &[u8]) {
        // Grow/patch the per-segment record with its head id.
        self.ensure_segment_record(segment_index).head = Some(hex_bytes(head));
    }

    fn streamable_layout(&mut self, segment_index: usize, info: &StreamableInfo) {
        let record = self.ensure_segment_record(segment_index);
        record.claimed_streamable = info.claimed;
        record.covered = info.covered;
        record.tail = info.tail;
    }

    fn diagnostic(&mut self, diagnostic: &gmeow_gts::model::Diagnostic) {
        // A reader diagnostic is a hard fold failure on the IR path (the IR is the
        // authority, no degraded fold). Record the first as the deferred error.
        self.fail(
            RdfDiagnostic::error(
                "rdf-ir-gts-fold-diagnostic",
                format!(
                    "GTS fold diagnostic {}: {}",
                    diagnostic.code, diagnostic.detail
                ),
            )
            .with_location({
                let location = RdfLocation::logical("gts:sink");
                match diagnostic.frame_index {
                    Some(frame_index) => location.with_gts_frame(frame_index),
                    None => location,
                }
            }),
        );
    }
}

impl SinkImporter {
    /// Per-segment reifier bindings (`reifier gts-id → (s, p, o) gts-ids`), so a
    /// Triple term delivered later can recover its components. Stored on the
    /// importer as a side table keyed by `(segment_index, reifier)`.
    fn reifier_bindings_insert(
        &mut self,
        segment_index: usize,
        reifier: usize,
        triple: gmeow_gts::model::Triple3,
    ) {
        self.reifier_bindings
            .insert((segment_index, reifier), triple);
    }

    /// Resolve the `(s, p, o)` of the triple a reifier binds, THROUGH this
    /// segment's remap, with a depth bound against cyclic quoted triples.
    fn resolve_triple_components(
        &self,
        segment_index: usize,
        reifier: usize,
        depth: usize,
    ) -> Result<(TermId, TermId, TermId), RdfDiagnostic> {
        if depth > MAX_GTS_TERM_NESTING_DEPTH {
            return Err(RdfDiagnostic::error(
                "rdf-ir-term-nesting-limit",
                "GTS triple-term nesting depth limit exceeded",
            )
            .with_location(
                RdfLocation::logical("gts:sink")
                    .with_gts_segment(segment_index)
                    .with_gts_reifier(reifier),
            ));
        }
        let Some(&(s, p, o)) = self.reifier_bindings.get(&(segment_index, reifier)) else {
            return Err(RdfDiagnostic::error(
                "rdf-ir-missing-reifier-binding",
                format!(
                    "GTS triple term references reifier {reifier} in segment \
                     {segment_index} with no recorded binding"
                ),
            )
            .with_location(
                RdfLocation::logical("gts:sink")
                    .with_gts_segment(segment_index)
                    .with_gts_reifier(reifier),
            ));
        };
        let s = self.resolve(segment_index, s, "reified subject")?;
        let p = self.resolve(segment_index, p, "reified predicate")?;
        let o = self.resolve(segment_index, o, "reified object")?;
        Ok((s, p, o))
    }

    /// Ensure a [`RdfSegmentRecord`] exists for `segment_index`, returning it.
    fn ensure_segment_record(&mut self, segment_index: usize) -> &mut RdfSegmentRecord {
        if let Some(position) = self
            .lookaside
            .segments
            .iter()
            .position(|record| record.index == segment_index)
        {
            return &mut self.lookaside.segments[position];
        }
        self.lookaside.segments.push(RdfSegmentRecord {
            index: segment_index,
            head: None,
            profile: None,
            claimed_streamable: false,
            covered: 0,
            tail: 0,
        });
        self.lookaside
            .segments
            .last_mut()
            .expect("segment record just pushed")
    }
}

/// The authoritative GTS ingestion path: folds GTS bytes into an [`RdfBundle`],
/// preserving per-segment blank-node scope (C2.a).
///
/// Drives [`gmeow_gts::reader::read_to_sink`] with `allow_segments = true` so a
/// multi-segment file is delivered as per-segment events (the only place segment
/// identity survives). Any reader diagnostic or dangling term reference is a HARD
/// failure (`Err`); on success the interned terms are frozen via
/// [`RdfDatasetBuilder::freeze`] and paired with the accumulated envelope.
pub fn import_gts_events(bytes: &[u8]) -> Result<RdfBundle, RdfDiagnostic> {
    let mut importer = SinkImporter::new();
    let _ = gmeow_gts::reader::read_to_sink(bytes, true, None, &mut importer);

    if let Some(error) = importer.error {
        return Err(error);
    }

    let lookaside = importer.lookaside;
    let dataset = importer.builder.freeze()?;
    Ok(RdfBundle::new(dataset, RdfEnvelope::new(lookaside)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::term::InternedTerm;
    use crate::ir::TermId;
    use gmeow_gts::model::Term;
    use gmeow_gts::writer::Writer;

    fn iri_term(value: &str) -> Term {
        Term {
            kind: TermKind::Iri,
            value: Some(value.to_owned()),
            datatype: None,
            lang: None,
            reifier: None,
        }
    }

    fn blank_term(label: &str) -> Term {
        Term {
            kind: TermKind::Bnode,
            value: Some(label.to_owned()),
            datatype: None,
            lang: None,
            reifier: None,
        }
    }

    /// Resolve `our_id` back to its interned blank `(label, scope)` for assertions.
    fn blank_scope(importer: &SinkImporter, id: TermId) -> (String, BlankScope) {
        match importer.builder.term(id) {
            InternedTerm::Blank { label, scope } => (label.to_string(), *scope),
            other => panic!("expected blank node, got {other:?}"),
        }
    }

    /// GATE 2 (a) — multi-segment blank-node scope isolation, driven DIRECTLY
    /// through the `StreamingSink` callbacks (no GTS bytes needed).
    ///
    /// Segment 0 and segment 1 both carry a blank labelled `b1` (DIFFERENT nodes)
    /// and an IRI `ex:s` (the SAME node). After freeze the two `b1` blanks MUST be
    /// distinct ids (per-segment scope) while `ex:s` MUST be one shared id.
    #[test]
    fn gate2_multi_segment_blank_scope_isolation_direct() {
        let mut importer = SinkImporter::new();

        // Segment 0: term 0 = ex:s, term 1 = ex:p, term 2 = _:b1, quad (s p b1).
        importer.term(0, 0, &iri_term("http://example.org/s"));
        importer.term(0, 1, &iri_term("http://example.org/p"));
        importer.term(0, 2, &blank_term("b1"));
        importer.quad(0, (0, 1, 2, None));

        // Segment 1: term 0 = ex:s (same IRI value), term 1 = ex:p2, term 2 = _:b1
        // (SAME label, DIFFERENT node), quad (s p2 b1).
        importer.term(1, 0, &iri_term("http://example.org/s"));
        importer.term(1, 1, &iri_term("http://example.org/p2"));
        importer.term(1, 2, &blank_term("b1"));
        importer.quad(1, (0, 1, 2, None));

        assert!(
            importer.error.is_none(),
            "no error expected: {:?}",
            importer.error
        );

        let seg0_s = importer.remaps[0][&0];
        let seg1_s = importer.remaps[1][&0];
        let seg0_b1 = importer.remaps[0][&2];
        let seg1_b1 = importer.remaps[1][&2];

        // The shared IRI interns to ONE id across both segments (value identity).
        assert_eq!(seg0_s, seg1_s, "ex:s is the same node across segments");

        // The same blank label in different segments interns to DISTINCT ids.
        assert_ne!(
            seg0_b1, seg1_b1,
            "_:b1 in segment 0 and segment 1 are DIFFERENT nodes (scope isolation)"
        );
        let (label0, scope0) = blank_scope(&importer, seg0_b1);
        let (label1, scope1) = blank_scope(&importer, seg1_b1);
        assert_eq!(label0, "b1");
        assert_eq!(label1, "b1");
        assert_eq!(scope0, BlankScope(1), "segment 0 → scope 1");
        assert_eq!(scope1, BlankScope(2), "segment 1 → scope 2");

        let dataset = importer.builder.freeze().expect("freeze");
        // 2 quads, distinct blanks: ex:s, ex:p, b1@s0, ex:p2, b1@s1 = 5 terms.
        assert_eq!(dataset.quad_count(), 2);
        assert_eq!(dataset.term_count(), 5);
    }

    /// A quad that references a GTS term id no `term` event introduced MUST surface
    /// as a hard `Err` (no silent skip).
    #[test]
    fn gate2_unknown_term_reference_is_err_direct() {
        let mut importer = SinkImporter::new();
        importer.term(0, 0, &iri_term("http://example.org/s"));
        importer.term(0, 1, &iri_term("http://example.org/p"));
        // Object id 9 was never introduced.
        importer.quad(0, (0, 1, 9, None));
        assert!(
            importer.error.is_some(),
            "dangling reference must defer an error"
        );
        let err = importer.error.unwrap();
        assert_eq!(err.code, "rdf-ir-dangling-term-ref");
    }

    /// Directional literals: GTS `Term` carries no base direction, so the sink path
    /// yields `direction == None`, but lexical form, datatype, and language survive.
    #[test]
    fn directional_literal_lexical_lang_survive_sink_path() {
        let mut importer = SinkImporter::new();
        importer.term(0, 0, &iri_term("http://example.org/s"));
        importer.term(0, 1, &iri_term("http://example.org/p"));
        importer.term(
            0,
            2,
            &Term {
                kind: TermKind::Literal,
                value: Some("Bonjour".to_owned()),
                datatype: None,
                lang: Some("FR".to_owned()),
                reifier: None,
            },
        );
        importer.quad(0, (0, 1, 2, None));
        assert!(importer.error.is_none());

        let lit_id = importer.remaps[0][&2];
        let dataset = importer.builder.freeze().expect("freeze");
        match dataset.resolve(lit_id) {
            crate::ir::TermRef::Literal {
                lexical,
                language,
                direction,
                ..
            } => {
                assert_eq!(lexical, "Bonjour", "lexical preserved verbatim");
                assert_eq!(language, Some("fr"), "language lowercased per C0.1");
                assert_eq!(direction, None, "GTS cannot carry base direction");
            }
            other => panic!("expected literal, got {other:?}"),
        }
    }

    /// A nested quoted-triple term survives the sink path: the inner triple is an
    /// object position of the outer triple.
    #[test]
    fn nested_triple_term_survives_sink_path() {
        let mut importer = SinkImporter::new();
        // Inner triple (ex:a ex:p ex:b) reified by reifier r0; outer triple
        // (ex:a ex:asserts <<inner>>) reified by reifier r1.
        importer.term(0, 0, &iri_term("http://example.org/a"));
        importer.term(0, 1, &iri_term("http://example.org/p"));
        importer.term(0, 2, &iri_term("http://example.org/b"));
        importer.term(0, 3, &iri_term("http://example.org/r0"));
        importer.reifier(0, 3, (0, 1, 2));
        // Inner triple TERM bound to reifier r0 (gts id 3).
        importer.term(
            0,
            4,
            &Term {
                kind: TermKind::Triple,
                value: None,
                datatype: None,
                lang: None,
                reifier: Some(3),
            },
        );
        importer.term(0, 5, &iri_term("http://example.org/asserts"));
        importer.term(0, 6, &iri_term("http://example.org/r1"));
        importer.reifier(0, 6, (0, 5, 4));
        importer.term(
            0,
            7,
            &Term {
                kind: TermKind::Triple,
                value: None,
                datatype: None,
                lang: None,
                reifier: Some(6),
            },
        );
        importer.quad(0, (0, 5, 7, None));
        assert!(importer.error.is_none(), "{:?}", importer.error);

        let inner = importer.remaps[0][&4];
        let outer = importer.remaps[0][&7];
        let dataset = importer.builder.freeze().expect("freeze");
        match dataset.resolve(outer) {
            crate::ir::TermRef::Triple { o, .. } => {
                assert_eq!(o, inner, "outer triple's object IS the inner triple term");
            }
            other => panic!("expected triple term, got {other:?}"),
        }
    }

    /// Multiple distinct reifiers binding ONE triple all survive.
    #[test]
    fn multiple_reifiers_for_one_triple_survive_sink_path() {
        let mut importer = SinkImporter::new();
        importer.term(0, 0, &iri_term("http://example.org/s"));
        importer.term(0, 1, &iri_term("http://example.org/p"));
        importer.term(0, 2, &iri_term("http://example.org/o"));
        importer.term(0, 3, &iri_term("http://example.org/r1"));
        importer.term(0, 4, &iri_term("http://example.org/r2"));
        importer.reifier(0, 3, (0, 1, 2));
        importer.reifier(0, 4, (0, 1, 2));
        assert!(importer.error.is_none());

        let dataset = importer.builder.freeze().expect("freeze");
        let reifiers: Vec<_> = dataset.reifiers().collect();
        assert_eq!(reifiers.len(), 2, "two distinct reifiers survive");
        // Both bind the same interned triple term.
        assert_eq!(reifiers[0].1, reifiers[1].1, "same triple term bound twice");
    }

    /// GATE 2 (b) — REAL multi-segment GTS bytes. Two `Writer::deterministic`
    /// segments are concatenated (the reader splits at header-shaped items), each
    /// reusing the blank label `b1` for a DIFFERENT node and sharing `ex:s`.
    /// `import_gts_events` MUST preserve scope: distinct `b1`, shared `ex:s`.
    #[test]
    fn gate2_multi_segment_blank_scope_isolation_roundtrip() {
        use gmeow_gts::model::{Graph, Term as GtsTerm, TermKind as GtsKind};

        fn segment(predicate: &str) -> Graph {
            let mut graph = Graph::default();
            graph.terms.push(GtsTerm {
                kind: GtsKind::Iri,
                value: Some("http://example.org/s".to_owned()),
                datatype: None,
                lang: None,
                reifier: None,
            });
            graph.terms.push(GtsTerm {
                kind: GtsKind::Iri,
                value: Some(predicate.to_owned()),
                datatype: None,
                lang: None,
                reifier: None,
            });
            graph.terms.push(GtsTerm {
                kind: GtsKind::Bnode,
                value: Some("b1".to_owned()),
                datatype: None,
                lang: None,
                reifier: None,
            });
            graph.quads.push((0, 1, 2, None));
            graph
        }

        let seg0 = Writer::deterministic(&segment("http://example.org/p"), "gmeow-rdf-test")
            .expect("segment 0 writer");
        let seg1 = Writer::deterministic(&segment("http://example.org/p2"), "gmeow-rdf-test")
            .expect("segment 1 writer");
        let mut bytes = seg0.to_bytes();
        bytes.extend_from_slice(&seg1.to_bytes());

        let bundle = import_gts_events(&bytes).expect("two-segment import");
        let dataset = &bundle.dataset;

        // Collect the blank-node (label, scope) pairs and the IRI subjects.
        let mut blank_scopes: Vec<(String, BlankScope)> = Vec::new();
        let mut subjects: Vec<&str> = Vec::new();
        for quad in dataset.quad_refs() {
            if let crate::ir::TermRef::Iri(iri) = quad.s {
                subjects.push(iri);
            }
            if let crate::ir::TermRef::Blank { label, scope } = quad.o {
                blank_scopes.push((label.to_owned(), scope));
            }
        }

        assert_eq!(dataset.quad_count(), 2, "two quads, one per segment");
        // ex:s appears as subject in BOTH quads but is one interned term.
        assert_eq!(subjects.len(), 2);
        assert!(subjects.iter().all(|s| *s == "http://example.org/s"));
        // Both quad objects are blank `b1`, but in DISTINCT scopes.
        assert_eq!(blank_scopes.len(), 2);
        assert!(blank_scopes.iter().all(|(label, _)| label == "b1"));
        let scope_a = blank_scopes[0].1;
        let scope_b = blank_scopes[1].1;
        assert_ne!(
            scope_a, scope_b,
            "the two _:b1 blanks are in distinct scopes"
        );

        // term_count: ex:s, ex:p, b1@seg0, ex:p2, b1@seg1 = 5 distinct terms.
        assert_eq!(dataset.term_count(), 5);
    }

    /// `import_gts_events` surfaces a malformed-bytes fold diagnostic as `Err`.
    #[test]
    fn import_rejects_malformed_bytes() {
        let err = import_gts_events(b"not a valid gts file").expect_err("must fail");
        assert_eq!(err.code, "rdf-ir-gts-fold-diagnostic");
    }
}
