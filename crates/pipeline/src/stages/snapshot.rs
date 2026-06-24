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
use gmeow_rdf::oxigraph::OxigraphStore;
use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::dataset::CanonicalizationAlgorithm;
use oxigraph::model::{Dataset, Quad};
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
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
) -> Result<Vec<u8>, PipelineError> {
    // ── the authored default graph (ontology + slice modules; NO imports) ──────
    let authored = load_authored_default(root)?;
    let authored_canon = canonicalize_nq(&authored, "base")?;

    // ── the named-graph sources ────────────────────────────────────────────────
    let imports = load_imports(root)?;
    let metadata = load_metadata(root)?;
    let alignments = load_alignments(root)?;
    let rdf12 = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(RDF12_PATH))
        .ok_or_else(|| stage_err("missing statements RDF 1.2 artifact"))?
        .to_vec();
    let slice_analysis = build_slice_analysis(root, &authored)?;
    let documentation = upstream
        .get("stage-docs-render")
        .and_then(|p| p.artifact(crate::stages::docs_render::DOCS_GRAPH_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing docs-render documentation graph"))?;

    // Pass 1: build WITHOUT the verify graph, then run native verify over the
    // default graph ∪ imports (the closed-world integrity constraints query that
    // union; the verify graph itself is never an input — #695).
    let verify_attestation = {
        let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
        ingest_nq(&store, authored_canon.as_bytes())?;
        ingest_turtle(&store, &imports)?;
        let view = OxigraphStore::new(&store);
        run_verify_attestation(root, &view)?
    };

    // ── the builder: route every source into its named graph ────────────────────
    let mut builder = SnapshotBuilder::new();
    // default ← the canonicalized authored ontology only.
    let base_quads = parse_nq(authored_canon.as_bytes())?;
    reject_quoted_triples(&base_quads, "<default>")?;
    builder.add_quads(&base_quads, None, Some("base"));
    // RDF 1.2 statement layer: base quads → graph/statements; reifies/annot global.
    builder
        .add_rdf12(
            &parse_rdf(&rdf12, RdfFormat::Turtle)?,
            Some(GRAPH_STATEMENTS),
            Some("stmt"),
        )
        .map_err(|e| stage_err(&format!("rdf12 ingest: {e}")))?;
    // graph/alignments ← SSSOM alignment axioms (canonicalized).
    add_named(&mut builder, &alignments, GRAPH_ALIGNMENTS, "align")?;
    // graph/imports ← vendored import closure.
    add_named(&mut builder, &imports, GRAPH_IMPORTS, "imports")?;
    // graph/metadata ← self-description.
    add_named(&mut builder, &metadata, GRAPH_METADATA, "metadata")?;
    // graph/slice-analysis ← computed ownership/dependency graph.
    add_named(
        &mut builder,
        &slice_analysis,
        GRAPH_SLICE_ANALYSIS,
        "slice-analysis",
    )?;
    // graph/verify ← the two-pass attestation.
    add_named(&mut builder, &verify_attestation, GRAPH_VERIFY, "verify")?;
    // graph/documentation ← the docs projection (N-Quads, already in its graph).
    add_named(&mut builder, &documentation, GRAPH_DOCUMENTATION, "doc")?;

    emit_gts(
        &builder,
        "dist",
        Some(vec!["gzip".to_string()]),
        blobs,
        Vec::new(),
        None,
        None,
        None,
        gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(|e| stage_err(&format!("emit_gts: {e}")))
}

/// Read this run's freshly-composed `gmeow.gts` snapshot bytes from the
/// `stage-snapshot` upstream product. Every fold-reading export leaf calls this
/// instead of `std::fs::read("generated/dist/gmeow.gts")`, so a single-pass run
/// reads THIS run's fold rather than the (potentially stale) committed file. The
/// bytes are fold-isomorphic to the committed snapshot (proven by `fold_parity`).
pub(crate) fn snapshot_bytes(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<u8>, PipelineError> {
    upstream
        .get("stage-snapshot")
        .and_then(|p| p.artifact(SNAPSHOT_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing stage-snapshot gmeow.gts artifact"))
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

const REP_MAPPINGS: &str = "mappings-archive";
const REP_CELLS: &str = "cells-archive";
const REP_QUERIES: &str = "queries-archive";
const REP_TESTS: &str = "tests-archive";
/// tar of the SHACL-derived JSON Schema + OpenAPI (#700), member = bare filename.
const REP_SCHEMAS: &str = "schemas-archive";
/// The full rendered ontology-docs static site (#897). The rep MUST equal the
/// string the runtime consumer (`create_docs._unpack_doc_archive`) looks up —
/// `"ontology-docs"`, NOT an `-archive` variant — so `gmeow extract-docs` finds it.
const REP_ONTOLOGY_DOCS: &str = "ontology-docs";
const ARCHIVE_MEDIA_TYPE: &str = "application/x-tar";
/// The GNU long-name sentinel: a `'L'`-typeflag record carrying a member path
/// that overflows the 100-byte USTAR `name` field. The doc-site archive (#897)
/// has paths well past 100 bytes; mappings/cells/queries/tests never do, so they
/// never emit this record and stay byte-identical.
const LONGLINK_NAME: &str = "././@LongLink";

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

/// Build the five bundle archive blobs from the repo tree. The SHACL-derived JSON
/// Schema + OpenAPI bytes are passed in from THIS run's `stage-export-json-schema`
/// product (not re-read from disk) so a single regenerate folds the fresh schema —
/// the committed `generated/schemas/*.json` are not flushed until phase 1 returns.
fn build_archive_blobs(
    root: &Path,
    schema_json: &[u8],
    openapi_json: &[u8],
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
    Ok(vec![
        archive_blob(REP_MAPPINGS, &mappings)?,
        archive_blob(REP_CELLS, &cells)?,
        archive_blob(REP_QUERIES, &queries)?,
        archive_blob(REP_TESTS, &tests)?,
        archive_blob(REP_SCHEMAS, &schemas)?,
    ])
}

/// Render the full ontology-docs static site and pack it into the single
/// `ontology-docs` archive blob (#897) — the producer half of repo-free
/// `gmeow extract-docs`.
///
/// The rust doc generator (`gmeow_docs::render_site_lang`) emits a complete site
/// (`index.md`/`index.html` per page, `assets/gmeow.css`, SVG diagrams,
/// `search-index.json`, `llms-docs.txt`, alias redirects) as a deterministic
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

    let mut members: Vec<(String, Vec<u8>)> = Vec::new();
    for lang in gmeow_docs::available_languages(&translations) {
        let site = gmeow_docs::render_site_lang(&model, &lang);
        let prefix = translations.internal_tag(&lang);
        for (path, bytes) in site.files {
            members.push((format!("{prefix}/{path}"), bytes));
        }
    }
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
        data: ustar_archive(members)?,
        media_type: ARCHIVE_MEDIA_TYPE.to_string(),
        rep: rep.to_string(),
    })
}

/// A byte-deterministic USTAR archive: per-member 512-byte header + 512-padded
/// data, terminated by two zero blocks. mtime/uid/gid = 0, mode = 0644.
///
/// A member whose name overflows the 100-byte `name` field is preceded by a GNU
/// `'L'` (`LongLink`) record carrying the full path (NUL-terminated, 512-padded);
/// the real header then truncates the name to 100 bytes (GNU convention — readers
/// take the path from the LongLink). Names ≤ 100 bytes emit no LongLink and are
/// byte-identical to the pre-#897 writer, so the existing archive blobs are
/// fold-stable.
fn ustar_archive(members: &[(String, Vec<u8>)]) -> Result<Vec<u8>, PipelineError> {
    let mut out: Vec<u8> = Vec::new();
    for (name, data) in members {
        if name.len() > 100 {
            let mut payload = name.as_bytes().to_vec();
            payload.push(0); // GNU LongLink bodies are NUL-terminated.
            out.extend_from_slice(&ustar_header(LONGLINK_NAME, payload.len(), b'L')?);
            out.extend_from_slice(&payload);
            let pad = (512 - payload.len() % 512) % 512;
            out.extend(std::iter::repeat_n(0u8, pad));
        }
        out.extend_from_slice(&ustar_header(name, data.len(), b'0')?);
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024)); // two trailing zero blocks
    Ok(out)
}

/// A single USTAR 512-byte header with the given `typeflag` (`b'0'` regular file,
/// `b'L'` GNU LongLink). A name longer than 100 bytes is truncated into the field
/// — the caller MUST have emitted a preceding `LongLink` record carrying the full
/// path (see [`ustar_archive`]). For a name ≤ 100 bytes the bytes are identical to
/// the pre-#897 single-typeflag header.
fn ustar_header(name: &str, size: usize, typeflag: u8) -> Result<[u8; 512], PipelineError> {
    let nb = name.as_bytes();
    let n = nb.len().min(100);
    let mut h = [0u8; 512];
    h[..n].copy_from_slice(&nb[..n]);
    write_octal(&mut h[100..108], 0o644); // mode
    write_octal(&mut h[108..116], 0); // uid
    write_octal(&mut h[116..124], 0); // gid
    write_octal(&mut h[124..136], size as u64); // size
    write_octal(&mut h[136..148], 0); // mtime
    for b in &mut h[148..156] {
        *b = b' '; // checksum field is spaces while the sum is computed
    }
    h[156] = typeflag;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    // 6 octal digits, then NUL + space (the canonical checksum encoding).
    let chk = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(chk.as_bytes());
    Ok(h)
}

/// Write `value` as right-justified, zero-padded octal into `field`, NUL-terminated.
fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let s = format!("{value:0width$o}");
    field[..width].copy_from_slice(&s.as_bytes()[..width]);
    field[width] = 0;
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
                "stage-docs-render".to_string(),
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
    fn kind(&self) -> StageKind {
        StageKind::Transform
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
        "snapshot.v5-fresh-schemas-blob"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
        // The embedded ontology-docs site (`build_docs_archive`) is rendered from
        // the docs model's raw sources (slice modules / `docs.md` / examples /
        // `docs/four-boxes.md` / per-slice `i18n/<lang>.po` translation catalogs),
        // which the consumed upstream products do not fully reflect. Declare them so
        // a doc-source edit busts this stage and re-renders the embedded site (cache
        // soundness, #897) — shared with `DocsRenderStage` via `docs_source_files`.
        crate::stages::docs_render::docs_source_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // THIS run's freshly-emitted JSON Schema + OpenAPI bytes, taken from the
        // `stage-export-json-schema` product rather than the committed on-disk files
        // (which are not written until phase 1 returns). Missing artifacts HARD-fail
        // (no-optionality, fail-closed) — the consumes edge guarantees they exist.
        let schema_json = input
            .upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
            .ok_or_else(|| {
                stage_err("missing stage-export-json-schema gmeow.schema.json artifact")
            })?
            .to_vec();
        let openapi_json = input
            .upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::OPENAPI_PATH))
            .ok_or_else(|| {
                stage_err("missing stage-export-json-schema gmeow.openapi.json artifact")
            })?
            .to_vec();
        let mut blobs = build_archive_blobs(input.root, &schema_json, &openapi_json)?;
        blobs.extend(build_guide_blobs(input.root)?);
        blobs.push(build_docs_archive(input.root)?);
        let gts = build_snapshot(input.root, input.upstream, blobs)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(SNAPSHOT_PATH.to_string(), gts);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

// ── default graph (authored ontology, NO imports) ───────────────────────────────

/// The localizable predicates (`i18n_catalog.LOCALIZABLE_PREDICATES`): the
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
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    // Per-file blank-node scoping: oxigraph reuses blank labels (`_:b0`, …) across
    // separate parse calls into ONE store, which COLLAPSES structurally-distinct
    // blank-node axioms (e.g. two `owl:AllDisjointClasses` lists) that the Python
    // build keeps distinct (rdflib skolemizes per-parse). Renaming each file's
    // blanks with a file-unique prefix before union preserves that distinctness.
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
    let mut scope = 0usize;
    ingest_turtle_scoped(&store, &std::fs::read(&onto)?, scope)?;
    scope += 1;
    for module in crate::stages::source_load::module_files(root)? {
        ingest_turtle_scoped(&store, &std::fs::read(&module)?, scope)?;
        scope += 1;
    }
    merge_translations(root, &store)?;
    add_guide_blobs(root, &store)?;
    store_to_nquads(&store)
}

/// Add the per-slice `gmeow:guideBlob` triple `_doc_blobs` injects into the
/// default graph: for every slice carrying a `docs.md`, link the slice IRI to the
/// `blake3:<hex>` content digest of that guide. The guide itself rides the bundle
/// as a content-addressed blob; this triple is its in-graph anchor.
fn add_guide_blobs(root: &Path, store: &Store) -> Result<(), PipelineError> {
    use oxigraph::model::{Literal, NamedNode, Quad};

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
        let quad = Quad::new(
            subject,
            guide_blob.clone(),
            Literal::new_simple_literal(digest),
            oxigraph::model::GraphName::DefaultGraph,
        );
        store
            .insert(&quad)
            .map_err(|e| stage_err(&format!("guideBlob insert: {e}")))?;
    }
    Ok(())
}

/// Merge the slice `.po` translations into `store`, mirroring `merge_terms`: for
/// every base-graph localizable literal `(iri, predicate)`, add a translated
/// literal `(iri, predicate, "msgstr"@<internal-tag>)` for each language that
/// translates it. The translation index + the BCP-47 → `x-gmeow-*` tag map come
/// from the native `gmeow_docs::Translations` (the same catalog the docs render).
fn merge_translations(root: &Path, store: &Store) -> Result<(), PipelineError> {
    use oxigraph::model::{Literal, NamedNode, Quad, Term};

    let catalog = gmeow_slice::SliceCatalog::discover(&root.join("slices"))
        .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);
    let langs: Vec<String> = translations.languages().to_vec();
    if langs.is_empty() {
        return Ok(());
    }

    // The base-graph localizable literals: `(subject_iri, predicate_iri)` whose
    // object is a literal (the allowed-keys set of merge_terms).
    let mut additions: Vec<Quad> = Vec::new();
    for pred in LOCALIZABLE_PREDICATES {
        let predicate = NamedNode::new(*pred).map_err(|e| stage_err(&format!("predicate: {e}")))?;
        for quad in store.quads_for_pattern(None, Some((&predicate).into()), None, None) {
            let quad = quad.map_err(|e| stage_err(&format!("scan: {e}")))?;
            let oxigraph::model::NamedOrBlankNode::NamedNode(subject) = &quad.subject else {
                continue;
            };
            if !matches!(&quad.object, Term::Literal(_)) {
                continue;
            }
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
    }
    for quad in additions {
        store
            .insert(&quad)
            .map_err(|e| stage_err(&format!("translation insert: {e}")))?;
    }
    Ok(())
}

// ── imports (graph/imports) ─────────────────────────────────────────────────────

fn load_imports(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let dir = root.join("imports");
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    for path in files {
        ingest_turtle(&store, &std::fs::read(&path)?)?;
    }
    store_to_nquads(&store)
}

// ── metadata (graph/metadata) ───────────────────────────────────────────────────

fn load_metadata(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let path = root.join("metadata").join("gmeow-self.ttl");
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    ingest_turtle(&store, &std::fs::read(&path)?)?;
    store_to_nquads(&store)
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

    // The emitter returns a Turtle body; normalize through a store to N-Quads so
    // the builder ingests it the same way as every other named-graph source.
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    ingest_turtle(&store, graph.turtle_body.as_bytes())?;
    store_to_nquads(&store)
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
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    ingest_nq(&store, authored_nq)?;
    let onto = oxigraph::model::NamedNode::new(GMEOW_NS.trim_end_matches('/'))
        .map_err(|e| stage_err(&format!("ontology IRI: {e}")))?;
    let version_info =
        oxigraph::model::NamedNode::new("http://www.w3.org/2002/07/owl#versionInfo").unwrap();
    for quad in store.quads_for_pattern(
        Some((&onto).into()),
        Some((&version_info).into()),
        None,
        None,
    ) {
        let quad = quad.map_err(|e| stage_err(&format!("version lookup: {e}")))?;
        if let oxigraph::model::Term::Literal(l) = &quad.object {
            return Ok(l.value().to_string());
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

    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    for path in files {
        let text = std::fs::read_to_string(&path)?;
        for (s, p, o) in alignment_rows(&text)? {
            let subject = oxigraph::model::NamedNode::new(&s)
                .map_err(|e| stage_err(&format!("alignment subject {s}: {e}")))?;
            let predicate = oxigraph::model::NamedNode::new(&p)
                .map_err(|e| stage_err(&format!("alignment predicate {p}: {e}")))?;
            let object = oxigraph::model::NamedNode::new(&o)
                .map_err(|e| stage_err(&format!("alignment object {o}: {e}")))?;
            let quad = Quad::new(
                subject,
                predicate,
                object,
                oxigraph::model::GraphName::DefaultGraph,
            );
            store
                .insert(&quad)
                .map_err(|e| stage_err(&format!("alignment insert: {e}")))?;
        }
    }
    store_to_nquads(&store)
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
fn run_verify_attestation(root: &Path, edb: &OxigraphStore<'_>) -> Result<Vec<u8>, PipelineError> {
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
    let store = Store::new().map_err(|e| stage_err(&format!("store: {e}")))?;
    ingest_turtle(&store, attestation.as_bytes())?;
    store_to_nquads(&store)
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
    writeln!(
        body,
        "<{GMEOW_NS}activity/native-verify> a <{GMEOW_NS}Activity> ;"
    )
    .unwrap();
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
    let mut dataset: Dataset = quads.iter().map(|q| q.as_ref().into_owned()).collect();
    dataset.canonicalize(CanonicalizationAlgorithm::Rdfc10 {
        hash_algorithm: oxigraph::model::dataset::CanonicalizationHashAlgorithm::Sha256,
    });
    // `QuadRef`'s Display renders `s p o g` WITHOUT the trailing N-Quads dot, so
    // append ` .` to each row to produce valid N-Quads the parser round-trips.
    let mut out: Vec<String> = dataset.iter().map(|q| format!("{q} .")).collect();
    out.sort_unstable();
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

fn parse_nq(bytes: &[u8]) -> Result<Vec<Quad>, PipelineError> {
    parse_rdf(bytes, RdfFormat::NQuads)
}

fn parse_rdf(bytes: &[u8], format: RdfFormat) -> Result<Vec<Quad>, PipelineError> {
    let mut quads = Vec::new();
    for quad in RdfParser::from_format(format).lenient().for_slice(bytes) {
        quads.push(quad.map_err(|e| stage_err(&format!("parse: {e}")))?);
    }
    Ok(quads)
}

fn ingest_turtle(store: &Store, bytes: &[u8]) -> Result<(), PipelineError> {
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_slice(bytes)
    {
        let quad = quad.map_err(|e| stage_err(&format!("turtle parse: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| stage_err(&format!("insert: {e}")))?;
    }
    Ok(())
}

/// Parse one Turtle file's bytes and insert it with every blank-node label
/// prefixed `f{scope}_…`, so blanks from distinct files never collide in the
/// shared store (the per-parse skolemization the Python build relies on).
fn ingest_turtle_scoped(store: &Store, bytes: &[u8], scope: usize) -> Result<(), PipelineError> {
    use oxigraph::model::{BlankNode, GraphName, NamedOrBlankNode, Quad, Term};

    let prefix = format!("f{scope}_");
    let rename_subject = |s: &NamedOrBlankNode| -> Result<NamedOrBlankNode, PipelineError> {
        Ok(match s {
            NamedOrBlankNode::BlankNode(b) => BlankNode::new(format!("{prefix}{}", b.as_str()))
                .map_err(|e| stage_err(&format!("blank rename: {e}")))?
                .into(),
            other => other.clone(),
        })
    };
    let rename_object = |o: &Term| -> Result<Term, PipelineError> {
        Ok(match o {
            Term::BlankNode(b) => Term::BlankNode(
                BlankNode::new(format!("{prefix}{}", b.as_str()))
                    .map_err(|e| stage_err(&format!("blank rename: {e}")))?,
            ),
            other => other.clone(),
        })
    };
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_slice(bytes)
    {
        let quad = quad.map_err(|e| stage_err(&format!("turtle parse: {e}")))?;
        let renamed = Quad::new(
            rename_subject(&quad.subject)?,
            quad.predicate.clone(),
            rename_object(&quad.object)?,
            GraphName::DefaultGraph,
        );
        store
            .insert(&renamed)
            .map_err(|e| stage_err(&format!("insert: {e}")))?;
    }
    Ok(())
}

fn ingest_nq(store: &Store, bytes: &[u8]) -> Result<(), PipelineError> {
    for quad in RdfParser::from_format(RdfFormat::NQuads)
        .lenient()
        .for_slice(bytes)
    {
        let quad = quad.map_err(|e| stage_err(&format!("n-quads parse: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| stage_err(&format!("insert: {e}")))?;
    }
    Ok(())
}

fn store_to_nquads(store: &Store) -> Result<Vec<u8>, PipelineError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut serializer = RdfSerializer::from_format(RdfFormat::NQuads).for_writer(&mut buf);
    for quad in store.iter() {
        let quad = quad.map_err(|e| stage_err(&e.to_string()))?;
        serializer
            .serialize_quad(&quad)
            .map_err(|e| stage_err(&format!("serialize: {e}")))?;
    }
    serializer
        .finish()
        .map_err(|e| stage_err(&format!("finish: {e}")))?;
    Ok(buf)
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
mod ustar_tests {
    use super::*;

    /// A minimal GNU/USTAR reader for the test: decodes `(name, data)` members,
    /// resolving `'L'` LongLink records into the following member's name. Mirrors
    /// how Python `tarfile` (the real `_unpack_doc_archive` consumer) reads them.
    fn parse(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        fn octal(field: &[u8]) -> usize {
            let s: String = field
                .iter()
                .take_while(|&&b| b != 0 && b != b' ')
                .map(|&b| b as char)
                .collect();
            usize::from_str_radix(s.trim(), 8).unwrap_or(0)
        }
        fn cstr(field: &[u8]) -> String {
            let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
            String::from_utf8_lossy(&field[..end]).into_owned()
        }
        let mut out = Vec::new();
        let mut pending: Option<String> = None;
        let mut off = 0usize;
        while off + 512 <= raw.len() {
            let header = &raw[off..off + 512];
            if header.iter().all(|&b| b == 0) {
                break; // trailing zero blocks
            }
            let size = octal(&header[124..136]);
            let typeflag = header[156];
            off += 512;
            let blocks = size.div_ceil(512);
            let body = &raw[off..off + blocks * 512];
            off += blocks * 512;
            if typeflag == b'L' {
                // LongLink body is the NUL-terminated path for the NEXT header.
                pending = Some(cstr(&body[..size]));
            } else {
                let name = pending.take().unwrap_or_else(|| cstr(&header[..100]));
                out.push((name, body[..size].to_vec()));
            }
        }
        out
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
        let raw = ustar_archive(&members).expect("archive");
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
        let raw = ustar_archive(&members).expect("archive");
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
        for asset in ["assets/gmeow.css", "search-index.json", "llms-docs.txt"] {
            let want = format!("x-gmeow-english/{asset}");
            assert!(
                members.iter().any(|(n, _)| n == &want),
                "expected site asset {want}"
            );
        }
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
    fn header_checksum_is_valid() {
        let h = ustar_header("x-gmeow-english/index.html", 42, b'0').expect("header");
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
        let mut probe = h;
        for b in &mut probe[148..156] {
            *b = b' ';
        }
        let computed: usize = probe.iter().map(|&b| b as usize).sum();
        assert_eq!(stored, computed);
    }
}
