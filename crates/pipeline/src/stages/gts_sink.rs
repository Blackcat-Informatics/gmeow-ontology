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

use crate::node::{SINK_CAPABILITY, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{PASS_ONE_RECEIPT_PATH, SNAPSHOT_PATH};

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = SNAPSHOT_PATH;

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
    carrier_consumes: Vec<String>,
    capabilities: Vec<String>,
}

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
            consumes: vec![
                "stage-snapshot".to_string(),
                // THIS run's eleven by-reference TAR archives (mappings / cells / queries
                // / tests / schemas / shapes / axioms / models-python / lang-projections
                // / statements / yaml-ld), folded once by
                // their own producer — the terminal reads them off that product and
                // re-folds nothing (PIPELINE_SPINE §3.2/§4). The edge also orders the
                // sink after every archive-member producer transitively, so the
                // JSON-Schema / Pydantic / generated-shape leaves need no direct edge.
                "stage-archive-blobs".to_string(),
                // THIS run's SIX trained zstd dictionaries and their
                // gmeow:CompressionDictionaryRealization records. The terminal is the one
                // point where the whole frame set exists, so it pins the dictionaries in
                // the pack's in-band "dct" map and seals one gmeow:MediumEnvelope per
                // frame it authors.
                //
                // Six, not seven: there is no gmeow-math-v1. A dictionary primes a
                // FRAME, and every math: named graph is unioned into the snapshot
                // payload — one frame, already primed in full by gmeow-core-v1, and
                // gmeow:payloadSchemaDictionary is maxQualifiedCardinality 1, so a second
                // dictionary on that frame is not merely unhelpful but unrepresentable.
                // No mathematical byte family exists to give one instead: the archive
                // fold takes dsl/mappings/**, the per-slice mappings/ and tests/ trees,
                // and the shape surfaces — slices/grounding/math/** reaches the bundle
                // ONLY as parsed RDF in the fold. So the mathematical content is fully
                // dictionary-compressed, by gmeow-core-v1, and nothing is lost.
                "stage-medium-dictionaries".to_string(),
                // The executable-docs "try it" surface reasons over the object-level EDB,
                // whose authored / imports / alignments graphs ride on the source-load
                // product (read, not re-loaded from disk).
                "stage-source-load".to_string(),
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
                // NOT `stage-statements`. The terminal used to staple the statement
                // layer's two byte-decorated projections into the generated-opaque
                // archive; they ride `statements-archive` now, folded by
                // `stage-archive-blobs` off that same product, so the sink reads nothing
                // from it and an edge nothing reads is removed rather than left standing.
                // The ORDERING it used to carry is unchanged: `stage-archive-blobs`
                // consumes `stage-statements`, and the sink consumes that.
                "stage-validate".to_string(),
                // The normalized verify receipt is an opaque generated-fanout member;
                // graph/verify itself already rides in the snapshot carrier.
                "stage-verify-attestation".to_string(),
                // The opaque fanout members ride in from their producing export leaves
                // (each rendered once, in the leaf); `collect_fanout_opaque_members` reads them
                // off these products instead of re-rendering from disk (§3.2/§4).
                "stage-export-agreement".to_string(),
                "stage-export-apache".to_string(),
                "stage-export-bench".to_string(),
                "stage-export-cost-ledger".to_string(),
                "stage-export-evals".to_string(),
                // The OntoLex vartrans terminology lowering: an RDF fanout named graph
                // folded from this run's fresh export-leaf product. (Its two NON-RDF
                // siblings — the glossary table and the TBX termbase — ride
                // `lang-projections-archive`, folded by `stage-archive-blobs`, not here.)
                "stage-export-glossary".to_string(),
                "stage-export-matrix".to_string(),
                "stage-export-metadata".to_string(),
                "stage-export-references".to_string(),
                "stage-export-research-objects".to_string(),
                // The LinkML/TypeScript/GraphQL developer schema surfaces: co-derived
                // from the same fresh shape compilation as json-schema/pydantic, folded
                // into REP_GENERATED from THIS run's fresh product (never re-derived
                // from the in-memory carrier — schemas is no longer carrier-projectable).
                "stage-export-schemas".to_string(),
                // The two slice-quality floor TSVs (P17 projection of the ontology
                // gmeow:AxisFloorCommitment / gmeow:SliceTierFloor individuals): opaque
                // REP_GENERATED fanout members read off this leaf's product.
                "stage-export-governance-floors".to_string(),
                // The two projection-vocabulary ratchet TSVs (P17 projection of the
                // ontology gmeow:ProjectionCeilingCommitment / gmeow:ProjectionVocabulary
                // individuals): opaque REP_GENERATED fanout members read off this leaf's
                // product.
                "stage-export-projection-ceilings".to_string(),
            ],
            // The terminal reads every dependency above, but only these three through
            // carrier lanes. Every other edge contributes committed logical-artifact
            // bytes to the opaque fanout; those bytes survive carrier release. Keeping
            // the distinction explicit lets the scheduler release the multi-million-
            // quad source/mappings/reason products before terminal serialization.
            carrier_consumes: vec![
                "stage-archive-blobs".to_string(),
                "stage-medium-dictionaries".to_string(),
                "stage-snapshot".to_string(),
            ],
            capabilities: vec![SINK_CAPABILITY.to_string()],
        }
    }
}

impl Default for GtsSinkStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GtsSinkStage {
    fn id(&self) -> &str {
        "stage-gts-sink"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn carrier_consumes(&self) -> &[String] {
        &self.carrier_consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
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
        "gts_sink.v14-observed-pass-one-receipt"
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
        let serialized = crate::stages::carrier::serialize_carrier_snapshot_with_receipt(
            input.root,
            input.upstream,
            carrier.as_ref(),
            &crate::medium::registry::MediumSelection::Authored,
        )?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(GTS_PATH.to_string(), serialized.bytes);
        artifacts.insert(
            PASS_ONE_RECEIPT_PATH.to_string(),
            serialized.pass_one_receipt,
        );
        let mut output = StageOutput::new(StageProduct::from_artifacts(self.id(), artifacts));
        output.timings = serialized.timings;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_declares_only_the_lanes_it_reads_as_carrier_inputs() {
        let sink = GtsSinkStage::new();
        assert_eq!(
            sink.carrier_consumes(),
            [
                "stage-archive-blobs",
                "stage-medium-dictionaries",
                "stage-snapshot",
            ]
        );
        for artifact_only in [
            "stage-source-load",
            "stage-compile-logic",
            "stage-mappings",
            "stage-reason",
            "stage-validate",
            "stage-verify-attestation",
        ] {
            assert!(
                sink.consumes().iter().any(|id| id == artifact_only),
                "{artifact_only} remains a declared DAG dependency"
            );
            assert!(
                !sink.carrier_consumes().iter().any(|id| id == artifact_only),
                "{artifact_only} supplies committed bytes, not a live carrier"
            );
        }
    }
}
