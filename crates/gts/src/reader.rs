// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! The GTS reader: parse a CBOR Sequence, verify the id/prev chain, fold the
//! log — mirror of `src/gmeow_tools/gts/reader.py`.
//!
//! Implements the Baseline Reader contract (§2.1): chain verification (§9.1),
//! the four-table fold (§7.5), opaque/damaged degradation (§7.6), torn-append
//! detection (§3), and the canonical diagnostics (§2.3). This baseline
//! carries no keys: `sig` frames record as `"unverified"` and
//! `encrypt`-class frames degrade to `missing-key` opaque nodes.

use std::collections::{HashMap, HashSet};

use ciborium::value::Value;

use crate::codec::{decode_chain, Codec, CodecError};
use crate::model::{
    Diagnostic, Graph, OpaqueNode, Quad, Signature, Suppression, Term, TermKind, Triple3,
};
use crate::wire::{content_id, digest_str, header_id, iter_items, map_get, unwrap_header};

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

enum PayloadError {
    /// Missing capability — degrade to an opaque node with this reason.
    Unavailable {
        reason: &'static str,
        detail: String,
    },
    /// Anything else — the frame is damaged.
    Damaged(String),
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

/// Mutable fold state; one per segment (and shared by the snapshot handler).
struct Folder<'a> {
    g: &'a mut Graph,
    catalog: HashMap<i128, Codec>,
}

impl Folder<'_> {
    fn diag(&mut self, code: &str, detail: String, index: Option<usize>) {
        self.g.diagnostics.push(Diagnostic {
            code: code.to_string(),
            detail,
            frame_index: index,
        });
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
                let decoded = decode_chain(&chain, db)?;
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
        let payload = match self.payload(frame, ftype == "blob") {
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
            "blob" => self.h_blob(&payload),
            "meta" => self.h_meta(&payload),
            "suppress" => self.h_suppress(&payload),
            "snapshot" => self.h_snapshot(&payload, index),
            "opaque" => self.h_opaque(&payload),
            // index / unknown structural frames are ignored by the baseline
            _ => {}
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
            self.g.quads.push((s, p, o, gslot));
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
            self.g.annotations.push((r, p, v));
        }
    }

    fn h_blob(&mut self, payload: &Value) {
        if let Value::Bytes(b) = payload {
            self.g.set_blob(digest_str(b), b.clone());
        }
        // else: external blob — bytes live elsewhere, referenced by "pub".digest (§12).
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
        self.g.suppressions.push(Suppression {
            targets: targets
                .iter()
                .filter(|t| matches!(t, Value::Map(_)))
                .cloned()
                .collect(),
            reason: map_get(entries, "reason")
                .and_then(as_text)
                .map(str::to_string),
            by: map_get(entries, "by").and_then(as_idx),
        });
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
                    self.g.set_blob(digest_str(bytes), bytes.clone());
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

    fn h_opaque(&mut self, payload: &Value) {
        if let Value::Map(entries) = payload {
            let id = match map_get(entries, "id") {
                Some(Value::Bytes(b)) => b.clone(),
                _ => Vec::new(),
            };
            self.g.opaque.push(OpaqueNode {
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
        self.g.opaque.push(OpaqueNode {
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
    if bounds.len() > 1 && !allow_segments {
        let mut g = read_segment(&items[..bounds[1]], 0);
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
        .map(|(&a, b)| read_segment(&items[a..b], a))
        .collect();

    let mut g = if folded.len() == 1 {
        folded.into_iter().next().expect("one segment")
    } else {
        union_segments(&folded)
    };

    if let Some(expected) = expected_head {
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
    let mut g = Graph::default();
    let (_, raw_header) = &items[0];
    let header = match unwrap_header(raw_header) {
        Ok(h) => h,
        Err(e) => {
            g.diagnostics.push(Diagnostic {
                code: "DamagedFrame".to_string(),
                detail: format!("invalid header: {e}"),
                frame_index: Some(index_offset),
            });
            return g;
        }
    };
    let stored_hid: Option<Vec<u8>> = match map_get(header, "id") {
        Some(Value::Bytes(b)) => Some(b.clone()),
        _ => None,
    };
    if stored_hid.as_deref() != Some(&header_id(header)[..]) {
        g.diagnostics.push(Diagnostic {
            code: "DamagedFrame".to_string(),
            detail: "header self-hash mismatch".to_string(),
            frame_index: Some(index_offset),
        });
    }
    let mut expected_prev: Vec<u8> = stored_hid.unwrap_or_default();

    {
        let catalog = catalog_from(header);
        let mut folder = Folder { g: &mut g, catalog };
        for (index, (_, raw)) in items[1..].iter().enumerate() {
            let abs_index = index + 1 + index_offset;
            let Value::Map(frame) = raw else {
                folder.diag(
                    "DamagedFrame",
                    "frame is not a map".to_string(),
                    Some(abs_index),
                );
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
            if let Some(sig) = map_get(frame, "sig") {
                let status = if matches!(sig, Value::Bytes(_)) {
                    // no key provider in this baseline — recorded, not verified
                    "unverified"
                } else {
                    // present but malformed — record as invalid, never drop
                    "invalid"
                };
                folder.g.signatures.push(Signature {
                    frame_id: computed.clone(),
                    kid: None,
                    status: status.to_string(),
                });
            }
            folder.fold_frame(frame, abs_index);
        }
    }

    g.segment_heads.push(expected_prev);
    let seg_meta = g.meta.clone();
    g.segment_meta.push(seg_meta);
    g.segment_profiles
        .push(text_or(map_get(header, "prof"), "generic").to_string());
    g
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
    Bnode(usize, Option<String>),
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
            // segment-local, never merged
            TermKind::Bnode => InternKey::Bnode(seg_idx, t.value.clone()),
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
        for (digest, bytes) in &seg.blobs {
            u.out.set_blob(digest.clone(), bytes.clone());
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
    }
    u.out
}
