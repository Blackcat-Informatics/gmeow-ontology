// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The structured multi-named-graph snapshot assembly (#861 P6 fold-parity gate).
//!
//! This re-cuts `src/gmeow_tools/gts_gen.py::build_snapshot_bytes` natively: the
//! committed `generated/dist/gmeow.gts` is NOT everything-in-the-default-graph —
//! it is a STRUCTURED snapshot whose default graph carries the AUTHORED ontology
//! only (`ontology/gmeow.ttl` + slice `module.ttl`, NO imports/mappings/reason),
//! with the import closure, self-description metadata, SSSOM alignment axioms,
//! the RDF 1.2 statement layer, the slice-analysis graph, the verify attestation,
//! and the documentation projection each riding their own named graph, plus the
//! RDF 1.2 reifier/annotation tables and the content-addressed blob channel.
//!
//! It assembles a [`gmeow_rdf::gts_compose::SnapshotBuilder`] directly — the same
//! pyo3-free core the `gmeow_rdf` Python producer now delegates to — routing each
//! source into the named graph `gts_gen.py` assigns it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_rdf::gts_compose::{emit_gts, BlobRow, SnapshotBuilder};
use gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset;
use gmeow_rdf::provenance::DatasetProvenance;
use gmeow_rdf::{parse_dataset, serialize_dataset, SerializeGraph};
use oxigraph::model::Quad;
use rayon::prelude::*;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::statements::RDF12_PATH;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The committed logical path of the serialized GTS bundle — the single artifact
/// this stage produces and every fold-reading leaf (and the sink) consumes.
pub const SNAPSHOT_PATH: &str = "generated/dist/gmeow.gts";

/// The named-graph IRIs (mirror `config.GTS_GRAPH_*`).
const GRAPH_IMPORTS: &str = "https://blackcatinformatics.ca/gmeow/graph/imports";
const GRAPH_METADATA: &str = "https://blackcatinformatics.ca/gmeow/graph/metadata";
const GRAPH_ALIGNMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/alignments";
const GRAPH_STATEMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";
const GRAPH_VERIFY: &str = "https://blackcatinformatics.ca/gmeow/graph/verify";
const GRAPH_SLICE_ANALYSIS: &str = "https://blackcatinformatics.ca/gmeow/graph/slice-analysis";
const GRAPH_DOCUMENTATION: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
/// The native↔external-corpus reasoning-divergence Findings, folded as their own
/// queryable named graph so a repo-free consumer reads every coverage divergence
/// (native-incomplete `DlGap` / native-disagrees `CorpusOnly`) against the W3C
/// published expected verdicts without re-grading the corpus. Sibling of
/// `graph/diagnostics` (correctness evidence, not validation/lint findings).
const GRAPH_CONFORMANCE: &str = "https://blackcatinformatics.ca/gmeow/graph/conformance";
/// The compiler's projection-report loss ledger, folded as its own queryable named
/// graph so a repo-free consumer reads every projection's preservation kind and
/// structural lossy drops without re-running the compiler.
const GRAPH_PROJECTION_LEDGER: &str =
    "https://blackcatinformatics.ca/gmeow/graph/projection-ledger";
const REP_SHACL_SARIF: &str = "gmeow:report/shacl/sarif";
const REP_SHACL_FINDINGS: &str = "gmeow:report/shacl/findings";

/// Assemble the structured `dist` snapshot bytes from the repo `root` and the
/// upstream stage products (statements RDF 1.2, mappings SSSOM, docs graph).
///
/// Mirrors the two-pass `build_snapshot_bytes`: pass 1 omits the verify graph,
/// the native verify lane runs over it, and pass 2 folds the attestation in as
/// `gmeow:graph/verify`.
pub fn build_snapshot(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
    blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
) -> Result<Vec<u8>, PipelineError> {
    let carrier = assemble_carrier(root, upstream)?;
    serialize_snapshot(&carrier, blobs, report_blobs)
}

/// Gather the by-reference archive blobs (#699/#746/#897/#940) + the SHACL report
/// blobs from `upstream`, and serialize the ALREADY-ASSEMBLED `carrier` into the
/// terminal `gmeow.gts` package — the SINGLE serialization the terminal gts sink
/// performs (#1132 Stage C). The carrier is taken off the snapshot product's bundle,
/// never re-assembled (the razor: transform transport→form at most once per pipeline).
///
/// Mirrors the former snapshot-stage blob gathering exactly: REP_AXIOMS/schemas from
/// the in-memory `stage-compile-logic` / `stage-export-json-schema` products (one-pass
/// freshness), the reasoning reports from `stage-reason`, and the SHACL SARIF/findings
/// from `stage-validate`. Every missing artifact HARD-fails (no-optionality).
pub(crate) fn serialize_carrier_snapshot(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
    carrier: &gmeow_rdf::RdfDataset,
) -> Result<Vec<u8>, PipelineError> {
    // THIS run's freshly-emitted JSON Schema + OpenAPI bytes (from the in-memory
    // product, not the on-disk files which are not written until phase 1 returns).
    let schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.schema.json artifact"))?
        .to_vec();
    let openapi_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::OPENAPI_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.openapi.json artifact"))?
        .to_vec();
    // THIS run's compiled axiom surface (REP_AXIOMS), from the stage-compile-logic
    // product so it never lags a regenerate.
    let compile_artifacts = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?
        .artifacts();
    let mut blobs = build_archive_blobs(root, &schema_json, &openapi_json, &compile_artifacts)?;
    blobs.extend(build_guide_blobs(root)?);
    blobs.push(build_docs_archive(root)?);
    blobs.push(build_reasoning_blob(upstream)?);
    // The opaque-fanout archive: every non-RDF generated/ fanout output, recomputed
    // from THIS run's carrier (superset law — RDF rides as named graphs, not here).
    blobs.push(build_fanout_opaque_blob(root, carrier, upstream)?);

    let shacl_json = upstream
        .get("stage-validate")
        .and_then(|p| p.artifact(crate::stages::validate::SHACL_JSON_PATH))
        .ok_or_else(|| stage_err("missing validate-stage SHACL diagnostics JSON"))?
        .to_vec();
    let shacl_sarif = upstream
        .get("stage-validate")
        .and_then(|p| p.artifact(crate::stages::validate::SHACL_SARIF_PATH))
        .ok_or_else(|| stage_err("missing validate-stage SHACL diagnostics SARIF"))?
        .to_vec();
    let report_blobs = vec![
        BlobRow {
            data: shacl_sarif,
            media_type: "application/sarif+json".to_string(),
            rep: REP_SHACL_SARIF.to_string(),
        },
        BlobRow {
            data: shacl_json,
            media_type: "application/json".to_string(),
            rep: REP_SHACL_FINDINGS.to_string(),
        },
    ];
    serialize_snapshot(carrier, blobs, report_blobs)
}

/// Assemble the FULL snapshot carrier: every named graph parsed into ONE native
/// `RdfDataset` and unioned once. The carried logic / relational-core / correspondence
/// / reasoning graphs ride in from the upstream producers' carriers (no re-derivation);
/// the snapshot-owned graphs (authored default, statement layer, imports, metadata,
/// alignments, slice-analysis, verify, documentation, diagnostics, conformance,
/// projection-ledger, provenance) are parsed and re-rooted here. This carrier is the
/// single internal transport — it is BOTH serialized to gts and carried as the snapshot
/// product's bundle, so the snapshot is assembled ONCE.
fn assemble_carrier(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    // ── the authored default graph (ontology + slice modules; NO imports) ──────
    let authored = load_authored_default(root)?;
    let authored_canon = canonicalize_nq(&authored, "base")?;
    // Reject quoted triples in the authored default graph (it carries none); the
    // default graph is NOT re-rooted.
    reject_quoted_triples(&parse_nq(authored_canon.as_bytes())?, "<default>")?;
    let base = parse_dataset(authored_canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("base parse: {e}")))?;

    // ── the snapshot-owned named-graph sources ─────────────────────────────────
    let imports = load_imports(root)?;
    let metadata = load_metadata(root)?;
    let alignments = load_alignments(root)?;
    let slice_analysis = build_slice_analysis(root, &authored)?;
    let rdf12 = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(RDF12_PATH))
        .ok_or_else(|| stage_err("missing statements RDF 1.2 artifact"))?
        .to_vec();
    let documentation = upstream
        .get("stage-docs-render")
        .and_then(|p| p.artifact(crate::stages::docs_render::DOCS_GRAPH_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing docs-render documentation graph"))?;
    // graph/diagnostics ← SHACL diagnostics ∪ logic-compile diagnostics.
    let mut diagnostics = upstream
        .get("stage-validate")
        .and_then(|p| p.artifact(crate::stages::validate::SHACL_RDF_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing validate-stage SHACL diagnostics RDF graph"))?;
    let compile_diagnostics = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::DIAG_RDF_PATH))
        .ok_or_else(|| stage_err("missing compile-logic diagnostics RDF graph"))?;
    if !diagnostics.is_empty() && !diagnostics.ends_with(b"\n") {
        diagnostics.push(b'\n');
    }
    diagnostics.extend_from_slice(compile_diagnostics);
    let conformance = upstream
        .get("stage-conformance")
        .and_then(|p| p.artifact(crate::stages::conformance::CONFORMANCE_NQ_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing conformance-stage divergence Finding graph"))?;
    let projection_ledger = {
        let report_ttl = upstream
            .get("stage-mappings")
            .and_then(|p| p.artifact(crate::stages::compile_logic::PROJECTION_REPORT_PATH))
            .ok_or_else(|| stage_err("missing mappings projection-report loss ledger"))?;
        turtle_to_nquads(report_ttl)?
    };
    // graph/verify ← native attestation over the authored ∪ imports EDB (the verify
    // graph is never its own input — #695).
    let verify_attestation = {
        let imports_ds = parse_dataset(&imports, "text/turtle", None)
            .map_err(|e| stage_err(&format!("verify imports parse: {e}")))?;
        let edb = gmeow_rdf::RdfDataset::union(&[base.as_ref(), imports_ds.as_ref()]);
        run_verify_attestation(root, &edb)?
    };
    // graph/provenance ← the gated public provenance projection (S0.5).
    let provenance_nt = build_provenance_projection(root)?;

    // ── the carried graphs ride in from the producers' carriers ────────────────
    let compile = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?;
    let reason = upstream
        .get("stage-reason")
        .ok_or_else(|| stage_err("missing stage-reason product for the Reasoning handle"))?;
    let logic_iri = crate::stages::compile_logic::GRAPH_LOGIC;
    let rc_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
    let corr_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
    let reasoning_iri = gmeow_logic::result_rdf::GRAPH_REASONING;

    // ── route every snapshot-owned source into its named graph, then union all ──
    let mut datasets: Vec<std::sync::Arc<gmeow_rdf::RdfDataset>> = vec![
        base,
        parse_into_graph(&rdf12, "text/turtle", GRAPH_STATEMENTS)?,
        parse_into_graph(&imports, "application/n-quads", GRAPH_IMPORTS)?,
        parse_into_graph(&metadata, "application/n-quads", GRAPH_METADATA)?,
        parse_into_graph(&alignments, "application/n-quads", GRAPH_ALIGNMENTS)?,
        parse_into_graph(&slice_analysis, "application/n-quads", GRAPH_SLICE_ANALYSIS)?,
        parse_into_graph(&verify_attestation, "application/n-quads", GRAPH_VERIFY)?,
        parse_into_graph(&documentation, "application/n-quads", GRAPH_DOCUMENTATION)?,
        parse_into_graph(&diagnostics, "application/n-quads", GRAPH_DIAGNOSTICS)?,
        parse_into_graph(
            &projection_ledger,
            "application/n-quads",
            GRAPH_PROJECTION_LEDGER,
        )?,
        parse_into_graph(
            provenance_nt.as_bytes(),
            "application/n-triples",
            crate::stages::provenance_graph::GRAPH_PROVENANCE,
        )?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(logic_iri),
            logic_iri,
        )?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(rc_iri),
            rc_iri,
        )?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(corr_iri),
            corr_iri,
        )?,
        rooted_in_graph(
            &reason.bundle().dataset().project_named_graph(reasoning_iri),
            reasoning_iri,
        )?,
    ];
    // graph/conformance is folded only when non-empty (an all-agree corpus has none).
    if !conformance.is_empty() {
        datasets.push(parse_into_graph(
            &conformance,
            "application/n-quads",
            GRAPH_CONFORMANCE,
        )?);
    }
    // graph/projections/<name>.edoal ← each committed EDOAL projection, one named
    // graph per file. EDOAL renders through the canonical-Turtle serializer, so the
    // fold of its named graph reproduces the committed bytes exactly (superset law).
    let mappings = upstream
        .get("stage-mappings")
        .ok_or_else(|| stage_err("missing stage-mappings product for projection graphs"))?;
    for (path, bytes) in mappings.artifacts() {
        if let Some(iri) = crate::stages::superset::edoal_projection_graph_iri(&path) {
            datasets.push(parse_into_graph(&bytes, "text/turtle", &iri)?);
        } else if crate::stages::superset::is_rdf_fanout_class(&path) {
            // The non-EDOAL RDF projections (core-prefixes / functions.fno /
            // list-functions), now emitted canonically by the mappings stage.
            if let Some(iri) = crate::stages::superset::rdf_fanout_graph_iri(&path) {
                datasets.push(parse_into_graph(&bytes, "text/turtle", &iri)?);
            }
        }
    }
    // graph/fanout/<path> ← every other RDF generated/ file, one named graph per file,
    // recomputed from THIS run's source. Each producing stage emits the committed file
    // as the canonical fold of these triples, so the superset gate reconstructs them
    // byte-for-byte (PIPELINE_SPINE §5; RDF travels as RDF, never a blob).
    for (path, bytes) in rdf_fanout_members(root, upstream)? {
        let iri = crate::stages::superset::rdf_fanout_graph_iri(&path)
            .ok_or_else(|| stage_err(&format!("non-RDF path in rdf_fanout_members: {path}")))?;
        let media_type = if path.ends_with(".nt") {
            "application/n-triples"
        } else if path.ends_with(".nq") {
            "application/n-quads"
        } else {
            "text/turtle"
        };
        datasets.push(parse_into_graph(&bytes, media_type, &iri)?);
    }
    let refs: Vec<&gmeow_rdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    Ok(std::sync::Arc::new(gmeow_rdf::RdfDataset::union(&refs)))
}

/// Every remaining RDF `generated/` file (committed-path → bytes) that rides as an
/// RDF-fanout named graph, recomputed from THIS run's source. Each is the canonical
/// fold the producing stage also emits as its committed file. Grows class-by-class.
fn rdf_fanout_members(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // profiles — `owl:Ontology` import closures, emitted as canonical Turtle.
    for (name, ttl) in crate::stages::profiles::render_profiles(root)? {
        out.insert(format!("generated/profiles/{name}"), ttl.into_bytes());
    }
    // research-objects — the RO-crate `.ttl` re-serializations + the `lillith.dcat.ttl`
    // CONSTRUCT (opaque JSON/HTML/XML members ride the opaque fanout blob).
    for (path, bytes) in crate::stages::research_objects::render_research_objects(root)? {
        if is_rdf_member(&path) {
            out.insert(path, bytes);
        }
    }
    // evals scores.ttl — the meta-claim Assessments (opaque MD/JSON ride the blob).
    for (path, bytes) in crate::stages::evals::render_evals(root)? {
        if is_rdf_member(&path) {
            out.insert(path, bytes);
        }
    }
    // gufo.ttl — the gUFO bridge projection, from the sink-consumed compile-logic
    // product (already emitted canonically).
    let gufo = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::GUFO_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/foundation/gufo.ttl in stage-compile-logic"))?;
    out.insert(crate::stages::compile_logic::GUFO_PATH.to_string(), gufo);

    // projection-report loss ledger (mappings) + the relational-core / correspondence
    // N-Triples programs (compile-logic), each emitted canonically by its stage.
    for (stage, path) in [
        (
            "stage-mappings",
            crate::stages::compile_logic::PROJECTION_REPORT_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::RELATIONAL_CORE_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::CORRESPONDENCE_PATH,
        ),
        // diagnostics `.nq` — one named graph per file (validate SHACL ∪ compile-logic),
        // each re-rooted into its own fanout container; the gate restamps back to the
        // shared `graph/diagnostics` label on fold.
        ("stage-validate", crate::stages::validate::SHACL_RDF_PATH),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_RDF_PATH,
        ),
    ] {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing {path} in {stage} for RDF fanout")))?;
        out.insert(path.to_string(), bytes);
    }
    Ok(out)
}

/// Assemble the OBJECT-LEVEL reasoned EDB: the authored default graph plus the
/// statement / import / alignment / logic / relational-core / correspondence named
/// graphs, in the EXACT graph layout [`assemble_carrier`] uses (so the reasoned
/// closure's worlds match the bundle's).
///
/// The meta/report graphs (metadata, slice-analysis, verify, documentation,
/// diagnostics, conformance, projection-ledger, provenance) are EXCLUDED: they assert
/// no object-level axioms, so they contribute zero inferences — reasoning over the
/// full fold vs this projection is isomorphic up to renaming of the content-addressed
/// Skolem witnesses. Excluding them makes the closure (and its witness IRIs) a
/// function of the ontology alone, not of its self-description. This is the single
/// EDB the sole `stage-reason` pass reasons over; it depends only on the
/// `stage-statements` and `stage-compile-logic` products plus the on-disk authored /
/// imports / alignments sources — never on the snapshot, so reasoning need not wait
/// on carrier assembly.
pub(crate) fn assemble_object_level_edb(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    let authored = load_authored_default(root)?;
    let authored_canon = canonicalize_nq(&authored, "base")?;
    reject_quoted_triples(&parse_nq(authored_canon.as_bytes())?, "<default>")?;
    let base = parse_dataset(authored_canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("base parse: {e}")))?;

    let imports = load_imports(root)?;
    let alignments = load_alignments(root)?;
    let rdf12 = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(RDF12_PATH))
        .ok_or_else(|| stage_err("missing statements RDF 1.2 artifact"))?
        .to_vec();
    let compile = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?;
    let logic_iri = crate::stages::compile_logic::GRAPH_LOGIC;
    let rc_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
    let corr_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;

    let datasets: Vec<std::sync::Arc<gmeow_rdf::RdfDataset>> = vec![
        base,
        parse_into_graph(&rdf12, "text/turtle", GRAPH_STATEMENTS)?,
        parse_into_graph(&imports, "application/n-quads", GRAPH_IMPORTS)?,
        parse_into_graph(&alignments, "application/n-quads", GRAPH_ALIGNMENTS)?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(logic_iri),
            logic_iri,
        )?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(rc_iri),
            rc_iri,
        )?,
        rooted_in_graph(
            &compile.bundle().dataset().project_named_graph(corr_iri),
            corr_iri,
        )?,
    ];
    let refs: Vec<&gmeow_rdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    Ok(std::sync::Arc::new(gmeow_rdf::RdfDataset::union(&refs)))
}

/// Serialize the fully-assembled carrier to the `dist`-profile `gmeow.gts` bytes: fold
/// the carrier into the snapshot frame (native ingestion), staple the JSON-LD-star /
/// OKF / caller blobs, and emit. The SOLE serialization of the snapshot.
fn serialize_snapshot(
    carrier: &gmeow_rdf::RdfDataset,
    blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
) -> Result<Vec<u8>, PipelineError> {
    let mut builder = SnapshotBuilder::new();
    builder
        .add_dataset(carrier)
        .map_err(|e| stage_err(&format!("fold carrier into snapshot: {e}")))?;
    // The JSON-LD-star + OKF archive blobs serialize the SAME native carrier dataset
    // (the value just folded into the builder) — no gts round-trip.
    let yaml_ld_blob = build_yaml_ld_blob_from_dataset(carrier)?;
    let okf_blob = build_okf_blob_from_dataset(carrier)?;
    let mut blobs = blobs;
    blobs.push(yaml_ld_blob);
    blobs.push(okf_blob);
    emit_gts(
        &builder,
        "dist",
        // zstd-rsyncable, not gzip: rsyncable keeps chunk boundaries stable across
        // regenerations, so a small ontology change yields a small binary diff in the
        // committed `gmeow.gts` instead of churning the whole file. (The absolute size
        // is larger than gzip at the gts library's current fixed zstd level — a
        // per-frame configurable level is tracked upstream in gmeow-gts.)
        Some(vec!["zstd-rsyncable".to_string()]),
        blobs,
        report_blobs,
        None,
        None,
        None,
        gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(|e| stage_err(&format!("emit_gts: {e}")))
}

/// Parse `bytes` natively and re-root every quad into `graph_iri` (see
/// [`rooted_in_graph`]).
fn parse_into_graph(
    bytes: &[u8],
    media_type: &str,
    graph_iri: &str,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    let parsed = parse_dataset(bytes, media_type, None)
        .map_err(|e| stage_err(&format!("parse <{graph_iri}>: {e}")))?;
    rooted_in_graph(&parsed, graph_iri)
}

/// Borrow the upstream `stage-snapshot` product (or HARD-fail if a leaf forgot to
/// declare the consumes edge — fail-closed, no-optionality).
fn snapshot_product(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<&StageProduct, PipelineError> {
    upstream
        .get("stage-snapshot")
        .ok_or_else(|| stage_err("missing stage-snapshot product"))
}

/// THIS run's terminal carrier dataset, read DIRECTLY off the `stage-snapshot`
/// product's bundle (#1132). The single internal transport: every export leaf reads
/// the carrier here instead of re-parsing the `gmeow.gts` bytes — GTS is exit-only,
/// produced by the terminal writer (`gts_sink`), never an internal transport.
///
/// The returned `Arc<RdfDataset>` shares the immutable carrier (no copy, no re-parse);
/// the carrier already holds every named graph the snapshot folds (authored default,
/// statements, imports, metadata, alignments, slice-analysis, verify, documentation,
/// diagnostics, conformance, projection-ledger, provenance, logic, relational-core,
/// correspondence, reasoning).
pub(crate) fn snapshot_dataset(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    Ok(snapshot_product(upstream)?.bundle().dataset_arc())
}

// ── Archive blobs (#861 regression fix) ─────────────────────────────────────────
//
// The pre-pipeline generator folded five TAR archives into `gmeow.gts` —
// `mappings-archive` / `cells-archive` / `queries-archive` / `tests-archive` /
// `schemas-archive` —
// that the wheel-mode consumer loaders read back (`gmeow_tools.bundle`:
// `bundled_sssom` / `bundled_cells` / `bundled_queries` / `bundled_tests`). The
// #861 pipeline cutover dropped the WRITER (only the reader survived, orphaned),
// so a repo-free `gmeow.gts` lost its lift maps / cells / queries / test specs and
// every wheel-mode consumer (up-projection, docs-from-bundle, export) broke. This
// restores the writer as a dep-free, byte-deterministic USTAR codec (sorted
// members, zeroed mtime/uid/gid, mode 0644) so the composed snapshot stays
// fold-stable. Member-name conventions MIRROR the reader: mappings/queries use the
// bare filename; cells/tests preserve the repo-relative path (so
// `bundled_cells_under(prefix)` can route by directory).

/// tar of the OPAQUE (non-RDF) fanout files under `generated/` that no dedicated
/// rep already carries — the byte-exact members are recomputed from THIS run's
/// carrier (carrier-reading leaves) or source (source-reading leaves) inside the
/// snapshot and keyed repo-relative. RDF outputs (`.ttl`/`.nt`/`.nq`) are NEVER
/// here: they ride as named graphs so the superset law reconstructs them as folds.
const REP_GENERATED: &str = "generated-opaque-archive";

const REP_MAPPINGS: &str = "mappings-archive";
const REP_CELLS: &str = "cells-archive";
const REP_QUERIES: &str = "queries-archive";
const REP_TESTS: &str = "tests-archive";
/// tar of the SHACL-derived JSON Schema + OpenAPI (#700), member = bare filename.
const REP_SCHEMAS: &str = "schemas-archive";
/// tar of the JSON-LD-star + YAML-LD-star serializations (#699).
const REP_YAMLLD: &str = "yaml-ld-archive";
/// tar of the Rust-rendered OKF bundle (#940), member = `gmeow-okf/...`.
const REP_OKF: &str = "okf-export";
/// The full rendered ontology-docs static site (#897). The rep MUST equal the
/// string the runtime consumer (`create_docs._unpack_doc_archive`) looks up —
/// `"ontology-docs"`, NOT an `-archive` variant — so `gmeow extract-docs` finds it.
const REP_ONTOLOGY_DOCS: &str = "ontology-docs";
/// tar of the FULL SHACL shape surface (#746), member = repo-relative path:
/// every `shapes/*.ttl` (incl. the 4 DSL/manifest lints the consumer's DSL phases
/// need) + every `generated/shapes/*.ttl` (P11 frame shapes) + every per-slice
/// `slices/<g>/<n>/shapes.ttl`. The full surface — NOT the validator's filtered
/// union — so a repo-free `gmeow validate` (#747) can re-derive both the data-graph
/// union and the DSL phases. The Python reader (`bundle.bundled_shapes`) MUST use
/// this exact rep string.
const REP_SHAPES: &str = "shapes-archive";
/// tar of the compiled logic/DL projection surface (#746), member = repo-relative
/// path: the small committed projections in [`AXIOM_FILES`]. NOT the big reasoning
/// OUTPUTS (inferred-closure / reasoning-explanations / dl-el-crosscheck-report),
/// which ride other channels. The Python reader (`bundle.bundled_axioms`) MUST use
/// this exact rep string.
const REP_AXIOMS: &str = "axioms-archive";
/// The compiled logic/DL projection files folded as [`REP_AXIOMS`] (#746): the
/// small, committed, drift-gated projections a repo-free consumer (#747) needs. The
/// big reasoning outputs are deliberately excluded. Order is canonical for the
/// fail-closed scan; the archive re-sorts members by key for determinism.
const AXIOM_FILES: [&str; 5] = [
    "generated/owl/gmeow-dl.ttl",
    "generated/owl/gmeow-el.ttl",
    "generated/logic/gmeow.logic.rdf12.ttl",
    "generated/logic/gmeow.rls",
    "generated/datalog/gmeow.dl",
];
/// tar of the native reasoner's REPORT artifacts (#667, wired #746): the entailment
/// explanations + the DL/EL cross-check ledger over THIS run's reasoned closure. The
/// closure itself already rides the bundle GRAPH (gts-compose folds `stage-reason`'s
/// closure); the reports are deliberately kept OUT of the ontology graph, so this
/// blob channel is how a repo-free consumer reads WHY each entailment holds and the
/// DL/EL agreement ledger WITHOUT re-running the engine (maximal information flow).
/// The Python reader (`bundle.bundled_reasoning`) MUST use this exact rep string.
const REP_REASONING: &str = "reasoning-archive";
const ARCHIVE_MEDIA_TYPE: &str = "application/x-tar";

/// The per-slice guide content blobs (each slice's `docs.md`), backing the
/// `gmeow:guideBlob "blake3:<hex>"` reference triples [`add_guide_blobs`] writes
/// into the documentation graph. The #861 cutover dropped these too — the
/// references shipped dangling. The blob digest the gts writer assigns
/// (`digest_string` = `blake3:<hex>`) equals the reference, so adding the SAME
/// `guide.content` bytes resolves the reference. The `doc-guide` rep is read by
/// digest (not by rep), so it just tags the channel.
fn build_guide_blobs(root: &Path) -> Result<Vec<BlobRow>, PipelineError> {
    let catalog = gmeow_slice::SliceCatalog::discover(&root.join("slices"))
        .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let mut blobs: Vec<BlobRow> = Vec::new();
    for record in catalog.records() {
        if let Some(guide) = record.artifacts.iter().find(|a| {
            a.role == gmeow_slice::ArtifactRole::Documentation && a.logical_path == "docs.md"
        }) {
            blobs.push(BlobRow {
                data: guide.content.clone(),
                media_type: "text/markdown".to_string(),
                rep: "doc-guide".to_string(),
            });
        }
    }
    blobs.sort_by(|a, b| a.data.cmp(&b.data));
    Ok(blobs)
}

/// Build the bundle archive blobs from the repo tree: mappings, cells, queries,
/// tests, schemas, the SHACL shape surface (#746), and the compiled logic/DL axiom
/// surface (#746). The SHACL-derived JSON Schema + OpenAPI bytes are passed in from
/// THIS run's `stage-export-json-schema` product (not re-read from disk) so a single
/// regenerate folds the fresh schema — the committed `generated/schemas/*.json` are
/// not flushed until phase 1 returns.
fn build_archive_blobs(
    root: &Path,
    schema_json: &[u8],
    openapi_json: &[u8],
    axiom_artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<BlobRow>, PipelineError> {
    // mappings + queries: member = bare filename.
    let mappings = members_basename(&list_files(&root.join("generated/mappings"), "sssom.tsv")?)?;
    let queries = members_basename(&list_files(&root.join("generated/queries"), "rq")?)?;
    // schemas: the SHACL-derived JSON Schema + OpenAPI (#700), member = bare
    // filename, taken from the in-memory stage product so the bundle never lags the
    // committed files by a regenerate. Byte-identical to the prior `members_basename`
    // member names (`gmeow.schema.json` / `gmeow.openapi.json`), so the fold is stable.
    let schemas = vec![
        ("gmeow.schema.json".to_string(), schema_json.to_vec()),
        ("gmeow.openapi.json".to_string(), openapi_json.to_vec()),
    ];
    // cells: equivalences + projections + slice mappings, member = repo-relative path.
    let mut cells: Vec<(String, Vec<u8>)> = Vec::new();
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/equivalences"), "ttl")?,
    )?);
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/projections"), "ttl")?,
    )?);
    cells.extend(members_relpath(root, &slice_files(root, "mappings")?)?);
    cells.sort_by(|a, b| a.0.cmp(&b.0));
    // tests: slices/*/*/tests/*.ttl (non-recursive), member = repo-relative path.
    let mut tests = members_relpath(root, &slice_files(root, "tests")?)?;
    tests.sort_by(|a, b| a.0.cmp(&b.0));
    // shapes (#746): the FULL SHACL surface, member = repo-relative path —
    // shapes/*.ttl + generated/shapes/*.ttl (P11, fail-closed if none) +
    // slices/<g>/<n>/shapes.ttl. Carried whole so a repo-free `gmeow validate`
    // (#747) can reassemble both the data-graph union and the DSL phases.
    let mut shapes: Vec<(String, Vec<u8>)> =
        members_relpath(root, &list_files(&root.join("shapes"), "ttl")?)?;
    let generated_shapes = list_files(&root.join("generated/shapes"), "ttl")?;
    if generated_shapes.is_empty() {
        // P11 frame-relativity must never silently drop — mirror shape_union's
        // fail-closed (the validator union requires generated frame shapes).
        return Err(stage_err(
            "no generated/shapes/*.ttl to fold into REP_SHAPES — run `gmeow regenerate frame-shapes` (P11 enforcement)",
        ));
    }
    shapes.extend(members_relpath(root, &generated_shapes)?);
    shapes.extend(members_relpath(
        root,
        &slice_named_files(root, "shapes.ttl")?,
    )?);
    shapes.sort_by(|a, b| a.0.cmp(&b.0));
    // axioms: the compiled logic/DL projection surface, member = repo-relative path.
    // Sourced from THIS run's `stage-compile-logic` product (not re-read from disk) so
    // a single regenerate folds the fresh projections — the committed files are not
    // flushed until phase 1 returns. Each MUST exist (no-optionality, fail-closed): a
    // partial archive would silently break the consumer.
    let mut axioms: Vec<(String, Vec<u8>)> = Vec::with_capacity(AXIOM_FILES.len());
    for rel in AXIOM_FILES {
        let bytes = axiom_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing axiom artifact {rel} in the stage-compile-logic product (fail-closed)"
            ))
        })?;
        axioms.push((rel.to_string(), bytes.clone()));
    }
    axioms.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(vec![
        archive_blob(REP_MAPPINGS, &mappings)?,
        archive_blob(REP_CELLS, &cells)?,
        archive_blob(REP_QUERIES, &queries)?,
        archive_blob(REP_TESTS, &tests)?,
        archive_blob(REP_SCHEMAS, &schemas)?,
        archive_blob(REP_SHAPES, &shapes)?,
        archive_blob(REP_AXIOMS, &axioms)?,
    ])
}

/// Fold the opaque (non-RDF, not-already-carried) members of one leaf's output into
/// the fanout member map.
fn take_opaque(members: &mut BTreeMap<String, Vec<u8>>, arts: BTreeMap<String, Vec<u8>>) {
    for (path, bytes) in arts {
        if !is_rdf_member(&path) && !opaque_already_carried(&path) {
            members.insert(path, bytes);
        }
    }
}

/// Whether an archive blob `rep` can contain committed `generated/` reconstruction
/// targets, so the superset gate decodes it. Excludes the source archives
/// (cells/tests carry `dsl/`+`slices/`), the `reason/` reports, the per-slice
/// guides, and the large `ontology-docs`/`okf`/`yaml-ld` payloads — none back a
/// `generated/` file, and the docs/okf archives are large enough to trip the zstd
/// decode safety bound. One authority, consulted by the gate.
pub(crate) fn archive_rep_carries_generated(rep: &str) -> bool {
    matches!(
        rep,
        REP_MAPPINGS | REP_QUERIES | REP_SCHEMAS | REP_AXIOMS | REP_SHAPES | REP_GENERATED
    )
}

/// Whether `path` is an RDF text artifact (carried as a NAMED GRAPH, never a blob).
fn is_rdf_member(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

/// Whether an opaque `generated/` file is already carried by another rep, so the
/// fanout archive must not double-carry it (keeps the superset reverse sweep clean).
fn opaque_already_carried(path: &str) -> bool {
    path.ends_with(".sssom.tsv")                        // REP_MAPPINGS
        || path.ends_with(".rq")                        // REP_QUERIES
        || path == "generated/schemas/gmeow.schema.json"  // REP_SCHEMAS
        || path == "generated/schemas/gmeow.openapi.json" // REP_SCHEMAS
        || path == "generated/datalog/gmeow.dl"           // REP_AXIOMS
        || path == "generated/logic/gmeow.rls" // REP_AXIOMS
}

/// Build the opaque-fanout archive [`REP_GENERATED`]: recompute each opaque
/// `generated/` fanout output from THIS run's carrier (carrier-reading leaves) or
/// source (source-reading leaves) and fold the non-RDF bytes into one deterministic
/// archive, keyed repo-relative. The post-snapshot export leaves still write the
/// disk files (additive — the writer retirement is a separate concern); the bytes
/// here are byte-identical to the committed files, which the superset gate proves.
fn build_fanout_opaque_blob(
    root: &Path,
    carrier: &gmeow_rdf::RdfDataset,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BlobRow, PipelineError> {
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // carrier-reading leaves (project from THIS run's carrier dataset).
    take_opaque(
        &mut members,
        crate::stages::lpg::render_from_dataset(carrier)?,
    );
    take_opaque(
        &mut members,
        crate::stages::schemas::render_schemas_from_dataset(carrier)?,
    );

    // source-reading leaves (read slices/metadata; no snapshot dependency).
    take_opaque(
        &mut members,
        crate::stages::references::render_references(root)?,
    );
    take_opaque(
        &mut members,
        crate::stages::bench::render_bench_leaderboard(root)?,
    );
    members.insert(
        crate::stages::apache::APACHE_PATH.to_string(),
        crate::stages::apache::render_apache(root)?.into_bytes(),
    );
    members.insert(
        crate::stages::matrix::MATRIX_PATH.to_string(),
        crate::stages::matrix::render_matrix(root)?.into_bytes(),
    );

    // evals + research-objects: the OPAQUE members only (their `.ttl`/`.dcat.ttl` ride
    // as named graphs). Recomputed from source — byte-identical to the committed files.
    take_opaque(&mut members, crate::stages::evals::render_evals(root)?);
    take_opaque(
        &mut members,
        crate::stages::research_objects::render_research_objects(root)?,
    );

    // diagnostics sidecars (`.json`/`.sarif`/`.html`) ride in from the sink-consumed
    // validate + compile-logic products; the `.nq` graphs ride as named graphs.
    for (stage, path) in [
        ("stage-validate", crate::stages::validate::SHACL_JSON_PATH),
        ("stage-validate", crate::stages::validate::SHACL_SARIF_PATH),
        ("stage-validate", crate::stages::validate::SHACL_HTML_PATH),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_JSON_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_SARIF_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_HTML_PATH,
        ),
    ] {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing {path} in {stage} for fanout archive")))?;
        members.insert(path.to_string(), bytes);
    }

    // loss matrices: deterministic, code-derived (verified by tests, not stage-built),
    // recomputed verbatim — the committed files equal the function output exactly.
    members.insert(
        "generated/rdf-loss-matrix.json".to_string(),
        gmeow_rdf::loss_matrix_json().into_bytes(),
    );
    members.insert(
        "generated/transcode-loss-matrix.json".to_string(),
        gmeow_rdf::transcode_loss_matrix_json().into_bytes(),
    );
    members.insert(
        "generated/transcode-matrix.json".to_string(),
        crate::transcode::transcode_matrix_json().into_bytes(),
    );

    // n3 rides in from the sink-consumed stage-compile-logic product (no recompute).
    let n3 = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::N3_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/n3/gmeow.n3 in stage-compile-logic"))?;
    members.insert(crate::stages::compile_logic::N3_PATH.to_string(), n3);

    // context.jsonld + dsl-stats: recomputed from source (the producing stage is not
    // sink-consumed; these are deterministic source projections, byte-identical to
    // the committed files).
    members.insert(
        crate::stages::mappings::JSONLD_CONTEXT_PATH.to_string(),
        gmeow_slice::emit_jsonld_context().into_bytes(),
    );
    members.insert(
        crate::stages::mappings::DSL_STATS_PATH.to_string(),
        gmeow_slice::emit_dsl_stats(root)
            .map_err(|e| stage_err(&format!("recompute dsl-stats: {e}")))?
            .into_bytes(),
    );

    let mut members: Vec<(String, Vec<u8>)> = members.into_iter().collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_GENERATED, &members)
}

/// Fold the native reasoner's explanation + DL/EL cross-check ledger REPORTS into a
/// deterministic [`REP_REASONING`] archive blob (#667, wired #746). Sourced from
/// `stage-reason`'s in-memory product (a `stage-snapshot` consumes-edge), so the fold
/// is ONE-PASS: no disk read, no dependency on the post-snapshot `stage-export-logic`
/// leaf, no convergence lag. The reasoned closure is NOT re-bundled here — it already
/// rides the bundle graph via `gts-compose`. Each artifact MUST exist (no-optionality,
/// fail-closed): a partial archive would silently strip the reasoning reports.
fn build_reasoning_blob(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BlobRow, PipelineError> {
    let get = |path: &str| -> Result<Vec<u8>, PipelineError> {
        upstream
            .get("stage-reason")
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing stage-reason artifact {path}")))
    };
    // Bundle-relative keys under `reason/` — deliberately NOT `generated/logic/…`, so
    // a consumer never mistakes these bundle-consistent reports (over the early-composed
    // closure that rides the graph) for the full-fold committed `generated/logic/` files
    // owned by `stage-export-logic`.
    let members = vec![
        (
            "reason/reasoning-explanations.rdf12.ttl".to_string(),
            get(crate::stages::reason::EXPLANATIONS_PATH)?,
        ),
        (
            "reason/dl-el-crosscheck-report.ttl".to_string(),
            get(crate::stages::reason::LEDGER_PATH)?,
        ),
        (
            "reason/perf-ledger.ttl".to_string(),
            get(crate::stages::reason::PERF_LEDGER_PATH)?,
        ),
    ];
    archive_blob(REP_REASONING, &members)
}

/// Pack the JSON-LD-star + YAML-LD-star serializations into a deterministic tar
/// archive blob (#699). Member names mirror the `dist/` logical paths.
fn build_yaml_ld_blob(jsonld: &[u8], yamlld: &[u8]) -> Result<BlobRow, PipelineError> {
    let members = vec![
        ("gmeow.jsonld".to_string(), jsonld.to_vec()),
        ("gmeow.yamlld".to_string(), yamlld.to_vec()),
    ];
    archive_blob(REP_YAMLLD, &members)
}

/// Build the YAML-LD archive by serializing the carrier dataset in-memory — the
/// SAME native carrier every fold-reading export leaf consumes (no gts round-trip).
fn build_yaml_ld_blob_from_dataset(
    carrier: &gmeow_rdf::RdfDataset,
) -> Result<BlobRow, PipelineError> {
    let jsonld = crate::stages::yaml_ld::serialize_graph(carrier)?;
    let yamlld = crate::stages::yaml_ld::serialize_graph_yaml(carrier, None)?;
    build_yaml_ld_blob(jsonld.as_bytes(), yamlld.as_bytes())
}

/// Build the OKF archive from the carrier dataset — the SAME native carrier the
/// fold-reading export leaves consume, avoiding a `stage-snapshot` ↔
/// `stage-export-okf` DAG cycle (no gts round-trip).
///
/// The public reader (`gmeow_tools.bundle.bundled_okf`) expects members relative to
/// the bundle root (`gmeow-okf/classes/Foo.md`), while the export leaf product is a
/// disk artifact under `dist/`. Strip only that leading `dist/` boundary and hard-fail
/// if a renderer path escapes it.
fn build_okf_blob_from_dataset(carrier: &gmeow_rdf::RdfDataset) -> Result<BlobRow, PipelineError> {
    let (title, version, terms) = crate::stages::export::collect_term_surface(carrier)?;
    let artifacts = crate::stages::okf::render_okf(&title, &version, &terms)?;
    let mut members: Vec<(String, Vec<u8>)> = Vec::with_capacity(artifacts.len());
    for (path, bytes) in artifacts {
        let member = path
            .strip_prefix("dist/")
            .ok_or_else(|| stage_err(&format!("OKF export path is not under dist/: {path}")))?;
        members.push((member.to_string(), bytes));
    }
    archive_blob(REP_OKF, &members)
}

/// Render the full ontology-docs static site and pack it into the single
/// `ontology-docs` archive blob (#897) — the producer half of repo-free
/// `gmeow extract-docs`.
///
/// The rust doc generator (`gmeow_docs::render_site_lang`) emits a complete site
/// (`index.md`/`index.html` per page, `assets/gmeow.css`, SVG diagrams,
/// `search-index.json`, `llms.txt`/`llms-full.txt`, alias redirects) as a deterministic
/// `BTreeMap<path, bytes>`. We render it once per available language and prefix
/// every member with that language's INTERNAL tag (`x-gmeow-english`,
/// `x-gmeow-<lang>`, …) — the exact `{tag}/` prefix `_unpack_doc_archive` filters
/// on (`resolve_doc_language` returns these internal tags). The prefix comes from
/// `Translations::internal_tag`, never the carrier key or a hardcoded string, so a
/// new `.po` catalog is picked up with the correct tag automatically.
fn build_docs_archive(root: &Path) -> Result<BlobRow, PipelineError> {
    let model = gmeow_docs::model::DocsModel::discover(root)
        .map_err(|e| stage_err(&format!("docs model discovery: {e}")))?;
    let catalog = gmeow_slice::SliceCatalog::discover(&root.join("slices"))
        .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);

    // Render each language's full site in parallel: the per-language renders are
    // independent pure functions of the shared read-only model, and this is the
    // dominant cost of the snapshot stage (which sits on the build DAG's serial
    // critical path). Results are collected then sorted by member path, so the
    // archive is byte-identical regardless of completion order.
    let langs = gmeow_docs::available_languages(&translations);
    let mut members: Vec<(String, Vec<u8>)> = langs
        .par_iter()
        .flat_map_iter(|lang| {
            let site = gmeow_docs::render_site_lang(&model, lang);
            let prefix = translations.internal_tag(lang);
            site.files
                .into_iter()
                .map(move |(path, bytes)| (format!("{prefix}/{path}"), bytes))
        })
        .collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_ONTOLOGY_DOCS, &members)
}

/// Every `*.<ext>` directly under `dir`, sorted by path (empty if the dir is absent).
fn list_files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, PipelineError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", dir.display()))),
    };
    let dot = format!(".{ext}");
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| stage_err(&format!("read_dir entry under {}: {e}", dir.display())))?
            .path();
        if path.is_file() && path.to_string_lossy().ends_with(&dot) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Every `slices/<group>/<name>/<sub>/*.ttl` (non-recursive past `<sub>/`), sorted.
fn slice_files(root: &Path, sub: &str) -> Result<Vec<PathBuf>, PipelineError> {
    let slices = root.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    let groups = match std::fs::read_dir(&slices) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", slices.display()))),
    };
    for group in groups {
        let gpath = group
            .map_err(|e| stage_err(&format!("slices group: {e}")))?
            .path();
        if !gpath.is_dir() {
            continue;
        }
        let names = std::fs::read_dir(&gpath)
            .map_err(|e| stage_err(&format!("read_dir {}: {e}", gpath.display())))?;
        for name in names {
            let npath = name
                .map_err(|e| stage_err(&format!("slices name: {e}")))?
                .path();
            if npath.is_dir() {
                out.extend(list_files(&npath.join(sub), "ttl")?);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Every `slices/<group>/<name>/<file>` (a single well-known FILE directly under
/// each slice dir, e.g. `shapes.ttl`), sorted. Unlike [`slice_files`] — which globs
/// a `<sub>/*.ttl` *directory* — this targets one named file per slice. Mirrors the
/// shacl crate's private `shape_union::slice_shape_files` walk (re-implemented here
/// because it is not `pub`, the same way [`slice_files`] duplicates a walk). A read
/// error HARD-FAILS so a slice subtree is never silently dropped (#746).
fn slice_named_files(root: &Path, file: &str) -> Result<Vec<PathBuf>, PipelineError> {
    let slices = root.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    let groups = match std::fs::read_dir(&slices) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", slices.display()))),
    };
    for group in groups {
        let gpath = group
            .map_err(|e| stage_err(&format!("slices group: {e}")))?
            .path();
        if !gpath.is_dir() {
            continue;
        }
        let names = std::fs::read_dir(&gpath)
            .map_err(|e| stage_err(&format!("read_dir {}: {e}", gpath.display())))?;
        for name in names {
            let npath = name
                .map_err(|e| stage_err(&format!("slices name: {e}")))?
                .path();
            if npath.is_dir() {
                let candidate = npath.join(file);
                if candidate.is_file() {
                    out.push(candidate);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `(filename, bytes)` members — the file's bare name (mappings / queries).
///
/// A read error HARD-FAILS rather than silently dropping the file: an incomplete
/// archive would silently break the wheel-mode consumers (no-optionality, the
/// no-silent-caps doctrine — the same as [`members_relpath`]).
fn members_basename(files: &[PathBuf]) -> Result<Vec<(String, Vec<u8>)>, PipelineError> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len());
    for p in files {
        let name = p
            .file_name()
            .ok_or_else(|| stage_err(&format!("archive member has no file name: {}", p.display())))?
            .to_string_lossy()
            .into_owned();
        let data =
            std::fs::read(p).map_err(|e| stage_err(&format!("read {}: {e}", p.display())))?;
        out.push((name, data));
    }
    Ok(out)
}

/// `(repo-relative-path, bytes)` members — the path under `root` (cells / tests).
fn members_relpath(
    root: &Path,
    files: &[PathBuf],
) -> Result<Vec<(String, Vec<u8>)>, PipelineError> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len());
    for p in files {
        let rel = p
            .strip_prefix(root)
            .map_err(|_| stage_err(&format!("path {} not under root", p.display())))?
            .to_string_lossy()
            .replace('\\', "/");
        let data =
            std::fs::read(p).map_err(|e| stage_err(&format!("read {}: {e}", p.display())))?;
        out.push((rel, data));
    }
    Ok(out)
}

/// One archive blob: a deterministic USTAR tar over `members`, tagged with `rep`.
fn archive_blob(rep: &str, members: &[(String, Vec<u8>)]) -> Result<BlobRow, PipelineError> {
    Ok(BlobRow {
        data: gmeow_rdf::ustar::write_archive(members).map_err(|e| stage_err(&e))?,
        media_type: ARCHIVE_MEDIA_TYPE.to_string(),
        rep: rep.to_string(),
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-snapshot` Transform stage (#861 P6): assembles the structured
/// multi-named-graph `dist` snapshot bytes (`build_snapshot`) as an in-memory
/// artifact. The split from the sink lets every fold-reading export leaf consume
/// THIS run's freshly-composed fold rather than re-reading the committed file
/// from disk; the sole [`crate::stages::gts_sink::GtsSinkStage`] then just writes
/// these bytes to `generated/dist/gmeow.gts` (the narrow-waist invariant — one
/// Sink, the disk writer).
pub struct SnapshotStage {
    consumes: Vec<String>,
}

impl SnapshotStage {
    /// Construct the snapshot stage. It reads the RDF 1.2 statement layer
    /// (`stage-statements`) and the documentation projection (`stage-docs-render`)
    /// products to assemble the structured snapshot, plus `stage-gts-compose` /
    /// `stage-reason` for the composed-fold / reasoned-closure wiring.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                // The logic compiler's in-memory product: the projection-report loss
                // ledger folds into the bundle as its own named graph and the compile
                // findings union into the diagnostics graph (no disk re-read).
                "stage-compile-logic".to_string(),
                // The external-corpus divergence Findings (graph/conformance):
                // the conformance stage grades the committed corpus's native frozen
                // verdict against the published external verdict and emits the
                // divergences as a gmeow:Finding N-Quads product folded here.
                "stage-conformance".to_string(),
                "stage-docs-render".to_string(),
                // The mappings product carries the FINAL projection-report loss ledger
                // (logic rows ∪ correspondence rows), folded into graph/projection-ledger.
                "stage-mappings".to_string(),
                // The SHACL→JSON-Schema export leaf (#700): its in-memory product
                // carries THIS run's freshly-emitted gmeow.schema.json / .openapi.json
                // bytes, which `build_archive_blobs` folds into the `schemas-archive`
                // blob. Without this edge the snapshot would re-read the (previous-run)
                // committed schema from disk and lag a regenerate behind (the bytes
                // are only flushed to disk AFTER phase 1 returns — run.rs:242-254).
                "stage-export-json-schema".to_string(),
                "stage-gts-compose".to_string(),
                "stage-reason".to_string(),
                "stage-statements".to_string(),
                "stage-validate".to_string(),
            ],
        }
    }
}

impl Default for SnapshotStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SnapshotStage {
    fn id(&self) -> &str {
        "stage-snapshot"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v5: fold the `schemas-archive` from the in-memory
        // `stage-export-json-schema` product (THIS run's fresh bytes) instead of
        // re-reading the committed `generated/schemas/*.json` from disk (#700) —
        // a single regenerate now folds the fresh schema. v4: render+tar+embed the
        // full ontology-docs site as the `ontology-docs` blob (#897). v3 added the
        // mappings/cells/queries/tests archive blobs + per-slice docs guide blobs.
        // v7 folds both the JSON-LD-star/YAML-LD-star archive (#699) and the
        // DAG-native SHACL diagnostics graph/report blobs (#936/#937). v8 folds
        // the Rust-rendered OKF archive into gmeow.gts (#940). v9 folds the full
        // SHACL shape surface (REP_SHAPES) and the compiled logic/DL axiom surface
        // (REP_AXIOMS) so a repo-free `gmeow validate` is self-sufficient (#746).
        // v10 wires the orphaned REP_REASONING reader to a writer: folds the native
        // reasoner's explanation + DL/EL cross-check ledger reports from stage-reason's
        // in-memory product routed through the first-class-output rail. v11 folds the compiler's projection-report
        // loss ledger as the projection-ledger named graph, unions the logic-compile
        // diagnostics into the diagnostics graph, and sources REP_AXIOMS from the
        // in-memory stage-compile-logic product (single-pass freshness).
        // v12 folds the external-corpus divergence Findings (graph/conformance) from
        // the stage-conformance product, when any committed case diverges from its
        // published expected verdict. v13 folds the dogfooded occurrence-based
        // provenance projection (graph/provenance) — the public compilation-unit +
        // per-lane carrier manifest (no runtime ids, S0.5) — and gates that every
        // authored quad carries ≥1 stage-origin (#1132 C9).
        "snapshot.v13-provenance-graph"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
        // The embedded ontology-docs site (`build_docs_archive`) is rendered from
        // the docs model's raw sources (slice modules / `docs.md` / examples /
        // `docs/four-boxes.md` / per-slice `i18n/<lang>.po` translation catalogs),
        // which the consumed upstream products do not fully reflect. Declare them so
        // a doc-source edit busts this stage and re-renders the embedded site (cache
        // soundness, #897) — shared with `DocsRenderStage` via `docs_source_files`.
        let mut files = crate::stages::docs_render::docs_source_files(root)?;
        // The folded shape surface (REP_SHAPES) is read from disk in
        // `build_archive_blobs`; declare it so a shape edit busts this stage and re-folds
        // the bundle — otherwise a changed shape could ship a stale gmeow.gts (cache
        // soundness). The compiled axiom surface (REP_AXIOMS) is now sourced from
        // the consumed `stage-compile-logic` product, whose digest already covers a logic
        // source change, so the AXIOM_FILES are no longer declared here.
        files.extend(list_files(&root.join("shapes"), "ttl")?);
        files.extend(list_files(&root.join("generated/shapes"), "ttl")?);
        files.extend(slice_named_files(root, "shapes.ttl")?);
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // Assemble the terminal carrier ONCE — the single native RdfDataset that every
        // export leaf (N-Quads/TriG/JSON-LD/OKF/LPG/metadata/logic AND the gts export)
        // reads off this product's bundle. NOTHING is serialized here: GTS is exit-only,
        // produced by the `stage-export-gts` leaf like any other export format.
        let carrier = assemble_carrier(input.root, input.upstream)?;
        let bundle = build_snapshot_bundle(carrier, input.upstream, BTreeMap::new())?;
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), std::sync::Arc::new(bundle)),
        })
    }
}

/// Build the snapshot product bundle: the byte-artifact lane (the emitted `gmeow.gts`)
/// riding over a dataset whose `graph/logic` and `graph/reasoning` named graphs are the
/// canonical projections of the compiled program and the typed reasoning result, with
/// the upstream typed [`PipelineHandle::Logic`] (#1132 C6) and
/// [`PipelineHandle::Reasoning`](crate::bundle::PipelineHandle::Reasoning) (#1132 C7)
/// re-pinned to those graphs' canonical digests.
///
/// Each handle's payload is taken from its upstream product's handle (never
/// re-compiled / re-run); the backing graph is re-derived from the SAME projection the
/// snapshot folded, so each pinned digest is a pure function of that projection alone.
/// A missing handle or a digest mismatch HARD-fails (no-optionality, fail-closed).
fn build_snapshot_bundle(
    carrier: std::sync::Arc<gmeow_rdf::RdfDataset>,
    upstream: &BTreeMap<String, StageProduct>,
    artifacts: BTreeMap<String, Vec<u8>>,
) -> Result<gmeow_rdf::PipelineBundle<crate::bundle::PipelineHandle>, PipelineError> {
    // ── the Logic handle payload + its backing graph/logic projection ────────────
    let compile = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product for the Logic handle"))?;
    let entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_LOGIC)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no Logic handle"))?;
    let crate::bundle::PipelineHandle::Logic(program) = &entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/logic is not the Logic arm",
        ));
    };
    let program = program.clone();

    // ── the RelationalCore handle payload + its backing graph/relational-core ─────
    let rc_entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_RELATIONAL_CORE)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no RelationalCore handle"))?;
    let crate::bundle::PipelineHandle::RelationalCore(rc_program) = &rc_entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/relational-core is not the RelationalCore arm",
        ));
    };
    let rc_program = rc_program.clone();

    // ── the Correspondence handle payload + its backing graph/correspondence ──────
    let corr_entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_CORRESPONDENCE)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no Correspondence handle"))?;
    let crate::bundle::PipelineHandle::Correspondence(corr_program) = &corr_entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/correspondence is not the Correspondence arm",
        ));
    };
    let corr_program = corr_program.clone();

    // ── the Reasoning handle payload + its backing graph/reasoning projection ─────
    let reason = upstream
        .get("stage-reason")
        .ok_or_else(|| stage_err("missing stage-reason product for the Reasoning handle"))?;
    let reason_entry = reason
        .bundle()
        .handle(gmeow_logic::result_rdf::GRAPH_REASONING)
        .ok_or_else(|| stage_err("stage-reason product carries no Reasoning handle"))?;
    let crate::bundle::PipelineHandle::Reasoning(result) = &reason_entry.payload else {
        return Err(stage_err(
            "stage-reason handle for graph/reasoning is not the Reasoning arm",
        ));
    };
    let result = result.clone();

    // The bundle's dataset IS the assembled snapshot carrier (the whole snapshot, the same
    // value serialized to gmeow.gts) — never a second, partial assembly. The carried
    // graph/logic + relational-core + correspondence + reasoning are already folded into
    // it, so each typed handle re-pins to ITS graph in the carrier (digest hard-checked).
    let mut bundle =
        crate::bundle::bundle_from_artifacts_over(carrier, artifacts, DatasetProvenance::new());
    let pinned_logic = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_LOGIC,
            crate::bundle::PipelineHandle::Logic(program),
            pinned_logic,
        )
        .map_err(|e| stage_err(&format!("re-pin Logic handle on snapshot product: {e}")))?;
    let pinned_reasoning = bundle.graph_digest(gmeow_logic::result_rdf::GRAPH_REASONING);
    bundle
        .pin_handle(
            gmeow_logic::result_rdf::GRAPH_REASONING,
            crate::bundle::PipelineHandle::Reasoning(result),
            pinned_reasoning,
        )
        .map_err(|e| stage_err(&format!("re-pin Reasoning handle on snapshot product: {e}")))?;
    let pinned_rc = bundle.graph_digest(crate::stages::compile_logic::GRAPH_RELATIONAL_CORE);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_RELATIONAL_CORE,
            crate::bundle::PipelineHandle::RelationalCore(rc_program),
            pinned_rc,
        )
        .map_err(|e| {
            stage_err(&format!(
                "re-pin RelationalCore handle on snapshot product: {e}"
            ))
        })?;
    let pinned_corr = bundle.graph_digest(crate::stages::compile_logic::GRAPH_CORRESPONDENCE);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_CORRESPONDENCE,
            crate::bundle::PipelineHandle::Correspondence(corr_program),
            pinned_corr,
        )
        .map_err(|e| {
            stage_err(&format!(
                "re-pin Correspondence handle on snapshot product: {e}"
            ))
        })?;
    Ok(bundle)
}

/// Build the per-quad provenance sidecar for the authored base graph, GATE it
/// (every authored quad must carry ≥1 occurrence — an unattributed quad is a HARD
/// FAIL, no-optionality), and project its PUBLIC projection into the deterministic
/// `graph/provenance` N-Triples (#1132 C9). Only public unit names/IRIs + kinds +
/// artifact paths reach the projection — NO runtime `UnitId` / `ArtifactId` /
/// `OriginSetId` (S0.5). The fixed carrier-lane manifest + the realized process
/// vocab (`gmeow:Procedure` / `gmeow:ProcedureStep` / `gmeow:Execution`) round it out.
fn build_provenance_projection(root: &Path) -> Result<String, PipelineError> {
    let (prov, expected) = crate::stages::source_load::attributed_base_provenance(root)?;
    // The hard-fail gate: every authored quad has ≥1 stage-origin occurrence and every
    // occurrence references a registered unit + artifact. A violation aborts the build.
    gmeow_rdf::provenance::check_provenance(&prov, &expected).map_err(|errors| {
        stage_err(&format!(
            "provenance gate: {} authored quad(s) unattributed or mis-attributed: {}",
            errors.len(),
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })?;
    Ok(crate::stages::provenance_graph::project_provenance_graph(
        &prov.public_projection(),
    ))
}

/// Re-root every quad of `src` into the named graph `graph_iri` (preserving the
/// graph-less reifier/annotation side-tables), so a carrier subgraph projected via
/// [`RdfDataset::project_named_graph`](gmeow_rdf::RdfDataset::project_named_graph) —
/// which strips the graph name to the default graph — folds back into ITS named graph,
/// never the authored default graph.
pub(crate) fn rooted_in_graph(
    src: &gmeow_rdf::RdfDataset,
    graph_iri: &str,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    use gmeow_rdf::{RdfDatasetBuilder, RdfTerm};
    let graph = RdfTerm::Iri(graph_iri.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in src.owned_quads() {
        quad.graph_name = Some(graph.clone());
        builder.push_owned_quad(&quad);
    }
    for reifier in src.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in src.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder
        .freeze()
        .map_err(|e| stage_err(&format!("re-root carrier graph <{graph_iri}>: {e}")))
}

// ── default graph (authored ontology, NO imports) ───────────────────────────────

/// The localizable predicates shared with `gmeow-docs` i18n compilation: the
/// vocabulary surface a slice `.po` catalog may translate. Full IRIs.
const LOCALIZABLE_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2000/01/rdf-schema#comment",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/2004/02/skos/core#scopeNote",
    "http://www.w3.org/2004/02/skos/core#example",
    "http://www.w3.org/2004/02/skos/core#prefLabel",
    "http://www.w3.org/2004/02/skos/core#altLabel",
    "http://www.w3.org/2004/02/skos/core#note",
    "http://purl.org/dc/terms/title",
    "http://purl.org/dc/terms/description",
    "https://blackcatinformatics.ca/gmeow/name",
    "https://blackcatinformatics.ca/gmeow/title",
    "https://blackcatinformatics.ca/gmeow/description",
    "https://blackcatinformatics.ca/gmeow/fullName",
];

/// Load `ontology/gmeow.ttl` + every slice `module.ttl` into one store, merge the
/// slice `.po` translations onto its localizable literals, and return canonical
/// N-Quads. This is `load_merged_graph(include_imports=False)` followed by
/// `merge_terms(graph, po_paths)` — the committed default graph is multilingual.
fn load_authored_default(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let onto = root.join("ontology").join("gmeow.ttl");
    // The root ontology is REQUIRED — the authored default graph is meaningless
    // without it. A missing `ontology/gmeow.ttl` HARD-fails rather than silently
    // assembling a partial default graph (no-optionality, #863).
    if !onto.is_file() {
        return Err(stage_err(&format!(
            "required root ontology {} is missing",
            onto.display()
        )));
    }
    // Root ontology + every slice `module.ttl`, each parsed into its OWN dataset so
    // its blank labels live in an independent scope, then merged via the native
    // standardize-apart `RdfDataset::union`. This REPLACES the per-file `f{scope}_`
    // string-prefixing (`ingest_turtle_scoped`) and the oxigraph `Store` accumulation:
    // the union's per-input `BlankScope` keeps structurally-distinct blank-node axioms
    // (two `owl:AllDisjointClasses` lists) disjoint, the very distinctness the build
    // relies on. Imports are EXCLUDED — they ride `graph/imports` (`load_imports`).
    let mut sources: Vec<Vec<u8>> = Vec::new();
    sources.push(std::fs::read(&onto)?);
    for module in crate::stages::source_load::module_files(root)? {
        sources.push(std::fs::read(&module)?);
    }
    let base = union_turtle_datasets(&sources)?;

    // The merged default graph as a flat oxigraph quad list (the union's standardized
    // blank labels), onto which the multilingual translations and per-slice guideBlob
    // anchors are folded natively — both add IRI-subject triples, so re-folding the
    // augmented list through `dataset_from_oxigraph_quads` is loss-free.
    let mut quads = flat_oxigraph_quads_from_dataset(&base)
        .map_err(|e| stage_err(&format!("base default graph → quads: {e}")))?;
    merge_translations(root, &mut quads)?;
    add_guide_blobs(root, &mut quads)?;

    let dataset = gmeow_rdf::dataset_from_oxigraph_quads(&quads)
        .map_err(|e| stage_err(&format!("authored default graph freeze: {e}")))?;
    dataset_to_nquads(&dataset)
}

/// Add the per-slice `gmeow:guideBlob` triple `_doc_blobs` injects into the
/// default graph: for every slice carrying a `docs.md`, link the slice IRI to the
/// `blake3:<hex>` content digest of that guide. The guide itself rides the bundle
/// as a content-addressed blob; this triple is its in-graph anchor.
fn add_guide_blobs(root: &Path, quads: &mut Vec<Quad>) -> Result<(), PipelineError> {
    use oxigraph::model::{Literal, NamedNode};

    let guide_blob = NamedNode::new(format!("{GMEOW_NS}guideBlob")).unwrap();
    let catalog = gmeow_slice::SliceCatalog::discover(&root.join("slices"))
        .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    for record in catalog.records() {
        let Some(guide) = record.artifacts.iter().find(|a| {
            a.role == gmeow_slice::ArtifactRole::Documentation && a.logical_path == "docs.md"
        }) else {
            continue;
        };
        let digest = format!("blake3:{}", blake3::hash(&guide.content).to_hex());
        let subject = NamedNode::new(&record.manifest.slice_iri)
            .map_err(|e| stage_err(&format!("slice IRI {}: {e}", record.manifest.slice_iri)))?;
        quads.push(Quad::new(
            subject,
            guide_blob.clone(),
            Literal::new_simple_literal(digest),
            oxigraph::model::GraphName::DefaultGraph,
        ));
    }
    Ok(())
}

/// Merge the slice `.po` translations into the default-graph quad list, mirroring
/// `merge_terms`: for every base-graph localizable literal `(iri, predicate)`, add a
/// translated literal `(iri, predicate, "msgstr"@<internal-tag>)` for each language
/// that translates it. The translation index + the BCP-47 → `x-gmeow-*` tag map come
/// from the native `gmeow_docs::Translations` (the same catalog the docs render).
///
/// The scan is over the pre-translation base quads (a snapshot taken before any
/// additions), so a translated literal is never itself re-scanned — matching the
/// original `quads_for_pattern` view of the base store.
fn merge_translations(root: &Path, quads: &mut Vec<Quad>) -> Result<(), PipelineError> {
    use oxigraph::model::{Literal, NamedNode, NamedOrBlankNode, Term};

    let catalog = gmeow_slice::SliceCatalog::discover(&root.join("slices"))
        .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);
    let langs: Vec<String> = translations.languages().to_vec();
    if langs.is_empty() {
        return Ok(());
    }
    let localizable: std::collections::BTreeSet<&str> =
        LOCALIZABLE_PREDICATES.iter().copied().collect();

    // The base-graph localizable literals: `(subject_iri, predicate_iri)` whose object
    // is a literal (the allowed-keys set of merge_terms). Scanned over the base quads
    // only; additions are appended afterwards.
    let mut additions: Vec<Quad> = Vec::new();
    for quad in quads.iter() {
        let pred = quad.predicate.as_str();
        if !localizable.contains(pred) {
            continue;
        }
        let NamedOrBlankNode::NamedNode(subject) = &quad.subject else {
            continue;
        };
        if !matches!(&quad.object, Term::Literal(_)) {
            continue;
        }
        let predicate = NamedNode::new(pred).map_err(|e| stage_err(&format!("predicate: {e}")))?;
        for lang in &langs {
            if let Some(msgstr) = translations.lookup(subject.as_str(), pred, lang) {
                let tag = translations.internal_tag(lang);
                let literal = Literal::new_language_tagged_literal(msgstr, &tag)
                    .map_err(|e| stage_err(&format!("lang literal {tag}: {e}")))?;
                additions.push(Quad::new(
                    subject.clone(),
                    predicate.clone(),
                    literal,
                    oxigraph::model::GraphName::DefaultGraph,
                ));
            }
        }
    }
    quads.extend(additions);
    Ok(())
}

// ── imports (graph/imports) ─────────────────────────────────────────────────────

fn load_imports(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let dir = root.join("imports");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    // Each import file is its own blank scope; merge via the standardize-apart union
    // (the native replacement for the per-file Store accumulation).
    let sources: Vec<Vec<u8>> = files.iter().map(std::fs::read).collect::<Result<_, _>>()?;
    dataset_to_nquads(&union_turtle_datasets(&sources)?)
}

// ── metadata (graph/metadata) ───────────────────────────────────────────────────

fn load_metadata(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let path = root.join("metadata").join("gmeow-self.ttl");
    turtle_to_nquads(&std::fs::read(&path)?)
}

// ── slice-analysis (graph/slice-analysis) ───────────────────────────────────────

/// Build the `gmeow:graph/slice-analysis` graph via the native ownership
/// analyzer — the Rust twin of `gts_gen.build_slice_analysis_graph`. The analyzer
/// reads AUTHORED slices only; `authored_nq` (the authored base graph as text)
/// feeds the emitter's self-attestation guard.
fn build_slice_analysis(root: &Path, authored_nq: &[u8]) -> Result<Vec<u8>, PipelineError> {
    use gmeow_slice::{
        emit_analysis_graph, OwnershipAnalyzer, OwnershipStatus, SliceCatalog, ToolchainContext,
    };

    let slices_dir = root.join("slices");
    let catalog = SliceCatalog::discover(&slices_dir)
        .map_err(|e| stage_err(&format!("slice catalog discover: {e}")))?;
    let report = OwnershipAnalyzer::new(&catalog)
        .analyze()
        .map_err(|e| stage_err(&format!("ownership analysis: {e}")))?;

    // tier map + every authored artifact raw digest (mirror PyOwnershipAnalyzer).
    let mut tier_of: std::collections::HashMap<gmeow_slice::SliceIri, u8> =
        std::collections::HashMap::new();
    let mut raw_digests: Vec<String> = Vec::new();
    for record in catalog.records() {
        tier_of.insert(
            record.manifest.slice_iri.clone(),
            tier_priority(record.manifest.tier.as_ref()),
        );
        for artifact in &record.artifacts {
            raw_digests.push(artifact.raw_digest.clone());
        }
    }
    raw_digests.sort_unstable();
    let digests: Vec<&str> = raw_digests.iter().map(String::as_str).collect();

    let term_count_of = |slice: &gmeow_slice::SliceIri| -> usize {
        report
            .ownership
            .values()
            .filter(|o| {
                matches!(o.status, OwnershipStatus::Validated) && &o.declared_owner == slice
            })
            .count()
    };
    let tier_lookup =
        |slice: &gmeow_slice::SliceIri| -> u8 { tier_of.get(slice).copied().unwrap_or(2) };

    let version = ontology_version(authored_nq)?;
    let toolchain = ToolchainContext::new(&version, "dist");
    let authored_text = String::from_utf8_lossy(authored_nq).into_owned();
    let graph = emit_analysis_graph(
        &report.edges,
        &authored_text,
        &digests,
        &toolchain,
        tier_lookup,
        term_count_of,
    )
    .map_err(|e| stage_err(&format!("slice-analysis emit: {e}")))?;

    // The emitter returns a Turtle body; normalize to N-Quads natively so the builder
    // ingests it the same way as every other named-graph source.
    turtle_to_nquads(graph.turtle_body.as_bytes())
}

fn tier_priority(tier: Option<&gmeow_slice::SliceTier>) -> u8 {
    use gmeow_slice::SliceTier;
    match tier {
        Some(SliceTier::Core) => 0,
        Some(SliceTier::Extension) => 1,
        Some(SliceTier::Domain) | Some(SliceTier::Unknown(_)) | None => 2,
    }
}

/// The authored ontology `owl:versionInfo` (a hard requirement — never defaulted).
fn ontology_version(authored_nq: &[u8]) -> Result<String, PipelineError> {
    let onto = GMEOW_NS.trim_end_matches('/');
    let version_info = "http://www.w3.org/2002/07/owl#versionInfo";
    for quad in parse_nq(authored_nq)? {
        if let oxigraph::model::NamedOrBlankNode::NamedNode(subject) = &quad.subject {
            if subject.as_str() == onto && quad.predicate.as_str() == version_info {
                if let oxigraph::model::Term::Literal(l) = &quad.object {
                    return Ok(l.value().to_string());
                }
            }
        }
    }
    Err(stage_err(&format!(
        "authored ontology {GMEOW_NS} has no owl:versionInfo"
    )))
}

// ── alignments (graph/alignments) ───────────────────────────────────────────────

/// Build the SSSOM alignment-axiom graph: one `(subject, predicate, object)`
/// triple per SSSOM data row with CURIEs expanded through the per-file
/// `# curie_map:` header, deduplicated. Mirrors
/// `mappings.build_alignment_graph(load_mappings())`. The source is the committed
/// `generated/mappings/*.sssom.tsv` (the mappings stage's byte-parity output).
fn load_alignments(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let dir = root.join("generated").join("mappings");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.to_string_lossy().ends_with(".sssom.tsv") {
            files.push(path);
        }
    }
    files.sort();

    let mut quads: Vec<Quad> = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path)?;
        for (s, p, o) in alignment_rows(&text)? {
            let subject = oxigraph::model::NamedNode::new(&s)
                .map_err(|e| stage_err(&format!("alignment subject {s}: {e}")))?;
            let predicate = oxigraph::model::NamedNode::new(&p)
                .map_err(|e| stage_err(&format!("alignment predicate {p}: {e}")))?;
            let object = oxigraph::model::NamedNode::new(&o)
                .map_err(|e| stage_err(&format!("alignment object {o}: {e}")))?;
            quads.push(Quad::new(
                subject,
                predicate,
                object,
                oxigraph::model::GraphName::DefaultGraph,
            ));
        }
    }
    let dataset = gmeow_rdf::dataset_from_oxigraph_quads(&quads)
        .map_err(|e| stage_err(&format!("alignment graph freeze: {e}")))?;
    dataset_to_nquads(&dataset)
}

/// Parse one SSSOM TSV into `(subject_iri, predicate_iri, object_iri)` rows,
/// expanding CURIEs through the file's `# curie_map:` header block.
fn alignment_rows(text: &str) -> Result<Vec<(String, String, String)>, PipelineError> {
    let mut curie_map: BTreeMap<String, String> = BTreeMap::new();
    let mut in_curie_map = false;
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<(String, String, String)> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            let trimmed = rest.trim();
            if trimmed == "curie_map:" {
                in_curie_map = true;
                continue;
            }
            if in_curie_map {
                // `#   prefix: namespace` — two leading spaces then `prefix: ns`.
                if let Some((prefix, ns)) = trimmed.split_once(':') {
                    // Only treat as a curie-map entry if it looks like `name: uri`.
                    let prefix = prefix.trim();
                    let ns = ns.trim();
                    if !prefix.is_empty() && (ns.contains("://") || ns.starts_with("urn:")) {
                        curie_map.insert(prefix.to_string(), ns.to_string());
                        continue;
                    }
                }
                // A non-curie header line ends the curie_map block.
                in_curie_map = false;
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<String> = line.split('\t').map(str::to_string).collect();
        if header.is_none() {
            header = Some(cols);
            continue;
        }
        let head = header.as_ref().unwrap();
        let get = |name: &str| -> Option<&str> {
            head.iter()
                .position(|h| h == name)
                .and_then(|i| cols.get(i).map(String::as_str))
        };
        let (Some(s), Some(p), Some(o)) =
            (get("subject_id"), get("predicate_id"), get("object_id"))
        else {
            continue;
        };
        if s.is_empty() || p.is_empty() || o.is_empty() {
            continue;
        }
        rows.push((
            expand_curie(s, &curie_map)?,
            expand_curie(p, &curie_map)?,
            expand_curie(o, &curie_map)?,
        ));
    }
    Ok(rows)
}

/// Expand a `prefix:local` CURIE through `curie_map` (an already-absolute IRI
/// passes through). Mirrors `mappings.expand_curie`.
fn expand_curie(
    curie: &str,
    curie_map: &BTreeMap<String, String>,
) -> Result<String, PipelineError> {
    if curie.starts_with("http://") || curie.starts_with("https://") || curie.starts_with("urn:") {
        return Ok(curie.to_string());
    }
    if let Some((prefix, local)) = curie.split_once(':') {
        if let Some(ns) = curie_map.get(prefix) {
            return Ok(format!("{ns}{local}"));
        }
    }
    Err(stage_err(&format!("unresolvable CURIE {curie:?}")))
}

// ── verify attestation (graph/verify) ───────────────────────────────────────────

/// Run the native verify lane over `edb` and build the attestation graph as
/// N-Quads. Mirrors `gts_gen.build_verify_attestation_graph` exactly (the same
/// `gmeow:QualityAssessment` vocabulary, one per query).
fn run_verify_attestation(
    root: &Path,
    edb: &gmeow_rdf::RdfDataset,
) -> Result<Vec<u8>, PipelineError> {
    let query_paths = verify_query_paths(root)?;
    let pairs: Vec<(String, String)> = query_paths
        .iter()
        .map(|(name, path)| {
            std::fs::read_to_string(path)
                .map(|sparql| (name.clone(), sparql))
                .map_err(PipelineError::from)
        })
        .collect::<Result<_, _>>()?;

    let report = gmeow_logic::verify::verify(edb, &pairs)
        .map_err(|e| stage_err(&format!("native verify: {e}")))?;

    // The failed set: stems whose finding is an error coded `verify.<stem>`.
    let mut failed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for finding in &report.findings {
        if matches!(finding.severity, gmeow_diagnostics::Severity::Error)
            && finding.code.starts_with("verify.")
        {
            failed.insert(finding.code["verify.".len()..].to_string());
        }
    }

    let attestation = emit_verify_attestation(&query_paths, &failed);
    turtle_to_nquads(attestation.as_bytes())
}

/// Sorted `(repo_relative_name, path)` for every verify query (core + slice).
fn verify_query_paths(root: &Path) -> Result<Vec<(String, std::path::PathBuf)>, PipelineError> {
    let mut out: Vec<(String, std::path::PathBuf)> = Vec::new();
    // Core: sorted queries/verify/*.rq.
    let core = root.join("queries").join("verify");
    let mut core_files: Vec<std::path::PathBuf> = Vec::new();
    if core.is_dir() {
        for entry in std::fs::read_dir(&core)? {
            let path = entry?.path();
            if path.extension().is_some_and(|x| x == "rq") {
                core_files.push(path);
            }
        }
    }
    core_files.sort();
    for path in core_files {
        out.push((rel_name(root, &path), path));
    }
    // Slice verify queries: slices/<group>/<name>/queries/verify/*.rq.
    let mut slice_files: Vec<std::path::PathBuf> = Vec::new();
    let slices = root.join("slices");
    if slices.is_dir() {
        for group in sorted_dirs(&slices)? {
            for slice in sorted_dirs(&group)? {
                let vdir = slice.join("queries").join("verify");
                if vdir.is_dir() {
                    for entry in std::fs::read_dir(&vdir)? {
                        let path = entry?.path();
                        if path.extension().is_some_and(|x| x == "rq") {
                            slice_files.push(path);
                        }
                    }
                }
            }
        }
    }
    slice_files.sort();
    for path in slice_files {
        out.push((rel_name(root, &path), path));
    }
    Ok(out)
}

/// Emit the verify-attestation Turtle (pure, deterministic). One
/// `gmeow:QualityAssessment` per query; mirrors `build_verify_attestation_graph`.
fn emit_verify_attestation(
    query_paths: &[(String, std::path::PathBuf)],
    failed: &std::collections::BTreeSet<String>,
) -> String {
    use std::fmt::Write;
    let mut body = String::new();
    writeln!(body, "@prefix gmeow: <{GMEOW_NS}> .").unwrap();
    writeln!(body, "@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .").unwrap();
    writeln!(
        body,
        "@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    let ontology_iri = GMEOW_NS.trim_end_matches('/');
    // The verify activity and per-query assessments are generated A-Box
    // instance data folded into the bundle's `graph/verify`, not vocabulary
    // surface: each typed subject carries a human label, its named-graph
    // provenance anchor, and the assertional `gmeow:boxABox` role so the bundle
    // satisfies the assertional-tier validation contract (no `skos:definition`).
    writeln!(
        body,
        "<{GMEOW_NS}activity/native-verify> a <{GMEOW_NS}Activity> ;"
    )
    .unwrap();
    writeln!(body, "    rdfs:label \"Native verify activity\" ;").unwrap();
    writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
    writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
    writeln!(
        body,
        "    <{GMEOW_NS}wasAssociatedWith> <{GMEOW_NS}agent/native-verify> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    for (name, _path) in query_paths {
        let stem = query_stem(name);
        let passed = !failed.contains(stem);
        writeln!(body, "<{GMEOW_NS}verify-attestation/{stem}>").unwrap();
        writeln!(body, "    a <{GMEOW_NS}QualityAssessment> ;").unwrap();
        writeln!(body, "    rdfs:label \"Verify attestation: {stem}\" ;").unwrap();
        writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
        writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
        writeln!(body, "    <{GMEOW_NS}assessedEntity> <{ontology_iri}> ;").unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}qualityDimension> <{GMEOW_NS}qualityDimensionLogicalConsistency> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}observationResult> \"{}\"^^xsd:boolean ;",
            if passed { "true" } else { "false" }
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasDerivedFrom> <{GMEOW_NS}verify-query/{stem}> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasGeneratedBy> <{GMEOW_NS}activity/native-verify> ."
        )
        .unwrap();
        writeln!(body).unwrap();
    }
    body
}

fn query_stem(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .strip_suffix(".rq")
        .unwrap_or(name)
}

// ── small helpers ───────────────────────────────────────────────────────────────

/// Canonicalize N-Quads bytes and route them into `graph_name` on `builder` — the
/// oxigraph-ingestion path the byte-golden tests use to author fixture snapshots.
/// Production assembly now goes through the native carrier ([`assemble_carrier`]).
#[cfg(test)]
fn add_named(
    builder: &mut SnapshotBuilder,
    nq_bytes: &[u8],
    graph_name: &str,
    scope: &str,
) -> Result<(), PipelineError> {
    let canon = canonicalize_nq(nq_bytes, scope)?;
    let quads = parse_nq(canon.as_bytes())?;
    reject_quoted_triples(&quads, graph_name)?;
    builder.add_quads(&quads, Some(graph_name), Some(scope));
    Ok(())
}

/// `SnapshotBuilder::add_quads` SILENTLY DROPS a quad whose object is a quoted
/// triple (`<<>>`), because the RDF-1.2 statement layer is meant to arrive only via
/// `add_rdf12` (as reifies/annotation rows), never as a base quoted-triple object.
/// In the pipeline these base/named graphs are plain RDF-1.1 N-Quads, so a quoted
/// triple here would be a real defect — HARD-fail rather than let `add_quads` drop
/// the statement and shrink the fold (no-optionality / no silent data loss, #863).
fn reject_quoted_triples(quads: &[Quad], graph_name: &str) -> Result<(), PipelineError> {
    if quads
        .iter()
        .any(|q| matches!(q.object, oxigraph::model::Term::Triple(_)))
    {
        return Err(stage_err(&format!(
            "graph {graph_name} carries a quoted-triple (<<>>) object that add_quads would \
             silently drop; the RDF-1.2 statement layer must arrive via add_rdf12, not as a base quad"
        )));
    }
    Ok(())
}

/// Canonicalize a graph's blank-node labels under RDFC-1.0, returning N-Quads.
/// Mirrors `compile_gts`'s `to_canonical_graph` before each `add_graph`.
fn canonicalize_nq(nq_bytes: &[u8], _scope: &str) -> Result<String, PipelineError> {
    let quads = parse_nq(nq_bytes)?;
    // Native full RDFC-1.0 (#910), replacing oxrdf `Dataset::canonicalize`. The blank
    // labeling is identical (both conformant SHA-256 RDFC-1.0) and the oxigraph term
    // serialization below is unchanged, so the emitted N-Quads are byte-stable.
    let canonical = gmeow_rdf::canonicalize_quads(quads)
        .map_err(|e| stage_err(&format!("canonicalize: {e}")))?;
    // `Quad`'s Display renders `s p o g` WITHOUT the trailing N-Quads dot, so append
    // ` .` to each row to produce valid N-Quads the parser round-trips.
    let mut out: Vec<String> = canonical.iter().map(|q| format!("{q} .")).collect();
    out.sort_unstable();
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

fn parse_nq(bytes: &[u8]) -> Result<Vec<Quad>, PipelineError> {
    parse_rdf(bytes, "application/n-quads")
}

/// Parse RDF text of `media_type` into a flat oxigraph quad list via the native
/// codecs. The IR fold + [`flat_oxigraph_quads_from_dataset`] un-fold are exact
/// inverses (set-equal to the original parse), so the RDF 1.2 statement layer's
/// `rdf:reifies`/annotation rows are re-materialized for `add_rdf12`'s own fold.
fn parse_rdf(bytes: &[u8], media_type: &str) -> Result<Vec<Quad>, PipelineError> {
    let dataset =
        parse_dataset(bytes, media_type, None).map_err(|e| stage_err(&format!("parse: {e}")))?;
    flat_oxigraph_quads_from_dataset(&dataset).map_err(|e| stage_err(&format!("IR → quads: {e}")))
}

/// Parse one Turtle source's bytes into a frozen [`RdfDataset`] via the native
/// codec. The IR fold standardizes blank labels per-dataset, so each parse is an
/// independent blank-node scope — [`RdfDataset::union`] keeps those scopes disjoint.
fn parse_turtle_dataset(
    bytes: &[u8],
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, PipelineError> {
    parse_dataset(bytes, "text/turtle", None).map_err(|e| stage_err(&format!("parse: {e}")))
}

/// Serialize a frozen [`RdfDataset`] to N-Quads, the same byte form every named-graph
/// source flows through before [`add_named`] re-canonicalizes it. Fully native: no
/// oxigraph `Store`, no oxigraph quad detour.
///
/// CRITICAL: the typed-literal lexical forms are canonicalized to the W3C-canonical XSD
/// mapping (`0.90` → `0.9`, `1.0` → `1.0`, `415.0` → `415.0`, `+00:00` → `Z`) via
/// [`gmeow_xsd::parse_by_iri`] + [`gmeow_xsd::XsdValue::canonical_lexical`]. The native
/// codecs PRESERVE raw lexical forms on a faithful round-trip, so without this normalize
/// the committed canonical bundle (and every artifact re-derived from it) would drift.
/// Byte-compatibility with oxigraph's literal value-space is NOT a goal — correct native
/// XSD-canonical output is. The canonicalization recurses into quoted-triple (RDF 1.2)
/// objects, and a malformed typed literal HARD-fails (no-optionality).
///
/// The mapped quads / reifier bindings / annotations are re-interned through a fresh
/// [`gmeow_rdf::RdfDatasetBuilder`] (carrying the full RDF 1.2 statement layer), so the
/// whole pass stays on the native kernel — no transient oxigraph `Store`.
fn dataset_to_nquads(dataset: &gmeow_rdf::RdfDataset) -> Result<Vec<u8>, PipelineError> {
    let mut builder = gmeow_rdf::RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        builder.push_owned_quad(&canonicalize_quad_xsd(quad)?);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&canonicalize_reifier_xsd(reifier)?);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&canonicalize_annotation_xsd(annotation)?);
    }
    let normalized = builder
        .freeze()
        .map_err(|e| stage_err(&format!("literal-canonical freeze: {e}")))?;
    serialize_dataset(
        normalized.as_ref(),
        "application/n-quads",
        SerializeGraph::Dataset,
    )
    .map_err(|e| stage_err(&format!("serialize: {e}")))
}

/// Canonicalize every typed-literal lexical form in an owned [`gmeow_rdf::RdfQuad`] to
/// the W3C XSD canonical mapping via gmeow-xsd, recursing into quoted-triple terms.
fn canonicalize_quad_xsd(
    mut quad: gmeow_rdf::RdfQuad,
) -> Result<gmeow_rdf::RdfQuad, PipelineError> {
    canonicalize_term_xsd(&mut quad.subject)?;
    canonicalize_term_xsd(&mut quad.object)?;
    if let Some(graph_name) = quad.graph_name.as_mut() {
        canonicalize_term_xsd(graph_name)?;
    }
    Ok(quad)
}

/// As [`canonicalize_quad_xsd`] for an owned RDF 1.2 reifier binding.
fn canonicalize_reifier_xsd(
    mut reifier: gmeow_rdf::RdfReifier,
) -> Result<gmeow_rdf::RdfReifier, PipelineError> {
    canonicalize_triple_xsd(&mut reifier.statement)?;
    canonicalize_term_xsd(&mut reifier.reifier)?;
    Ok(reifier)
}

/// As [`canonicalize_quad_xsd`] for an owned RDF 1.2 statement annotation.
fn canonicalize_annotation_xsd(
    mut annotation: gmeow_rdf::RdfAnnotation,
) -> Result<gmeow_rdf::RdfAnnotation, PipelineError> {
    canonicalize_term_xsd(&mut annotation.reifier)?;
    canonicalize_term_xsd(&mut annotation.object)?;
    Ok(annotation)
}

/// Recurse a single owned [`gmeow_rdf::RdfTriple`], canonicalizing its term literals.
fn canonicalize_triple_xsd(triple: &mut gmeow_rdf::RdfTriple) -> Result<(), PipelineError> {
    canonicalize_term_xsd(&mut triple.subject)?;
    canonicalize_term_xsd(&mut triple.object)?;
    Ok(())
}

/// Canonicalize a single owned [`gmeow_rdf::RdfTerm`] in place: a typed literal with a
/// recognized XSD datatype is rewritten to its W3C-canonical lexical form, a
/// quoted-triple term recurses, and every other term (IRI, blank node, language-tagged
/// literal, `xsd:string`/unrecognized-datatype literal) is left VERBATIM.
///
/// A malformed lexical for a RECOGNIZED XSD datatype HARD-fails (`Err` from
/// `parse_by_iri`): an authored ontology should never carry one, so surface it
/// (no-optionality) rather than silently passing it through.
fn canonicalize_term_xsd(term: &mut gmeow_rdf::RdfTerm) -> Result<(), PipelineError> {
    match term {
        gmeow_rdf::RdfTerm::Literal(literal) => {
            // A language tag (rdf:langString) has no numeric value space — verbatim.
            if literal.language.is_some() {
                return Ok(());
            }
            if let Some(datatype_iri) = literal.datatype.as_deref() {
                match gmeow_xsd::parse_by_iri(&literal.lexical_form, datatype_iri) {
                    // Recognized XSD datatype → rewrite to the canonical lexical form.
                    Ok(Some(value)) => literal.lexical_form = value.canonical_lexical(),
                    // Unrecognized datatype IRI → leave the lexical form VERBATIM.
                    Ok(None) => {}
                    // Malformed lexical for a recognized XSD datatype → HARD-fail.
                    Err(e) => {
                        return Err(stage_err(&format!(
                            "malformed typed literal {:?}^^<{datatype_iri}> in the authored ontology: {e:?}",
                            literal.lexical_form
                        )));
                    }
                }
            }
            // A literal with no datatype (xsd:string) has no numeric value space —
            // verbatim.
            Ok(())
        }
        gmeow_rdf::RdfTerm::Triple(triple) => canonicalize_triple_xsd(triple),
        gmeow_rdf::RdfTerm::Iri(_) | gmeow_rdf::RdfTerm::BlankNode(_) => Ok(()),
    }
}

/// Parse a single Turtle source and serialize it straight to N-Quads (no `Store`).
/// The native equivalent of the old `Store::new()+ingest_turtle+store_to_nquads`
/// trio for single-file named-graph sources (metadata, slice-analysis).
fn turtle_to_nquads(bytes: &[u8]) -> Result<Vec<u8>, PipelineError> {
    dataset_to_nquads(parse_turtle_dataset(bytes)?.as_ref())
}

/// The standardize-apart union of several Turtle sources into ONE default-graph
/// dataset. Each source is parsed independently (its own blank scope) and merged via
/// [`RdfDataset::union`], whose per-input `BlankScope` keeps structurally-distinct
/// blank-node axioms (e.g. two `owl:AllDisjointClasses` lists) disjoint — the native
/// replacement for the removed `ingest_turtle_scoped` string-prefix scoping.
fn union_turtle_datasets(sources: &[Vec<u8>]) -> Result<gmeow_rdf::RdfDataset, PipelineError> {
    let owned: Vec<std::sync::Arc<gmeow_rdf::RdfDataset>> = sources
        .iter()
        .map(|bytes| parse_turtle_dataset(bytes))
        .collect::<Result<_, _>>()?;
    let refs: Vec<&gmeow_rdf::RdfDataset> = owned.iter().map(AsRef::as_ref).collect();
    Ok(gmeow_rdf::RdfDataset::union(&refs))
}

fn sorted_dirs(dir: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn rel_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn stage_err(message: &str) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-gts-sink".to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod xsd_canon_tests {
    use super::*;
    use gmeow_rdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple};

    const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

    /// Serialize a single-quad dataset through `dataset_to_nquads` and return the
    /// canonical N-Quads as a string.
    fn nquads_of(quad: RdfQuad) -> String {
        let mut b = RdfDatasetBuilder::new();
        b.push_owned_quad(&quad);
        let ds = b.freeze().expect("freeze");
        String::from_utf8(dataset_to_nquads(ds.as_ref()).expect("nquads")).expect("utf8")
    }

    fn typed_quad(lexical: &str, datatype: &str) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::typed(lexical, datatype)),
        )
    }

    /// A recognized XSD datatype is rewritten to its W3C-canonical lexical form —
    /// `415.0`→`415.0`, `0.90`→`0.9`, `+00:00`→`Z` (correct native output; oxigraph
    /// byte-parity is NOT a goal).
    #[test]
    fn recognized_xsd_literal_is_canonicalized() {
        for (lex, datatype, expected) in [
            ("0.90", XSD_DECIMAL, "0.9"),
            ("415.0", XSD_DECIMAL, "415.0"),
            ("-200.0", XSD_DECIMAL, "-200.0"),
            (
                "2024-06-01T10:00:00+00:00",
                XSD_DATETIME,
                "2024-06-01T10:00:00Z",
            ),
        ] {
            let nq = nquads_of(typed_quad(lex, datatype));
            assert!(
                nq.contains(&format!("\"{expected}\"^^<{datatype}>")),
                "{lex}^^<{datatype}> must canonicalize to {expected}; got:\n{nq}"
            );
        }
    }

    /// A language-tagged literal passes through VERBATIM (rdf:langString has no
    /// numeric value space).
    #[test]
    fn language_tagged_literal_is_verbatim() {
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::language_tagged("hallo", "de")),
        ));
        assert!(
            nq.contains("\"hallo\"@de"),
            "lang literal verbatim; got:\n{nq}"
        );
    }

    /// An unrecognized-datatype literal passes through VERBATIM (parse_by_iri →
    /// Ok(None)): `0.90` keeps its trailing zero under a custom datatype.
    #[test]
    fn unknown_datatype_literal_is_verbatim() {
        let custom = "https://example.org/myType";
        let nq = nquads_of(typed_quad("0.90", custom));
        assert!(
            nq.contains(&format!("\"0.90\"^^<{custom}>")),
            "unknown-datatype literal keeps its raw lexical form; got:\n{nq}"
        );
    }

    /// A plain `xsd:string`-no-datatype literal passes through VERBATIM.
    #[test]
    fn plain_string_literal_is_verbatim() {
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::simple("0.90")),
        ));
        assert!(nq.contains("\"0.90\""), "plain string verbatim; got:\n{nq}");
    }

    /// A malformed lexical for a RECOGNIZED XSD datatype HARD-fails (no-optionality):
    /// an authored ontology should never carry one, so surface it.
    #[test]
    fn malformed_recognized_literal_hard_fails() {
        let mut b = RdfDatasetBuilder::new();
        b.push_owned_quad(&typed_quad("not-a-decimal", XSD_DECIMAL));
        let ds = b.freeze().expect("freeze");
        let err = dataset_to_nquads(ds.as_ref())
            .expect_err("a malformed xsd:decimal must hard-fail, not pass through");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("malformed typed literal"),
            "error must name the malformed typed literal; got: {msg}"
        );
    }

    /// A literal nested inside a quoted-triple (RDF 1.2 `<< s p o >>`) object is
    /// canonicalized too (the recursion contract): `xsd:decimal` `0.90`→`0.9`.
    #[test]
    fn quoted_triple_object_literal_is_canonicalized() {
        let inner = RdfTriple::new(
            RdfTerm::iri("https://example.org/qs"),
            "https://example.org/qp",
            RdfTerm::literal(RdfLiteral::typed("0.90", XSD_DECIMAL)),
        );
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::triple(inner),
        ));
        assert!(
            nq.contains(&format!("\"0.9\"^^<{XSD_DECIMAL}>")),
            "the literal inside a quoted triple must canonicalize 0.90→0.9; got:\n{nq}"
        );
        assert!(
            !nq.contains("\"0.90\""),
            "the raw 0.90 form must not survive inside the quoted triple; got:\n{nq}"
        );
    }
}

#[cfg(test)]
mod ustar_tests {
    use super::*;

    /// The GNU long-name sentinel used in wire-format assertions.
    const LONGLINK_NAME: &str = "././@LongLink";

    /// Decode `(name, bytes)` members from a USTAR archive via the shared codec.
    fn parse(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        gmeow_rdf::ustar::read_archive(raw).unwrap()
    }

    #[test]
    fn long_member_name_round_trips_via_longlink() {
        let long = format!(
            "x-gmeow-english/terms/classes/gmeow-{}.html",
            "A".repeat(90)
        );
        assert!(long.len() > 100, "fixture must exceed the 100-byte field");
        let members = vec![
            (long.clone(), b"<html>long</html>".to_vec()),
            ("x-gmeow-english/index.html".to_string(), b"idx".to_vec()),
        ];
        let raw = gmeow_rdf::ustar::write_archive(&members).expect("archive");
        let got = parse(&raw);
        assert_eq!(got, members, "GNU LongLink path must round-trip exactly");

        // The first record on the wire is the 'L' LongLink, then the real header
        // whose name field is the 100-byte truncation of the long path.
        assert_eq!(raw[156], b'L', "first record is a LongLink");
        assert_eq!(&raw[0..LONGLINK_NAME.len()], LONGLINK_NAME.as_bytes());
    }

    #[test]
    fn short_names_emit_no_longlink_and_stay_plain_ustar() {
        let members = vec![
            ("mappings/a.sssom.tsv".to_string(), b"x".to_vec()),
            ("slices/core/x/tests/t.ttl".to_string(), vec![0u8; 600]),
        ];
        let raw = gmeow_rdf::ustar::write_archive(&members).expect("archive");
        // No member name overflows 100 bytes, so NO 'L' record may appear: the
        // four existing consumer archives must stay byte-identical (fold-stable).
        assert!(
            !raw.chunks(512).any(|c| c.len() == 512 && c[156] == b'L'),
            "short-name archive must not emit a LongLink record"
        );
        // The first header carries the full name inline (typeflag '0', ustar magic).
        assert_eq!(raw[156], b'0');
        assert_eq!(&raw[257..263], b"ustar\0");
        assert_eq!(&raw[263..265], b"00");
        assert_eq!(parse(&raw), members);
    }

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn build_docs_archive_packs_the_rendered_site() {
        let blob = build_docs_archive(&repo_root()).expect("docs archive");
        assert_eq!(blob.rep, REP_ONTOLOGY_DOCS);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the site archive must carry members");

        // Every member is under an INTERNAL `x-gmeow-*/` tag (English carrier plus
        // any translation language) — exactly the `{tag}/` prefix
        // `_unpack_doc_archive` filters on, NOT the carrier key (`english/`).
        assert!(
            members.iter().all(|(n, _)| n.starts_with("x-gmeow-")),
            "every member must carry an internal-tag prefix, got e.g. {:?}",
            members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
        );
        assert!(
            members
                .iter()
                .any(|(n, _)| n == "x-gmeow-english/index.html"),
            "the English landing page must be present"
        );
        // The site carries its structural assets (deterministic, language-keyed).
        for asset in [
            "assets/gmeow.css",
            "search-index.json",
            "llms.txt",
            "llms-full.txt",
        ] {
            let want = format!("x-gmeow-english/{asset}");
            assert!(
                members.iter().any(|(n, _)| n == &want),
                "expected site asset {want}"
            );
        }
        // The #1027 per-term card surface: at least one `card.md` file must
        // be present in the archive under the English carrier tag.
        let card_md_present = members
            .iter()
            .any(|(n, _)| n.starts_with("x-gmeow-english/terms/") && n.ends_with("/card.md"));
        assert!(
            card_md_present,
            "expected at least one x-gmeow-english/terms/<slug>/card.md in the docs archive"
        );
        // Member names CAN exceed the 100-byte USTAR field (LongLink-covered).
        // Today's longest stays under it, so LongLink is a defensive net rather
        // than currently-triggered — `long_member_name_round_trips_via_longlink`
        // is the dedicated proof. Logged so a future overflow is visible.
        let max_len = members.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        eprintln!(
            "ontology-docs: {} members, longest name {max_len}B",
            members.len()
        );
    }

    #[test]
    fn build_archive_blobs_folds_the_shapes_surface() {
        let root = repo_root();
        // schema/openapi bytes are irrelevant to the shapes blob; pass empty. The axiom
        // surface is irrelevant here too, but it must be present (fail-closed), so mirror
        // the committed projections into the artifact map.
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        let blobs = build_archive_blobs(&root, b"", b"", &axiom_artifacts).expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_SHAPES)
            .expect("REP_SHAPES blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the shape surface must carry members");
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();

        // Base hand-authored shape + the generated frame shape (P11) + ≥1 per-slice.
        assert!(names.contains(&"shapes/gmeow-shapes.ttl"));
        assert!(names.contains(&"generated/shapes/frame-shapes.ttl"));
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl")),
            "at least one per-slice shapes.ttl must be folded"
        );
        // The FULL surface carries the 4 DSL/manifest lints (the validator filters
        // them OUT of its data-graph union, but the consumer's DSL phases need them).
        for dsl in [
            "shapes/mapping-dsl-shapes.ttl",
            "shapes/statement-dsl-shapes.ttl",
            "shapes/test-dsl-shapes.ttl",
            "shapes/slice-manifest-shapes.ttl",
        ] {
            assert!(
                names.contains(&dsl),
                "DSL lint {dsl} must be in the FULL shape surface"
            );
        }
        // Member count == on-disk count (no silent drops).
        let on_disk = list_files(&root.join("shapes"), "ttl").unwrap().len()
            + list_files(&root.join("generated/shapes"), "ttl")
                .unwrap()
                .len()
            + slice_named_files(&root, "shapes.ttl").unwrap().len();
        assert_eq!(
            members.len(),
            on_disk,
            "every shape file must be folded exactly once"
        );
        // The slice-shape subset matches an independent on-disk enumeration — pins
        // `slice_named_files` against drift from the shacl crate's private walk.
        let folded_slices: std::collections::BTreeSet<&str> = names
            .iter()
            .copied()
            .filter(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl"))
            .collect();
        let disk_slices: std::collections::BTreeSet<String> =
            slice_named_files(&root, "shapes.ttl")
                .unwrap()
                .iter()
                .map(|p| {
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
        let disk_slices_ref: std::collections::BTreeSet<&str> =
            disk_slices.iter().map(String::as_str).collect();
        assert_eq!(folded_slices, disk_slices_ref);
        // Keys sorted (deterministic fold).
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "shape members must be sorted by key");
    }

    #[test]
    fn build_archive_blobs_folds_the_axiom_surface() {
        let root = repo_root();
        // The axiom surface is now sourced from the stage-compile-logic product; mirror
        // that here by reading the committed projections into the artifact map.
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        let blobs = build_archive_blobs(&root, b"", b"", &axiom_artifacts).expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_AXIOMS)
            .expect("REP_AXIOMS blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        let names: std::collections::BTreeSet<&str> =
            members.iter().map(|(n, _)| n.as_str()).collect();
        // Exactly the 5 compiled projections — no more, no less.
        let want: std::collections::BTreeSet<&str> = AXIOM_FILES.iter().copied().collect();
        assert_eq!(
            names, want,
            "REP_AXIOMS must carry exactly the 5 projection files"
        );
        // The big reasoning OUTPUTS ride other channels — never in REP_AXIOMS.
        for big in [
            "generated/logic/inferred-closure.rdf12.ttl",
            "generated/logic/reasoning-explanations.rdf12.ttl",
            "generated/logic/dl-el-crosscheck-report.ttl",
        ] {
            assert!(!names.contains(big), "{big} must NOT be in REP_AXIOMS");
        }
        // Determinism: rebuild and assert byte-equality.
        let again = build_archive_blobs(&root, b"", b"", &axiom_artifacts).expect("archive blobs");
        let blob2 = again.iter().find(|b| b.rep == REP_AXIOMS).unwrap();
        assert_eq!(
            blob.data, blob2.data,
            "REP_AXIOMS must be byte-deterministic"
        );
    }

    #[test]
    fn build_reasoning_blob_folds_the_report_artifacts() {
        // Construct a fake stage-reason product with the two report artifacts (avoids
        // running the reasoner); proves the wiring (rep, keys, fail-closed).
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# explanations".to_vec(),
        );
        artifacts.insert(
            crate::stages::reason::LEDGER_PATH.to_string(),
            b"# ledger".to_vec(),
        );
        artifacts.insert(
            crate::stages::reason::PERF_LEDGER_PATH.to_string(),
            b"# perf ledger".to_vec(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", artifacts),
        );
        let blob = build_reasoning_blob(&upstream).expect("reasoning blob");
        assert_eq!(blob.rep, REP_REASONING);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        let names: std::collections::BTreeSet<&str> =
            members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "reason/dl-el-crosscheck-report.ttl",
                "reason/perf-ledger.ttl",
                "reason/reasoning-explanations.rdf12.ttl"
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<&str>>(),
            "REP_REASONING carries the report artifacts under bundle-relative keys"
        );
        // Missing artifact HARD-fails (no-optionality, fail-closed).
        let empty: BTreeMap<String, StageProduct> = BTreeMap::new();
        assert!(
            build_reasoning_blob(&empty).is_err(),
            "a missing stage-reason product must fail closed"
        );
    }

    #[test]
    fn build_okf_archive_packs_the_rust_rendered_bundle() {
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).expect("committed gts");
        let dataset = gmeow_rdf::import_gts_events(&gts)
            .expect("import committed gts")
            .dataset;
        let blob = build_okf_blob_from_dataset(dataset.as_ref()).expect("okf archive");
        assert_eq!(blob.rep, REP_OKF);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the OKF archive must carry members");
        assert!(
            members.iter().all(|(n, _)| n.starts_with("gmeow-okf/")),
            "every OKF member must be bundle-relative under gmeow-okf/, got e.g. {:?}",
            members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
        );
        assert!(
            members.iter().any(|(n, _)| n == "gmeow-okf/index.md"),
            "root OKF index must be present"
        );
        for required_dir in ["classes", "properties", "individuals"] {
            let prefix = format!("gmeow-okf/{required_dir}/");
            assert!(
                members
                    .iter()
                    .any(|(n, _)| n.starts_with(&prefix) && !n.ends_with("/index.md")),
                "expected at least one OKF document under {prefix}"
            );
        }
        let root_index = members
            .iter()
            .find(|(n, _)| n == "gmeow-okf/index.md")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("root index bytes");
        assert!(
            root_index.contains("LOSSY projection"),
            "root OKF index must declare projection loss"
        );

        let blob2 = build_okf_blob_from_dataset(dataset.as_ref()).expect("second okf archive");
        assert_eq!(blob.data, blob2.data, "OKF archive must be deterministic");
    }

    #[test]
    fn header_checksum_is_valid() {
        // Build a minimal archive and inspect the first 512-byte header.
        let members = vec![("x-gmeow-english/index.html".to_string(), vec![0u8; 42])];
        let raw = gmeow_rdf::ustar::write_archive(&members).expect("archive");
        let h: &[u8] = &raw[..512];
        // The stored checksum equals the sum of all bytes with the checksum field
        // taken as spaces — the canonical USTAR self-check.
        let stored = usize::from_str_radix(
            std::str::from_utf8(&h[148..154])
                .unwrap()
                .trim_matches('\0')
                .trim(),
            8,
        )
        .unwrap();
        let mut probe = [0u8; 512];
        probe.copy_from_slice(h);
        for b in &mut probe[148..156] {
            *b = b' ';
        }
        let computed: usize = probe.iter().map(|&b| b as usize).sum();
        assert_eq!(stored, computed);
    }
}

#[cfg(test)]
mod conformance_fold_tests {
    use super::*;

    /// Read every named-graph IRI present in a folded snapshot's quad table.
    fn folded_graph_names(gts: &[u8]) -> std::collections::BTreeSet<String> {
        let g = gmeow_rdf::gts::read_graph(gts, true).expect("read_graph");
        let mut names = std::collections::BTreeSet::new();
        for &(_, _, _, gname) in &g.quads {
            if let Some(gid) = gname {
                if let Some(value) = g.terms.get(gid).and_then(|t| t.value.clone()) {
                    names.insert(value);
                }
            }
        }
        names
    }

    /// A synthetic divergence Finding folds into the `graph/conformance` named
    /// graph of the emitted snapshot — the C3 fold contract. Constructed
    /// independently of the (currently all-agree) committed corpus so the assertion
    /// holds regardless of whether a real divergence exists today.
    #[test]
    fn synthetic_divergence_lands_in_graph_conformance() {
        // One CorpusOnly + one DlGap divergence, projected to gmeow:Finding N-Quads
        // in the conformance graph by the shared emitter.
        let conformance = gmeow_conformance::divergence::emit_divergence_nq(
            "w3c-owl2-el",
            &[
                gmeow_logic::reason::ExternalComparison {
                    case: "clash".to_owned(),
                    world: "https://gmeow.example/w3c-owl2-el/clash/w".to_owned(),
                    native: "consistent".to_owned(),
                    published: "inconsistent".to_owned(),
                },
                gmeow_logic::reason::ExternalComparison {
                    case: "beyond-el".to_owned(),
                    world: "https://gmeow.example/w3c-owl2-el/beyond-el/w".to_owned(),
                    native: "incomplete".to_owned(),
                    published: "consistent".to_owned(),
                },
            ],
        );
        assert!(
            !conformance.is_empty(),
            "the synthetic divergence must emit Findings"
        );

        // Fold it through the SAME add_named path build_snapshot uses, emit, and
        // read the bundle back.
        let mut builder = SnapshotBuilder::new();
        // A non-empty default graph so the bundle is well-formed.
        let base = parse_nq(
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        )
        .expect("base parse");
        builder.add_quads(&base, None, Some("base"));
        add_named(
            &mut builder,
            conformance.as_bytes(),
            GRAPH_CONFORMANCE,
            "conformance",
        )
        .expect("fold conformance graph");

        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["gzip".to_string()]),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        let names = folded_graph_names(&gts);
        assert!(
            names.contains(GRAPH_CONFORMANCE),
            "the folded snapshot must carry the graph/conformance named graph; got {names:?}"
        );
    }

    /// An empty divergence (the all-agree corpus) is skipped — folding empty bytes
    /// must NOT add a phantom `graph/conformance` slot.
    #[test]
    fn empty_divergence_adds_no_conformance_graph() {
        let mut builder = SnapshotBuilder::new();
        let base = parse_nq(
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        )
        .expect("base parse");
        builder.add_quads(&base, None, Some("base"));
        // Mirror build_snapshot's guard: an empty graph is never add_named'd.
        let conformance: Vec<u8> = Vec::new();
        if !conformance.is_empty() {
            add_named(&mut builder, &conformance, GRAPH_CONFORMANCE, "conformance").expect("fold");
        }

        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["gzip".to_string()]),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        assert!(
            !folded_graph_names(&gts).contains(GRAPH_CONFORMANCE),
            "an all-agree corpus must not fold a phantom graph/conformance"
        );
    }
}

#[cfg(test)]
mod logic_graph_golden_tests {
    use super::*;
    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram};

    const GRAPH_LOGIC: &str = crate::stages::compile_logic::GRAPH_LOGIC;

    /// A small, FIXED clean program — its canonical RDF-1.2 projection is the byte
    /// golden subject. Deliberately synthetic (not the real module) so the golden is
    /// stable and the per-graph fold is regression-pinned independent of the full
    /// gmeow.gts and independent of any logic-module edit.
    fn fixed_program() -> LogicProgram {
        let ax = |s: &str, o: &str| {
            LogicAxiom::new(
                s,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                o,
                false,
                false,
                ContextualScope::default(),
            )
            .expect("valid axiom")
        };
        LogicProgram::new(
            vec![
                ax(
                    "https://blackcatinformatics.ca/gmeow/Animal",
                    "https://blackcatinformatics.ca/logic/Kind",
                ),
                ax(
                    "https://blackcatinformatics.ca/gmeow/Cat",
                    "https://blackcatinformatics.ca/logic/Subkind",
                ),
            ],
            vec![],
            vec![],
            None,
        )
    }

    /// Read the canonical N-Quads of one named graph from an emitted snapshot,
    /// sorted — a deterministic byte surface for the per-graph golden.
    fn folded_graph_nquads(gts: &[u8], graph_iri: &str) -> String {
        let g = gmeow_rdf::gts::read_graph(gts, true).expect("read_graph");
        let mut rows: Vec<String> = Vec::new();
        for &(s, p, o, gname) in &g.quads {
            let Some(gid) = gname else { continue };
            let in_graph = g
                .terms
                .get(gid)
                .and_then(|t| t.value.clone())
                .is_some_and(|v| v == graph_iri);
            if !in_graph {
                continue;
            }
            let term = |id: usize| -> String {
                let t = &g.terms[id];
                match t.value.clone() {
                    Some(v) if v.starts_with("http") || v.starts_with("urn:") => format!("<{v}>"),
                    Some(v) => v,
                    None => format!("_:{id}"),
                }
            };
            rows.push(format!("{} {} {} .", term(s), term(p), term(o)));
        }
        rows.sort();
        rows.join("\n")
    }

    /// Byte golden (#1132 C6): the `graph/logic` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic program. Pins the per-graph fold path
    /// (canonical RDF-1.2 → N-Quads → add_named canonicalization → emit → read-back)
    /// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
    /// byte-identical (determinism).
    #[test]
    fn graph_logic_fold_byte_golden() {
        let arts = gmeow_logic_compile::projections::compile_program(&fixed_program())
            .expect("compile fixed program");
        let logic_nq = turtle_to_nquads(arts.canonical_rdf12.as_bytes()).expect("turtle → nq");

        let build = || {
            let mut builder = SnapshotBuilder::new();
            let base = parse_nq(
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            )
            .expect("base parse");
            builder.add_quads(&base, None, Some("base"));
            add_named(&mut builder, &logic_nq, GRAPH_LOGIC, "logic").expect("fold graph/logic");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_LOGIC);
        assert!(!folded.is_empty(), "graph/logic must carry the projection");
        insta::assert_snapshot!("graph_logic_fold", folded);

        // Determinism: a second build folds the SAME graph/logic content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_LOGIC),
            folded,
            "the graph/logic fold must be byte-deterministic"
        );
    }

    const GRAPH_REASONING: &str = gmeow_logic::result_rdf::GRAPH_REASONING;

    /// A FIXED synthetic reasoning result — the byte-golden subject for the
    /// `graph/reasoning` fold (deliberately synthetic so the golden is stable and
    /// independent of any reasoner output).
    fn fixed_reasoning_result() -> gmeow_logic::result::ReasoningResult {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };
        ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Supported,
            ResultProvenance::native(
                "contract:golden",
                "https://blackcatinformatics.ca/gmeow/graph/world/actual",
            ),
            ResultPayload::Empty,
        )
    }

    /// Byte golden (#1132 C7): the `graph/reasoning` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic reasoning result. Pins the per-graph fold path
    /// (project → N-Triples → add_named canonicalization → emit → read-back)
    /// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
    /// byte-identical (determinism).
    #[test]
    fn graph_reasoning_fold_byte_golden() {
        let reasoning_nt =
            gmeow_logic::result_rdf::project_reasoning_result(&fixed_reasoning_result());

        let build = || {
            let mut builder = SnapshotBuilder::new();
            let base = parse_nq(
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            )
            .expect("base parse");
            builder.add_quads(&base, None, Some("base"));
            add_named(
                &mut builder,
                reasoning_nt.as_bytes(),
                GRAPH_REASONING,
                "reasoning",
            )
            .expect("fold graph/reasoning");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_REASONING);
        assert!(
            !folded.is_empty(),
            "graph/reasoning must carry the projection"
        );
        insta::assert_snapshot!("graph_reasoning_fold", folded);

        // Determinism: a second build folds the SAME graph/reasoning content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_REASONING),
            folded,
            "the graph/reasoning fold must be byte-deterministic"
        );
    }

    const GRAPH_RELATIONAL_CORE: &str = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;

    /// A FIXED synthetic relational-core program — the byte-golden subject for the
    /// `graph/relational-core` fold (a clean Horn program with one rule, so the golden
    /// is stable and independent of the real module).
    fn fixed_relational_core() -> gmeow_logic_compile::relational_core::RelationalCoreProgram {
        use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
        use gmeow_logic_compile::relational_core::lower_program;
        let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let ax = |s: &str, p: &str, o: &str| {
            LogicAxiom::new(s, p, o, false, false, ContextualScope::default()).expect("axiom")
        };
        // ?x sc ?z :- ?x sc ?y, ?y sc ?z .
        let rule = LogicRule::new(
            ax("?x", sc, "?z"),
            vec![ax("?x", sc, "?y"), ax("?y", sc, "?z")],
            vec![],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(
            vec![ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                sc,
                "https://blackcatinformatics.ca/gmeow/Animal",
            )],
            vec![rule],
            vec![],
            None,
        );
        lower_program(&program)
    }

    /// Byte golden (#1132 C8): the `graph/relational-core` named-graph content of an
    /// emitted snapshot, over a FIXED synthetic relational-core program. Pins the
    /// per-graph fold path (lower → project N-Triples → add_named canonicalization →
    /// emit → read-back) byte-for-byte, independent of the full gmeow.gts. A second emit
    /// is asserted byte-identical (determinism).
    #[test]
    fn graph_relational_core_fold_byte_golden() {
        let rc_nt =
            gmeow_logic_compile::relational_core::project_relational_core(&fixed_relational_core());

        let build = || {
            let mut builder = SnapshotBuilder::new();
            let base = parse_nq(
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            )
            .expect("base parse");
            builder.add_quads(&base, None, Some("base"));
            add_named(
                &mut builder,
                rc_nt.as_bytes(),
                GRAPH_RELATIONAL_CORE,
                "relcore",
            )
            .expect("fold graph/relational-core");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_RELATIONAL_CORE);
        assert!(
            !folded.is_empty(),
            "graph/relational-core must carry the projection"
        );
        insta::assert_snapshot!("graph_relational_core_fold", folded);

        // Determinism: a second build folds the SAME graph/relational-core content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_RELATIONAL_CORE),
            folded,
            "the graph/relational-core fold must be byte-deterministic"
        );
    }

    const GRAPH_CORRESPONDENCE: &str = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;

    /// Byte golden (#1132 C10): the `graph/correspondence` named-graph content of an
    /// emitted snapshot, over the §14 affine-triangle worked example. Pins the per-graph
    /// fold path (construct → project N-Triples → add_named canonicalization → emit →
    /// read-back) byte-for-byte, independent of the full gmeow.gts. Also asserts the
    /// load-bearing correctness point in the folded bytes: `skos:relatedMatch` present,
    /// `skos:exactMatch` + `owl:equivalentClass` absent, the loss-ledger row present. A
    /// second emit is asserted byte-identical (determinism).
    #[test]
    fn graph_correspondence_fold_byte_golden() {
        let corr_nt = gmeow_logic_compile::projections::correspondence::project_correspondence(
            &gmeow_logic_compile::projections::correspondence::affine_triangle_worked_example(),
        );

        let build = || {
            let mut builder = SnapshotBuilder::new();
            let base = parse_nq(
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            )
            .expect("base parse");
            builder.add_quads(&base, None, Some("base"));
            add_named(
                &mut builder,
                corr_nt.as_bytes(),
                GRAPH_CORRESPONDENCE,
                "correspondence",
            )
            .expect("fold graph/correspondence");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_CORRESPONDENCE);
        assert!(
            !folded.is_empty(),
            "graph/correspondence must carry the projection"
        );
        // The load-bearing correctness point, asserted on the FOLDED snapshot bytes —
        // checking the alignment PREDICATE position, not bare substrings (the loss-ledger
        // prose mentions the forbidden predicate names as disclosure, not as edges).
        assert!(
            folded.contains("<http://www.w3.org/2004/02/skos/core#relatedMatch>"),
            "the folded correspondence graph keeps the overlap at skos:relatedMatch:\n{folded}"
        );
        assert!(
            !folded.contains("<http://www.w3.org/2004/02/skos/core#exactMatch>"),
            "the folded correspondence graph MUST NOT emit a skos:exactMatch edge:\n{folded}"
        );
        assert!(
            !folded.contains("<http://www.w3.org/2002/07/owl#equivalentClass>"),
            "the folded correspondence graph MUST NOT emit an owl:equivalentClass edge:\n{folded}"
        );
        assert!(
            folded.contains("lossyDrop"),
            "the folded correspondence graph MUST carry the loss-ledger row:\n{folded}"
        );
        insta::assert_snapshot!("graph_correspondence_fold", folded);

        // Determinism: a second build folds the SAME graph/correspondence content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_CORRESPONDENCE),
            folded,
            "the graph/correspondence fold must be byte-deterministic"
        );
    }

    const GRAPH_PROVENANCE: &str = crate::stages::provenance_graph::GRAPH_PROVENANCE;

    /// A FIXED synthetic provenance projection — the byte-golden subject for the
    /// `graph/provenance` fold. Three units (root / source / import) so every
    /// `OriginKind` branch is exercised; deliberately synthetic so the golden is
    /// stable and independent of the real ontology (whose unit set churns).
    fn fixed_provenance_projection() -> Vec<(usize, String, String, String, Option<String>)> {
        vec![
            (
                0,
                "imports/prov.ttl".to_string(),
                "import".to_string(),
                "imports/prov.ttl".to_string(),
                None,
            ),
            (
                1,
                "ontology/gmeow.ttl".to_string(),
                "root-ontology".to_string(),
                "ontology/gmeow.ttl".to_string(),
                None,
            ),
            (
                2,
                "slices/core/epistemics/module.ttl".to_string(),
                "source".to_string(),
                "slices/core/epistemics/module.ttl".to_string(),
                None,
            ),
        ]
    }

    /// Byte golden (#1132 C9): the `graph/provenance` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic provenance projection. Pins the per-graph fold
    /// path (public projection → N-Triples → add_named canonicalization → emit →
    /// read-back) byte-for-byte, independent of the full gmeow.gts. A second emit is
    /// asserted byte-identical (determinism). The golden ALSO proves S0.5 (no runtime id).
    #[test]
    fn graph_provenance_fold_byte_golden() {
        let prov_nt = crate::stages::provenance_graph::project_provenance_graph(
            &fixed_provenance_projection(),
        );

        let build = || {
            let mut builder = SnapshotBuilder::new();
            let base = parse_nq(
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            )
            .expect("base parse");
            builder.add_quads(&base, None, Some("base"));
            add_named(
                &mut builder,
                prov_nt.as_bytes(),
                GRAPH_PROVENANCE,
                "provenance",
            )
            .expect("fold graph/provenance");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_PROVENANCE);
        assert!(
            !folded.is_empty(),
            "graph/provenance must carry the projection"
        );
        // S0.5: the folded bytes must NOT contain any runtime id.
        assert!(
            !folded.contains("unit#"),
            "no runtime UnitId in graph/provenance"
        );
        assert!(
            !folded.contains("artifact#"),
            "no runtime ArtifactId in graph/provenance"
        );
        assert!(
            !folded.contains("origin-set#"),
            "no runtime OriginSetId in graph/provenance"
        );
        insta::assert_snapshot!("graph_provenance_fold", folded);

        // Determinism: a second build folds the SAME graph/provenance content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_PROVENANCE),
            folded,
            "the graph/provenance fold must be byte-deterministic"
        );
    }

    /// The hard-fail attribution gate passes on the REAL ontology (#1132 C9): every
    /// authored quad carries ≥1 stage-origin occurrence. Builds the real per-quad
    /// provenance sidecar and runs `check_provenance` over its full coverage set.
    #[test]
    fn real_ontology_every_quad_is_attributed() {
        let root = repo_root();
        let (prov, expected) =
            crate::stages::source_load::attributed_base_provenance(&root).expect("attribute");
        assert!(
            expected.len() > 5_000,
            "real authored base graph unexpectedly small: {} quads",
            expected.len()
        );
        gmeow_rdf::provenance::check_provenance(&prov, &expected)
            .expect("every authored quad must carry ≥1 stage-origin occurrence");
        // The public projection over the real ontology must carry NO runtime id.
        for (_quad, name, kind, artifact, _loc) in prov.public_projection() {
            for field in [&name, &kind, &artifact] {
                assert!(!field.contains("unit#"), "runtime UnitId leaked: {field}");
                assert!(
                    !field.contains("artifact#"),
                    "runtime ArtifactId leaked: {field}"
                );
                assert!(
                    !field.contains("origin-set#"),
                    "runtime OriginSetId leaked: {field}"
                );
            }
        }
    }

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }
}

#[cfg(test)]
mod native_assembly_tests {
    use super::*;

    /// Count the `owl:AllDisjointClasses` typed subjects (blank nodes) and the
    /// `owl:members` list-head triples in a canonical N-Quads blob.
    fn disjoint_shape(canon: &str) -> (usize, usize) {
        let all_disjoint = canon
            .lines()
            .filter(|l| {
                l.contains("<http://www.w3.org/2002/07/owl#AllDisjointClasses>")
                    && l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>")
            })
            .count();
        let members = canon
            .lines()
            .filter(|l| l.contains("<http://www.w3.org/2002/07/owl#members>"))
            .count();
        (all_disjoint, members)
    }

    /// Two distinct `owl:AllDisjointClasses` axioms authored in SEPARATE files MUST
    /// survive the native `RdfDataset::union` standardize-apart as TWO distinct blank
    /// lists — never collapsing into one. This is exactly why the removed
    /// `ingest_turtle_scoped` string-prefixed per-file blanks; the union's per-input
    /// `BlankScope` is its native replacement. Each file independently mints `_:b0`
    /// (the codecs restart blank counters per parse), so without standardize-apart the
    /// two axioms would merge into a single subject and one of the lists would vanish.
    #[test]
    fn two_all_disjoint_lists_survive_union_distinctly() {
        // Two files, each with ONE owl:AllDisjointClasses over a DIFFERENT class set,
        // both anonymous (blank-node subject + blank-node list cells).
        let file_a = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:A ex:B ex:C ) .
"#;
        let file_b = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:D ex:E ) .
"#;

        let union = union_turtle_datasets(&[file_a.to_vec(), file_b.to_vec()])
            .expect("union two disjoint files");
        let nq = dataset_to_nquads(&union).expect("union → n-quads");
        let canon = canonicalize_nq(&nq, "base").expect("canonicalize union");

        let (subjects, members) = disjoint_shape(&canon);
        assert_eq!(
            subjects, 2,
            "the union must keep TWO distinct AllDisjointClasses subjects (one per file); \
             a collapse would leave only 1.\nCanonical:\n{canon}"
        );
        assert_eq!(
            members, 2,
            "each AllDisjointClasses must keep its own owl:members list head"
        );

        // The two list contents (3-element and 2-element) must both be present — a
        // collapse would lose one set entirely. Count rdf:first cells: 3 + 2 = 5.
        let first_cells = canon
            .lines()
            .filter(|l| l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#first>"))
            .count();
        assert_eq!(
            first_cells, 5,
            "both lists (3 + 2 members) must survive distinctly; got {first_cells} rdf:first cells"
        );

        // Contrast: parsing BOTH files into ONE dataset WITHOUT standardize-apart
        // would let the two `_:b0` subjects collide. We can't easily force that here,
        // but the union path above is the production assembly — its 2-subject result
        // is the proof the native union preserves per-file distinctness.
    }

    /// The projection-ledger named graph built natively (`turtle_to_nquads`) is
    /// graph-isomorphic to the legacy oxigraph-`Store` conversion of the SAME Turtle
    /// report. Both paths canonicalize to byte-identical N-Quads (no blank-label drift,
    /// no literal-form drift), so the C3 native swap is a faithful replacement.
    #[test]
    fn projection_ledger_matches_oxigraph_conversion() {
        // A representative projection-report fragment: typed loss-ledger entries with
        // a blank-node structural-drop list (exercises blank canonicalization).
        let report_ttl = br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
gmeow:projection/okf
    a gmeow:ProjectionLedgerEntry ;
    rdfs:label "OKF projection" ;
    gmeow:preservationKind gmeow:Lossy ;
    gmeow:droppedCount "3"^^xsd:integer ;
    gmeow:structuralDrop [ gmeow:dropKind gmeow:StatementLayer ] .
"#;

        // Native path: the C3 helper.
        let native = turtle_to_nquads(report_ttl).expect("native turtle → n-quads");
        let native_canon = canonicalize_nq(&native, "projledger").expect("canon native");

        // Legacy path: parse via the native codec, route through an oxigraph Store,
        // serialize back (the exact `Store::new()+ingest_turtle+store_to_nquads` the
        // C3 swap removed).
        let store = oxigraph::store::Store::new().expect("store");
        for quad in parse_rdf(report_ttl, "text/turtle").expect("parse report") {
            store.insert(&quad).expect("insert");
        }
        let store_ds = gmeow_rdf::oxigraph::dataset_from_store(&store).expect("store → ds");
        let legacy = serialize_dataset(&store_ds, "application/n-quads", SerializeGraph::Dataset)
            .expect("serialize store");
        let legacy_canon = canonicalize_nq(&legacy, "projledger").expect("canon legacy");

        assert_eq!(
            native_canon, legacy_canon,
            "the native projection-ledger N-Quads must be graph-isomorphic to the \
             oxigraph-Store conversion (canonical byte-equality)"
        );
    }

    /// `load_authored_default` over the real repo tree produces a non-empty
    /// multilingual default graph (the union path + native translation/guideBlob fold),
    /// and the guideBlob anchors land on real slice IRIs.
    #[test]
    fn authored_default_assembles_natively() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let nq = load_authored_default(&root).expect("authored default graph");
        let text = String::from_utf8(nq).expect("utf-8 n-quads");
        assert!(
            !text.trim().is_empty(),
            "the default graph must be non-empty"
        );
        // The root ontology declaration must be present (the ontology IRI has no
        // trailing slash — `…/gmeow`, distinct from the `…/gmeow/` namespace prefix).
        assert!(
            text.contains("<https://blackcatinformatics.ca/gmeow>"),
            "the authored default graph must carry the root ontology subject"
        );
        // At least one guideBlob anchor (per-slice docs.md digest) must be folded.
        assert!(
            text.contains("<https://blackcatinformatics.ca/gmeow/guideBlob>")
                && text.contains("blake3:"),
            "the native guideBlob fold must inject blake3 digest anchors"
        );
        // Determinism: a second assembly is byte-identical.
        let again = load_authored_default(&root).expect("authored default graph (2)");
        assert_eq!(
            text.as_bytes(),
            again.as_slice(),
            "the native authored assembly must be byte-deterministic"
        );
    }
}
