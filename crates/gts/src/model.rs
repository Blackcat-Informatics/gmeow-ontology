// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! In-memory data model for the folded graph — mirror of
//! `src/gmeow_tools/gts/model.py`.
//!
//! A [`Term`] is a single RDF term carried by integer id (§7.1). The folded
//! [`Graph`] is the deterministic replay of the append-only frame log (§7.5):
//! four id-keyed tables, content-addressed blobs, plus opaque/damaged nodes
//! and reader diagnostics. `reifiers` and `meta` are insertion-ordered maps
//! (Python `dict` semantics): re-binding an existing key replaces the value
//! but keeps the original position.

use ciborium::value::Value;

/// Well-known datatype IRIs used by the literal-defaulting rule (§7.1).
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// The kind of an RDF term, matching the wire `"k"` field (§7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TermKind {
    Iri = 0,
    Literal = 1,
    Bnode = 2,
    Triple = 3,
}

impl TermKind {
    /// Parse the wire `"k"` value; an unknown kind defaults to IRI (§7.1).
    pub fn from_wire(k: Option<i128>) -> TermKind {
        match k {
            Some(1) => TermKind::Literal,
            Some(2) => TermKind::Bnode,
            Some(3) => TermKind::Triple,
            _ => TermKind::Iri,
        }
    }
}

/// An RDF term identified by append-order id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    pub kind: TermKind,
    /// IRI string, literal lexical form, or blank-node label (file-local).
    pub value: Option<String>,
    /// Term-id of the literal's datatype IRI, when explicit.
    pub datatype: Option<usize>,
    /// Literal language tag (BCP 47).
    pub lang: Option<String>,
    /// Term-id of the reifier of a quoted triple (`kind == Triple`).
    pub reifier: Option<usize>,
}

/// A quad of term-ids; the graph slot is `None` for the default graph.
pub type Quad = (usize, usize, usize, Option<usize>);
pub type Triple3 = (usize, usize, usize);

/// A frame the reader could not decode, surfaced rather than dropped (§7.6).
#[derive(Clone, Debug)]
pub struct OpaqueNode {
    pub id: Vec<u8>,
    pub frame_type: String,
    /// `"unknown-codec"` | `"missing-key"` | `"damaged"`.
    pub reason: String,
    /// `"none"` | `"valid"` | `"invalid"` | `"unverified"`.
    pub sigstat: String,
    pub pub_meta: Option<Value>,
    pub recipients: Option<Vec<Value>>,
}

/// A recorded `suppress` directive (§11) — a display/precedence overlay.
#[derive(Clone, Debug)]
pub struct Suppression {
    /// Target maps (`{"kind": "term"|"quad"|"reifier"|"frame"|"blob", ...}`).
    pub targets: Vec<Value>,
    pub reason: Option<String>,
    pub by: Option<usize>,
}

/// A machine-observable reader diagnostic (§2.3).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: String,
    pub detail: String,
    pub frame_index: Option<usize>,
}

/// The verification outcome for a signed frame (§9.2).
///
/// `cose` retains the raw COSE_Sign1 bytes so streamable compaction (§10.1)
/// can carry the signature *detached* — forever verifiable against
/// `frame_id` even after the frame itself is re-authored into a new chain.
#[derive(Clone, Debug)]
pub struct Signature {
    pub frame_id: Vec<u8>,
    pub kid: Option<String>,
    /// `"valid"` | `"invalid"` | `"unverified"`.
    pub status: String,
    pub cose: Option<Vec<u8>>,
}

/// One segment's layout state (§3.3).
///
/// `covered`/`head` come from the segment's last intact `index` frame;
/// `tail` counts the legal unpresaged frames after it ("streamable through
/// frame *covered*, accretive tail of *tail* frame(s)"). For an unclaimed
/// (accretive) segment all fields are their zero values.
#[derive(Clone, Debug, Default)]
pub struct StreamableInfo {
    pub claimed: bool,
    pub covered: usize,
    pub tail: usize,
    pub head: Option<Vec<u8>>,
}

/// The folded result of a GTS log.
#[derive(Default, Debug)]
pub struct Graph {
    pub terms: Vec<Term>,
    pub quads: Vec<Quad>,
    /// Reifier-id → triple bindings, insertion-ordered.
    pub reifiers: Vec<(usize, Triple3)>,
    pub annotations: Vec<Triple3>,
    /// `blake3:<hex>` digest → inline bytes, insertion-ordered.
    pub blobs: Vec<(String, Vec<u8>)>,
    /// Declared blob metadata by digest — the blob frame's `"pub"` map
    /// (`mt`, `rep`, …) retained through the fold so tooling can list
    /// contents and assert media types without re-walking frames (§12).
    pub blob_meta: Vec<(String, Value)>,
    /// File-level shallow-merged metadata, insertion-ordered.
    pub meta: Vec<(String, Value)>,
    pub suppressions: Vec<Suppression>,
    pub opaque: Vec<OpaqueNode>,
    pub signatures: Vec<Signature>,
    pub diagnostics: Vec<Diagnostic>,
    /// Ordered per-segment head ids (§3.1) — the file's composite identity.
    pub segment_heads: Vec<Vec<u8>>,
    /// Per-segment header profiles; the effective requirement set is the
    /// union (§3.1, §13).
    pub segment_profiles: Vec<String>,
    /// Per-segment folded meta, preserved alongside the file-level merge.
    pub segment_meta: Vec<Vec<(String, Value)>>,
    /// Per-segment layout state (§3.3), in file order — the
    /// declared-vs-computed streamable claim, its covered boundary, and the
    /// accretive tail.
    pub segment_streamable: Vec<StreamableInfo>,
}

impl Graph {
    /// Look up a reifier binding.
    pub fn reifier(&self, rid: usize) -> Option<Triple3> {
        self.reifiers
            .iter()
            .find(|(r, _)| *r == rid)
            .map(|(_, spo)| *spo)
    }

    /// Bind a reifier, replacing in place (Python dict assignment).
    pub fn set_reifier(&mut self, rid: usize, spo: Triple3) {
        if let Some(slot) = self.reifiers.iter_mut().find(|(r, _)| *r == rid) {
            slot.1 = spo;
        } else {
            self.reifiers.push((rid, spo));
        }
    }

    /// Set a meta key, replacing in place (Python dict assignment).
    pub fn set_meta(&mut self, key: String, value: Value) {
        if let Some(slot) = self.meta.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.meta.push((key, value));
        }
    }

    /// Record a blob's declared metadata, replacing in place.
    pub fn set_blob_meta(&mut self, digest: String, meta: Value) {
        if let Some(slot) = self.blob_meta.iter_mut().find(|(d, _)| *d == digest) {
            slot.1 = meta;
        } else {
            self.blob_meta.push((digest, meta));
        }
    }

    /// Store an inline blob under its digest, replacing in place.
    pub fn set_blob(&mut self, digest: String, data: Vec<u8>) {
        if let Some(slot) = self.blobs.iter_mut().find(|(d, _)| *d == digest) {
            slot.1 = data;
        } else {
            self.blobs.push((digest, data));
        }
    }

    /// The effective datatype IRI of a literal, applying §7.1 defaulting.
    ///
    /// The fold sanitizes `datatype` ids, but `Graph` is constructible by
    /// callers — an out-of-range id falls back to `xsd:string`, never panics.
    pub fn datatype_iri(&self, t: &Term) -> String {
        if let Some(dt) = t.datatype {
            return self
                .terms
                .get(dt)
                .and_then(|term| term.value.clone())
                .unwrap_or_else(|| XSD_STRING.to_string());
        }
        if t.lang.is_some() {
            RDF_LANG_STRING.to_string()
        } else {
            XSD_STRING.to_string()
        }
    }
}
