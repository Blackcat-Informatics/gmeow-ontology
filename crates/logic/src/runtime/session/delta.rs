// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The content-addressed [`SessionDelta`] and its active-state [`Suppression`]s.
//!
//! A delta carries only [`RdfDataset`] inputs and holds no authority handle: the
//! session references an authorized workspace commit but is **never** an authority
//! writer. Two distinct preconditions govern application (see [`SessionDelta`]):
//! `base_commit` is the AUTHORIZATION anchor (invariant across applies) and
//! `expected_head` is the TRANSITION anchor (the structural double-apply guard).

use std::sync::Arc;

use purrdf::{RdfDataset, SerializeGraph, parse_dataset, serialize_dataset};

use crate::runtime::frame;
use crate::seam::WorldSourceIdentity;

use super::identity::dataset_content_digest;

/// The media type the replayable committed-delta payload is serialized/parsed under.
/// N-Quads carries the delta's single-world graph name, so a serialize→parse round-trip
/// reproduces the exact re-homed facts the maintainer folds.
const NQUADS_MEDIA_TYPE: &str = "application/n-quads";

/// One active-state retirement: a set of rows whose closure membership moves `1 → 0`.
///
/// This is retirement, **never erasure** — the physical fact arena is monotone; a
/// suppression moves set membership while the arena row and its provenance survive
/// (aligned with the suppression-never-erasure discipline of the transaction executor).
///
/// Not `Clone`: it owns an [`RdfDataset`], which is intentionally move-only.
#[derive(Debug)]
#[non_exhaustive]
pub struct Suppression {
    /// The rows to retire (weight `-1` at the closure boundary).
    pub row: RdfDataset,
}

impl Suppression {
    /// Construct a suppression over the given rows.
    #[must_use]
    pub fn new(row: RdfDataset) -> Self {
        Self { row }
    }
}

/// A content-addressed delta from an authorized workspace commit: additions (facts to
/// insert) plus retirements (active state to suppress).
///
/// Construct via [`SessionDelta::new`], which computes the [`delta_identity`] content
/// address; `#[non_exhaustive]` so adding a field is additive.
///
/// # The two preconditions
///
/// The authorized EDB [`WorldSourceIdentity`] is invariant across applies (the session
/// is not an authority writer), so `base_commit == data_generation` holds both before
/// AND after a delta commits and cannot discriminate a re-applied delta. The journal
/// state-hash (`head`) DOES advance on each commit, so `expected_head` is the field that
/// makes double-apply detectable. [`super::ReasoningSession::apply`] checks BOTH:
/// (1) authorization `base_commit == identity.data_generation`; (2) transition
/// `expected_head == head`.
///
/// [`delta_identity`]: Self::delta_identity
///
/// Not `Clone`: it owns [`RdfDataset`] inputs, which are intentionally move-only.
#[derive(Debug)]
#[non_exhaustive]
pub struct SessionDelta {
    /// The authorized workspace commit whose facts this delta departs from — the
    /// AUTHORIZATION anchor.
    pub base_commit: WorldSourceIdentity,
    /// The prior journal state-hash this delta must extend — the TRANSITION anchor
    /// (distinct from `base_commit`; the double-apply guard).
    pub expected_head: String,
    /// Facts to insert (weight `+1`).
    pub additions: RdfDataset,
    /// Active state to retire (weight `-1`).
    pub retirements: Vec<Suppression>,
    /// Optional committed-derivation budget for the insertion.
    pub max_steps: Option<u64>,
    /// Framed-BLAKE3 content address over every field above — deterministic and
    /// order-independent in the datasets it hashes.
    pub delta_identity: String,
}

impl SessionDelta {
    /// Construct a delta and compute its content address.
    ///
    /// `delta_identity` frames `base_commit`, `expected_head`, the canonical sorted
    /// content digest of `additions`, each suppression's sorted content digest (in
    /// list order), and `max_steps`, under domain `gmeow-logic-session-delta-v1`. It is
    /// deterministic and independent of the internal quad order within each dataset.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `additions` or a suppression dataset cannot be rendered to its
    /// canonical rows (the same typed-EDB bridge the maintainer uses).
    pub fn new(
        base_commit: WorldSourceIdentity,
        expected_head: impl Into<String>,
        additions: RdfDataset,
        retirements: Vec<Suppression>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<Self> {
        let expected_head = expected_head.into();

        let additions_digest =
            dataset_content_digest(b"gmeow-logic-session-delta-additions-v1", &additions)?;
        let mut suppression_digests = Vec::with_capacity(retirements.len());
        for suppression in &retirements {
            suppression_digests.push(dataset_content_digest(
                b"gmeow-logic-session-delta-suppression-v1",
                &suppression.row,
            )?);
        }

        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, b"domain", b"gmeow-logic-session-delta-v1");
        frame(
            &mut hasher,
            b"base-commit-gen",
            base_commit.generation.as_bytes(),
        );
        frame(
            &mut hasher,
            b"base-commit-contract",
            base_commit.source_contract.as_bytes(),
        );
        frame(&mut hasher, b"expected-head", expected_head.as_bytes());
        frame(&mut hasher, b"additions", additions_digest.as_bytes());
        for suppression_digest in &suppression_digests {
            frame(&mut hasher, b"suppression", suppression_digest.as_bytes());
        }
        frame(
            &mut hasher,
            b"max-steps",
            &max_steps.map_or_else(|| b"none".to_vec(), |steps| steps.to_le_bytes().to_vec()),
        );
        let delta_identity = hasher.finalize().to_hex().to_string();

        Ok(Self {
            base_commit,
            expected_head,
            additions,
            retirements,
            max_steps,
            delta_identity,
        })
    }
}

/// The serializable, replayable projection of ONE committed [`SessionDelta`] persisted in
/// a [`super::Checkpoint`].
///
/// It captures exactly what a deterministic replay needs to reproduce the delta over the
/// authorized base: the additions and each suppression's rows rendered to canonical
/// (line-sorted) N-Quads, plus the committed step budget. It deliberately carries NO
/// precondition anchors (`base_commit` / `expected_head`): those are reconstructed from
/// the live replay state on restore (the data-generation is invariant — the session is
/// never an authority writer — and each head is reproduced deterministically), and the
/// replayed journal head is verified against the checkpoint's durable head. Persisting
/// these already-authorized RDF fact-sets in the checkpoint is a session artifact, not a
/// new authorized data-generation.
///
/// `#[non_exhaustive]` so adding a field is additive; all fields are strings/plain data
/// so the whole payload is `Clone` (unlike the move-only [`SessionDelta`] it projects).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CommittedDelta {
    /// The delta's additions dataset, canonical (line-sorted) N-Quads (empty for a
    /// suppression-only delta).
    pub additions_nquads: String,
    /// Each suppression's rows, canonical (line-sorted) N-Quads, in the delta's list
    /// order.
    pub retirement_nquads: Vec<String>,
    /// The committed-derivation budget the original apply ran under (replayed verbatim).
    pub max_steps: Option<u64>,
}

impl CommittedDelta {
    /// Capture the replayable projection of a committed delta by serializing its
    /// additions and each suppression's rows to canonical N-Quads.
    ///
    /// # Errors
    ///
    /// Returns `Err` if a dataset cannot be serialized to (valid UTF-8) N-Quads.
    pub(crate) fn capture(delta: &SessionDelta) -> gmeow_errors::Result<Self> {
        let additions_nquads = dataset_to_canonical_nquads(&delta.additions)?;
        let mut retirement_nquads = Vec::with_capacity(delta.retirements.len());
        for suppression in &delta.retirements {
            retirement_nquads.push(dataset_to_canonical_nquads(&suppression.row)?);
        }
        Ok(Self {
            additions_nquads,
            retirement_nquads,
            max_steps: delta.max_steps,
        })
    }

    /// Reconstruct a [`SessionDelta`] from this payload, anchored on the supplied
    /// `base_commit` (the invariant authorization anchor) and `expected_head` (the live
    /// replay head). Because the additions/retirements round-trip the identical facts,
    /// the reconstructed `delta_identity` — hence the journal transition it drives — is
    /// bit-identical to the originally-committed one.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the stored N-Quads cannot be parsed back into a dataset or the
    /// delta cannot be re-addressed.
    pub(crate) fn replay(
        &self,
        base_commit: WorldSourceIdentity,
        expected_head: impl Into<String>,
    ) -> gmeow_errors::Result<SessionDelta> {
        let additions = dataset_from_nquads(&self.additions_nquads)?;
        let mut retirements = Vec::with_capacity(self.retirement_nquads.len());
        for row in &self.retirement_nquads {
            retirements.push(Suppression::new(dataset_from_nquads(row)?));
        }
        SessionDelta::new(
            base_commit,
            expected_head,
            additions,
            retirements,
            self.max_steps,
        )
    }
}

/// A hard error building the serializable delta payload (never a silent degrade).
fn payload_error(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Engine { detail })
}

/// Serialize a dataset to canonical N-Quads: the serializer's N-Quads lines, trimmed and
/// lexicographically sorted, so the byte payload (and thus the checkpoint content
/// address) is a deterministic, order-independent function of the ground fact set.
fn dataset_to_canonical_nquads(dataset: &RdfDataset) -> gmeow_errors::Result<String> {
    let bytes = serialize_dataset(dataset, NQUADS_MEDIA_TYPE, SerializeGraph::Dataset)
        .map_err(|error| payload_error(format!("serialize session delta to N-Quads: {error}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|error| payload_error(format!("session delta N-Quads is not UTF-8: {error}")))?;
    let mut lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines.sort_unstable();
    Ok(lines.join("\n"))
}

/// Parse canonical N-Quads back into an owned dataset (the inverse of
/// [`dataset_to_canonical_nquads`]). A freshly-parsed dataset is a single-owner `Arc`, so
/// it unwraps into an owned value without cloning.
fn dataset_from_nquads(nquads: &str) -> gmeow_errors::Result<RdfDataset> {
    let dataset = parse_dataset(nquads.as_bytes(), NQUADS_MEDIA_TYPE, None)
        .map_err(|error| payload_error(format!("parse session delta from N-Quads: {error}")))?;
    Arc::try_unwrap(dataset).map_err(|_| {
        payload_error("a freshly-parsed session delta dataset was unexpectedly shared".to_owned())
    })
}
