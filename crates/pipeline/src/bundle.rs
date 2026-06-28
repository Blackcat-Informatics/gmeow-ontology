// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pipeline-side carrier types (#1132 C4): the [`PipelineHandle`] typed-handle
//! enum and the byte-artifact lane the [`StageProduct`](crate::node::StageProduct)
//! threads stage→stage as an [`Arc<PipelineBundle<PipelineHandle>>`].
//!
//! # The carrier
//!
//! C1 landed the generic [`PipelineBundle<H>`] in `gmeow-rdf-core`: a frozen RDF
//! dataset + lookaside + content-addressed blob store + provenance sidecar + a
//! typed-handle lane. This module plugs the pipeline's concrete handle payload
//! into that lane (`H = PipelineHandle`) and provides the byte-artifact bridge the
//! existing stages still speak.
//!
//! # The byte-artifact lane (this task is a CARRIER swap, not a serializer rewrite)
//!
//! Today every stage emits *named byte artifacts by logical path* and downstream
//! stages read them back by path. C4 swaps the CARRIER (a `BTreeMap<String,Vec<u8>>`
//! in `StageProduct`) for the structured bundle WITHOUT changing the bytes any
//! stage produces or reads — byte-identity of every committed artifact must hold
//! (`make check-generated`). To do that with zero behavioural change, the named
//! byte artifacts are stored INSIDE the bundle:
//!
//! * each artifact's bytes live in the bundle's [`ContentStore`] (the one owner of
//!   payload bytes, by-reference doctrine), and
//! * a [`RdfLookasideResource`] of kind [`RdfLookasideKind::Blob`] indexes it by
//!   `name = logical_path`, `content_digest = blob hex` — so `bundle_artifact(path)`
//!   reconstructs the exact bytes (`name → digest → blobs.get(digest)`).
//!
//! This makes C4 a pure carrier swap: the `(logical_path → bytes)` surface
//! `run_full` writes/compares is preserved bit-for-bit. C2/C3/C5 then progressively
//! replace these byte reads with dataset/lane reads and retire the blob lane per
//! stage. The lane is marked clearly so those tasks can find it.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::result::ReasoningResult;
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::relational_core::RelationalCoreProgram;
use gmeow_rdf::provenance::DatasetProvenance;
use gmeow_rdf::{
    ContentStore, GtsBundle, PipelineBundle, RdfDataset, RdfDatasetBuilder, RdfLookaside,
    RdfLookasideKind, RdfLookasideResource,
};

/// The pipeline-side typed-handle payload carried in the bundle's handle lane.
///
/// Each arm is a typed projection over a named graph the bundle carries; later
/// tasks (#1132 C7–C10) fill the remaining arms with their real payloads. For C4
/// the lane only had to EXIST; C6 lands the FIRST real typed handle:
/// [`Logic`](Self::Logic) now carries the compiled [`LogicProgram`] itself (the
/// content-addressed IR), pinned to its backing `graph/logic` canonical RDF-1.2
/// projection — a consumer takes the handle and NEVER re-parses the logic graph.
/// The remaining arms still wrap their backing graph as an [`Arc<RdfDataset>`]
/// placeholder so the variant is real and content-addressable.
///
/// `#[non_exhaustive]` so later tasks grow the payloads additively.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PipelineHandle {
    /// The logic layer: the compiled [`LogicProgram`] (the typed, content-addressed
    /// IR) over its backing `graph/logic` named graph — the REAL handle (#1132 C6),
    /// not the C4 placeholder. Its backing graph is the canonical RDF-1.2 projection
    /// of this program; the program's [`canonical_key`](LogicProgram::canonical_key)
    /// is its content identity.
    Logic(Arc<LogicProgram>),
    /// The reasoning layer: the typed [`ReasoningResult`] (the five-axis verdict +
    /// provenance bundle) over its backing `graph/reasoning` named graph — the REAL
    /// handle (#1132 C7), not the C4 placeholder. Its backing graph is the
    /// deterministic RDF projection of this result
    /// ([`project_reasoning_result`](gmeow_logic::result_rdf::project_reasoning_result));
    /// a consumer takes the typed handle and reads the verdict/provenance without
    /// re-running the reasoner. On a cache hit the cache re-derives the
    /// verdict-and-provenance result from the backing graph via
    /// [`parse_reasoning_graph`](gmeow_logic::result_rdf::parse_reasoning_graph)
    /// (the binding rows / closure quads live in the bundle's dataset, not re-copied
    /// here — see the projection's round-trip contract).
    Reasoning(Arc<ReasoningResult>),
    /// The relational-core layer: the typed [`RelationalCoreProgram`] (the engine-agnostic
    /// Datalog±-with-stratified-negation dialect lowered from the compiled program's Horn
    /// rules) over its backing `graph/relational-core` named graph — the REAL handle
    /// (#1132 C8), not the C4 placeholder. Its backing graph is the deterministic RDF
    /// projection of this dialect
    /// ([`project_relational_core`](gmeow_logic_compile::relational_core::project_relational_core));
    /// a consumer takes the typed handle and reads the lowered rules/facts/residue
    /// WITHOUT re-lowering. On a cache hit the cache re-derives the dialect from the
    /// backing graph via
    /// [`parse_relational_core`](gmeow_logic_compile::relational_core::parse_relational_core).
    /// When the full-FOL formula lowering lands, its richer non-Horn lowering plugs into
    /// THIS arm (it produces the same dialect with carried residue) — the carrier never
    /// changes shape.
    RelationalCore(Arc<RelationalCoreProgram>),
    /// The correspondence/alignment layer: the typed [`CorrespondenceProgram`] (the set
    /// of `logic:Correspondence` IR nodes + caveats + declared preservation polarity)
    /// over its backing `graph/correspondence` named graph — the REAL handle (#1132 C10),
    /// not the C4 placeholder. Its backing graph is the deterministic RDF projection of
    /// this program
    /// ([`project_correspondence`](gmeow_logic_compile::projections::correspondence::project_correspondence));
    /// a consumer takes the typed handle and reads the alignment surface (which keeps a
    /// caveated overlap at `skos:relatedMatch`, never `skos:exactMatch` /
    /// `owl:equivalentClass` — the overclaim gate forbids over-alignment) WITHOUT
    /// re-projecting. On a cache hit the cache re-derives the program from the backing
    /// graph via
    /// [`parse_correspondence`](gmeow_logic_compile::projections::correspondence::parse_correspondence).
    Correspondence(Arc<CorrespondenceProgram>),
}

/// The parse-once-and-share snapshot views (#1132 C5).
///
/// `stage-snapshot` emits the structured `gmeow.gts` bytes ONCE (`emit_gts`). Every
/// fold-reading export leaf needs ONE of two parsed views of those exact bytes:
///
/// * the [`GtsBundle`] event-import view (`import_gts_events`) — the value-interned
///   RDF 1.2 dataset the reasoning / lpg / metadata leaves fold over, and
/// * the [`gmeow_gts::model::Graph`] model view (`gts::read_graph`) — the term/quad
///   surface the export / parquet / okf / schemas / yaml-ld leaves render from.
///
/// Before C5 each leaf re-parsed the SAME `gmeow.gts` bytes independently (N-1
/// redundant parses). C5 makes `stage-snapshot` parse the EMITTED bytes ONCE into
/// both views and carry them here on its [`crate::node::StageProduct`], so the leaves
/// consume the shared in-memory view. Because the views are the parse of the exact
/// emitted bytes (done once), they are byte-identical to what each leaf parsed
/// before — the carrier swap changes WHEN the parse happens (once, in the snapshot),
/// not WHAT it yields.
///
/// The views ride on the `StageProduct`, NOT inside the content-addressed
/// `PipelineBundle` (they are derived, non-canonical projections — keeping them off
/// the bundle leaves the bundle digest a pure function of the GTS bytes). They are
/// therefore NOT reconstructed on a cache hit; a leaf that runs while its upstream
/// snapshot was served from cache falls back to parsing the lane bytes (the same
/// bytes, the same parse — still byte-identical). On the fresh-run path
/// (`make check-generated` wipes the cache) the snapshot always runs and the views
/// are always present, so the redundant parses are eliminated.
#[derive(Debug, Clone)]
pub struct SnapshotViews {
    /// The `import_gts_events` event-import view (RDF 1.2 dataset + envelope).
    pub events: Arc<GtsBundle>,
    /// The `gts::read_graph` model view (term/quad surface).
    pub graph: Arc<gmeow_gts::model::Graph>,
}

impl SnapshotViews {
    /// Pair the two parsed views of one snapshot's `gmeow.gts` bytes.
    pub fn new(events: Arc<GtsBundle>, graph: Arc<gmeow_gts::model::Graph>) -> Self {
        Self { events, graph }
    }
}

/// The lookaside-resource name prefix marking a byte-artifact lane entry. A bundle
/// resource whose `name` carries no special prefix IS the artifact's logical path;
/// the kind ([`RdfLookasideKind::Blob`]) disambiguates a byte artifact from any
/// future typed sidecar resources.
///
/// TEMPORARY (C4): this whole byte-artifact lane is scaffolding that C2/C3/C5 retire
/// per stage as they migrate to dataset/lane-native reads. Grep `byte-artifact lane`.
const ARTIFACT_KIND: RdfLookasideKind = RdfLookasideKind::Blob;

/// Build a `PipelineBundle<PipelineHandle>` carrying `artifacts` (logical path →
/// bytes) in its byte-artifact lane: each artifact's bytes go into the content
/// store and a [`RdfLookasideResource`] indexes it by path.
///
/// The dataset is empty and the provenance is whatever `provenance` supplies (the
/// scheduler threads the run's provenance in). This is the C4 carrier for the
/// existing named-artifact stages; it is deterministic (sorted lane, idempotent
/// content store) so the bundle digest is stable.
pub fn bundle_from_artifacts(
    artifacts: BTreeMap<String, Vec<u8>>,
    provenance: DatasetProvenance,
) -> PipelineBundle<PipelineHandle> {
    bundle_from_artifacts_over(empty_dataset(), artifacts, provenance)
}

/// Like [`bundle_from_artifacts`] but over an explicit backing `dataset` (the lane
/// rides alongside it). Used where a stage's primary output IS a dataset and it
/// also carries named byte artifacts.
pub fn bundle_from_artifacts_over(
    dataset: Arc<RdfDataset>,
    artifacts: BTreeMap<String, Vec<u8>>,
    provenance: DatasetProvenance,
) -> PipelineBundle<PipelineHandle> {
    let mut blobs = ContentStore::new();
    let mut lookaside = RdfLookaside::default();
    // BTreeMap iterates in sorted key order — the lane is deterministic.
    for (path, bytes) in artifacts {
        let digest = blobs.insert(bytes);
        lookaside.resources.push(
            RdfLookasideResource::new(ARTIFACT_KIND)
                .with_name(path)
                .with_digest(digest.to_hex()),
        );
    }
    PipelineBundle::new(dataset, lookaside, Arc::new(blobs), provenance)
}

/// Reconstruct the exact bytes of the byte-artifact lane entry at `logical_path`,
/// or `None` if no such artifact rides the bundle.
///
/// TEMPORARY (C4): the byte-artifact lane read path. C2/C3/C5 replace per-stage
/// callers with dataset/lane reads. Grep `byte-artifact lane`.
pub fn bundle_artifact<'b>(
    bundle: &'b PipelineBundle<PipelineHandle>,
    logical_path: &str,
) -> Option<&'b [u8]> {
    let resource = bundle
        .lookaside()
        .resources_of_kind(ARTIFACT_KIND)
        .find(|r| r.name.as_deref() == Some(logical_path))?;
    let hex = resource.content_digest.as_deref()?;
    let digest = gmeow_rdf::ContentDigest::from_hex(hex)?;
    bundle.blobs().get(&digest).map(Vec::as_slice)
}

/// Reconstruct the full `(logical_path → bytes)` map of a bundle's byte-artifact
/// lane, sorted by path. The inverse of [`bundle_from_artifacts`].
///
/// TEMPORARY (C4): `run_full` writes/compares the committed artifacts off this map.
/// Grep `byte-artifact lane`.
pub fn bundle_artifacts(bundle: &PipelineBundle<PipelineHandle>) -> BTreeMap<String, Vec<u8>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        let (Some(name), Some(hex)) =
            (resource.name.as_deref(), resource.content_digest.as_deref())
        else {
            continue;
        };
        let Some(digest) = gmeow_rdf::ContentDigest::from_hex(hex) else {
            continue;
        };
        if let Some(bytes) = bundle.blobs().get(&digest) {
            out.insert(name.to_owned(), bytes.clone());
        }
    }
    out
}

/// Replace a bundle's provenance sidecar with `provenance` in place, cloning the
/// shared carrier only if needed (`Arc::make_mut`). The pipeline scheduler uses
/// this to thread the run's per-stage provenance into the produced carrier
/// (#1132 C4 deliverable 3) so the bundle CARRIES a provenance sidecar; the full
/// graph/occurrence projection over it is C9.
pub fn set_bundle_provenance(
    bundle: &mut Arc<PipelineBundle<PipelineHandle>>,
    provenance: DatasetProvenance,
) {
    Arc::make_mut(bundle).set_provenance(provenance);
}

/// A fresh empty frozen dataset — the backing graph for artifact-only bundles.
fn empty_dataset() -> Arc<RdfDataset> {
    RdfDatasetBuilder::new()
        .freeze()
        .expect("an empty dataset is always valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arts(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect()
    }

    #[test]
    fn artifact_lane_round_trips_exact_bytes() {
        let artifacts = arts(&[
            ("generated/a.ttl", b"alpha"),
            ("generated/b.nq", b"bravo"),
            ("pipeline/base.nq", b""), // empty bytes are representable
        ]);
        let bundle = bundle_from_artifacts(artifacts.clone(), DatasetProvenance::new());
        assert_eq!(
            bundle_artifact(&bundle, "generated/a.ttl"),
            Some(&b"alpha"[..])
        );
        assert_eq!(bundle_artifact(&bundle, "pipeline/base.nq"), Some(&b""[..]));
        assert_eq!(bundle_artifact(&bundle, "missing"), None);
        assert_eq!(bundle_artifacts(&bundle), artifacts);
    }

    #[test]
    fn shared_bytes_dedup_but_both_paths_reconstruct() {
        // Two artifacts with identical bytes share one content-store blob, yet both
        // logical paths must reconstruct the bytes (the resource index is per-path).
        let artifacts = arts(&[("x", b"same"), ("y", b"same")]);
        let bundle = bundle_from_artifacts(artifacts.clone(), DatasetProvenance::new());
        assert_eq!(bundle.blobs().len(), 1, "equal bytes stored once");
        assert_eq!(bundle_artifacts(&bundle), artifacts);
    }

    #[test]
    fn bundle_digest_changes_with_artifacts_and_is_stable() {
        let a = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
        let b = bundle_from_artifacts(arts(&[("p", b"two")]), DatasetProvenance::new());
        let a2 = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
        assert_ne!(a.digest(), b.digest(), "different bytes → different digest");
        assert_eq!(a.digest(), a2.digest(), "same artifacts → same digest");
    }

    #[test]
    fn pipeline_handle_logic_carries_the_compiled_program() {
        // The Logic arm now carries the REAL typed IR (#1132 C6), not a backing-graph
        // placeholder: an empty program is a valid, cloneable payload.
        let program = Arc::new(LogicProgram::new(vec![], vec![], vec![], None));
        let h = PipelineHandle::Logic(program);
        assert!(matches!(h, PipelineHandle::Logic(_)));
    }
}
