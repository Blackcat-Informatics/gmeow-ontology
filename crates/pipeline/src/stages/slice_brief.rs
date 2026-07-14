// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `slice_brief` stage: assemble a `gmeow:AuthoringPacket` for every in-repo slice
//! (all batches) and fold the union into the carrier as the `graph/authoring-briefs`
//! corpus — the shippable authoring deliverable (Design A: the packet corpus ships in
//! `gmeow.gts`, not only behind a test-side gate).
//!
//! This is a SOURCE-reading leaf (like `math_producers`): it consumes no upstream
//! product. For each slice discovered under `slices/` (the SINGLE discovery authority
//! [`gmeow_slice_quality::discover_slice_dirs`], shared with the quality sweep) it:
//!
//! 1. computes a per-term **exemplar tier** = ANNOTATION COMPLETENESS — the count of the
//!    six authoring coat predicates present on the term (`rdfs:label`, `skos:definition`,
//!    `skos:example`, `gmeow:useWhen`, `gmeow:avoidWhen`, `gmeow:howToUse`). This is a
//!    deterministic, source-only reading (no scoring infra, no bundle) that the library
//!    injects as the exemplar authority (dependency inversion — the library never picks a
//!    scoring authority);
//! 2. partitions the slice's sorted defined terms into `~25`-term batches and calls
//!    [`gmeow_slice_brief::assemble_packet`] for each batch `0..N`; and
//! 3. parses every packet's canonical turtle and UNIONs them all into ONE dataset rooted
//!    at [`crate::stages::carrier::GRAPH_AUTHORING_BRIEFS`].
//!
//! The snapshot presenter reads this base graph back via `producer_graph` and folds it
//! into `gmeow.gts`; a fanout twin re-roots the SAME triples into their
//! `graph/fanout/<path>` reconstruction graph so the superset gate folds them to
//! `generated/briefs/authoring-packets.nt` (RDF travels as RDF — PIPELINE_SPINE §5).
//!
//! Determinism: slices iterate sorted, batches ascend, and the library's assembly is
//! byte-stable, so the unioned carrier dataset is byte-deterministic. A slice with zero
//! authorable terms contributes zero packets (skipped, never an error — only an explicit
//! out-of-range batch on the CLI path is an error, and this stage only passes valid batch
//! indices). A read / parse / assembly failure is a HARD FAIL — propagated, never swallowed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};

use gmeow_slice_brief::{BriefInputs, assemble_packet};
use gmeow_slice_quality::graph;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{GRAPH_AUTHORING_BRIEFS, parse_into_graph};

/// The interim partition chunk size — MUST match the library's private `assemble::CHUNK`
/// so this stage's batch enumeration lines up with `assemble_packet`'s partition exactly
/// (batch `n` covers `n*CHUNK .. (n+1)*CHUNK`).
const CHUNK: usize = 25;

/// The number of same-slice exemplar coats each packet seeks.
const EXEMPLAR_TARGET: usize = 3;

/// The `rdfs:isDefinedBy` IRI (the slice-membership predicate).
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
/// The six authoring coat predicates whose presence count is the per-term exemplar tier
/// (annotation completeness). Fully-qualified so the reading is source-only and stable.
const COAT_PREDICATES: [&str; 6] = [
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/2004/02/skos/core#example",
    "https://blackcatinformatics.ca/gmeow/useWhen",
    "https://blackcatinformatics.ca/gmeow/avoidWhen",
    "https://blackcatinformatics.ca/gmeow/howToUse",
];

/// The `slice_brief` pipeline stage — a leaf compute node. It consumes no upstream
/// product (it reads the authored slice sources directly) and attaches the single
/// `graph/authoring-briefs` corpus to its carrier dataset.
pub struct SliceBriefStage {
    consumes: Vec<String>,
}

impl SliceBriefStage {
    /// Construct the stage. It reads nothing upstream — the packets are assembled from
    /// the authored slice sources on disk at `run()`.
    pub fn new() -> Self {
        Self {
            consumes: Vec::new(),
        }
    }
}

impl Default for SliceBriefStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SliceBriefStage {
    fn id(&self) -> &str {
        "stage-slice-brief"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v1: assemble a gmeow:AuthoringPacket per in-repo slice batch and fold the union
        // into the carrier as graph/authoring-briefs (folded to
        // generated/briefs/authoring-packets.nt by stage-snapshot).
        "slice_brief.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The cache key must bust when ANY source a packet reads changes (a stale source
        // would ship a stale packet in gmeow.gts). `assemble_packet` reads, per slice: the
        // slice graph (module.ttl + examples/ + tests/, via `slice_ttl_paths`), the slice
        // identity (manifest.ttl), the alignment linkage (mappings/*.ttl), and the
        // translation catalogs (i18n/*.po). Declare every one of them.
        let slices_root = root.join("slices");
        let mut files: Vec<PathBuf> = Vec::new();
        for slice_dir in gmeow_slice_quality::discover_slice_dirs(&slices_root) {
            files.extend(gmeow_slice_quality::report::slice_ttl_paths(&slice_dir));
            let manifest = slice_dir.join("manifest.ttl");
            if manifest.is_file() {
                files.push(manifest);
            }
            collect_ext(&slice_dir.join("mappings"), "ttl", &mut files);
            collect_ext(&slice_dir.join("i18n"), "po", &mut files);
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let slices_root = input.root.join("slices");
        // Iterate slices in the SINGLE discovery authority's sorted order (dogfooding
        // coherence with the quality sweep) so the unioned corpus is byte-stable.
        let mut graphs: Vec<std::sync::Arc<RdfDataset>> = Vec::new();
        for slice_dir in gmeow_slice_quality::discover_slice_dirs(&slices_root) {
            let plan = slice_plan(&slice_dir)?;
            // A slice with zero authorable terms contributes zero packets (skip, no error).
            if plan.term_count == 0 {
                continue;
            }
            // batch n covers terms n*CHUNK .. (n+1)*CHUNK; n*CHUNK < term_count for every
            // n < n_batches, so every batch index passed to assemble_packet is in range.
            let n_batches = plan.term_count.div_ceil(CHUNK);
            for n in 0..n_batches {
                let packet = assemble_packet(&BriefInputs {
                    slice_dir: &slice_dir,
                    axis: None,
                    batch: Some(n as u32),
                    exemplar_tiers: &plan.tiers,
                    exemplar_target: EXEMPLAR_TARGET,
                })?;
                graphs.push(parse_into_graph(
                    packet.to_turtle().as_bytes(),
                    "text/turtle",
                    GRAPH_AUTHORING_BRIEFS,
                )?);
            }
        }
        let refs: Vec<&RdfDataset> = graphs.iter().map(|g| g.as_ref()).collect();
        let dataset = std::sync::Arc::new(RdfDataset::union(&refs));
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            BTreeMap::new(),
        )))
    }
}

/// The per-slice assembly plan: the count of authorable (defined) terms — from which the
/// batch enumeration derives — and the injected per-term exemplar tiers.
struct SlicePlan {
    term_count: usize,
    tiers: BTreeMap<String, i64>,
}

/// Load one slice's graph the SAME way [`assemble_packet`] does (module + examples +
/// tests + mappings), find its defined terms, and compute each term's exemplar tier =
/// the count of the six authoring coat predicates present on it (annotation completeness).
/// The defined-term set and its ordering match `assemble`'s internal partition, so the
/// batch count derived here lines up with `assemble_packet`'s partition exactly.
///
/// # Errors
/// Hard-fails if the slice graph cannot be read, or the `manifest.ttl` declares no
/// `gmeow:Slice` (a malformed slice — never a silent skip; `assemble_packet` hard-fails
/// on the same condition).
fn slice_plan(slice_dir: &Path) -> Result<SlicePlan, gmeow_errors::Diag> {
    // Match `assemble_packet`'s dataset: slice graph PLUS the alignment linkage under
    // `mappings/` (`slice_ttl_paths` omits the latter).
    let mut paths = gmeow_slice_quality::report::slice_ttl_paths(slice_dir);
    collect_ext(&slice_dir.join("mappings"), "ttl", &mut paths);
    paths.sort();
    paths.dedup();
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    let ds = gmeow_slice_quality::dataset_from_paths(&path_refs)?;

    // Slice identity comes from `manifest.ttl` (not part of the slice graph).
    let manifest = slice_dir.join("manifest.ttl");
    let mds = gmeow_slice_quality::dataset_from_paths(&[manifest.as_path()])?;
    let slice_iri = graph::instances_of(&mds, &graph::g("Slice"))
        .into_iter()
        .next()
        .ok_or_else(|| {
            stage_err(&format!(
                "{} declares no gmeow:Slice (a malformed slice — cannot brief it)",
                manifest.display()
            ))
        })?;

    let terms = defined_terms(&ds, &slice_iri);

    // Per-term exemplar tier = annotation completeness (source-only coat-predicate count).
    let pred_ids: Vec<Option<purrdf::TermId>> =
        COAT_PREDICATES.iter().map(|p| graph::id(&ds, p)).collect();
    let mut tiers: BTreeMap<String, i64> = BTreeMap::new();
    for term in &terms {
        let Some(tid) = graph::id(&ds, term) else {
            continue;
        };
        let mut count = 0i64;
        for pid in pred_ids.iter().flatten() {
            if graph::has_any(&ds, tid, *pid) {
                count += 1;
            }
        }
        tiers.insert(term.clone(), count);
    }

    Ok(SlicePlan {
        term_count: terms.len(),
        tiers,
    })
}

/// Every IRI subject the slice defines (`rdfs:isDefinedBy` the slice IRI, excluding the
/// slice individual itself), sorted ascending and deduped — mirrors the library's private
/// `assemble::defined_terms`, so this stage's batch count matches its partition.
fn defined_terms(ds: &RdfDataset, slice_iri: &str) -> Vec<String> {
    let (Some(pred), Some(slice_id)) =
        (graph::id(ds, RDFS_IS_DEFINED_BY), graph::id(ds, slice_iri))
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(pred), Some(slice_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) if iri != slice_iri => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Recursively collect existing files with extension `ext` under `dir` into `out`.
fn collect_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ext(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-slice-brief".to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stage attaches EXACTLY the authoring-briefs graph, carrying real
    /// `gmeow:AuthoringPacket` triples — the proof the packet corpus reaches the carrier
    /// (and thence `gmeow.gts`), not merely a test.
    #[test]
    fn run_attaches_the_authoring_briefs_graph() {
        let root = repo_root();
        let stage = SliceBriefStage::new();
        let upstream = BTreeMap::new();
        let out = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("slice_brief stage runs");
        let dataset = out.product.dataset();
        let projected = dataset.project_named_graph(GRAPH_AUTHORING_BRIEFS);
        assert!(
            projected.quad_count() > 0,
            "graph/authoring-briefs must carry the assembled packet corpus"
        );
        // The corpus must carry the packet type (proof the packets, not stray triples).
        let n = count_packets(&projected);
        assert!(
            n > 0,
            "graph/authoring-briefs must carry real gmeow:AuthoringPacket individuals, got {n}"
        );
    }

    /// Determinism: two runs attach byte-identical carrier datasets (slices iterate
    /// sorted, batches ascend, the library assembly is byte-stable).
    #[test]
    fn run_is_deterministic() {
        let root = repo_root();
        let stage = SliceBriefStage::new();
        let upstream = BTreeMap::new();
        let a = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("run a");
        let b = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("run b");
        assert_eq!(
            purrdf::canonical_flat_nquads(a.product.dataset()).expect("canon a"),
            purrdf::canonical_flat_nquads(b.product.dataset()).expect("canon b"),
            "the authoring-briefs carrier dataset must be deterministic"
        );
    }

    fn count_packets(ds: &RdfDataset) -> usize {
        graph::instances_of(ds, &graph::g("AuthoringPacket")).len()
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }
}
