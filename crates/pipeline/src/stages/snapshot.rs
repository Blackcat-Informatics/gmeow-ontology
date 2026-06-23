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
use std::path::Path;

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
    builder.add_quads(&parse_nq(authored_canon.as_bytes())?, None, Some("base"));
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
        "snapshot.v1-structured"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let gts = build_snapshot(input.root, input.upstream, Vec::new())?;
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
    let mut scope = 0usize;
    if onto.exists() {
        ingest_turtle_scoped(&store, &std::fs::read(&onto)?, scope)?;
        scope += 1;
    }
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
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ttl"))
        .collect();
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
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".sssom.tsv"))
        .collect();
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
    let mut core_files: Vec<std::path::PathBuf> = if core.is_dir() {
        std::fs::read_dir(&core)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "rq"))
            .collect()
    } else {
        Vec::new()
    };
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
    builder.add_quads(&parse_nq(canon.as_bytes())?, Some(graph_name), Some(scope));
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
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
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
