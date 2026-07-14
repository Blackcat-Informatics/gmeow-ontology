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
//!    `skos:example`, `gmeow:useWhen`, `gmeow:avoidWhen`, `gmeow:howToUse`) via the SINGLE
//!    canonical library tiering [`gmeow_slice_brief::completeness_tiers`], shared with the
//!    `gmeow slice brief` CLI so an in-repo slice's CLI brief and its committed projection
//!    tier terms identically. It is a deterministic, source-only reading (no scoring infra,
//!    no bundle) that the library injects as the exemplar authority (dependency inversion —
//!    the library never picks a scoring authority);
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

use purrdf::RdfDataset;

use gmeow_slice_brief::{BriefInputs, assemble_packet, batch_count, defined_terms};
use gmeow_slice_quality::graph;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{GRAPH_AUTHORING_BRIEFS, parse_into_graph};

/// The number of same-slice exemplar coats each packet seeks.
const EXEMPLAR_TARGET: usize = 3;

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
            collect_ext(&slice_dir.join("mappings"), "ttl", &mut files)?;
            collect_ext(&slice_dir.join("i18n"), "po", &mut files)?;
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
            // The SINGLE canonical batch-enumeration arithmetic (`gmeow_slice_brief::batch_count`,
            // sharing `assemble::CHUNK` with `assemble_packet`'s own partition) — every batch
            // index `0..n_batches` this derives is in range for `assemble_packet`.
            let n_batches = batch_count(plan.term_count);
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
/// tests + mappings), find its defined terms (for the batch enumeration), and take each
/// term's exemplar tier from the SINGLE canonical library tiering
/// [`gmeow_slice_brief::completeness_tiers`] (annotation completeness). The defined-term
/// set and its ordering match `assemble`'s internal partition, so the batch count derived
/// here lines up with `assemble_packet`'s partition exactly.
///
/// # Errors
/// Hard-fails if the slice graph cannot be read, or the `manifest.ttl` declares no
/// `gmeow:Slice` (a malformed slice — never a silent skip; `assemble_packet` hard-fails
/// on the same condition).
fn slice_plan(slice_dir: &Path) -> Result<SlicePlan, gmeow_errors::Diag> {
    // Match `assemble_packet`'s dataset: slice graph PLUS the alignment linkage under
    // `mappings/` (`slice_ttl_paths` omits the latter).
    let mut paths = gmeow_slice_quality::report::slice_ttl_paths(slice_dir);
    collect_ext(&slice_dir.join("mappings"), "ttl", &mut paths)?;
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

    // Per-term exemplar tier = annotation completeness — computed by the SINGLE
    // canonical library tiering so this pipeline projection and the `gmeow slice
    // brief` CLI tier terms identically.
    let tiers = gmeow_slice_brief::completeness_tiers(slice_dir)?;

    Ok(SlicePlan {
        term_count: terms.len(),
        tiers,
    })
}

/// Recursively collect existing files with extension `ext` under `dir` into `out`.
/// A directory that does not exist is a legitimate "absent" input (`Ok`, nothing
/// collected); any OTHER `read_dir`/entry/file-type error (permission denied, I/O
/// error, not-a-directory, symlink loop, ...) is a HARD FAIL — propagated, never
/// laundered into a silent "no mappings / no translations" result.
///
/// # Errors
/// Propagates any `read_dir`/entry/`file_type` error other than
/// [`std::io::ErrorKind::NotFound`].
fn collect_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), gmeow_errors::Diag> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(stage_err(&format!(
                "{}: read_dir failed: {e}",
                dir.display()
            )));
        }
    };
    for entry in rd {
        let entry = entry.map_err(|e| {
            stage_err(&format!(
                "{}: directory entry read failed: {e}",
                dir.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|e| {
            stage_err(&format!(
                "{}: file_type failed: {e}",
                entry.path().display()
            ))
        })?;
        let p = entry.path();
        if file_type.is_dir() {
            collect_ext(&p, ext, out)?;
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
    Ok(())
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

    /// A missing directory is a legitimate "absent" input to [`collect_ext`]:
    /// `Ok(())`, nothing collected — never an error.
    #[test]
    fn collect_ext_absent_directory_is_ok_and_empty() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("does-not-exist");
        assert!(!dir.exists(), "precondition: {dir:?} must not exist");

        let mut out = Vec::new();
        let result = collect_ext(&dir, "ttl", &mut out);

        assert!(
            result.is_ok(),
            "a NotFound read_dir must be treated as absent (Ok), got {result:?}"
        );
        assert!(
            out.is_empty(),
            "an absent directory must collect zero paths, got {out:?}"
        );
    }

    /// A `read_dir` failure that is NOT `NotFound` (here: the parent path
    /// component is a plain file, so the OS refuses with `NotADirectory`/`ENOTDIR`)
    /// MUST propagate as an `Err` from [`collect_ext`], never be laundered into
    /// "no mappings / no translations here". Deterministic — does not depend on
    /// running as non-root (unlike a permission-bits test, which root would bypass).
    #[test]
    fn collect_ext_non_directory_parent_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let marker_file = temp.path().join("marker");
        std::fs::write(&marker_file, b"not a directory").expect("write marker file");

        // `marker_file` is a plain file, so `marker_file/mappings` cannot be a
        // directory: `read_dir` must fail with something other than `NotFound`.
        let bogus_dir = marker_file.join("mappings");
        let mut out = Vec::new();
        let result = collect_ext(&bogus_dir, "ttl", &mut out);

        assert!(
            result.is_err(),
            "a non-NotFound read_dir error must propagate as Err, got {result:?}"
        );
        assert!(
            out.is_empty(),
            "no paths must be collected on the error path, got {out:?}"
        );
    }
}
