// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The content-addressed [`SessionDelta`] and its active-state [`Suppression`]s.
//!
//! A delta carries only [`RdfDataset`] inputs and holds no authority handle: the
//! session references an authorized workspace commit but is **never** an authority
//! writer. Two distinct preconditions govern application (see [`SessionDelta`]):
//! `base_commit` is the AUTHORIZATION anchor (invariant across applies) and
//! `expected_head` is the TRANSITION anchor (the structural double-apply guard).

use purrdf::RdfDataset;

use crate::runtime::frame;
use crate::seam::WorldSourceIdentity;

use super::identity::dataset_content_digest;

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
