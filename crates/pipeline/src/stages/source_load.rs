// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `source_load` stage (#861 P3): parse the authored ontology sources into
//! one in-memory base graph.
//!
//! This is the root of the build DAG. It loads `ontology/gmeow.ttl`, every
//! `slices/<group>/<name>/module.ttl`, and every `imports/*.ttl` into a single
//! oxigraph store — the RDF 1.1 base graph the Python build assembled via
//! `load_merged_graph(include_imports=…)`. The store is published as an in-memory
//! N-Quads artifact so downstream stages parse it from memory instead of
//! re-reading `gmeow.gts` from disk per generator (the bottleneck #861 removes).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::collections::HashMap;

use gmeow_rdf::oxigraph::{
    flat_oxigraph_quads_from_dataset, flat_oxigraph_quads_from_dataset_scoped, GraphPolicy,
};
use gmeow_rdf::provenance::{DatasetProvenance, OriginKind};
use gmeow_rdf::{parse_dataset, serialize_dataset, QuadHandle, SerializeGraph};
use oxigraph::model::Quad;
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct, SOURCE_ORIGIN};

/// The `OriginKind` an authored file contributes, by its repo-relative role:
/// `ontology/gmeow.ttl` is the [`OriginKind::RootOntology`], every `imports/*.ttl`
/// is an [`OriginKind::Import`], and every slice `module.ttl` is an
/// [`OriginKind::Source`]. The classification is a pure function of the path, so the
/// provenance attribution is reproducible (no-optionality — every authored file maps
/// to a concrete kind, never an unknown).
fn authored_origin_kind(root: &Path, path: &Path) -> OriginKind {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    if rel_str == "ontology/gmeow.ttl" {
        OriginKind::RootOntology
    } else if rel_str.starts_with("imports/") {
        OriginKind::Import
    } else {
        OriginKind::Source
    }
}

/// Build the per-quad provenance sidecar for the authored base graph (#1132 C9).
///
/// Every authored file (`ontology/gmeow.ttl`, every slice `module.ttl`, every
/// `imports/*.ttl`) is registered as one compilation [`unit`](DatasetProvenance::register_unit)
/// — by its repo-relative path, with the path-derived [`OriginKind`] — and one
/// [`artifact`](DatasetProvenance::register_artifact) under that same path. Each quad the
/// file contributes is recorded as one [`AssertionOccurrence`](gmeow_rdf::provenance::AssertionOccurrence)
/// keyed by a content-deduplicated [`QuadHandle`]: two files asserting the same triple
/// collapse to ONE handle but TWO occurrences (the set-valued S0.3 invariant). Blank-node
/// labels are standardized per file (the same FNV scope the load store uses), so a
/// structurally-distinct blank axiom in two files keeps two handles.
///
/// Returns `(provenance, expected_handles)` where `expected_handles` is every distinct
/// handle minted — the coverage set [`check_provenance`](gmeow_rdf::provenance::check_provenance)
/// asserts is fully attributed. An UNATTRIBUTED authored quad is impossible by
/// construction (every quad is recorded as it is seen); the gate is the hard-fail proof.
pub fn attributed_base_provenance(
    root: &Path,
) -> Result<(DatasetProvenance, Vec<QuadHandle>), PipelineError> {
    let mut prov = DatasetProvenance::new();
    // Content key (the standardized oxigraph quad) → its deduplicated handle. Two files
    // asserting an identical triple share the handle but record distinct occurrences.
    let mut handle_of: HashMap<Quad, QuadHandle> = HashMap::new();
    let mut next: u32 = 0;

    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        let scope = path.display().to_string();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let kind = authored_origin_kind(root, &path);
        let unit = prov.register_unit(rel.clone(), kind);
        let artifact = prov.register_artifact(rel);

        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| PipelineError::Parse(format!("syntax error in {scope}: {e}")))?;
        let quads = flat_oxigraph_quads_from_dataset_scoped(&dataset, &scope)
            .map_err(|e| PipelineError::Parse(format!("IR → quads in {scope}: {e}")))?;
        for quad in quads {
            let handle = *handle_of.entry(quad).or_insert_with(|| {
                let h = QuadHandle::from_index(next);
                next += 1;
                h
            });
            prov.record_occurrence(handle, unit, artifact, None);
        }
    }

    let mut expected: Vec<QuadHandle> = handle_of.into_values().collect();
    expected.sort_unstable_by_key(|h| h.index());
    Ok((prov, expected))
}

/// Logical path of the published base graph (N-Quads, in-memory dataflow).
pub const BASE_GRAPH_PATH: &str = "pipeline/base-graph.nq";

/// Load `ontology/gmeow.ttl` + all slice modules + all imports into one store.
pub fn load_authored_store(root: &Path) -> Result<Store, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        // SCOPE by the source path: each authored file is a distinct RDF document, so
        // its anonymous blanks (`_:gts_<counter>`, restarting per parse) must not merge
        // with another file's identically-labelled blanks when they share this store.
        let scope = path.display().to_string();
        rdf_bytes_into_store_scoped(&store, &bytes, "text/turtle", &scope, &scope)?;
    }
    Ok(store)
}

/// Like [`rdf_bytes_into_store`] but scopes blank node labels by `scope_key` (a stable
/// per-source identity) so documents accumulated into one store keep disjoint blanks.
pub fn rdf_bytes_into_store_scoped(
    store: &Store,
    bytes: &[u8],
    media_type: &str,
    scope_key: &str,
    context: &str,
) -> Result<(), PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    for quad in flat_oxigraph_quads_from_dataset_scoped(&dataset, scope_key)
        .map_err(|e| PipelineError::Parse(format!("IR → quads in {context}: {e}")))?
    {
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// The sorted authored Turtle files that form the base graph (the hidden-input
/// closure `source_load` declares so the cache key cannot go stale).
pub fn authored_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut files: Vec<PathBuf> = Vec::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.exists() {
        files.push(onto);
    }
    files.extend(module_files(root)?);
    files.extend(ttl_files_in(&root.join("imports"))?);
    files.sort();
    Ok(files)
}

/// Every `slices/<group>/<name>/module.ttl`.
pub fn module_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    if !slices.is_dir() {
        return Ok(out);
    }
    for group in sorted_dirs(&slices)? {
        for slice_dir in sorted_dirs(&group)? {
            let module = slice_dir.join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    Ok(out)
}

/// Every `slices/<group>/<name>/manifest.ttl` (the sibling of each `module.ttl`),
/// for export leaves whose cache key must reflect the slice manifests they read
/// directly from disk (catalog, profiles, matrix — `gmeow:sliceProfile` /
/// `sliceTier` / `sliceDependsOn` live in the manifest, NOT the composed fold).
pub fn manifest_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    for module in module_files(root)? {
        let manifest = module.with_file_name("manifest.ttl");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
    Ok(out)
}

fn ttl_files_in(dir: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    // Fail-fast on a read_dir entry error: a transient FS error must surface, not
    // silently drop a slice group/dir (no-optionality, #863).
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Serialize a store to canonical N-Quads bytes (sorted lines) for in-memory
/// passing between stages.
pub fn store_to_nquads(store: &Store) -> Result<Vec<u8>, PipelineError> {
    let dataset = gmeow_rdf::oxigraph::dataset_from_store(store)
        .map_err(|e| PipelineError::Parse(e.to_string()))?;
    dataset_to_sorted_nquads(&dataset)
}

/// Serialize a frozen [`gmeow_rdf::RdfDataset`] to the SAME deterministic N-Quads byte form
/// [`store_to_nquads`] produces (full RDF 1.2 statement layer, lines sorted
/// bytewise ascending, trailing newline). This is the single dataset → N-Quads
/// projection the pipeline's in-memory dataflow speaks; the `gts_compose` stage
/// projects the composed UNION dataset through it for the `composed.nq` byte lane.
pub fn dataset_to_sorted_nquads(dataset: &gmeow_rdf::RdfDataset) -> Result<Vec<u8>, PipelineError> {
    let buf = serialize_dataset(dataset, "application/n-quads", SerializeGraph::Dataset)
        .map_err(|e| PipelineError::Parse(format!("serialize failed: {e}")))?;
    // Sort lines for determinism (oxigraph iteration order is not guaranteed).
    let text = String::from_utf8(buf)
        .map_err(|e| PipelineError::Parse(format!("non-utf8 n-quads: {e}")))?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines.sort_unstable();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

/// Parse the published base-graph N-Quads artifact back into a store (the
/// in-memory hand-off downstream stages use instead of re-reading from disk).
pub fn parse_base_graph(bytes: &[u8]) -> Result<Store, PipelineError> {
    let dataset = parse_dataset(bytes, "application/n-quads", None)
        .map_err(|e| PipelineError::Parse(format!("base-graph parse: {e}")))?;
    gmeow_rdf::oxigraph::store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| PipelineError::Parse(format!("base-graph store: {e}")))
}

/// Parse RDF text `bytes` of `media_type` into a fresh oxigraph store via the
/// native codecs, preserving named graphs. `context` labels parse errors.
pub fn rdf_bytes_to_store(
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<Store, PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    gmeow_rdf::oxigraph::store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| PipelineError::Parse(format!("store build for {context}: {e}")))
}

/// Parse RDF text `bytes` of `media_type` and insert the resulting quads into an
/// existing `store`, accumulating across calls. `context` labels parse errors.
pub fn rdf_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<(), PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    for quad in flat_oxigraph_quads_from_dataset(&dataset)
        .map_err(|e| PipelineError::Parse(format!("IR → quads in {context}: {e}")))?
    {
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// Parse Turtle `bytes` into a fresh oxigraph store via the native codecs.
///
/// The native `parse_dataset` folds the RDF 1.2 statement layer, and
/// `store_from_dataset` materialises the result with named graphs preserved (a
/// stand-alone Turtle document only ever populates the default graph, so the
/// policy is equivalent to flattening here). `context` labels parse errors.
pub fn turtle_bytes_to_store(bytes: &[u8], context: &str) -> Result<Store, PipelineError> {
    rdf_bytes_to_store(bytes, "text/turtle", context)
}

/// Parse Turtle `bytes` and insert the resulting quads into an existing `store`,
/// accumulating across calls. `context` labels parse errors.
pub fn turtle_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    context: &str,
) -> Result<(), PipelineError> {
    rdf_bytes_into_store(store, bytes, "text/turtle", context)
}

/// Parse Turtle `bytes` from a distinct source document (`scope_key` = its stable
/// identity, e.g. the file path) and insert into `store`, scoping blank-node labels
/// so several source files accumulated into one store keep disjoint anonymous blanks.
pub fn turtle_bytes_into_store_scoped(
    store: &Store,
    bytes: &[u8],
    scope_key: &str,
) -> Result<(), PipelineError> {
    rdf_bytes_into_store_scoped(store, bytes, "text/turtle", scope_key, scope_key)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `source_load` pipeline stage — the authored-source loader. Holds
/// [`SOURCE_ORIGIN`], so the scheduler stamps its emitted quads' provenance origin as
/// `Source` (the kind-enum replacement: origin is read off a capability, not a tag).
pub struct SourceLoadStage {
    capabilities: Vec<String>,
}

impl SourceLoadStage {
    /// Construct the loader, declaring the [`SOURCE_ORIGIN`] capability (mirrored by
    /// the slice `gmeow:stage-source-load gmeow:hasCapability gmeow:sourceOrigin`).
    pub fn new() -> Self {
        Self {
            capabilities: vec![SOURCE_ORIGIN.to_string()],
        }
    }
}

impl Default for SourceLoadStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SourceLoadStage {
    fn id(&self) -> &str {
        "stage-source-load"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn impl_version(&self) -> &str {
        "source_load.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let store = load_authored_store(input.root)?;
        // Carry the authored base graph as the bundle's frozen dataset (the native
        // contribution `gts_compose` unions), and keep emitting the BASE_GRAPH_PATH
        // N-Quads byte lane for the pre-C3 byte readers. Both come from the SAME
        // store via `dataset_from_store` — no extra serialize→parse round-trip.
        let dataset = gmeow_rdf::oxigraph::dataset_from_store(&store)
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
        let nq = dataset_to_sorted_nquads(&dataset)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), nq);
        Ok(StageOutput {
            product: StageProduct::from_artifacts_over(self.id(), dataset, artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn source_load_parses_the_whole_ontology() {
        let root = repo_root();
        let store = load_authored_store(&root).expect("load");
        // The merged authored graph is substantial (50+ slices); sanity-floor it.
        assert!(
            store.len().unwrap() > 5_000,
            "authored base graph unexpectedly small: {} quads",
            store.len().unwrap()
        );
        // Round-trips through the in-memory N-Quads hand-off.
        let nq = store_to_nquads(&store).expect("serialize");
        let back = parse_base_graph(&nq).expect("reparse");
        assert_eq!(store.len().unwrap(), back.len().unwrap());
    }

    #[test]
    fn authored_files_includes_root_and_modules() {
        let root = repo_root();
        let files = authored_files(&root).unwrap();
        assert!(files.iter().any(|p| p.ends_with("ontology/gmeow.ttl")));
        assert!(files
            .iter()
            .any(|p| p.ends_with("slices/core/pipeline/module.ttl")));
        assert!(
            files.len() > 50,
            "expected 50+ authored files, got {}",
            files.len()
        );
    }
}
