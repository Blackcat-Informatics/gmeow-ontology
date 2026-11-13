// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage: the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. The STRUCTURED multi-named-graph `dist`
//! snapshot is ASSEMBLED upstream by [`crate::stages::carrier::SnapshotStage`]
//! (fold-isomorphic to the committed `generated/dist/gmeow.gts`, the parity
//! gate). This sink consumes that one `stage-snapshot` product and re-emits its
//! `gmeow.gts` bytes as the sink artifact — the single, well-defined disk-write
//! the `run_full` orchestration performs. Splitting the assembly (a Transform)
//! from the serialization exit (this Sink) is what lets every fold-reading export
//! leaf consume THIS run's freshly-composed fold rather than the stale committed
//! file (the single-pass invariant).

use std::collections::BTreeMap;

use crate::node::{StageBody, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{PASS_ONE_RECEIPT_PATH, SNAPSHOT_PATH};

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = SNAPSHOT_PATH;

/// Complete native ingestion/loss receipt, bound to the terminal output bytes.
pub const INGESTION_RECEIPT_PATH: &str = "generated/dist/gmeow.gts.ingestion.cbor";

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {}

impl GtsSinkStage {
    /// Construct the sink. It consumes the assembled carrier (`stage-snapshot`), the
    /// already-folded by-reference TAR archives (`stage-archive-blobs`), and the blob
    /// sources it staples itself: the in-memory reasoning / SHACL-report products and the
    /// opaque `generated/` fanout members read off their producing export leaves.
    /// It holds [`SINK_CAPABILITY`] — the sole serialization exit the loader requires
    /// exactly one stage to hold (mirrored by the slice
    /// `gmeow:stage-gts-sink gmeow:hasCapability gmeow:sinkCapability`).
    pub fn new() -> Self {
        Self {
            // The terminal reads every dependency above, but only these three through
            // carrier lanes. Every other edge contributes committed logical-artifact
            // bytes to the opaque fanout; those bytes survive carrier release. Keeping
            // the distinction explicit lets the scheduler release the multi-million-
            // quad source/mappings/reason products before terminal serialization.
        }
    }
}

impl Default for GtsSinkStage {
    fn default() -> Self {
        Self::new()
    }
}

impl StageBody for GtsSinkStage {
    fn id(&self) -> &str {
        "stage-gts-sink"
    }
    fn output_family(&self) -> crate::node::OutputFamily {
        crate::node::OutputFamily::Generated
    }
    fn impl_version(&self) -> &str {
        // v4: the opaque fanout members (references / bench / apache / matrix / eval +
        // research-object sidecars / metadata) ride in from their producing export leaves;
        // `collect_fanout_opaque_members` reads them off those products instead of re-rendering
        // from disk, and dsl-stats / context ride off the already-consumed
        // stage-mappings product (§3.2 transform-once, §4 pure terminal).
        // v5: REP_SHAPES' generated members (result-shapes.ttl + frame-shapes.ttl)
        // are folded from the consumed export-leaf products instead of a stale
        // disk read, matching the validation-shapes.ttl freshness rule.
        // v6: the by-reference TAR archives are no longer folded here at all —
        // they are READ off the `stage-archive-blobs` product (the fold moved to its
        // own stage so the archives exist mid-DAG).
        // v7: the generated-opaque archive SHEDS four members — the statement layer's
        // two byte projections and the two non-RDF terminology surfaces — which now ride
        // `statements-archive` / `lang-projections-archive`. The emitted bytes and the
        // emitted `opaque` fanout manifest both change, so the key moves.
        // v8: fold the dedicated verify stage's normalized JSON receipt into the
        // generated-opaque archive; graph/verify continues to ride in the snapshot.
        // v9: seal whole-carrier digest preimages sequentially, then consume/release
        // the snapshot builder before its one canonical encode. Emitted bytes stay
        // identical; peak memory no longer includes the builder plus two redundant
        // whole-payload serializations.
        // v10: declare the exact three carrier-lane inputs independently from the
        // artifact-only dependency edges. Emitted bytes are unchanged; the scheduler
        // may release every artifact-only producer's graph/handle/blob carrier at its
        // true last reader instead of retaining it until this terminal runs.
        // v11: build the flat RDFC stratum union directly from each source's native
        // iterators, with one blank scope per source, then canonicalize that one frozen
        // dataset. This removes two whole-stratum quad vectors and a redundant refreeze;
        // emitted bytes and the standardize-apart contract are unchanged.
        // v12: persist a keyed, selection-independent RDFC stratum receipt beside the
        // terminal artifact. A counterfactual same-carrier emission reuses that exact
        // digest instead of canonicalizing the multi-million-quad carrier twice.
        // v13: extend that receipt to the pass-1 snapshot content identity too. Both
        // canonical preimages are selection-independent; malformed or mismatched
        // receipts hard-fail and emitted GTS bytes are unchanged.
        // v14: report the production sink's allocation phases, structural counts, and
        // RSS observations while retaining the keyed pass-one receipt. Telemetry is
        // report-only and does not enter artifact identity.
        "gts_sink.v18-math-target-artifacts"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The terminal gts ARCHIVE writer: serialize THIS run's carrier
        // into the single `gmeow.gts` package. GTS is exit-only — produced HERE and
        // nowhere else; every internal export leaf reads the carrier dataset off the
        // snapshot product's bundle, never these bytes. The carrier is taken off the
        // bundle (no re-assembly — the razor: transform transport→form once); the
        // by-reference TAR archives are READ off the `stage-archive-blobs` product and
        // stapled alongside it, never re-folded here.
        let carrier = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        let serialized = crate::stages::carrier::serialize_carrier_snapshot(
            input.root,
            input.upstream,
            &carrier,
            &crate::medium::registry::MediumSelection::Authored,
        )?;
        snapshot_output(self.id(), serialized)
    }
}

/// Publish bytes and their full receipt into the same committed stage product.
fn snapshot_output(
    stage_id: &str,
    serialized: crate::stages::carrier::SerializedCarrierSnapshot,
) -> Result<StageOutput, gmeow_errors::Diag> {
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        INGESTION_RECEIPT_PATH.to_string(),
        serialized.emission.ingestion_receipt()?,
    );
    artifacts.insert(GTS_PATH.to_string(), serialized.emission.bytes);
    artifacts.insert(
        PASS_ONE_RECEIPT_PATH.to_string(),
        serialized.pass_one_receipt,
    );
    let mut output = StageOutput::new(StageProduct::from_artifacts(stage_id, artifacts)?);
    output.timings = serialized.timings;
    Ok(output)
}

#[path = "gts_sink.tests.rs"]
#[cfg(test)]
mod tests;
