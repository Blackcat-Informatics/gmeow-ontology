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

use oxigraph::model::Quad;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use crate::bundle::{RdfBundle, UnitMetadata};
// The byte-emitting compose core now lives in the pyo3-free `gts_compose` module
// (#861 P6); this surface is the thin pyo3 wrapper that delegates to it.
use crate::gts_compose::{emit_gts, BlobRow, SnapshotBuilder, DEFAULT_RSYNCABLE_THRESHOLD};
use crate::ir::{RdfDataset, RdfDatasetBuilder};
use crate::provenance::{DatasetProvenance, OriginKind};
use crate::py_store::{parse_quads, PyRdfFormat};
use crate::NativeRdfFormat;

/// The `rep`-label prefix every S3 slice-artifact blob carries (#820 S3). A blob
/// authored from the slice catalog rides ahead of the snapshot with
/// `rep == "slice-artifact:{role}:{logical_path}"`, so a repo-free consumer can
/// recover each ontology artifact by role + logical path + content digest. This
/// is the SAME content-addressed blob channel `doc_blobs` use — never a parallel
/// one (greenfield, one embedding).
const SLICE_ARTIFACT_REP_PREFIX: &str = "slice-artifact:";
/// One slice artifact row passed from Python (`gts_gen.py` via `gmeow_slice`):
/// `(slice_iri, slice_name, role, logical_path, content)`. `logical_path` is the
/// repo-relative path (e.g. `slices/core/epistemics/module.ttl`) and is the
/// bundle's normalized artifact path. Only the small ontology text artifacts
/// (module / shapes / docs / manifest) are passed here; the large external DATA
/// blobs (`graph.blobs`) STAY by-reference and never travel this channel
/// (blob-by-reference doctrine, gmeow-gts#248).
struct SliceArtifactRow {
    slice_iri: String,
    slice_name: String,
    role: String,
    logical_path: String,
    content: Vec<u8>,
}

/// Intern an oxigraph subject/blank node into the IR builder.
fn intern_ir_subject(
    b: &mut RdfDatasetBuilder,
    s: &oxigraph::model::NamedOrBlankNode,
) -> crate::ir::TermId {
    use crate::ir::BlankScope;
    use oxigraph::model::NamedOrBlankNode;
    match s {
        NamedOrBlankNode::NamedNode(n) => b.intern_iri(n.as_str().to_string()),
        NamedOrBlankNode::BlankNode(bn) => {
            b.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT)
        }
    }
}

/// Intern an oxigraph object term into the IR builder (recursive for triple terms).
fn intern_ir_object(b: &mut RdfDatasetBuilder, o: &oxigraph::model::Term) -> crate::ir::TermId {
    use crate::{RdfLiteral, RdfTextDirection};
    use oxigraph::model::{BaseDirection, Term as OxTerm};
    match o {
        OxTerm::NamedNode(n) => b.intern_iri(n.as_str().to_string()),
        OxTerm::BlankNode(bn) => {
            use crate::ir::BlankScope;
            b.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT)
        }
        OxTerm::Literal(l) => {
            let direction = l.direction().map(|d| match d {
                BaseDirection::Ltr => RdfTextDirection::Ltr,
                BaseDirection::Rtl => RdfTextDirection::Rtl,
            });
            b.intern_literal(RdfLiteral {
                lexical_form: l.value().to_string(),
                datatype: Some(l.datatype().as_str().to_string()),
                language: l.language().map(str::to_string),
                direction,
            })
        }
        OxTerm::Triple(t) => {
            let s = intern_ir_subject(b, &t.subject);
            let p = b.intern_iri(t.predicate.as_str().to_string());
            let inner_o = intern_ir_object(b, &t.object);
            b.intern_triple(s, p, inner_o)
        }
    }
}

/// Build a frozen [`RdfDataset`] from a flat oxigraph quad list. Used so the
/// production [`RdfBundle`] carries the actual hot graph (not a placeholder) while
/// it gates the artifact index.
fn dataset_from_ox_quads(quads: &[Quad]) -> Result<std::sync::Arc<RdfDataset>, String> {
    use crate::ir::BlankScope;
    use oxigraph::model::GraphName;

    let mut b = RdfDatasetBuilder::new();
    for q in quads {
        let s = intern_ir_subject(&mut b, &q.subject);
        let p = b.intern_iri(q.predicate.as_str().to_string());
        let o = intern_ir_object(&mut b, &q.object);
        let g = match &q.graph_name {
            GraphName::DefaultGraph => None,
            GraphName::NamedNode(n) => Some(b.intern_iri(n.as_str().to_string())),
            GraphName::BlankNode(bn) => {
                Some(b.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT))
            }
        };
        b.push_quad(s, p, o, g);
    }
    b.freeze()
        .map_err(|e| format!("bundle dataset freeze failed: {e}"))
}

/// Assemble the self-describing S3 [`RdfBundle`] from the slice-artifact rows and
/// the parsed base graph, hard-fail `validate()` it, and return the artifact bytes
/// as content-addressed [`BlobRow`]s to embed (#820 S3, gap G4).
///
/// One [`UnitId`] per slice (metadata = slice IRI + name), one content-addressed
/// `ArtifactRecord` per ontology artifact, every blob inserted into the bundle's
/// `ContentStore`. The producer emits a SINGLE `snapshot` frame, so every unit is
/// associated with that one snapshot segment (segment 0) — set-valued and never
/// assuming one-segment == one-slice. The blob rows ride the SAME channel
/// `doc_blobs` use; the bundle's `dataset` carries the real hot graph.
fn assemble_slice_bundle(
    base_quads: &[Quad],
    rows: &[SliceArtifactRow],
) -> Result<Vec<BlobRow>, String> {
    const SNAPSHOT_SEGMENT: usize = 0;

    let dataset = dataset_from_ox_quads(base_quads)?;
    let provenance = DatasetProvenance::new();
    let mut bundle = RdfBundle::new(dataset, provenance);

    let mut blob_rows: Vec<BlobRow> = Vec::with_capacity(rows.len());
    for row in rows {
        // One UnitId per slice (idempotent intern); metadata = IRI + name.
        let unit = bundle
            .provenance
            .register_unit(row.slice_iri.clone(), OriginKind::Source);
        bundle.add_unit(
            unit,
            UnitMetadata::new(row.slice_iri.clone(), row.slice_name.clone()),
        );
        // One content-addressed artifact per ontology file (bytes → ContentStore).
        let artifact = bundle
            .provenance
            .register_artifact(row.logical_path.clone());
        bundle.add_artifact(
            artifact,
            unit,
            row.logical_path.clone(),
            row.role.clone(),
            row.content.clone(),
        );
        // Every unit lives in the single snapshot segment (set-valued S0.7).
        bundle.associate_segment(SNAPSHOT_SEGMENT, unit);

        // The SAME content-addressed blob channel doc_blobs ride: rep encodes
        // role + logical path so a repo-free consumer recovers each artifact.
        blob_rows.push(BlobRow {
            data: row.content.clone(),
            media_type: media_type_for(&row.logical_path),
            rep: format!(
                "{SLICE_ARTIFACT_REP_PREFIX}{}:{}",
                row.role, row.logical_path
            ),
        });
    }

    // HARD-fail on any structural violation BEFORE serialization (no-optionality).
    bundle.validate().map_err(|e| e.to_string())?;
    Ok(blob_rows)
}

/// Infer a stable MIME type for a slice artifact path (mirrors the slice catalog's
/// `infer_media_type`, kept local to avoid a kernel→slice dependency edge).
fn media_type_for(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "ttl" => "text/turtle",
        "nt" => "application/n-triples",
        "nq" => "application/n-quads",
        "sparql" | "rq" => "application/sparql-query",
        "md" => "text/markdown",
        "yaml" | "yml" | "cff" => "application/yaml",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
    .to_string()
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

/// Parse the slice-artifact rows passed from Python: each is the tuple
/// `(slice_iri, slice_name, role, logical_path, content)`.
fn slice_artifact_rows_from_py(
    rows: Option<&Bound<'_, PyList>>,
) -> PyResult<Vec<SliceArtifactRow>> {
    let Some(rows) = rows else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(rows.len());
    for item in rows.iter() {
        let (slice_iri, slice_name, role, logical_path, content): (
            String,
            String,
            String,
            String,
            Vec<u8>,
        ) = item.extract().map_err(|_| {
            PyValueError::new_err(
                "slice artifact rows must be (slice_iri, slice_name, role, logical_path, content)",
            )
        })?;
        out.push(SliceArtifactRow {
            slice_iri,
            slice_name,
            role,
            logical_path,
            content,
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

/// Build a `dist`-profile GTS snapshot from serialized RDF bytes — the shared
/// front half of the JSON-LD-star / RDF-XML serializers. RDF-1.1 quads only
/// (the compat `Graph` facade carries no quoted-triple terms).
fn rdf_to_gts_snapshot(data: &Bound<'_, PyBytes>, format: PyRdfFormat) -> PyResult<Vec<u8>> {
    let ox_quads = parse_rdf(data, format)?;
    let mut builder = SnapshotBuilder::default();
    builder.add_quads(&ox_quads, None, None);
    emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(PyValueError::new_err)
}

/// Serialize RDF bytes to **JSON-LD-star** (RDF-1.2-faithful) via the gmeow-gts
/// codec: RDF → GTS snapshot → `gmeow_gts::yamlld::to_json_ld_string`. This is the
/// RDF-1.2-first JSON-LD form the published `*.jsonld` artifacts now emit (#834).
#[pyfunction]
#[pyo3(signature = (data, *, format))]
fn to_json_ld(data: &Bound<'_, PyBytes>, format: PyRdfFormat) -> PyResult<String> {
    let gts_bytes = rdf_to_gts_snapshot(data, format)?;
    let graph = gmeow_gts::reader::read(&gts_bytes, false, None);
    gmeow_gts::yamlld::to_json_ld_string(&graph)
        .map_err(|e| PyValueError::new_err(format!("json-ld-star serialization error: {e}")))
}

/// Parse **JSON-LD-star** text into N-Quads bytes, via the gmeow-gts codec:
/// `gmeow_gts::from_yamlld::from_json_ld` → GTS → N-Quads.
#[pyfunction]
fn from_json_ld(py: Python<'_>, text: &str) -> PyResult<Py<PyBytes>> {
    let gts_bytes = gmeow_gts::from_yamlld::from_json_ld(text)
        .map_err(|e| PyValueError::new_err(format!("json-ld-star parse error: {e}")))?;
    let graph = gmeow_gts::reader::read(&gts_bytes, false, None);
    let nquads = gmeow_gts::nquads::to_nquads(&graph);
    Ok(PyBytes::new(py, nquads.as_bytes()).unbind())
}

/// Serialize RDF bytes to **RDF/XML** via the gmeow-gts codec: RDF → GTS snapshot
/// → `gmeow_gts::rdf_codecs::to_rdf_xml`.
#[pyfunction]
#[pyo3(signature = (data, *, format))]
fn to_rdf_xml(data: &Bound<'_, PyBytes>, format: PyRdfFormat) -> PyResult<String> {
    let gts_bytes = rdf_to_gts_snapshot(data, format)?;
    let graph = gmeow_gts::reader::read(&gts_bytes, false, None);
    gmeow_gts::rdf_codecs::to_rdf_xml(&graph)
        .map_err(|e| PyValueError::new_err(format!("rdf/xml serialization error: {e}")))
}

/// Parse **RDF/XML** text into N-Quads bytes, via the gmeow-gts codec:
/// `gmeow_gts::rdf_codecs::from_rdf_xml` → GTS → N-Quads.
#[pyfunction]
fn from_rdf_xml(py: Python<'_>, text: &str) -> PyResult<Py<PyBytes>> {
    let gts_bytes = gmeow_gts::rdf_codecs::from_rdf_xml(text)
        .map_err(|e| PyValueError::new_err(format!("rdf/xml parse error: {e}")))?;
    let graph = gmeow_gts::reader::read(&gts_bytes, false, None);
    let nquads = gmeow_gts::nquads::to_nquads(&graph);
    Ok(PyBytes::new(py, nquads.as_bytes()).unbind())
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
    slice_artifacts=None,
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
    slice_artifacts: Option<&Bound<'_, PyList>>,
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

    // S3 (#820, gap G4): assemble the self-describing RdfBundle from the slice
    // catalog rows, hard-fail `validate()`, and fold each ontology artifact in as
    // a content-addressed blob through the SAME channel doc_blobs ride. The base
    // graph is the bundle's hot dataset. Large external DATA blobs (graph.blobs)
    // are NOT passed here and STAY by-reference (blob-by-reference doctrine).
    let mut all_doc_blobs = blob_rows_from_py(doc_blobs)?;
    let slice_rows = slice_artifact_rows_from_py(slice_artifacts)?;
    if !slice_rows.is_empty() {
        let bundle_blobs =
            assemble_slice_bundle(&base, &slice_rows).map_err(PyValueError::new_err)?;
        all_doc_blobs.extend(bundle_blobs);
    }

    let bytes = emit_gts(
        &builder,
        "dist",
        transform,
        all_doc_blobs,
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

fn rdf_format(format: PyRdfFormat) -> NativeRdfFormat {
    format.to_native()
}

/// Register the native GTS producer surface on the `gmeow_rdf` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(gts_from_quads, m)?)?;
    m.add_function(wrap_pyfunction!(gts_from_rdf12_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(compile_gts_native, m)?)?;
    m.add_function(wrap_pyfunction!(snapshot_content_id_native, m)?)?;
    m.add_function(wrap_pyfunction!(feedback_bundle_native, m)?)?;
    m.add_function(wrap_pyfunction!(to_json_ld, m)?)?;
    m.add_function(wrap_pyfunction!(from_json_ld, m)?)?;
    m.add_function(wrap_pyfunction!(to_rdf_xml, m)?)?;
    m.add_function(wrap_pyfunction!(from_rdf_xml, m)?)?;
    crate::py_gts_dataset::register(m)?;
    Ok(())
}
