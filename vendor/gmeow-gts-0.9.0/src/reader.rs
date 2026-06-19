// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The GTS reader: parse a CBOR Sequence, verify the id/prev chain, fold the
//! log — mirror of `src/gmeow_tools/gts/reader.py`.
//!
//! Implements the Baseline Reader contract (§2.1): chain verification (§9.1),
//! the value-union fold (§7.5), opaque/damaged degradation (§7.6), torn-append
//! detection (§3), and the canonical diagnostics (§2.3). The default
//! [`read`] path carries no content keys: `sig` frames record as
//! `"unverified"` and `encrypt`-class frames degrade to `missing-key` opaque
//! nodes. Callers that hold content keys can use [`read_with_options`].

use std::collections::{HashMap, HashSet};

use ciborium::value::Value;

use crate::codec::{decode_chain, decode_chain_with_decrypt, Codec, CodecError};
use crate::mmr;
use crate::model::{
    Diagnostic, Graph, OpaqueNode, Quad, Signature, StreamableInfo, Suppression, Term, TermKind,
    Triple3,
};
use crate::stream::DIGEST as STREAM_DIGEST;
use crate::wire::{
    content_id, digest_str, header_id, hex, iter_items, map_get, unwrap_header, MAGIC, VERSION,
};

fn as_i128(v: &Value) -> Option<i128> {
    if let Value::Integer(i) = v {
        Some(i128::from(*i))
    } else {
        None
    }
}

/// Coerce a value to a non-negative index, else `None` (Python `_as_int`).
fn as_idx(v: &Value) -> Option<usize> {
    as_i128(v).and_then(|n| usize::try_from(n).ok())
}

fn as_text(v: &Value) -> Option<&str> {
    if let Value::Text(t) = v {
        Some(t)
    } else {
        None
    }
}

fn text_or<'a>(v: Option<&'a Value>, default: &'a str) -> &'a str {
    v.and_then(as_text).unwrap_or(default)
}

/// Python-style rendering of an optional graph slot for diagnostic details.
fn fmt_opt(g: Option<usize>) -> String {
    match g {
        Some(v) => v.to_string(),
        None => "None".to_string(),
    }
}

fn diag_code_for(reason: &str) -> &'static str {
    match reason {
        "missing-key" => "MissingKey",
        _ => "UnknownCodec",
    }
}

fn pub_digest(value: &Value) -> Option<String> {
    let Value::Map(entries) = value else {
        return None;
    };
    match map_get(entries, "digest") {
        Some(Value::Text(text)) if text.starts_with("blake3:") => Some(text.clone()),
        Some(Value::Text(text)) => Some(format!("blake3:{text}")),
        Some(Value::Bytes(bytes)) if bytes.len() == 32 => Some(format!("blake3:{}", hex(bytes))),
        _ => None,
    }
}

enum PayloadError {
    /// Missing capability — degrade to an opaque node with this reason.
    Unavailable {
        reason: &'static str,
        detail: String,
    },
    /// Anything else — the frame is damaged.
    Damaged(String),
}

#[derive(Clone, Debug)]
struct IndexRecord {
    abs_index: usize,
    count: usize,
    head: Vec<u8>,
    mmr: Option<Vec<u8>>,
}

impl From<CodecError> for PayloadError {
    fn from(e: CodecError) -> Self {
        match e {
            CodecError::Unavailable { reason, detail } => {
                PayloadError::Unavailable { reason, detail }
            }
            CodecError::Failed(detail) => PayloadError::Damaged(detail),
        }
    }
}

fn decrypt_codec(
    codec: &Codec,
    data: &[u8],
    content_key: &ContentKeyResolver<'_>,
) -> Result<Vec<u8>, CodecError> {
    if codec.name != "cose-encrypt0" {
        return Err(CodecError::Unavailable {
            reason: "missing-key",
            detail: format!("no decryptor for encrypt codec '{}'", codec.name),
        });
    }
    crate::cose::decrypt0(data, |kid| content_key(kid)).map_err(|err| CodecError::Unavailable {
        reason: "missing-key",
        detail: format!("{} decrypt failed: {err}", codec.name),
    })
}

/// Final state returned by [`read_to_sink`].
///
/// The streaming API emits rows as segment-local events and does not construct
/// the final union [`Graph`]. This result carries the observable file-level
/// state that callers still need for verification and freshness checks.
#[derive(Clone, Debug, Default)]
pub struct StreamingReadResult {
    /// Reader diagnostics in the same order as [`read`] would report them.
    pub diagnostics: Vec<Diagnostic>,
    /// Ordered per-segment head ids.
    pub segment_heads: Vec<Vec<u8>>,
    /// Ordered per-segment profile names.
    pub segment_profiles: Vec<String>,
    /// Ordered per-segment streamable-layout state.
    pub segment_streamable: Vec<StreamableInfo>,
    /// Byte offset of a torn trailing CBOR item, if present.
    pub torn: Option<usize>,
}

/// Sink for [`read_to_sink`] events.
///
/// Term ids in events are segment-local ids. A sink that needs a file-level
/// union should intern by value using the same rules as [`read`]; sinks that
/// project rows into a table can usually consume the segment-local ids directly
/// with the segment index as the scope.
pub trait StreamingSink {
    /// Accepted term row.
    fn term(&mut self, _segment_index: usize, _term_id: usize, _term: &Term) {}
    /// Accepted quad row.
    fn quad(&mut self, _segment_index: usize, _quad: Quad) {}
    /// Accepted reifier binding.
    fn reifier(&mut self, _segment_index: usize, _reifier: usize, _triple: Triple3) {}
    /// Accepted annotation row.
    fn annotation(&mut self, _segment_index: usize, _annotation: Triple3) {}
    /// Accepted suppression directive.
    fn suppression(&mut self, _segment_index: usize, _suppression: &Suppression) {}
    /// Accepted inline blob digest and declared metadata.
    fn blob(&mut self, _segment_index: usize, _digest: &str, _meta: Option<&Value>) {}
    /// Opaque frame produced by unknown, encrypted, or damaged payloads.
    fn opaque(&mut self, _segment_index: usize, _opaque: &OpaqueNode) {}
    /// Signature status observed on a frame.
    fn signature(&mut self, _segment_index: usize, _signature: &Signature) {}
    /// Reader diagnostic.
    fn diagnostic(&mut self, _diagnostic: &Diagnostic) {}
    /// Completed segment head.
    fn segment_head(&mut self, _segment_index: usize, _head: &[u8]) {}
    /// Completed segment streamable-layout state.
    fn streamable_layout(&mut self, _segment_index: usize, _info: &StreamableInfo) {}
}

/// Resolves a 32-byte content key by COSE recipient `kid`.
pub type ContentKeyResolver<'a> = dyn Fn(&str) -> Option<[u8; 32]> + 'a;

/// Reader options for keyed/full-reader paths.
#[derive(Clone, Copy, Default)]
pub struct ReadOptions<'a> {
    /// Permit multi-segment `cat` composition.
    pub allow_segments: bool,
    /// Expected last segment head for freshness/truncation checks.
    pub expected_head: Option<&'a [u8]>,
    /// Optional content-key provider for `COSE_Encrypt0` payloads.
    pub content_key: Option<&'a ContentKeyResolver<'a>>,
}

impl<'a> ReadOptions<'a> {
    /// Build options matching the legacy [`read`] signature.
    pub fn new(allow_segments: bool, expected_head: Option<&'a [u8]>) -> Self {
        Self {
            allow_segments,
            expected_head,
            content_key: None,
        }
    }

    /// Add a content-key provider for decrypting `COSE_Encrypt0` frames.
    pub fn with_content_key(mut self, resolver: &'a ContentKeyResolver<'a>) -> Self {
        self.content_key = Some(resolver);
        self
    }
}

fn push_diagnostic(
    g: &mut Graph,
    sink: &mut Option<&mut dyn StreamingSink>,
    diagnostic: Diagnostic,
) {
    if let Some(sink) = sink.as_deref_mut() {
        sink.diagnostic(&diagnostic);
    }
    g.diagnostics.push(diagnostic);
}

fn push_result_diagnostic(
    result: &mut StreamingReadResult,
    sink: &mut dyn StreamingSink,
    diagnostic: Diagnostic,
) {
    sink.diagnostic(&diagnostic);
    result.diagnostics.push(diagnostic);
}

fn absorb_segment_result(result: &mut StreamingReadResult, segment: &Graph) {
    result
        .diagnostics
        .extend(segment.diagnostics.iter().cloned());
    result
        .segment_heads
        .extend(segment.segment_heads.iter().cloned());
    result
        .segment_profiles
        .extend(segment.segment_profiles.iter().cloned());
    result
        .segment_streamable
        .extend(segment.segment_streamable.iter().cloned());
}

/// Mutable fold state; one per segment (and shared by the snapshot handler).
struct Folder<'g, 's, 'k> {
    g: &'g mut Graph,
    sink: Option<&'s mut dyn StreamingSink>,
    content_key: Option<&'k ContentKeyResolver<'k>>,
    segment_index: usize,
    catalog: HashMap<i128, Codec>,
    // Layout-state bookkeeping (§3.3): intact index frames seen, digests the
    // graph has described via stream:digest so far, and each inline blob's
    // arrival (frame index, digest, was-it-described-at-arrival).
    index_records: Vec<IndexRecord>,
    described: HashSet<String>,
    blob_events: Vec<(usize, String, bool)>,
}

impl Folder<'_, '_, '_> {
    fn with_sink(&mut self, f: impl FnOnce(usize, &mut dyn StreamingSink)) {
        if let Some(sink) = self.sink.as_deref_mut() {
            f(self.segment_index, sink);
        }
    }

    fn diag(&mut self, code: &str, detail: String, index: Option<usize>) {
        push_diagnostic(
            self.g,
            &mut self.sink,
            Diagnostic {
                code: code.to_string(),
                detail,
                frame_index: index,
            },
        );
    }

    fn emit_blob(&mut self, digest: &str) {
        let meta = self
            .g
            .blob_meta
            .iter()
            .find(|(stored, _)| stored == digest)
            .map(|(_, meta)| meta.clone());
        self.with_sink(|segment_index, sink| sink.blob(segment_index, digest, meta.as_ref()));
    }

    fn push_opaque(&mut self, opaque: OpaqueNode) {
        self.with_sink(|segment_index, sink| sink.opaque(segment_index, &opaque));
        self.g.opaque.push(opaque);
    }

    fn push_signature(&mut self, signature: Signature) {
        self.with_sink(|segment_index, sink| sink.signature(segment_index, &signature));
        self.g.signatures.push(signature);
    }

    fn resolve_codecs(&self, ids: &[Value]) -> Result<Vec<Codec>, PayloadError> {
        let mut chain = Vec::with_capacity(ids.len());
        for cid in ids {
            let codec = as_i128(cid).and_then(|c| self.catalog.get(&c));
            match codec {
                Some(c) => chain.push(c.clone()),
                None => {
                    return Err(PayloadError::Unavailable {
                        reason: "unknown-codec",
                        detail: format!("codec id {cid:?} not in catalog"),
                    })
                }
            }
        }
        Ok(chain)
    }

    /// Resolve a frame's logical payload (§6.1); error on missing capability.
    fn payload(&self, frame: &[(Value, Value)], blob: bool) -> Result<Value, PayloadError> {
        let d = map_get(frame, "d");
        if let Some(Value::Array(ids)) = map_get(frame, "x") {
            if !ids.is_empty() {
                let Some(Value::Bytes(db)) = d else {
                    return Err(PayloadError::Damaged(
                        "transformed frame 'd' must be a byte string".to_string(),
                    ));
                };
                let chain = self.resolve_codecs(ids)?;
                let decoded = if let Some(content_key) = self.content_key {
                    let decrypt =
                        |codec: &Codec, data: &[u8]| decrypt_codec(codec, data, content_key);
                    decode_chain_with_decrypt(&chain, db, Some(&decrypt))?
                } else {
                    decode_chain(&chain, db)?
                };
                if blob {
                    return Ok(Value::Bytes(decoded));
                }
                return ciborium::de::from_reader(&decoded[..])
                    .map_err(|e| PayloadError::Damaged(e.to_string()));
            }
        }
        Ok(d.cloned().unwrap_or(Value::Null))
    }

    /// Fold one already-verified frame into the graph.
    ///
    /// Total: a missing capability degrades to an opaque node, and a corrupt
    /// payload degrades to a `damaged` opaque node — the reader never aborts.
    fn fold_frame(&mut self, frame: &[(Value, Value)], index: usize) {
        let ftype = text_or(map_get(frame, "t"), "").to_string();
        if ftype == "blob" {
            self.h_blob_frame(frame, index);
            return;
        }
        let payload = match self.payload(frame, false) {
            Err(PayloadError::Unavailable { reason, detail }) => {
                self.opaque(frame, &ftype, reason);
                self.diag(diag_code_for(reason), detail, Some(index));
                return;
            }
            Err(PayloadError::Damaged(detail)) => {
                self.opaque(frame, &ftype, "damaged");
                self.diag(
                    "DamagedFrame",
                    format!("payload decode failed: {detail}"),
                    Some(index),
                );
                return;
            }
            Ok(p) => p,
        };
        match ftype.as_str() {
            "terms" => self.h_terms(&payload, index),
            "quads" => self.h_quads(&payload, index),
            "reifies" => self.h_reifies(&payload, index),
            "annot" => self.h_annot(&payload, index),
            "meta" => self.h_meta(&payload),
            "suppress" => self.h_suppress(&payload),
            "snapshot" => self.h_snapshot(&payload, index),
            "index" => self.h_index(&payload, index),
            "opaque" => self.h_opaque(&payload),
            _ => {
                self.opaque(frame, &ftype, "unknown-frame-type");
                self.diag(
                    "UnknownFrameType",
                    format!("unsupported frame type {ftype:?}"),
                    Some(index),
                );
            }
        }
    }

    // -- per-type handlers ---------------------------------------------------

    fn h_terms(&mut self, payload: &Value, index: usize) {
        let Value::Array(rows) = payload else { return };
        for raw in rows {
            let Value::Map(entries) = raw else { continue };
            let kind = TermKind::from_wire(map_get(entries, "k").and_then(as_i128));
            let value = map_get(entries, "v").and_then(as_text).map(str::to_string);
            let lang = map_get(entries, "l").and_then(as_text).map(str::to_string);
            let dt_raw = map_get(entries, "dt").and_then(as_i128);
            let rf_raw = map_get(entries, "rf").and_then(as_i128);
            let tid = self.g.terms.len() as i128;
            let term_id = self.g.terms.len();
            // Sanitise refs: dt/rf MUST name an already-introduced term
            // (§7.5). A forward/out-of-bounds ref is diagnosed and dropped,
            // so resolution and serialisation can never panic. (A negative
            // ref is dropped silently — same diagnostic surface as Python,
            // which only flags refs at-or-past the current id.)
            let sanitize = |r: Option<i128>| match r {
                Some(d) if (0..tid).contains(&d) => Some(d as usize),
                _ => None,
            };
            let dt = sanitize(dt_raw);
            let rf = sanitize(rf_raw);
            let out_of_range = |r: Option<i128>| matches!(r, Some(d) if d >= tid);
            if out_of_range(dt_raw) || out_of_range(rf_raw) {
                self.diag(
                    "ForwardReference",
                    format!("term {tid} has an out-of-range ref"),
                    Some(index),
                );
            }
            self.g.terms.push(Term {
                kind,
                value,
                datatype: dt,
                lang,
                reifier: rf,
            });
            if let Some(sink) = self.sink.as_deref_mut() {
                sink.term(self.segment_index, term_id, &self.g.terms[term_id]);
            }
        }
    }

    fn h_quads(&mut self, payload: &Value, index: usize) {
        let Value::Array(rows) = payload else { return };
        for row in rows {
            let Value::Array(items) = row else { continue };
            if items.len() < 3 {
                continue;
            }
            let (s, p, o) = (as_idx(&items[0]), as_idx(&items[1]), as_idx(&items[2]));
            let has_graph = items.len() >= 4;
            let gslot = if has_graph { as_idx(&items[3]) } else { None };
            if s.is_none() || p.is_none() || o.is_none() || (has_graph && gslot.is_none()) {
                self.diag(
                    "DamagedFrame",
                    "quad has non-integer term ids".to_string(),
                    Some(index),
                );
                continue;
            }
            let (s, p, o) = (s.unwrap(), p.unwrap(), o.unwrap());
            if !self.check_positions(s, p, o, gslot, index) {
                continue;
            }
            let quad = (s, p, o, gslot);
            self.g.quads.push(quad);
            self.with_sink(|segment_index, sink| sink.quad(segment_index, quad));
            // Layout bookkeeping (§3.3): a stream:digest quad describes an
            // upcoming manifestation — record the IOU for the blob check.
            if self.g.terms[p].value.as_deref() == Some(STREAM_DIGEST) {
                if let Some(obj) = &self.g.terms[o].value {
                    self.described.insert(obj.clone());
                }
            }
        }
    }

    fn h_reifies(&mut self, payload: &Value, index: usize) {
        let Value::Map(entries) = payload else { return };
        for (k, spo) in entries {
            let Some(rid) = as_i128(k) else { continue };
            let Value::Array(items) = spo else { continue };
            if items.len() != 3 {
                continue;
            }
            let (s, p, o) = (as_idx(&items[0]), as_idx(&items[1]), as_idx(&items[2]));
            let n = self.g.terms.len();
            let rid_ok = rid >= 0 && (rid as usize) < n;
            let spo_ok = matches!((s, p, o), (Some(s), Some(p), Some(o))
                if s < n && p < n && o < n);
            if !rid_ok || !spo_ok {
                self.diag(
                    "DamagedFrame",
                    format!("reifier {rid} has bad/out-of-range ids"),
                    Some(index),
                );
                continue;
            }
            let rid = rid as usize;
            let triple: Triple3 = (s.unwrap(), p.unwrap(), o.unwrap());
            if let Some(existing) = self.g.reifier(rid) {
                if existing != triple {
                    self.diag(
                        "ConflictingReifier",
                        format!("reifier {rid} rebound"),
                        Some(index),
                    );
                    continue; // keep the first binding
                }
            }
            self.g.set_reifier(rid, triple);
            self.with_sink(|segment_index, sink| sink.reifier(segment_index, rid, triple));
        }
    }

    fn h_annot(&mut self, payload: &Value, index: usize) {
        let Value::Array(rows) = payload else { return };
        for row in rows {
            let Value::Array(items) = row else { continue };
            if items.len() != 3 {
                continue;
            }
            let (r, p, v) = (as_idx(&items[0]), as_idx(&items[1]), as_idx(&items[2]));
            let n = self.g.terms.len();
            let ok = matches!((r, p, v), (Some(r), Some(p), Some(v))
                if r < n && p < n && v < n);
            if !ok {
                self.diag(
                    "DamagedFrame",
                    "annot row has bad/out-of-range ids".to_string(),
                    Some(index),
                );
                continue;
            }
            let (r, p, v) = (r.unwrap(), p.unwrap(), v.unwrap());
            if self.g.terms[p].kind != TermKind::Iri {
                self.diag(
                    "PositionConstraint",
                    format!("annot predicate {p} not an IRI"),
                    Some(index),
                );
                continue;
            }
            let annotation = (r, p, v);
            self.g.annotations.push(annotation);
            self.with_sink(|segment_index, sink| sink.annotation(segment_index, annotation));
        }
    }

    fn h_blob_frame(&mut self, frame: &[(Value, Value)], index: usize) {
        let d = map_get(frame, "d");
        let pub_meta = map_get(frame, "pub")
            .filter(|value| matches!(value, Value::Map(_)))
            .cloned();

        let chain = match map_get(frame, "x") {
            Some(Value::Array(ids)) if !ids.is_empty() => match self.resolve_codecs(ids) {
                Ok(chain) => chain,
                Err(PayloadError::Unavailable { reason, detail }) => {
                    self.opaque(frame, "blob", reason);
                    self.diag(diag_code_for(reason), detail, Some(index));
                    return;
                }
                Err(PayloadError::Damaged(detail)) => {
                    self.opaque(frame, "blob", "damaged");
                    self.diag(
                        "DamagedFrame",
                        format!("payload decode failed: {detail}"),
                        Some(index),
                    );
                    return;
                }
            },
            _ => Vec::new(),
        };

        if chain.iter().any(|codec| codec.cls == "encrypt") {
            match self.payload(frame, true) {
                Ok(Value::Bytes(bytes)) => {
                    let digest = digest_str(&bytes);
                    if let Some(meta) = pub_meta {
                        self.g.set_blob_meta(digest.clone(), meta);
                    }
                    self.blob_events.push((
                        index,
                        digest.clone(),
                        self.described.contains(&digest),
                    ));
                    self.g.set_blob(digest.clone(), bytes);
                    self.emit_blob(&digest);
                }
                Ok(_) => {}
                Err(PayloadError::Unavailable { reason, detail }) => {
                    self.opaque(frame, "blob", reason);
                    self.diag(diag_code_for(reason), detail, Some(index));
                }
                Err(PayloadError::Damaged(detail)) => {
                    self.opaque(frame, "blob", "damaged");
                    self.diag(
                        "DamagedFrame",
                        format!("payload decode failed: {detail}"),
                        Some(index),
                    );
                }
            }
            return;
        }

        if let Some(digest) = pub_meta.as_ref().and_then(pub_digest) {
            if let Some(meta) = pub_meta {
                self.g.set_blob_meta(digest.clone(), meta);
            }
            if let Some(Value::Bytes(raw)) = d {
                if chain.is_empty() {
                    self.g.set_blob(digest.clone(), raw.clone());
                } else {
                    self.g.set_lazy_blob(digest.clone(), raw.clone(), chain);
                }
            }
            self.blob_events
                .push((index, digest.clone(), self.described.contains(&digest)));
            self.emit_blob(&digest);
            return;
        }

        let Some(Value::Bytes(_)) = d else {
            return;
        };
        match self.payload(frame, true) {
            Ok(Value::Bytes(bytes)) => {
                let digest = digest_str(&bytes);
                if let Some(meta) = pub_meta {
                    self.g.set_blob_meta(digest.clone(), meta);
                }
                self.blob_events
                    .push((index, digest.clone(), self.described.contains(&digest)));
                self.g.set_blob(digest.clone(), bytes);
                self.emit_blob(&digest);
            }
            Ok(_) => {}
            Err(PayloadError::Unavailable { reason, detail }) => {
                self.opaque(frame, "blob", reason);
                self.diag(diag_code_for(reason), detail, Some(index));
            }
            Err(PayloadError::Damaged(detail)) => {
                self.opaque(frame, "blob", "damaged");
                self.diag(
                    "DamagedFrame",
                    format!("payload decode failed: {detail}"),
                    Some(index),
                );
            }
        }
    }

    fn h_meta(&mut self, payload: &Value) {
        if let Value::Map(entries) = payload {
            for (k, v) in entries {
                let key = as_text(k)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{k:?}"));
                self.g.set_meta(key, v.clone());
            }
        }
    }

    fn h_suppress(&mut self, payload: &Value) {
        let Value::Map(entries) = payload else { return };
        let Some(Value::Array(targets)) = map_get(entries, "targets") else {
            return;
        };
        let suppression = Suppression {
            targets: targets
                .iter()
                .filter(|t| matches!(t, Value::Map(_)))
                .cloned()
                .collect(),
            reason: map_get(entries, "reason")
                .and_then(as_text)
                .map(str::to_string),
            by: map_get(entries, "by").and_then(as_idx),
        };
        self.with_sink(|segment_index, sink| sink.suppression(segment_index, &suppression));
        self.g.suppressions.push(suppression);
    }

    /// Fold a self-contained snapshot (§10).
    ///
    /// Shifts the snapshot's local term ids into the outer id space and
    /// re-dispatches through the normal handlers, so a snapshot gets the SAME
    /// semantic checks as the equivalent streamed frames.
    fn h_snapshot(&mut self, payload: &Value, index: usize) {
        let Value::Map(entries) = payload else { return };
        let base = self.g.terms.len();
        // Shift a valid local id into the outer space; pass non-ints through
        // so the downstream handler's own checks reject them with diagnostics.
        let sh = |v: &Value| -> Value {
            match as_idx(v) {
                Some(iv) => Value::from((iv + base) as u64),
                None => v.clone(),
            }
        };
        let sh_row = |row: &Value| -> Value {
            match row {
                Value::Array(items) => Value::Array(items.iter().map(sh).collect()),
                other => other.clone(),
            }
        };

        if let Some(Value::Array(snap_terms)) = map_get(entries, "terms") {
            let shifted: Vec<Value> = snap_terms
                .iter()
                .map(|raw| match raw {
                    Value::Map(term_entries) => Value::Map(
                        term_entries
                            .iter()
                            .map(|(k, v)| {
                                if matches!(as_text(k), Some("dt") | Some("rf")) {
                                    (k.clone(), sh(v))
                                } else {
                                    (k.clone(), v.clone())
                                }
                            })
                            .collect(),
                    ),
                    other => other.clone(),
                })
                .collect();
            self.h_terms(&Value::Array(shifted), index);
        }
        if let Some(Value::Array(quads)) = map_get(entries, "quads") {
            self.h_quads(&Value::Array(quads.iter().map(sh_row).collect()), index);
        }
        if let Some(Value::Map(reifies)) = map_get(entries, "reifies") {
            let shifted: Vec<(Value, Value)> = reifies
                .iter()
                .map(|(rid, spo)| (sh(rid), sh_row(spo)))
                .collect();
            self.h_reifies(&Value::Map(shifted), index);
        }
        if let Some(Value::Array(annot)) = map_get(entries, "annot") {
            self.h_annot(&Value::Array(annot.iter().map(sh_row).collect()), index);
        }
        if let Some(Value::Map(blobs)) = map_get(entries, "blobs") {
            for (_, b) in blobs {
                if let Value::Bytes(bytes) = b {
                    let digest = digest_str(bytes);
                    self.g.set_blob(digest.clone(), bytes.clone());
                    self.emit_blob(&digest);
                }
            }
        }
        if let Some(Value::Map(meta)) = map_get(entries, "meta") {
            for (k, v) in meta {
                let key = as_text(k)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{k:?}"));
                self.g.set_meta(key, v.clone());
            }
        }
    }

    /// Record an intact `index` frame (§6.2) for the layout check (§3.3).
    ///
    /// The index stays an accelerator for the fold itself; only `count` and
    /// `head` are consumed here, as the covered-region boundary. A payload
    /// without a valid count/head pair is simply not an intact index.
    fn h_index(&mut self, payload: &Value, index: usize) {
        let Value::Map(entries) = payload else { return };
        let count = map_get(entries, "count").and_then(as_idx);
        let head = map_get(entries, "head");
        if let (Some(count), Some(Value::Bytes(head))) = (count, head) {
            let mmr = match map_get(entries, "mmr") {
                Some(Value::Bytes(root)) => Some(root.clone()),
                _ => None,
            };
            self.index_records.push(IndexRecord {
                abs_index: index,
                count,
                head: head.clone(),
                mmr,
            });
        }
    }

    fn h_opaque(&mut self, payload: &Value) {
        if let Value::Map(entries) = payload {
            let id = match map_get(entries, "id") {
                Some(Value::Bytes(b)) => b.clone(),
                _ => Vec::new(),
            };
            self.push_opaque(OpaqueNode {
                id,
                frame_type: text_or(map_get(entries, "type"), "opaque").to_string(),
                reason: text_or(map_get(entries, "reason"), "unknown-codec").to_string(),
                sigstat: text_or(map_get(entries, "sigstat"), "none").to_string(),
                pub_meta: map_get(entries, "pub").cloned(),
                recipients: None,
            });
        }
    }

    // -- helpers ---------------------------------------------------------------

    /// Bounds-check, then enforce §7.4 positions; diagnose + reject on violation.
    fn check_positions(
        &mut self,
        s: usize,
        p: usize,
        o: usize,
        g: Option<usize>,
        index: usize,
    ) -> bool {
        let n = self.g.terms.len();
        let in_bounds = s < n && p < n && o < n && g.is_none_or(|gv| gv < n);
        if !in_bounds {
            self.diag(
                "PositionConstraint",
                format!(
                    "quad ({s},{p},{o},{}) has out-of-range term ids",
                    fmt_opt(g)
                ),
                Some(index),
            );
            return false;
        }
        let mut ok = self.g.terms[p].kind == TermKind::Iri;
        if self.g.terms[s].kind == TermKind::Literal {
            ok = false;
        }
        if let Some(gv) = g {
            if matches!(self.g.terms[gv].kind, TermKind::Literal | TermKind::Triple) {
                ok = false;
            }
        }
        if !ok {
            self.diag(
                "PositionConstraint",
                format!("quad ({s},{p},{o},{}) violates positions", fmt_opt(g)),
                Some(index),
            );
        }
        ok
    }

    fn opaque(&mut self, frame: &[(Value, Value)], ftype: &str, reason: &str) {
        let id = match map_get(frame, "id") {
            Some(Value::Bytes(b)) => b.clone(),
            _ => Vec::new(),
        };
        let sigstat = if map_get(frame, "sig").is_some() {
            "unverified"
        } else {
            "none"
        };
        let recipients = match map_get(frame, "to") {
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .filter(|t| matches!(t, Value::Map(_)))
                    .cloned()
                    .collect(),
            ),
            _ => None,
        };
        self.push_opaque(OpaqueNode {
            id,
            frame_type: ftype.to_string(),
            reason: reason.to_string(),
            sigstat: sigstat.to_string(),
            pub_meta: map_get(frame, "pub").cloned(),
            recipients,
        });
    }
}

/// §3.1 boundary rule: a map carrying `"gts"` and lacking `"t"`.
fn is_header_item(item: &Value) -> bool {
    let inner = match item {
        Value::Tag(_, inner) => inner.as_ref(),
        other => other,
    };
    if let Value::Map(entries) = inner {
        map_get(entries, "gts").is_some() && map_get(entries, "t").is_none()
    } else {
        false
    }
}

fn catalog_from(header: &[(Value, Value)]) -> HashMap<i128, Codec> {
    let mut out = HashMap::new();
    if let Some(Value::Map(raw)) = map_get(header, "cat") {
        for (cid, entry) in raw {
            if let (Some(cid), Value::Map(fields)) = (as_i128(cid), entry) {
                out.insert(
                    cid,
                    Codec {
                        name: text_or(map_get(fields, "name"), "").to_string(),
                        cls: text_or(map_get(fields, "cls"), "encode").to_string(),
                    },
                );
            }
        }
    }
    out
}

/// Read and fold a GTS file into a [`Graph`].
///
/// Verifies each segment's header genesis hash, every frame's self-`id`, and
/// the per-segment `prev` chain, recording diagnostics; damaged and
/// undecodable frames fold to opaque nodes (§7.6) rather than aborting.
/// Multi-segment files (§3.1) fold per segment and union BY TERM VALUE
/// (term-ids are segment-scoped; blank nodes stay segment-local).
///
/// With `allow_segments = false` the reader emulates a pre-§3.1 reader: a
/// segment boundary is a FATAL `SegmentBoundary` diagnostic and nothing past
/// it is folded (§16, vector 17). `expected_head`, when given, is compared
/// against the LAST segment's head; a mismatch records `TruncatedLog`.
pub fn read(data: &[u8], allow_segments: bool, expected_head: Option<&[u8]>) -> Graph {
    read_with_options(data, ReadOptions::new(allow_segments, expected_head))
}

/// Read and fold a GTS file using explicit options.
pub fn read_with_options(data: &[u8], options: ReadOptions<'_>) -> Graph {
    let (items, torn) = iter_items(data);
    if items.is_empty() {
        let mut g = Graph::default();
        g.diagnostics.push(Diagnostic {
            code: "EmptyFile".to_string(),
            detail: "no CBOR items".to_string(),
            frame_index: None,
        });
        return g;
    }

    // Split into segments at header-shaped items (§3.1).
    let bounds: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, (_, item))| is_header_item(item))
        .map(|(i, _)| i)
        .collect();
    if bounds.first() != Some(&0) {
        let mut g = Graph::default();
        g.diagnostics.push(Diagnostic {
            code: "DamagedFrame".to_string(),
            detail: "first item is not a header".to_string(),
            frame_index: Some(0),
        });
        return g;
    }
    if bounds.len() > 1 && !options.allow_segments {
        let mut g = read_segment_with_sink(&items[..bounds[1]], 0, 0, None, options.content_key);
        g.diagnostics.push(Diagnostic {
            code: "SegmentBoundary".to_string(),
            detail: format!(
                "segment boundary at item {} but reader is in pre-segment mode; \
                 remainder of file NOT folded (folding it with file-global \
                 term-ids would silently misfold — §16)",
                bounds[1]
            ),
            frame_index: Some(bounds[1]),
        });
        return g;
    }

    let ends = bounds.iter().skip(1).copied().chain([items.len()]);
    let folded: Vec<Graph> = bounds
        .iter()
        .zip(ends)
        .map(|(&a, b)| read_segment_with_sink(&items[a..b], a, 0, None, options.content_key))
        .collect();

    let mut g = if folded.len() == 1 {
        folded.into_iter().next().expect("one segment")
    } else {
        union_segments(&folded)
    };

    if let Some(expected) = options.expected_head {
        let last_head = g.segment_heads.last().cloned().unwrap_or_default();
        if last_head != expected {
            g.diagnostics.push(Diagnostic {
                code: "TruncatedLog".to_string(),
                detail: "observed head does not match expected head".to_string(),
                frame_index: None,
            });
        }
    }
    if let Some(offset) = torn {
        g.diagnostics.push(Diagnostic {
            code: "TornAppendError".to_string(),
            detail: format!("torn at offset {offset}"),
            frame_index: None,
        });
    }
    g
}

/// Read a GTS file into a [`StreamingSink`] without constructing the final
/// union [`Graph`].
///
/// The same header id, frame id, `prev` chain, payload, and layout checks used
/// by [`read`] are applied while each segment is consumed. Events use
/// segment-local term ids; the returned [`StreamingReadResult`] carries the
/// final diagnostics, segment heads, profiles, and streamable-layout state.
pub fn read_to_sink(
    data: &[u8],
    allow_segments: bool,
    expected_head: Option<&[u8]>,
    sink: &mut dyn StreamingSink,
) -> StreamingReadResult {
    read_to_sink_with_options(data, ReadOptions::new(allow_segments, expected_head), sink)
}

/// Read a GTS file into a [`StreamingSink`] using explicit options.
pub fn read_to_sink_with_options(
    data: &[u8],
    options: ReadOptions<'_>,
    sink: &mut dyn StreamingSink,
) -> StreamingReadResult {
    let (items, torn) = iter_items(data);
    let mut result = StreamingReadResult {
        torn,
        ..StreamingReadResult::default()
    };
    if items.is_empty() {
        push_result_diagnostic(
            &mut result,
            sink,
            Diagnostic {
                code: "EmptyFile".to_string(),
                detail: "no CBOR items".to_string(),
                frame_index: None,
            },
        );
        return result;
    }

    let bounds: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, (_, item))| is_header_item(item))
        .map(|(i, _)| i)
        .collect();
    if bounds.first() != Some(&0) {
        push_result_diagnostic(
            &mut result,
            sink,
            Diagnostic {
                code: "DamagedFrame".to_string(),
                detail: "first item is not a header".to_string(),
                frame_index: Some(0),
            },
        );
        return result;
    }
    if bounds.len() > 1 && !options.allow_segments {
        let segment = read_segment_with_sink(
            &items[..bounds[1]],
            0,
            0,
            Some(&mut *sink),
            options.content_key,
        );
        absorb_segment_result(&mut result, &segment);
        push_result_diagnostic(
            &mut result,
            sink,
            Diagnostic {
                code: "SegmentBoundary".to_string(),
                detail: format!(
                    "segment boundary at item {} but reader is in pre-segment mode; \
                     remainder of file NOT folded (folding it with file-global \
                     term-ids would silently misfold — §16)",
                    bounds[1]
                ),
                frame_index: Some(bounds[1]),
            },
        );
        return result;
    }

    let ends = bounds.iter().skip(1).copied().chain([items.len()]);
    for (segment_index, (&a, b)) in bounds.iter().zip(ends).enumerate() {
        let segment = read_segment_with_sink(
            &items[a..b],
            a,
            segment_index,
            Some(&mut *sink),
            options.content_key,
        );
        absorb_segment_result(&mut result, &segment);
    }

    if let Some(expected) = options.expected_head {
        let last_head = result.segment_heads.last().cloned().unwrap_or_default();
        if last_head != expected {
            push_result_diagnostic(
                &mut result,
                sink,
                Diagnostic {
                    code: "TruncatedLog".to_string(),
                    detail: "observed head does not match expected head".to_string(),
                    frame_index: None,
                },
            );
        }
    }
    if let Some(offset) = torn {
        push_result_diagnostic(
            &mut result,
            sink,
            Diagnostic {
                code: "TornAppendError".to_string(),
                detail: format!("torn at offset {offset}"),
                frame_index: None,
            },
        );
    }
    result
}

/// The per-segment view of a file — the input to composition tooling (§14.1):
/// each segment folded independently, plus the file-level torn marker and any
/// fatal pre-segmentation diagnostic.
pub struct FileSegments {
    /// One fold per segment, in file order, each carrying its OWN diagnostics.
    pub segments: Vec<Graph>,
    /// Byte offset of a torn trailing item (§3), if any.
    pub torn: Option<usize>,
    /// Set when the file never reaches segmentation (empty, or the first item
    /// is not a header) — `segments` is empty in that case.
    pub fatal: Option<Diagnostic>,
}

/// Fold a file segment-by-segment WITHOUT unioning — the composition ledger
/// view that `gts info`/`gts verify` report per-segment (§14.1).
pub fn read_file_segments(data: &[u8]) -> FileSegments {
    let (items, torn) = iter_items(data);
    if items.is_empty() {
        return FileSegments {
            segments: Vec::new(),
            torn,
            fatal: Some(Diagnostic {
                code: "EmptyFile".to_string(),
                detail: "no CBOR items".to_string(),
                frame_index: None,
            }),
        };
    }
    let bounds: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, (_, item))| is_header_item(item))
        .map(|(i, _)| i)
        .collect();
    if bounds.first() != Some(&0) {
        return FileSegments {
            segments: Vec::new(),
            torn,
            fatal: Some(Diagnostic {
                code: "DamagedFrame".to_string(),
                detail: "first item is not a header".to_string(),
                frame_index: Some(0),
            }),
        };
    }
    let ends = bounds.iter().skip(1).copied().chain([items.len()]);
    let segments: Vec<Graph> = bounds
        .iter()
        .zip(ends)
        .map(|(&a, b)| read_segment(&items[a..b], a))
        .collect();
    FileSegments {
        segments,
        torn,
        fatal: None,
    }
}

/// Fold ONE segment (header + frames) into a [`Graph`] (§7.5).
///
/// `index_offset` is the segment's absolute position in the file's item
/// sequence, so multi-segment diagnostics report ABSOLUTE indices.
fn read_segment(items: &[(usize, Value)], index_offset: usize) -> Graph {
    read_segment_with_sink(items, index_offset, 0, None, None)
}

fn read_segment_with_sink(
    items: &[(usize, Value)],
    index_offset: usize,
    segment_index: usize,
    mut sink: Option<&mut dyn StreamingSink>,
    content_key: Option<&ContentKeyResolver<'_>>,
) -> Graph {
    let mut g = Graph::default();
    let (_, raw_header) = &items[0];
    let header = match unwrap_header(raw_header) {
        Ok(h) => h,
        Err(e) => {
            push_diagnostic(
                &mut g,
                &mut sink,
                Diagnostic {
                    code: "DamagedFrame".to_string(),
                    detail: format!("invalid header: {e}"),
                    frame_index: Some(index_offset),
                },
            );
            return g;
        }
    };
    let stored_hid: Option<Vec<u8>> = match map_get(header, "id") {
        Some(Value::Bytes(b)) => Some(b.clone()),
        _ => None,
    };
    if stored_hid.as_deref() != Some(&header_id(header)[..]) {
        push_diagnostic(
            &mut g,
            &mut sink,
            Diagnostic {
                code: "DamagedFrame".to_string(),
                detail: "header self-hash mismatch".to_string(),
                frame_index: Some(index_offset),
            },
        );
    }
    if map_get(header, "gts").and_then(as_text) != Some(MAGIC)
        || map_get(header, "v").and_then(as_i128) != Some(i128::from(VERSION))
    {
        push_diagnostic(
            &mut g,
            &mut sink,
            Diagnostic {
                code: "DamagedFrame".to_string(),
                detail: format!(
                    "unsupported header magic/version {:?}/{:?}",
                    map_get(header, "gts"),
                    map_get(header, "v")
                ),
                frame_index: Some(index_offset),
            },
        );
    }
    let mut expected_prev: Vec<u8> = stored_hid.unwrap_or_default();
    // per-frame chain ids, by 0-based frame position
    let mut frame_ids: Vec<Vec<u8>> = Vec::new();

    let (index_records, blob_events, restored_sink) = {
        let catalog = catalog_from(header);
        let mut folder = Folder {
            g: &mut g,
            sink: sink.take(),
            content_key,
            segment_index,
            catalog,
            index_records: Vec::new(),
            described: HashSet::new(),
            blob_events: Vec::new(),
        };
        for (index, (_, raw)) in items[1..].iter().enumerate() {
            let abs_index = index + 1 + index_offset;
            let Value::Map(frame) = raw else {
                folder.diag(
                    "DamagedFrame",
                    "frame is not a map".to_string(),
                    Some(abs_index),
                );
                frame_ids.push(Vec::new());
                continue;
            };
            let stored_id: Option<&Vec<u8>> = match map_get(frame, "id") {
                Some(Value::Bytes(b)) => Some(b),
                _ => None,
            };
            let computed = content_id(frame);
            if stored_id.map(|b| &b[..]) != Some(&computed[..]) {
                folder.diag(
                    "DamagedFrame",
                    "frame self-hash mismatch".to_string(),
                    Some(abs_index),
                );
                let ftype = text_or(map_get(frame, "t"), "").to_string();
                folder.opaque(frame, &ftype, "damaged");
                expected_prev = stored_id.cloned().unwrap_or(computed);
                frame_ids.push(expected_prev.clone());
                continue;
            }
            let prev_ok = matches!(map_get(frame, "prev"),
                Some(Value::Bytes(b)) if *b == expected_prev);
            if !prev_ok {
                folder.diag(
                    "BrokenChain",
                    "prev does not match".to_string(),
                    Some(abs_index),
                );
            }
            expected_prev = computed.clone();
            frame_ids.push(expected_prev.clone());
            if let Some(sig) = map_get(frame, "sig") {
                // No key provider in this baseline — a well-formed signature
                // is recorded as "unverified" with its raw COSE bytes retained
                // (compaction carries it detached, §10.1); a malformed one is
                // recorded as "invalid", never silently dropped.
                let (status, cose) = match sig {
                    Value::Bytes(b) => ("unverified", Some(b.clone())),
                    _ => ("invalid", None),
                };
                folder.push_signature(Signature {
                    frame_id: computed.clone(),
                    kid: None,
                    status: status.to_string(),
                    cose,
                });
            }
            folder.fold_frame(frame, abs_index);
        }
        (folder.index_records, folder.blob_events, folder.sink)
    };
    sink = restored_sink;

    g.segment_heads.push(expected_prev);
    if let Some(sink) = sink.as_deref_mut() {
        sink.segment_head(
            segment_index,
            g.segment_heads
                .last()
                .expect("segment head was just pushed"),
        );
    }
    let seg_meta = g.meta.clone();
    g.segment_meta.push(seg_meta);
    g.segment_profiles
        .push(text_or(map_get(header, "prof"), "generic").to_string());
    check_index_mmr(&mut g, &index_records, &frame_ids, index_offset, &mut sink);
    let info = layout_check(
        &mut g,
        header,
        &index_records,
        &blob_events,
        &frame_ids,
        index_offset,
        &mut sink,
    );
    g.segment_streamable.push(info);
    if let Some(sink) = sink {
        sink.streamable_layout(
            segment_index,
            g.segment_streamable
                .last()
                .expect("streamable info was just pushed"),
        );
    }
    g
}

/// Compute one segment's layout state and check its claim (§3.3).
///
/// For a segment claiming `"layout": "streamable"`: (a) it must carry an
/// intact `index` footer, (b) the last index's `head` must be the id of
/// frame `count`, and (c) every covered inline blob must arrive after the
/// `stream:digest` quad describing it. Frames after the last index are the
/// legal accretive tail — boundary info, never a diagnostic. Unknown layout
/// values impose no check (§5).
fn layout_check(
    g: &mut Graph,
    header: &[(Value, Value)],
    index_records: &[IndexRecord],
    blob_events: &[(usize, String, bool)],
    frame_ids: &[Vec<u8>],
    index_offset: usize,
    sink: &mut Option<&mut dyn StreamingSink>,
) -> StreamableInfo {
    let claimed = matches!(map_get(header, "layout"), Some(Value::Text(t)) if t == "streamable");
    let total = frame_ids.len();
    if !claimed {
        return StreamableInfo::default();
    }
    let Some(record) = index_records.last() else {
        push_diagnostic(
            g,
            sink,
            Diagnostic {
                code: "StreamableLayoutError".to_string(),
                detail: "segment claims layout 'streamable' but carries no intact \
                     index footer (§3.3)"
                    .to_string(),
                frame_index: None,
            },
        );
        return StreamableInfo {
            claimed: true,
            covered: 0,
            tail: total,
            head: None,
        };
    };
    let (abs_pos, count, head) = (record.abs_index, record.count, &record.head);
    let rel_pos = abs_pos - index_offset; // 1-based frame position of the index
    let tail = total - rel_pos;
    // The footer must IMMEDIATELY follow the frames it covers (§3.3): a
    // permissive `count <= rel_pos - 1` would let frames sit between the
    // covered prefix and the footer, counted neither as covered nor as tail.
    if count != rel_pos - 1 || count < 1 || frame_ids[count - 1] != *head {
        push_diagnostic(
            g,
            sink,
            Diagnostic {
                code: "StreamableLayoutError".to_string(),
                detail: format!(
                    "index footer contradicts the frames it covers: count {count} \
                 must name the frame immediately before the footer and head \
                 must be that frame's id (§3.3)"
                ),
                frame_index: Some(abs_pos),
            },
        );
    }
    for (blob_abs, digest, described) in blob_events {
        let blob_rel = blob_abs - index_offset;
        if blob_rel <= count && !described {
            push_diagnostic(
                g,
                sink,
                Diagnostic {
                    code: "StreamableLayoutError".to_string(),
                    detail: format!(
                        "covered blob {digest} delivered before its stream:digest \
                     description (catalog-before-payload, §3.3)"
                    ),
                    frame_index: Some(*blob_abs),
                },
            );
        }
    }
    StreamableInfo {
        claimed: true,
        covered: count,
        tail,
        head: Some(head.clone()),
    }
}

fn check_index_mmr(
    g: &mut Graph,
    index_records: &[IndexRecord],
    frame_ids: &[Vec<u8>],
    index_offset: usize,
    sink: &mut Option<&mut dyn StreamingSink>,
) {
    for record in index_records {
        let Some(root) = &record.mmr else {
            continue;
        };
        let rel_pos = record.abs_index.saturating_sub(index_offset);
        let preceding = rel_pos.saturating_sub(1);
        let mut detail = None;
        if root.len() != 32 {
            detail = Some("index mmr root is not a 32-byte digest".to_string());
        } else if record.count > preceding {
            detail = Some(format!(
                "index mmr covers {} frame(s), but only {preceding} precede the index",
                record.count
            ));
        } else if record.count > frame_ids.len() {
            detail = Some(format!(
                "index mmr covers {} frame(s), but the segment has {} frame id(s)",
                record.count,
                frame_ids.len()
            ));
        } else if record.count > 0 && frame_ids[record.count - 1] != record.head {
            detail = Some("index mmr head does not match the last covered frame".to_string());
        } else {
            let computed = mmr::root(&frame_ids[..record.count]);
            if computed != *root {
                detail = Some("index mmr root does not match the covered frame ids".to_string());
            }
        }
        if let Some(detail) = detail {
            push_diagnostic(
                g,
                sink,
                Diagnostic {
                    code: "IndexMmrError".to_string(),
                    detail,
                    frame_index: Some(record.abs_index),
                },
            );
        }
    }
}

// --------------------------------------------------------------------------- //
// Multi-segment union (§3.1, §7.5): term-ids are segment-scoped compression
// artifacts; the union re-interns BY TERM VALUE. Blank nodes carry a segment
// discriminator (labels are segment-local and never merge); quoted-triple
// terms intern recursively through their reifier's interned identity. Because
// the union is value-interned, "apply suppression value-wise" (§11) reduces
// to applying it by result-id.
// --------------------------------------------------------------------------- //

#[derive(Clone, PartialEq, Eq, Hash)]
enum InternKey {
    Iri(Option<String>),
    Lit(Option<String>, String, Option<String>),
    Bnode(usize, Option<String>, Option<usize>),
    Qt(Option<usize>),
}

#[derive(Default)]
struct Unioner {
    out: Graph,
    intern: HashMap<InternKey, usize>,
}

impl Unioner {
    fn key_for(&mut self, seg: &Graph, seg_idx: usize, tid: usize) -> InternKey {
        let t = &seg.terms[tid];
        match t.kind {
            TermKind::Iri => InternKey::Iri(t.value.clone()),
            TermKind::Literal => {
                InternKey::Lit(t.value.clone(), seg.datatype_iri(t), t.lang.clone())
            }
            // Non-empty labels are segment-local; absent/empty labels are fresh
            // anonymous nodes keyed by their source term entry (§7.1).
            TermKind::Bnode => {
                let label = t.value.as_ref().filter(|v| !v.is_empty()).cloned();
                let anon_tid = label.is_none().then_some(tid);
                InternKey::Bnode(seg_idx, label, anon_tid)
            }
            // Quoted triple: identity is the reifier's interned identity.
            TermKind::Triple => InternKey::Qt(t.reifier.map(|rf| self.map_term(seg, seg_idx, rf))),
        }
    }

    fn map_term(&mut self, seg: &Graph, seg_idx: usize, tid: usize) -> usize {
        let key = self.key_for(seg, seg_idx, tid);
        if let Some(&got) = self.intern.get(&key) {
            return got;
        }
        let t = seg.terms[tid].clone();
        let datatype = t.datatype.map(|d| self.map_term(seg, seg_idx, d));
        let reifier = t.reifier.map(|r| self.map_term(seg, seg_idx, r));
        // Blank nodes are relabelled with a segment prefix (§7.1 permits
        // isomorphism-preserving relabeling): within a segment, byte-identical
        // entries already intern to one union term (§7.8); ACROSS segments the
        // same label names DIFFERENT nodes, and emitting the raw label from
        // the union would merge them. Label-less nodes (absent or empty "v")
        // are distinct TERMS under the intern key, so their serialized labels
        // must stay distinct too — the union id disambiguates them. Computed
        // after dt/rf mapping so out.terms.len() IS this term's id.
        let value = if t.kind == TermKind::Bnode {
            Some(match t.value.as_deref() {
                Some(label) if !label.is_empty() => format!("s{seg_idx}.{label}"),
                _ => format!("s{seg_idx}._anon{}", self.out.terms.len()),
            })
        } else {
            t.value.clone()
        };
        self.out.terms.push(Term {
            kind: t.kind,
            value,
            datatype,
            lang: t.lang,
            reifier,
        });
        let new_id = self.out.terms.len() - 1;
        self.intern.insert(key, new_id);
        new_id
    }

    /// Re-intern a suppression's id-addressed targets (§11).
    ///
    /// Digest-addressed targets (`frame`, `blob`) pass through unchanged
    /// (content-ids are file-global). Id-addressed targets resolve in their
    /// OWN segment and re-intern into the union — exactly the value-wise
    /// application the spec requires, because the union is value-interned.
    fn remap_suppression(&mut self, sup: &Suppression, seg: &Graph, seg_idx: usize) -> Suppression {
        let n = seg.terms.len();
        let mut new_targets = Vec::with_capacity(sup.targets.len());
        for target in &sup.targets {
            let Value::Map(entries) = target else {
                new_targets.push(target.clone());
                continue;
            };
            let kind = text_or(map_get(entries, "kind"), "");
            if kind == "frame" || kind == "blob" {
                new_targets.push(target.clone());
                continue;
            }
            let mapped: Vec<(Value, Value)> = entries
                .iter()
                .map(|(k, v)| {
                    let key = as_text(k);
                    if (kind == "term" || kind == "reifier") && key == Some("id") {
                        if let Some(tid) = as_idx(v) {
                            if tid < n {
                                let new = self.map_term(seg, seg_idx, tid);
                                return (k.clone(), Value::from(new as u64));
                            }
                        }
                    } else if kind == "quad" && key == Some("q") {
                        if let Value::Array(ids) = v {
                            let remapped: Vec<Value> = ids
                                .iter()
                                .map(|x| match as_idx(x) {
                                    Some(tid) if tid < n => {
                                        Value::from(self.map_term(seg, seg_idx, tid) as u64)
                                    }
                                    _ => x.clone(),
                                })
                                .collect();
                            return (k.clone(), Value::Array(remapped));
                        }
                    }
                    (k.clone(), v.clone())
                })
                .collect();
            new_targets.push(Value::Map(mapped));
        }
        Suppression {
            targets: new_targets,
            reason: sup.reason.clone(),
            // "by" is a segment-scoped term-id (the suppressing agent) —
            // remap it into the union's id space like every other id ref.
            by: sup
                .by
                .and_then(|b| (b < n).then(|| self.map_term(seg, seg_idx, b))),
        }
    }
}

/// Union per-segment folds into one value-interned [`Graph`].
fn union_segments(segments: &[Graph]) -> Graph {
    let mut u = Unioner::default();
    let mut seen: HashSet<Quad> = HashSet::new();
    for (seg_idx, seg) in segments.iter().enumerate() {
        for &(s, p, o, gq) in &seg.quads {
            let q: Quad = (
                u.map_term(seg, seg_idx, s),
                u.map_term(seg, seg_idx, p),
                u.map_term(seg, seg_idx, o),
                gq.map(|x| u.map_term(seg, seg_idx, x)),
            );
            if seen.insert(q) {
                // the folded graph is a set (§7.8)
                u.out.quads.push(q);
            }
        }
        for &(rf, (s, p, o)) in &seg.reifiers {
            let new_rf = u.map_term(seg, seg_idx, rf);
            let spo = (
                u.map_term(seg, seg_idx, s),
                u.map_term(seg, seg_idx, p),
                u.map_term(seg, seg_idx, o),
            );
            u.out.set_reifier(new_rf, spo);
        }
        for &(r, p, v) in &seg.annotations {
            let row = (
                u.map_term(seg, seg_idx, r),
                u.map_term(seg, seg_idx, p),
                u.map_term(seg, seg_idx, v),
            );
            u.out.annotations.push(row);
        }
        for (digest, entry) in &seg.blobs {
            u.out.set_blob_entry(digest.clone(), entry.clone());
        }
        for (digest, meta) in &seg.blob_meta {
            u.out.set_blob_meta(digest.clone(), meta.clone());
        }
        for (k, v) in &seg.meta {
            // file-level shallow merge; later segments win
            u.out.set_meta(k.clone(), v.clone());
        }
        u.out.segment_meta.extend(seg.segment_meta.iter().cloned());
        for sup in &seg.suppressions {
            let remapped = u.remap_suppression(sup, seg, seg_idx);
            u.out.suppressions.push(remapped);
        }
        u.out.opaque.extend(seg.opaque.iter().cloned());
        u.out.signatures.extend(seg.signatures.iter().cloned());
        u.out.diagnostics.extend(seg.diagnostics.iter().cloned());
        u.out
            .segment_heads
            .extend(seg.segment_heads.iter().cloned());
        u.out
            .segment_profiles
            .extend(seg.segment_profiles.iter().cloned());
        u.out
            .segment_streamable
            .extend(seg.segment_streamable.iter().cloned());
    }
    u.out
}
