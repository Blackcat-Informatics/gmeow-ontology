// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pipeline-side carrier types (C4): the [`PipelineHandle`] typed-handle
//! enum and the byte-artifact lane the [`StageProduct`](crate::node::StageProduct)
//! threads stage→stage as an [`Arc<PipelineBundle<PipelineHandle>>`].
//!
//! # The carrier
//!
//! C1 landed the generic [`PipelineBundle<H>`] in `purrdf`: a frozen RDF
//! dataset + lookaside + content-addressed blob store + provenance sidecar + a
//! typed-handle lane. This module plugs the pipeline's concrete handle payload
//! into that lane (`H = PipelineHandle`) and preserves the selected terminal
//! artifact bytes alongside native stage publications.
//!
//! # The terminal artifact lane
//!
//! Required named artifacts retain their exact logical-path identity inside the
//! bundle. Native consumers borrow complete typed values, including programs and
//! final diagnostic reports, rather than reparsing these terminal projections.
//! The fixed-point check still compares every selected artifact's exact bytes:
//!
//! * each artifact's bytes live in the bundle's [`ContentStore`] (the one owner of
//!   payload bytes, by-reference doctrine), and
//! * a [`RdfLookasideResource`] of kind [`RdfLookasideKind::Blob`] indexes it by
//!   `name = logical_path`, `content_digest = blob hex` — so `bundle_artifact(path)`
//!   reconstructs the exact bytes (`name → digest → blobs.get(digest)`).
//!
//! The `(logical_path → bytes)` surface `run_full` writes and compares remains
//! distinct from native payload identity. A missing typed publication cannot be
//! replaced by a surviving artifact with the same apparent contents.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::result::ReasoningResult;
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::relational_core::RelationalCoreProgram;
use purrdf::provenance::DatasetProvenance;
use purrdf::{
    ContentDigest, ContentStore, PipelineBundle, RdfBlobRecord, RdfDataset, RdfDatasetBuilder,
    RdfLookaside, RdfLookasideKind, RdfLookasideResource,
};

mod diagnostics;
#[cfg(test)]
pub(crate) use diagnostics::tests as diagnostic_test_support;
pub use diagnostics::{DiagnosticReportOwner, DiagnosticsPublication};
pub(crate) use diagnostics::{diagnostics_from_product, pin_diagnostics, snapshot_diagnostics};

/// Native stage products bound to their governed named-graph projections.
///
/// Every arm carries its complete typed payload. Action identity commits to all
/// fields, including fields omitted by a lossy RDF projection. Persistent cache
/// hydration restores the native codec and authenticates that complete identity;
/// consumers reuse these values without parsing or lowering the graph again.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PipelineHandle {
    /// Ephemeral original source parses and their portable source commitments.
    /// Released after its last declared native consumer; never a persistent fixture.
    SourceCatalog(Arc<crate::stages::parse_sources::SourceCatalog>),
    /// Canonical compiled logic IR and its retained source information, pinned
    /// to the `graph/logic` projection.
    Logic(Arc<LogicProgram>),
    /// The compiler's program and mandatory report inputs, sharing one native
    /// publication until the compile product's last declared consumer.
    CompiledLogic(Arc<CompiledLogicPublication>),
    /// Complete final diagnostic reports, including fields omitted by the RDF
    /// finding graph. The snapshot shares the two original producer reports.
    Diagnostics(Arc<DiagnosticsPublication>),
    /// Reasoning axes, provenance, complete answer payload and declared row
    /// schema, pinned to the governed `graph/reasoning` summary. Publication
    /// verifies the represented summary against native emission; hydration
    /// preserves fields that the summary deliberately omits.
    Reasoning(Arc<ReasoningResult>),
    /// Lowered relational facts, rules and residue, pinned to their
    /// `graph/relational-core` projection.
    RelationalCore(Arc<RelationalCoreProgram>),
    /// Typed correspondences and their evidence, pinned to the
    /// `graph/correspondence` projection.
    Correspondence(Arc<CorrespondenceProgram>),
    /// Final law-bearing correspondences and admitted property/source authority,
    /// pinned to mappings' `graph/correspondence-laws` projection.
    GroundingCatalog(Arc<gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog>),
}

/// The compiler-owned input to mappings' final projection report.
/// Counts, judgment rows and every native diagnostic field are authenticated as
/// one typed payload. Projection bodies remain in their selected artifact lanes.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LogicReportInputs {
    /// Compiler-owned axiom/rule/profile/formula counts. Final correspondence,
    /// lawful-uplift and claimed-uplift counts stay zero until mappings owns them.
    pub header: gmeow_logic_compile::projections::report::ReportHeader,
    /// Source-and-example correspondence total before mappings adds its audit.
    pub base_correspondence_count: usize,
    /// Source-and-example proved uplift total before mappings adds its audit.
    pub base_lawful_uplift_count: usize,
    /// Report judgments without any serialized projection bodies.
    pub projections: Vec<gmeow_logic_compile::projections::report::ProjectionReportRow>,
    /// Complete compiler loss witnesses, including source and causal attribution.
    pub loss: gmeow_logic_compile::loss_ledger::LossLedger,
}

/// The exact program and report inputs published by the compile-logic stage.
/// A program-only snapshot is a different publication: it cannot satisfy a
/// mappings consumer that requires the compiler's complete report inputs.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CompiledLogicPublication {
    /// The same compiled program borrowed by native downstream consumers.
    pub program: Arc<LogicProgram>,
    /// Mandatory compiler report inputs; absence cannot select a weaker path.
    pub report: LogicReportInputs,
}

impl PipelineHandle {
    /// Borrow a complete program from either explicitly supported publication.
    /// Consumers that need report inputs must require `CompiledLogic` themselves.
    pub fn logic_program(&self) -> Option<&Arc<LogicProgram>> {
        match self {
            Self::Logic(program) => Some(program),
            Self::CompiledLogic(publication) => Some(&publication.program),
            Self::SourceCatalog(_)
            | Self::Diagnostics(_)
            | Self::Reasoning(_)
            | Self::RelationalCore(_)
            | Self::Correspondence(_)
            | Self::GroundingCatalog(_) => None,
        }
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

/// The logical-path prefix marking an INTERNAL dataflow artifact: bytes that exist
/// only to travel from one stage to its declared consumers, such as
/// `pipeline/base-graph.nq`. Output reconciliation skips these paths.
/// [`release_carrier`] preserves the internal paths in the scheduler's declared
/// reader set. Both operations use this prefix to identify internal paths.
pub const INTERNAL_ARTIFACT_PREFIX: &str = "pipeline/";

/// Rebuild `bundle` retaining ONLY its COMMITTED byte-artifact-lane entries (every lane
/// entry whose logical path does NOT start with [`INTERNAL_ARTIFACT_PREFIX`]) plus the
/// internal entries named in `kept_internal`.
///
/// Everything else is released: the frozen dataset (an empty one replaces it), the
/// typed-handle lane, every by-reference blob record, the provenance sidecar, and every
/// other internal `pipeline/`-prefixed artifact. `kept_internal` is the set of internal
/// artifacts that consumers declare they read
/// ([`crate::node::Stage::internal_artifact_reads`]); without it they would receive a
/// product that silently lacks them. The scheduler calls this when a product's exact
/// remaining carrier-reader count reaches zero and no artifact-only view of it is in
/// flight. The residue shares the surviving payload allocations. An artifact-only view
/// serves the same lane by reference (`StageProduct::consumer_artifact_view`). The
/// resulting product is a TOMBSTONE —
/// [`StageProduct::carrier_released`](crate::node::StageProduct::carrier_released) is
/// set and the product keeps its ORIGINAL `digest`, because the digest is the identity
/// witness of the carrier that was released, not a fold over this residue.
///
/// Determinism: the surviving lane is rebuilt in the source lookaside's iteration order
/// over a strict subset of its entries, so the operation is a pure, idempotent function
/// of the input bundle and `kept_internal`. A carrier reader never observes it: the
/// scheduler releases a product only after every dispatched carrier reader has completed.
///
/// # Errors
/// A malformed artifact-resource content digest, or a lane entry whose bytes are missing
/// from the content store, is a corrupt carrier — a HARD FAIL, never a silently shorter
/// lane (no-optionality).
pub fn release_carrier(
    bundle: &PipelineBundle<PipelineHandle>,
    kept_internal: &std::collections::BTreeSet<&str>,
) -> Result<PipelineBundle<PipelineHandle>, gmeow_errors::Diag> {
    let mut lookaside = RdfLookaside::default();
    let mut retained_digests = std::collections::BTreeSet::new();
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        let (Some(name), Some(hex)) =
            (resource.name.as_deref(), resource.content_digest.as_deref())
        else {
            continue;
        };
        if name.starts_with(INTERNAL_ARTIFACT_PREFIX) && !kept_internal.contains(name) {
            continue;
        }
        let digest = purrdf::ContentDigest::from_hex(hex).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
                "release_carrier: malformed resource content digest {hex:?}"
            )))
        })?;
        if !bundle.blobs().contains(&digest) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode::new(
                format!(
                    "release_carrier: byte-artifact lane entry {name} references content \
                     digest {hex} which the bundle's content store does not hold"
                ),
            )));
        }
        retained_digests.insert(digest);
        lookaside.resources.push(resource.clone());
    }
    let mut blobs = bundle.blobs().clone();
    blobs.retain(|digest| retained_digests.contains(digest));
    Ok(PipelineBundle::new(
        empty_dataset(),
        lookaside,
        Arc::new(blobs),
        DatasetProvenance::new(),
    ))
}

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

/// Like [`bundle_from_artifacts_over`] but ALSO folds one representation-keyed raw
/// blob into the bundle's by-reference blob lane: the bytes ride the same
/// [`ContentStore`] and an [`RdfBlobRecord`] indexes them by `representation` (the
/// model the `transform:denied` raw-JSON lane uses). The blob round-trips through the
/// per-stage cache unchanged (the cache mirrors the lookaside blob records + the
/// content store), so a cache-hit product carries the identical blob. `bundle_rep_blob`
/// reads it back by representation.
///
/// Because the blob is content-addressed into the store, it participates in the
/// bundle's `digest()` (so a diagnostics change re-keys the producer's product), but it
/// does NOT touch the byte-artifact lane [`bundle_artifacts`] reads — it is a distinct
/// lane, invisible to the committed-artifact reconcile.
pub fn bundle_from_artifacts_over_with_rep_blob(
    dataset: Arc<RdfDataset>,
    artifacts: BTreeMap<String, Vec<u8>>,
    provenance: DatasetProvenance,
    representation: &str,
    media_type: &str,
    blob_bytes: Vec<u8>,
) -> PipelineBundle<PipelineHandle> {
    let mut blobs = ContentStore::new();
    let mut lookaside = RdfLookaside::default();
    for (path, bytes) in artifacts {
        let digest = blobs.insert(bytes);
        lookaside.resources.push(
            RdfLookasideResource::new(ARTIFACT_KIND)
                .with_name(path)
                .with_digest(digest.to_hex()),
        );
    }
    let decoded_len = blob_bytes.len() as u64;
    let digest = blobs.insert(blob_bytes);
    lookaside.blobs.push(RdfBlobRecord {
        digest: digest.to_hex(),
        media_type: Some(media_type.to_owned()),
        representation: Some(representation.to_owned()),
        decoded_len: Some(decoded_len),
        metadata: BTreeMap::new(),
        origin: None,
    });
    PipelineBundle::new(dataset, lookaside, Arc::new(blobs), provenance)
}

/// Fold one representation-keyed raw blob into an ALREADY-ASSEMBLED bundle (e.g. the
/// compile-logic bundle, after its typed handles are pinned), returning the bundle with
/// the blob in its by-reference lane. Existing payloads keep their backing
/// allocations and the original graph pins stay attached. See
/// [`bundle_from_artifacts_over_with_rep_blob`] for the lane semantics.
pub fn attach_rep_blob(
    mut bundle: PipelineBundle<PipelineHandle>,
    representation: &str,
    media_type: &str,
    blob_bytes: Vec<u8>,
) -> Result<PipelineBundle<PipelineHandle>, gmeow_errors::Diag> {
    // Clone the store without rehashing its verified payloads.
    let mut blobs = bundle.blobs().clone();
    let mut records = bundle.lookaside().blobs.clone();
    let decoded_len = blob_bytes.len() as u64;
    let digest = blobs.insert(blob_bytes);
    records.push(RdfBlobRecord {
        digest: digest.to_hex(),
        media_type: Some(media_type.to_owned()),
        representation: Some(representation.to_owned()),
        decoded_len: Some(decoded_len),
        metadata: BTreeMap::new(),
        origin: None,
    });
    bundle
        .replace_resident_blob_records(records, blobs)
        .map_err(gmeow_errors::Diag::from_rdf)?;
    Ok(bundle)
}

/// Read back the bytes of the representation-keyed blob folded by
/// [`bundle_from_artifacts_over_with_rep_blob`] / [`attach_rep_blob`], or `None` when no
/// blob record declares `representation`.
pub fn bundle_rep_blob<'b>(
    bundle: &'b PipelineBundle<PipelineHandle>,
    representation: &str,
) -> Option<&'b [u8]> {
    let record = bundle
        .lookaside()
        .blobs
        .iter()
        .find(|r| r.representation.as_deref() == Some(representation))?;
    let digest = ContentDigest::from_hex(&record.digest)?;
    bundle.blobs().get(&digest).map(AsRef::as_ref)
}

/// Select the representation records this carrier still serves. Preserve bytes
/// addressed by another record or a resource, and leave unrelated store entries
/// intact. Return the number of removed records and bytes detached from this store.
pub(crate) fn retain_rep_blobs(
    bundle: &mut Arc<PipelineBundle<PipelineHandle>>,
    keep: impl Fn(&str) -> bool,
) -> gmeow_errors::Result<(usize, usize)> {
    let removed = |record: &RdfBlobRecord| {
        record
            .representation
            .as_deref()
            .is_some_and(|rep| !keep(rep))
    };
    let mut detached =
        std::collections::HashSet::with_hasher(purrdf_hash::fixed::FixedState::new());
    let mut removed_records = 0;
    for record in bundle
        .lookaside()
        .blobs
        .iter()
        .filter(|record| removed(record))
    {
        let digest = sidecar_digest(&record.digest)?;
        if !bundle.blobs().contains(&digest) {
            let detail =
                format!("carrier representation references missing content digest {digest}");
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode::new(
                detail,
            )));
        }
        detached.insert(digest);
        removed_records += 1;
    }
    if detached.is_empty() {
        return Ok((0, 0));
    }

    let lookaside = bundle.lookaside();
    let records: Vec<_> = lookaside
        .blobs
        .iter()
        .filter(|record| !removed(record))
        .cloned()
        .collect();
    for hex in lookaside
        .resources
        .iter()
        .filter_map(|resource| resource.content_digest.as_deref())
        .chain(records.iter().map(|record| record.digest.as_str()))
    {
        detached.remove(&sidecar_digest(hex)?);
    }
    let detached_bytes = detached
        .iter()
        .filter_map(|digest| bundle.blobs().get(digest))
        .map(Vec::len)
        .sum();
    let mut blobs = bundle.blobs().clone();
    if !detached.is_empty() {
        blobs.retain(|digest| !detached.contains(digest));
    }
    Arc::make_mut(bundle)
        .replace_resident_blob_records(records, blobs)
        .map_err(gmeow_errors::Diag::from_rdf)?;
    Ok((removed_records, detached_bytes))
}

/// Count representation records and digest entries removed by a carrier swap.
/// A shared digest that remains in the artifact residue contributes zero bytes.
pub(crate) fn released_rep_blobs(
    carrier: &PipelineBundle<PipelineHandle>,
    residue: &PipelineBundle<PipelineHandle>,
) -> gmeow_errors::Result<(usize, usize)> {
    let mut detached =
        std::collections::HashSet::with_hasher(purrdf_hash::fixed::FixedState::new());
    let mut records = 0;
    for record in &carrier.lookaside().blobs {
        if record.representation.is_some() {
            let digest = sidecar_digest(&record.digest)?;
            if !residue.blobs().contains(&digest) {
                detached.insert(digest);
            }
            records += 1;
        }
    }
    let bytes = detached
        .iter()
        .filter_map(|digest| carrier.blobs().get(digest))
        .map(|bytes| bytes.len())
        .sum();
    Ok((records, bytes))
}

fn sidecar_digest(hex: &str) -> gmeow_errors::Result<ContentDigest> {
    ContentDigest::from_hex(hex).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
            "carrier lookaside references malformed content digest {hex:?}"
        )))
    })
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
    let digest = purrdf::ContentDigest::from_hex(hex)?;
    bundle.blobs().get(&digest).map(AsRef::as_ref)
}

/// Reconstruct the full `(logical_path → bytes)` map of a bundle's byte-artifact
/// lane, sorted by path. The inverse of [`bundle_from_artifacts`].
///
/// TEMPORARY (C4): `run_full` writes/compares the committed artifacts off this map.
/// Grep `byte-artifact lane`.
pub fn bundle_artifacts(bundle: &PipelineBundle<PipelineHandle>) -> BTreeMap<String, Vec<u8>> {
    bundle_artifacts_where(bundle, |_| true)
}

/// [`bundle_artifacts`] restricted to the entries `wanted` accepts, which is asked about
/// an entry's logical path only once the entry is complete (it has a name, a valid digest
/// and its bytes in the store) and before its bytes are copied, so an entry it refuses is
/// never cloned.
pub fn bundle_artifacts_where(
    bundle: &PipelineBundle<PipelineHandle>,
    mut wanted: impl FnMut(&str) -> bool,
) -> BTreeMap<String, Vec<u8>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        let (Some(name), Some(hex)) =
            (resource.name.as_deref(), resource.content_digest.as_deref())
        else {
            continue;
        };
        let Some(digest) = purrdf::ContentDigest::from_hex(hex) else {
            continue;
        };
        if let Some(bytes) = bundle.blobs().get(&digest)
            && wanted(name)
        {
            out.insert(name.to_owned(), bytes.to_vec());
        }
    }
    out
}

/// Borrow the complete byte-artifact lane without cloning payload bytes.
///
/// This is the read path for consumers that select a small subset of a large
/// producer lane. Building an owned [`BTreeMap`] first would duplicate every
/// artifact before the selector discarded most of them. Unlike the legacy
/// convenience projection above, this boundary is fail-closed: malformed lane
/// metadata is a corrupt carrier rather than an absent artifact.
///
/// # Errors
/// A byte-artifact resource is structurally incomplete or references invalid or
/// missing content.
pub fn bundle_artifact_refs(
    bundle: &PipelineBundle<PipelineHandle>,
) -> Result<BTreeMap<&str, &[u8]>, gmeow_errors::Diag> {
    let mut out: BTreeMap<&str, &[u8]> = BTreeMap::new();
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        let name = resource.name.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode::new(
                "byte-artifact resource carries no logical name".to_string(),
            ))
        })?;
        let hex = resource.content_digest.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
                "byte-artifact resource {name:?} carries no content digest"
            )))
        })?;
        let digest = ContentDigest::from_hex(hex).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
                "byte-artifact resource {name:?} carries malformed content digest {hex:?}"
            )))
        })?;
        let bytes = bundle.blobs().get(&digest).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
                "byte-artifact resource {name:?} references missing content-store digest {hex}"
            )))
        })?;
        if out.insert(name, bytes.as_ref()).is_some() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode::new(
                format!("byte-artifact lane carries duplicate logical name {name:?}"),
            )));
        }
    }
    Ok(out)
}

/// The `(logical path → content digests)` of the byte-artifact lane entries of `bundle`
/// whose logical path `serves` accepts: the identities [`release_carrier`] would keep, read
/// without building the residue.
pub(crate) fn artifact_lane_records(
    bundle: &PipelineBundle<PipelineHandle>,
    serves: impl Fn(&str) -> bool,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut records: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        if let (Some(name), Some(digest)) = (&resource.name, &resource.content_digest)
            && serves(name)
        {
            records
                .entry(name.clone())
                .or_default()
                .insert(digest.clone());
        }
    }
    records
}

/// The payload bytes of `bundle`'s byte-artifact lane, as `(committed, internal)`: the
/// entries [`release_carrier`] always keeps, and the `pipeline/` ones. An entry whose
/// bytes are not in the content store counts nothing. For the memory record.
pub(crate) fn artifact_lane_bytes(bundle: &PipelineBundle<PipelineHandle>) -> (u64, u64) {
    let mut committed = 0u64;
    let mut internal = 0u64;
    for resource in bundle.lookaside().resources_of_kind(ARTIFACT_KIND) {
        let (Some(name), Some(hex)) =
            (resource.name.as_deref(), resource.content_digest.as_deref())
        else {
            continue;
        };
        let Some(bytes) =
            ContentDigest::from_hex(hex).and_then(|digest| bundle.blobs().get(&digest))
        else {
            continue;
        };
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if name.starts_with(INTERNAL_ARTIFACT_PREFIX) {
            internal = internal.saturating_add(len);
        } else {
            committed = committed.saturating_add(len);
        }
    }
    (committed, internal)
}

/// Replace a bundle's provenance sidecar with `provenance` in place, cloning the
/// shared carrier only if needed (`Arc::make_mut`). The pipeline scheduler uses
/// this to thread the run's per-stage provenance into the produced carrier
/// (C4 deliverable 3) so the bundle CARRIES a provenance sidecar; the full
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

#[path = "bundle.tests.rs"]
#[cfg(test)]
mod tests;
