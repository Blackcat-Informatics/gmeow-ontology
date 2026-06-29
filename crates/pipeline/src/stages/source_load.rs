// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `source_load` stage (#861 P3): parse the authored ontology sources into
//! one in-memory base graph.
//!
//! This is the root of the build DAG. It loads `ontology/gmeow.ttl`, every
//! `slices/<group>/<name>/module.ttl`, and every `imports/*.ttl` into a single
//! native [`RdfDataset`](gmeow_rdf::RdfDataset) — the RDF 1.1 base graph the Python
//! build assembled via `load_merged_graph(include_imports=…)`. The dataset is the
//! frozen carrier downstream stages union and project from, with the N-Quads byte
//! lane published alongside so the pre-carrier byte readers parse it from memory
//! instead of re-reading `gmeow.gts` from disk per generator (the bottleneck #861
//! removes). EPIC #906: oxigraph-free — every parse routes through the native
//! `gmeow_rdf::parse_dataset` codecs and merges via `RdfDataset::union`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::collections::HashMap;

use gmeow_rdf::provenance::{DatasetProvenance, OriginKind};
use gmeow_rdf::{
    flat_rdf_quads_from_dataset, parse_dataset, serialize_dataset, QuadHandle, RdfDataset, RdfQuad,
    RdfTerm, RdfTriple, SerializeGraph,
};

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

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
    // Content key (the per-file-scoped native quad, location stripped so two identical
    // triples on different source lines collapse exactly as the old oxigraph quad key
    // did) → its deduplicated handle. Two files asserting an identical triple share the
    // handle but record distinct occurrences (the set-valued S0.3 invariant).
    let mut handle_of: HashMap<RdfQuad, QuadHandle> = HashMap::new();
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
        // SCOPE blank labels by the source path: each authored file is a distinct RDF
        // document whose anonymous blanks restart per parse, so a structurally-distinct
        // blank axiom in two files must keep two handles. The native flat un-fold mirrors
        // the old `flat_oxigraph_quads_from_dataset_scoped` exactly (same FNV prefix), and
        // the location is stripped so the dedup key is the pure `(s, p, o, g)` content.
        let prefix = blank_scope_prefix(&scope);
        let quads = flat_rdf_quads_from_dataset(&dataset);
        for quad in quads {
            let key = rescope_quad_blanks_keyless(&quad, &prefix);
            let handle = *handle_of.entry(key).or_insert_with(|| {
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

/// A stable (FNV-1a) blank-node label prefix for a source document — the native twin
/// of `gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset_scoped`'s scoping, kept
/// byte-identical so the per-file provenance handle partition (and thus the committed
/// `graph/provenance` projection) is preserved across the oxigraph removal (#906).
/// Deterministic across processes and stages — the same `scope_key` always yields the
/// same prefix.
fn blank_scope_prefix(scope_key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in scope_key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("g{hash:016x}")
}

/// Rescope every blank-node label in `term` with `prefix`, recursing into quoted triples.
fn rescope_term_blanks(term: &RdfTerm, prefix: &str) -> RdfTerm {
    match term {
        RdfTerm::BlankNode(label) => RdfTerm::blank_node(format!("{prefix}{label}")),
        RdfTerm::Triple(triple) => RdfTerm::triple(RdfTriple::new(
            rescope_term_blanks(&triple.subject, prefix),
            triple.predicate.clone(),
            rescope_term_blanks(&triple.object, prefix),
        )),
        other => other.clone(),
    }
}

/// Build the location-free, blank-scoped dedup key for one native quad. The location is
/// dropped (the old oxigraph `Quad` key carried none) so two identical triples collapse to
/// ONE handle, and blank labels are prefixed by the per-source scope so distinct blanks
/// across files stay distinct.
fn rescope_quad_blanks_keyless(quad: &RdfQuad, prefix: &str) -> RdfQuad {
    let mut key = RdfQuad::new(
        rescope_term_blanks(&quad.subject, prefix),
        quad.predicate.clone(),
        rescope_term_blanks(&quad.object, prefix),
    );
    key.graph_name = quad
        .graph_name
        .as_ref()
        .map(|g| rescope_term_blanks(g, prefix));
    key
}

/// Logical path of the published base graph (N-Quads, in-memory dataflow).
pub const BASE_GRAPH_PATH: &str = "pipeline/base-graph.nq";

/// Load `ontology/gmeow.ttl` + all slice modules + all imports into one frozen dataset.
///
/// Each authored file is parsed standalone (its anonymous blanks `_:gts_<counter>`
/// restart at 0 per parse), and the per-file datasets are merged via
/// [`RdfDataset::union`], which standardizes blank scopes apart per input (the native
/// twin of the old per-source FNV blank-prefix ingest) so two files' identically-labelled
/// anonymous blanks stay disjoint. The union canonicalizes on freeze, so the result is
/// order-independent.
pub fn load_authored_dataset(root: &Path) -> Result<Arc<RdfDataset>, PipelineError> {
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        let scope = path.display().to_string();
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| PipelineError::Parse(format!("syntax error in {scope}: {e}")))?;
        parsed.push(dataset);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(|d| d.as_ref()).collect();
    Ok(Arc::new(RdfDataset::union(&refs)))
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

/// Serialize a frozen [`RdfDataset`] to the deterministic N-Quads byte form (full
/// RDF 1.2 statement layer, lines sorted bytewise ascending, trailing newline). This
/// is the single dataset → N-Quads projection the pipeline's in-memory dataflow speaks;
/// the `gts_compose` stage projects the composed UNION dataset through it for its byte
/// lane.
pub fn dataset_to_sorted_nquads(dataset: &gmeow_rdf::RdfDataset) -> Result<Vec<u8>, PipelineError> {
    let buf = serialize_dataset(dataset, "application/n-quads", SerializeGraph::Dataset)
        .map_err(|e| PipelineError::Parse(format!("serialize failed: {e}")))?;
    // Sort lines for determinism (serializer iteration order is not guaranteed).
    let text = String::from_utf8(buf)
        .map_err(|e| PipelineError::Parse(format!("non-utf8 n-quads: {e}")))?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines.sort_unstable();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

/// Parse the published base-graph N-Quads artifact back into a frozen dataset (the
/// in-memory hand-off downstream stages use instead of re-reading from disk).
pub fn parse_base_graph(bytes: &[u8]) -> Result<Arc<RdfDataset>, PipelineError> {
    parse_dataset(bytes, "application/n-quads", None)
        .map_err(|e| PipelineError::Parse(format!("base-graph parse: {e}")))
}

/// Parse RDF text `bytes` of `media_type` into a fresh frozen [`RdfDataset`] via the
/// native codecs, preserving named graphs. `context` labels parse errors.
pub fn rdf_bytes_to_dataset(
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<Arc<RdfDataset>, PipelineError> {
    parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))
}

/// Parse Turtle `bytes` into a fresh frozen [`RdfDataset`] via the native codecs.
///
/// The native `parse_dataset` folds the RDF 1.2 statement layer; a stand-alone Turtle
/// document only ever populates the default graph. `context` labels parse errors.
pub fn turtle_bytes_to_dataset(
    bytes: &[u8],
    context: &str,
) -> Result<Arc<RdfDataset>, PipelineError> {
    rdf_bytes_to_dataset(bytes, "text/turtle", context)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `source_load` pipeline stage.
pub struct SourceLoadStage;

impl Stage for SourceLoadStage {
    fn id(&self) -> &str {
        "stage-source-load"
    }
    fn kind(&self) -> StageKind {
        StageKind::SourceLoad
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "source_load.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // Carry the authored base graph as the bundle's frozen dataset (the native
        // contribution `gts_compose` unions), and keep emitting the BASE_GRAPH_PATH
        // N-Quads byte lane for the pre-C3 byte readers. Both come from the SAME
        // native dataset — no oxigraph store, no extra serialize→parse round-trip.
        let dataset = load_authored_dataset(input.root)?;
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
        let dataset = load_authored_dataset(&root).expect("load");
        // The merged authored graph is substantial (50+ slices); sanity-floor it.
        assert!(
            dataset.quad_count() > 5_000,
            "authored base graph unexpectedly small: {} quads",
            dataset.quad_count()
        );
        // Round-trips through the in-memory N-Quads hand-off.
        let nq = dataset_to_sorted_nquads(&dataset).expect("serialize");
        let back = parse_base_graph(&nq).expect("reparse");
        assert_eq!(dataset.quad_count(), back.quad_count());
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
