// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native `RDF → GTS` producer surface for the `gmeow_rdf` Python extension
//! (#819 Task 8 / C7).
//!
//! This module moves the byte-emitting core of `src/gmeow_tools/gts_producer.py`
//! into Rust. The Python `_Builder` interns terms, content-sorts them, and emits
//! a SINGLE `dist`-profile `snapshot` frame (preceded by blob frames, and — when
//! signing — a transport-key `meta` frame). It does **not** use
//! [`gmeow_gts::writer::Writer::deterministic`] (which emits separate
//! `terms`/`quads`/`reifies`/`annot` frames); it authors the snapshot frame
//! directly via `Writer::add_frame("snapshot", …)`.
//!
//! To preserve **byte-identity** with the existing producer — and, crucially, the
//! `snapshot_content_id()` self-attestation that `feedback_bundle.py` relies on
//! (#654) — this module replicates `_Builder` exactly:
//!
//! * the same interning order (append-order, scope-aware blank nodes);
//! * the same content sort (`(kind, value, datatype-IRI, lang)`, IRIs first);
//! * the same snapshot payload map (`terms` + `quads`, plus `reifies`/`annot`
//!   when non-empty);
//! * the same blob ordering (`(rep, decoded-bytes)`);
//! * the same per-payload `zstd-rsyncable` selection above the threshold;
//! * the same transport-key `meta` frame on the signed path.
//!
//! All CBOR encoding, canonicalization, frame-id chaining, and signing is
//! delegated to `gmeow-gts` — never hand-rolled.

use std::collections::HashMap;

use ciborium::value::Value;
use ed25519_dalek::SigningKey;
use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::wire::{blake3_256, canonical, hex};
use gmeow_gts::writer::{term_to_wire, Writer};
use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, NamedOrBlankNode, Quad, Term as OxTerm};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use crate::py_store::{parse_quads, PyRdfFormat};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const DEFAULT_RSYNCABLE_THRESHOLD: usize = 65536;

/// A remapped quad row in canonical term ids (`g == None` is the default graph).
type CanonQuad = (usize, usize, usize, Option<usize>);
/// A remapped `(reifier, (s, p, o))` reifies binding in canonical term ids.
type CanonReifies = (usize, (usize, usize, usize));
/// A remapped `(reifier, predicate, object)` annotation in canonical term ids.
type CanonAnnot = (usize, usize, usize);
/// The fully canonical snapshot tables (`_Builder._canonical_tables`).
type CanonTables = (
    Vec<Term>,
    Vec<CanonQuad>,
    Vec<CanonReifies>,
    Vec<CanonAnnot>,
);

/// One interned term plus its content-sort key. Mirrors `gts.model.Term` rows
/// in the Python `_Interner`, but carries the datatype as the IRI STRING (the
/// post-canonicalization id is assigned later) so the sort key is value-stable.
#[derive(Clone)]
struct TermRow {
    kind: TermKind,
    value: String,
    /// The datatype IRI string for a typed literal (interned later as a term).
    datatype: Option<String>,
    lang: Option<String>,
}

/// An accumulating snapshot builder mirroring `gts_producer._Builder`.
///
/// Term ids are append-order during ingestion (process-unstable), then re-id'd
/// by content in [`Self::canonical_tables`] so the emitted bytes are a pure
/// function of the inputs.
#[derive(Default)]
struct SnapshotBuilder {
    terms: Vec<TermRow>,
    /// Intern index keyed by `(kind, value, datatype-or-empty, lang-or-empty)`,
    /// matching the Python `_Interner` keys exactly.
    index: HashMap<(u8, String, String, String), usize>,
    /// Blank-node intern index keyed by `(scope, label)` (C0.2): two equal
    /// labels in different ingest scopes stay distinct terms.
    bnode_index: HashMap<(Option<String>, String), usize>,
    quads: Vec<(usize, usize, usize, Option<usize>)>,
    /// reifier-id → (s, p, o); a `Vec` preserving first-bind, dedup on rebind.
    reifies: Vec<(usize, (usize, usize, usize))>,
    annot: Vec<(usize, usize, usize)>,
}

impl SnapshotBuilder {
    fn intern_key(
        kind: u8,
        value: &str,
        datatype: Option<&str>,
        lang: Option<&str>,
    ) -> (u8, String, String, String) {
        (
            kind,
            value.to_owned(),
            datatype.unwrap_or("").to_owned(),
            lang.unwrap_or("").to_owned(),
        )
    }

    fn intern_iri(&mut self, iri: &str) -> usize {
        let key = Self::intern_key(TermKind::Iri as u8, iri, None, None);
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.terms.len();
        self.terms.push(TermRow {
            kind: TermKind::Iri,
            value: iri.to_owned(),
            datatype: None,
            lang: None,
        });
        self.index.insert(key, id);
        id
    }

    fn intern_bnode(&mut self, label: &str, scope: Option<&str>) -> usize {
        // Scope-prefix the stored value exactly as Python's `_Interner.bnode`:
        // `None` keeps the raw label; a scope yields `"{scope}-{label}"`.
        let bkey = (scope.map(str::to_owned), label.to_owned());
        if let Some(&id) = self.bnode_index.get(&bkey) {
            return id;
        }
        let value = match scope {
            None => label.to_owned(),
            Some(scope) => format!("{scope}-{label}"),
        };
        let id = self.terms.len();
        self.terms.push(TermRow {
            kind: TermKind::Bnode,
            value,
            datatype: None,
            lang: None,
        });
        self.bnode_index.insert(bkey, id);
        id
    }

    fn intern_literal(&mut self, lex: &str, datatype: Option<&str>, lang: Option<&str>) -> usize {
        // Ensure the datatype IRI is interned (IRIs sort before literals, so the
        // datatype id always precedes the literal — §7.5, preserved here).
        if let Some(dt) = datatype {
            self.intern_iri(dt);
        }
        let key = Self::intern_key(TermKind::Literal as u8, lex, datatype, lang);
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.terms.len();
        self.terms.push(TermRow {
            kind: TermKind::Literal,
            value: lex.to_owned(),
            datatype: datatype.map(str::to_owned),
            lang: lang.map(str::to_owned),
        });
        self.index.insert(key, id);
        id
    }

    /// Intern an oxigraph term in subject/object/graph position. Triple terms are
    /// not interned here (the RDF 1.2 statement layer is ingested as reifies/annot
    /// rows, never as quoted-triple base terms — matching `_Builder._rdflib`).
    fn intern_ox_term(&mut self, term: &OxTerm, scope: Option<&str>) -> Option<usize> {
        match term {
            OxTerm::NamedNode(n) => Some(self.intern_iri(n.as_str())),
            OxTerm::BlankNode(b) => Some(self.intern_bnode(b.as_str(), scope)),
            OxTerm::Literal(l) => {
                // Mirror `_ox`: a language-tagged literal carries no datatype; a
                // plain `xsd:string` is stored WITHOUT a datatype (it is implied).
                if let Some(lang) = l.language() {
                    Some(self.intern_literal(l.value(), None, Some(lang)))
                } else {
                    let dt = l.datatype();
                    let dt = if dt.as_str() == XSD_STRING {
                        None
                    } else {
                        Some(dt.as_str())
                    };
                    Some(self.intern_literal(l.value(), dt, None))
                }
            }
            OxTerm::Triple(_) => None,
        }
    }

    fn intern_ox_subject(&mut self, subject: &NamedOrBlankNode, scope: Option<&str>) -> usize {
        match subject {
            NamedOrBlankNode::NamedNode(n) => self.intern_iri(n.as_str()),
            NamedOrBlankNode::BlankNode(b) => self.intern_bnode(b.as_str(), scope),
        }
    }

    fn intern_ox_graph(&mut self, graph: &GraphName, scope: Option<&str>) -> Option<usize> {
        match graph {
            GraphName::DefaultGraph => None,
            GraphName::NamedNode(n) => Some(self.intern_iri(n.as_str())),
            GraphName::BlankNode(b) => Some(self.intern_bnode(b.as_str(), scope)),
        }
    }

    /// Ingest a flat oxigraph quad list as a base graph (RDF 1.1), mirroring
    /// `_Builder.add_graph`. `default_graph_name` assigns rows that carry no name
    /// of their own to a named graph (the snapshot's source-partitioning hook).
    fn add_quads(&mut self, quads: &[Quad], default_graph_name: Option<&str>, scope: Option<&str>) {
        let default_gid = default_graph_name.map(|name| self.intern_iri(name));
        for quad in quads {
            let sid = self.intern_ox_subject(&quad.subject, scope);
            let pid = self.intern_iri(quad.predicate.as_str());
            let Some(oid) = self.intern_ox_term(&quad.object, scope) else {
                continue;
            };
            let gid = match &quad.graph_name {
                GraphName::DefaultGraph => default_gid,
                other => self.intern_ox_graph(other, scope),
            };
            self.quads.push((sid, pid, oid, gid));
        }
    }

    /// Bind a reifier first-wins, erroring on a conflicting rebind (the producer's
    /// strict contract — `_Builder.add_rdf12` pass 1).
    fn bind_reifier(&mut self, rid: usize, spo: (usize, usize, usize)) -> Result<(), String> {
        if let Some((_, existing)) = self.reifies.iter().find(|(r, _)| *r == rid) {
            if *existing != spo {
                return Err(format!("conflicting reifier rebind for term id {rid}"));
            }
            return Ok(());
        }
        self.reifies.push((rid, spo));
        Ok(())
    }

    /// Ingest the RDF 1.2 statement layer from a parsed quad list (the
    /// `rdf:reifies` triple-terms + annotations), mirroring `_Builder.add_rdf12`.
    fn add_rdf12(
        &mut self,
        quads: &[Quad],
        graph_name: Option<&str>,
        scope: Option<&str>,
    ) -> Result<(), String> {
        let default_gid = graph_name.map(|name| self.intern_iri(name));
        let mut reifier_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut pending: Vec<(usize, usize, usize)> = Vec::new();

        // Pass 1: reifies bindings establish which subjects are reifiers.
        for quad in quads {
            let is_reifies = quad.predicate.as_str() == RDF_REIFIES;
            if let (true, OxTerm::Triple(triple)) = (is_reifies, &quad.object) {
                let rid = self.intern_ox_subject(&quad.subject, scope);
                let qs = self.intern_ox_subject(&triple.subject, scope);
                let qp = self.intern_iri(triple.predicate.as_str());
                let Some(qo) = self.intern_ox_term(&triple.object, scope) else {
                    continue;
                };
                reifier_ids.insert(rid);
                self.bind_reifier(rid, (qs, qp, qo))?;
            } else {
                let sid = self.intern_ox_subject(&quad.subject, scope);
                let pid = self.intern_iri(quad.predicate.as_str());
                let Some(oid) = self.intern_ox_term(&quad.object, scope) else {
                    continue;
                };
                pending.push((sid, pid, oid));
            }
        }
        // Pass 2: a reifier's other triples are annotations; the rest are base quads.
        for (sid, pid, oid) in pending {
            if reifier_ids.contains(&sid) {
                self.annot.push((sid, pid, oid));
            } else {
                self.quads.push((sid, pid, oid, default_gid));
            }
        }
        Ok(())
    }

    /// Re-id every term by content and sort every row (`_Builder._canonical_tables`).
    ///
    /// Returns the canonical `(wire_terms, quads, reifies, annot)` ready for the
    /// snapshot payload. Terms sort by `(kind, value, datatype-IRI, lang)` with
    /// IRIs first, so every literal's datatype IRI precedes it.
    fn canonical_tables(&self) -> CanonTables {
        let n = self.terms.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&a| self.sort_key(a));
        let mut remap = vec![0usize; n];
        for (new_id, &old) in order.iter().enumerate() {
            remap[old] = new_id;
        }

        // Wire terms in new-id order; the datatype field becomes the remapped id
        // of its IRI term (interned earlier, so it has an old id and thus a new id).
        let wire_terms: Vec<Term> = order
            .iter()
            .map(|&old| {
                let row = &self.terms[old];
                let datatype = row.datatype.as_ref().map(|dt| {
                    let old_dt = self.index[&Self::intern_key(TermKind::Iri as u8, dt, None, None)];
                    remap[old_dt]
                });
                Term {
                    kind: row.kind,
                    value: Some(row.value.clone()),
                    datatype,
                    lang: row.lang.clone(),
                    direction: None,
                    reifier: None,
                }
            })
            .collect();

        // Quads: remap, dedup, sort by (graph[None=-1], s, p, o).
        let mut quad_set: std::collections::BTreeSet<(i64, usize, usize, usize, Option<usize>)> =
            std::collections::BTreeSet::new();
        for &(s, p, o, g) in &self.quads {
            let g = g.map(|g| remap[g]);
            let gkey = g.map(|g| g as i64).unwrap_or(-1);
            quad_set.insert((gkey, remap[s], remap[p], remap[o], g));
        }
        let quads: Vec<(usize, usize, usize, Option<usize>)> = quad_set
            .into_iter()
            .map(|(_, s, p, o, g)| (s, p, o, g))
            .collect();

        // Reifies: remap, sort by reifier id (the Python dict is built in
        // remapped-id-sorted order; CBOR canonical re-sorts map keys anyway).
        let mut reifies: Vec<(usize, (usize, usize, usize))> = self
            .reifies
            .iter()
            .map(|&(rid, (s, p, o))| (remap[rid], (remap[s], remap[p], remap[o])))
            .collect();
        reifies.sort_by_key(|(rid, _)| *rid);

        // Annot: remap, dedup, sort.
        let mut annot_set: std::collections::BTreeSet<(usize, usize, usize)> =
            std::collections::BTreeSet::new();
        for &(r, p, v) in &self.annot {
            annot_set.insert((remap[r], remap[p], remap[v]));
        }
        let annot: Vec<(usize, usize, usize)> = annot_set.into_iter().collect();

        (wire_terms, quads, reifies, annot)
    }

    fn sort_key(&self, tid: usize) -> (u8, String, String, String) {
        let t = &self.terms[tid];
        let dt = t.datatype.clone().unwrap_or_default();
        (
            t.kind as u8,
            t.value.clone(),
            dt,
            t.lang.clone().unwrap_or_default(),
        )
    }

    /// The canonical `snapshot` frame payload (`_Builder._snapshot_payload`).
    fn snapshot_payload(&self) -> Value {
        let (terms, quads, reifies, annot) = self.canonical_tables();
        let mut entries: Vec<(Value, Value)> = vec![
            (
                "terms".into(),
                Value::Array(terms.iter().map(term_to_wire).collect()),
            ),
            (
                "quads".into(),
                Value::Array(
                    quads
                        .iter()
                        .map(|&(s, p, o, g)| {
                            let mut row = vec![iv(s), iv(p), iv(o)];
                            if let Some(g) = g {
                                row.push(iv(g));
                            }
                            Value::Array(row)
                        })
                        .collect(),
                ),
            ),
        ];
        if !reifies.is_empty() {
            entries.push((
                "reifies".into(),
                Value::Map(
                    reifies
                        .iter()
                        .map(|&(rid, (s, p, o))| (iv(rid), Value::Array(vec![iv(s), iv(p), iv(o)])))
                        .collect(),
                ),
            ));
        }
        if !annot.is_empty() {
            entries.push((
                "annot".into(),
                Value::Array(
                    annot
                        .iter()
                        .map(|&(r, p, v)| Value::Array(vec![iv(r), iv(p), iv(v)]))
                        .collect(),
                ),
            ));
        }
        Value::Map(entries)
    }

    /// The `blake3:<hex>` content address of the snapshot payload
    /// (`_Builder.snapshot_content_id`).
    fn snapshot_content_id(&self) -> String {
        let bytes = canonical(&self.snapshot_payload());
        format!("blake3:{}", hex(&blake3_256(&bytes)))
    }
}

fn iv(n: usize) -> Value {
    Value::Integer(ciborium::value::Integer::from(n as u64))
}

/// A `(data, media_type, rep)` blob row, ingested from Python tuples.
struct BlobRow {
    data: Vec<u8>,
    media_type: String,
    rep: String,
}

/// Choose `zstd-rsyncable` for large payloads when the base chain is the default
/// `["zstd"]` (`_Builder.to_gts.choose_transform`).
fn choose_transform(base_chain: &[String], payload_len: usize, threshold: usize) -> Vec<String> {
    if base_chain.len() == 1 && base_chain[0] == "zstd" && payload_len > threshold {
        vec!["zstd-rsyncable".to_string()]
    } else {
        base_chain.to_vec()
    }
}

/// Emit the snapshot bundle bytes from an accumulated builder (`_Builder.to_gts`).
#[allow(clippy::too_many_arguments)]
fn emit_gts(
    builder: &SnapshotBuilder,
    profile: &str,
    transform: Option<Vec<String>>,
    doc_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
    rsyncable_threshold: usize,
) -> Result<Vec<u8>, String> {
    // No-optionality: signer xor public_key_armor is an error (both or neither).
    let signing = match (&signer_secret, &public_key_armor) {
        (Some(_), Some(_)) => true,
        (None, None) => false,
        _ => return Err("signer and public_key_armor must be supplied together".to_string()),
    };

    let base_chain = transform.unwrap_or_else(|| vec!["zstd".to_string()]);

    let mut writer = Writer::new(profile);
    if signing {
        let secret = signer_secret.expect("signing implies a secret");
        let kid = signer_kid.ok_or("signing requires a kid")?;
        writer.sign_with(SigningKey::from_bytes(&secret), &kid);
        // The transport-key meta frame, signed along with every later frame.
        let armor = public_key_armor.expect("signing implies a public key");
        let meta = Value::Map(vec![(
            "gts:transportKey".into(),
            Value::Map(vec![
                ("kid".into(), Value::Text(kid)),
                ("gpg".into(), Value::Text(armor)),
            ]),
        )]);
        writer.add_meta(meta);
    }

    // Blob frames ride AHEAD of the snapshot, sorted by (rep, decoded-bytes).
    let mut all_blobs: Vec<BlobRow> = doc_blobs;
    all_blobs.extend(report_blobs);
    all_blobs.sort_by(|a, b| a.rep.cmp(&b.rep).then_with(|| a.data.cmp(&b.data)));
    for blob in &all_blobs {
        let chain = choose_transform(&base_chain, blob.data.len(), rsyncable_threshold);
        // `add_blob` does not take a transform; author the frame directly so the
        // per-payload rsyncable selection is honored (parity with `_Builder`).
        let pub_meta = Value::Map(vec![
            (
                "digest".into(),
                Value::Text(gmeow_gts::writer::digest_string(&blob.data)),
            ),
            ("mt".into(), Value::Text(blob.media_type.clone())),
            ("rep".into(), Value::Text(blob.rep.clone())),
        ]);
        let options = gmeow_gts::writer::FrameOptions {
            raw: Some(blob.data.clone()),
            transform: chain,
            pub_meta: Some(pub_meta),
            ..Default::default()
        };
        writer
            .add_frame_with_options("blob", options)
            .map_err(|e| e.to_string())?;
    }

    let payload = builder.snapshot_payload();
    let snapshot_bytes = canonical(&payload);
    let chain = choose_transform(&base_chain, snapshot_bytes.len(), rsyncable_threshold);
    let options = gmeow_gts::writer::FrameOptions {
        payload: Some(payload),
        transform: chain,
        ..Default::default()
    };
    writer
        .add_frame_with_options("snapshot", options)
        .map_err(|e| e.to_string())?;

    Ok(writer.to_bytes())
}

// ── Python helpers ────────────────────────────────────────────────────────────

fn blob_rows_from_py(blobs: Option<&Bound<'_, PyList>>) -> PyResult<Vec<BlobRow>> {
    let Some(blobs) = blobs else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(blobs.len());
    for item in blobs.iter() {
        let (data, media_type, rep): (Vec<u8>, String, String) = item
            .extract()
            .map_err(|_| PyValueError::new_err("blob rows must be (bytes, media_type, rep)"))?;
        out.push(BlobRow {
            data,
            media_type,
            rep,
        });
    }
    Ok(out)
}

fn secret_array(secret: Option<&Bound<'_, PyBytes>>) -> PyResult<Option<[u8; 32]>> {
    match secret {
        None => Ok(None),
        Some(bytes) => {
            let raw = bytes.as_bytes();
            let arr: [u8; 32] = raw
                .try_into()
                .map_err(|_| PyValueError::new_err("signer secret must be 32 raw Ed25519 bytes"))?;
            Ok(Some(arr))
        }
    }
}

/// Parse RDF bytes leniently into oxigraph quads. The lenient parser accepts
/// private-use language tags (`@x-gmeow-*`) that the strict `gmeow_rdf.Literal`
/// constructor would reject — the producer therefore lowers rdflib sources to
/// N-Quads/Turtle bytes and parses HERE, never building `Quad` objects.
fn parse_rdf(data: &Bound<'_, PyBytes>, format: PyRdfFormat) -> PyResult<Vec<Quad>> {
    parse_quads(data.as_bytes(), rdf_format(format))
        .map_err(|e| PyValueError::new_err(format!("parse error: {e}")))
}

// ── Module-level functions ────────────────────────────────────────────────────

/// Produce a GTS snapshot from a serialized RDF 1.1 base graph (Turtle/N-Quads
/// bytes, parsed leniently). Mirrors `gts_producer.gts_from_graph`. `transform`
/// defaults to `["zstd"]` when `None`.
#[pyfunction]
#[pyo3(signature = (data, *, format, profile="dist", transform=None))]
fn gts_from_quads(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    format: PyRdfFormat,
    profile: &str,
    transform: Option<Vec<String>>,
) -> PyResult<Py<PyBytes>> {
    let ox_quads = parse_rdf(data, format)?;
    let mut builder = SnapshotBuilder::default();
    builder.add_quads(&ox_quads, None, None);
    let bytes = emit_gts(
        &builder,
        profile,
        transform,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(PyValueError::new_err)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// Produce a GTS snapshot from an RDF 1.2 statement-layer artifact's bytes
/// (parsed natively as Turtle/N-Quads). Mirrors `gts_producer.gts_from_rdf12`.
#[pyfunction]
#[pyo3(signature = (data, *, format, profile="dist", transform=None))]
fn gts_from_rdf12_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    format: PyRdfFormat,
    profile: &str,
    transform: Option<Vec<String>>,
) -> PyResult<Py<PyBytes>> {
    let quads = parse_rdf(data, format)?;
    let mut builder = SnapshotBuilder::default();
    builder
        .add_rdf12(&quads, None, None)
        .map_err(PyValueError::new_err)?;
    let bytes = emit_gts(
        &builder,
        profile,
        transform,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(PyValueError::new_err)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// One named-graph ingest row passed from Python: `(data, format, graph_name, scope)`.
/// `graph_name`/`scope` may be `None` (the default graph / un-scoped blank nodes).
type NamedGraphRow<'py> = (
    Bound<'py, PyBytes>,
    PyRdfFormat,
    Option<String>,
    Option<String>,
);

/// The full statement-complete compiler, mirroring `gts_producer.compile_gts`.
///
/// `base_data` is the canonicalized RDF 1.1 base graph as RDF bytes (the caller
/// canonicalizes blank-node labels with RDFC-1.0 before serializing, exactly as
/// the Python `compile_gts` does via `to_canonical_graph`). It is parsed leniently
/// HERE so private-use language tags survive. `rdf12_data` is the RDF 1.2 statement
/// layer's bytes. `named_graphs` carries the alignment graph and any extra named
/// graphs as `(data, format, graph_name, scope)` rows.
#[pyfunction]
#[pyo3(signature = (
    base_data,
    base_format,
    *,
    base_scope=None,
    rdf12_data=None,
    rdf12_format=None,
    rdf12_graph_name=None,
    rdf12_scope=None,
    named_graphs=None,
    transform=None,
    doc_blobs=None,
    report_blobs=None,
    signer_secret=None,
    signer_kid=None,
    public_key_armor=None,
    rsyncable_threshold=DEFAULT_RSYNCABLE_THRESHOLD,
))]
#[allow(clippy::too_many_arguments)]
fn compile_gts_native(
    py: Python<'_>,
    base_data: &Bound<'_, PyBytes>,
    base_format: PyRdfFormat,
    base_scope: Option<String>,
    rdf12_data: Option<&Bound<'_, PyBytes>>,
    rdf12_format: Option<PyRdfFormat>,
    rdf12_graph_name: Option<String>,
    rdf12_scope: Option<String>,
    named_graphs: Option<Vec<NamedGraphRow<'_>>>,
    transform: Option<Vec<String>>,
    doc_blobs: Option<&Bound<'_, PyList>>,
    report_blobs: Option<&Bound<'_, PyList>>,
    signer_secret: Option<&Bound<'_, PyBytes>>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
    rsyncable_threshold: usize,
) -> PyResult<Py<PyBytes>> {
    let mut builder = SnapshotBuilder::default();

    let base = parse_rdf(base_data, base_format)?;
    builder.add_quads(&base, None, base_scope.as_deref());

    if let Some(data) = rdf12_data {
        let format = rdf12_format
            .ok_or_else(|| PyValueError::new_err("rdf12_data requires rdf12_format"))?;
        let quads = parse_rdf(data, format)?;
        builder
            .add_rdf12(&quads, rdf12_graph_name.as_deref(), rdf12_scope.as_deref())
            .map_err(PyValueError::new_err)?;
    }

    for (data, format, graph_name, scope) in named_graphs.unwrap_or_default() {
        let ox = parse_rdf(&data, format)?;
        builder.add_quads(&ox, graph_name.as_deref(), scope.as_deref());
    }

    let bytes = emit_gts(
        &builder,
        "dist",
        transform,
        blob_rows_from_py(doc_blobs)?,
        blob_rows_from_py(report_blobs)?,
        secret_array(signer_secret)?,
        signer_kid,
        public_key_armor,
        rsyncable_threshold,
    )
    .map_err(PyValueError::new_err)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// The `blake3:<hex>` snapshot content id of a base graph (RDF bytes), mirroring
/// `_Builder.snapshot_content_id` for the feedback-bundle self-attestation (#654).
#[pyfunction]
#[pyo3(signature = (data, *, format))]
fn snapshot_content_id_native(data: &Bound<'_, PyBytes>, format: PyRdfFormat) -> PyResult<String> {
    let ox_quads = parse_rdf(data, format)?;
    let mut builder = SnapshotBuilder::default();
    builder.add_quads(&ox_quads, None, None);
    Ok(builder.snapshot_content_id())
}

/// Build a feedback bundle: a base graph (RDF bytes) as the snapshot, report blobs
/// riding ahead. Mirrors `feedback_bundle.build_feedback_bundle`'s `_Builder.to_gts`.
#[pyfunction]
#[pyo3(signature = (data, *, format, report_blobs=None))]
fn feedback_bundle_native(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    format: PyRdfFormat,
    report_blobs: Option<&Bound<'_, PyList>>,
) -> PyResult<Py<PyBytes>> {
    let ox_quads = parse_rdf(data, format)?;
    let mut builder = SnapshotBuilder::default();
    builder.add_quads(&ox_quads, None, None);
    let bytes = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        blob_rows_from_py(report_blobs)?,
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(PyValueError::new_err)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

fn rdf_format(format: PyRdfFormat) -> RdfFormat {
    match format {
        PyRdfFormat::TURTLE => RdfFormat::Turtle,
        PyRdfFormat::N_TRIPLES => RdfFormat::NTriples,
        PyRdfFormat::N_QUADS => RdfFormat::NQuads,
        PyRdfFormat::TRIG => RdfFormat::TriG,
    }
}

/// Register the native GTS producer surface on the `gmeow_rdf` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(gts_from_quads, m)?)?;
    m.add_function(wrap_pyfunction!(gts_from_rdf12_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(compile_gts_native, m)?)?;
    m.add_function(wrap_pyfunction!(snapshot_content_id_native, m)?)?;
    m.add_function(wrap_pyfunction!(feedback_bundle_native, m)?)?;
    crate::py_gts_dataset::register(m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Pure-Rust coverage of the `SnapshotBuilder` core (no Python interpreter):
    //! interning order, content sort, the snapshot payload, and the content-id.
    use super::*;
    use crate::py_store::parse_quads;
    use oxigraph::io::RdfFormat;

    fn ingest(nq: &str) -> SnapshotBuilder {
        let quads = parse_quads(nq.as_bytes(), RdfFormat::NQuads).expect("parse");
        let mut b = SnapshotBuilder::default();
        b.add_quads(&quads, None, None);
        b
    }

    #[test]
    fn content_sort_is_iris_first_then_value() {
        // Two IRIs + a literal; after canonical_tables the term ids sort by
        // (kind, value, …) so the literal (kind=1) follows every IRI (kind=0).
        let b = ingest(
            "<https://e/s> <https://e/p> \"z\" .\n<https://e/s> <https://e/p> <https://e/a> .\n",
        );
        let (terms, _quads, _r, _a) = b.canonical_tables();
        // The last term is the literal; everything before it is an IRI.
        let (last, rest) = terms.split_last().expect("non-empty");
        assert_eq!(last.kind, TermKind::Literal);
        assert!(rest.iter().all(|t| t.kind == TermKind::Iri));
    }

    #[test]
    fn xsd_string_datatype_is_implicit() {
        // A plain literal and an explicit xsd:string literal intern to the SAME
        // term (the datatype is implied), so only one literal term exists.
        let b = ingest(concat!(
            "<https://e/s> <https://e/p> \"x\" .\n",
            "<https://e/s2> <https://e/p> ",
            "\"x\"^^<http://www.w3.org/2001/XMLSchema#string> .\n",
        ));
        let (terms, _q, _r, _a) = b.canonical_tables();
        let literals = terms.iter().filter(|t| t.kind == TermKind::Literal).count();
        assert_eq!(
            literals, 1,
            "explicit xsd:string folds with the bare literal"
        );
    }

    #[test]
    fn snapshot_content_id_is_order_independent() {
        // The content id is a pure function of the graph, independent of ingest
        // order (the canonical content sort + dedup makes it so).
        let a = ingest(
            "<https://e/a> <https://e/p> <https://e/b> .\n<https://e/c> <https://e/p> <https://e/d> .\n",
        );
        let b = ingest(
            "<https://e/c> <https://e/p> <https://e/d> .\n<https://e/a> <https://e/p> <https://e/b> .\n",
        );
        assert_eq!(a.snapshot_content_id(), b.snapshot_content_id());
        assert!(a.snapshot_content_id().starts_with("blake3:"));
    }

    #[test]
    fn rdf12_reifier_classifies_annotations() {
        // A reifier's non-reifies triples become annotations; the reified triple
        // becomes a reifies binding; the reifier subject is not a base quad.
        let quads = parse_quads(
            concat!(
                "<https://e/r> ",
                "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
                "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
                "<https://e/r> <https://e/confidence> \"0.9\" .\n",
            )
            .as_bytes(),
            RdfFormat::NTriples,
        )
        .expect("parse rdf12");
        let mut b = SnapshotBuilder::default();
        b.add_rdf12(&quads, None, None).expect("ingest");
        let (_terms, quads, reifies, annot) = b.canonical_tables();
        assert_eq!(reifies.len(), 1, "one reifies binding");
        assert_eq!(annot.len(), 1, "one annotation row");
        assert!(quads.is_empty(), "reifier subject is not a base quad");
    }

    #[test]
    fn conflicting_reifier_rebind_is_rejected() {
        let quads = parse_quads(
            concat!(
                "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
                "<<( <https://e/s> <https://e/p> <https://e/o1> )>> .\n",
                "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
                "<<( <https://e/s> <https://e/p> <https://e/o2> )>> .\n",
            )
            .as_bytes(),
            RdfFormat::NTriples,
        )
        .expect("parse");
        let mut b = SnapshotBuilder::default();
        let err = b
            .add_rdf12(&quads, None, None)
            .expect_err("conflict must error");
        assert!(err.contains("conflicting reifier rebind"), "{err}");
    }
}
