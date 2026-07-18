// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed, integrity-checked, identity-gated [`Checkpoint`]s.
//!
//! A checkpoint stores the [`SessionIdentity`], the authorized EDB generation, the
//! journal head, and the ORDERED sequence of committed deltas applied since the base
//! generation. It carries NO circuit state: `restore` deterministically re-materializes
//! the base EDB then REPLAYS the stored deltas (approach a — the iteration history is
//! fragile across solver bumps and has no measured perf need), reproducing the exact
//! post-apply closure and head. Identical checkpoints collide by `content_address`
//! (content-addressed, folding the serialized deltas so a tampered delta payload is
//! caught by the same integrity gate as any other field).

use crate::runtime::frame;

use super::delta::CommittedDelta;
use super::identity::SessionIdentity;
use super::outcome::IntegrityFault;

/// A durable, content-addressed session checkpoint.
///
/// Construct via [`Checkpoint::new`]; `#[non_exhaustive]` so adding a field is additive.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Checkpoint {
    /// The seven-axis identity the checkpoint was minted under.
    pub identity: SessionIdentity,
    /// The authorized EDB data-generation (`urn:blake3:` address).
    pub edb_generation: String,
    /// The journal head at checkpoint time — the durable double-apply precondition
    /// anchor a restart resumes from, and the value the replayed head is verified
    /// against on restore.
    pub journal_head: String,
    /// The ordered sequence of committed deltas applied on top of `edb_generation`, oldest
    /// first — the durable record `restore` replays to reproduce the post-apply state. A
    /// base (pre-apply) checkpoint carries an empty sequence.
    pub deltas: Vec<CommittedDelta>,
    /// Framed-BLAKE3 content address over every field above (identity, EDB generation,
    /// journal head, AND the serialized deltas).
    pub content_address: String,
}

impl Checkpoint {
    /// Compute the content address for
    /// `(identity, edb_generation, journal_head, deltas)`.
    fn compute_address(
        identity: &SessionIdentity,
        edb_generation: &str,
        journal_head: &str,
        deltas: &[CommittedDelta],
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, b"domain", b"gmeow-logic-session-checkpoint-v1");
        frame(
            &mut hasher,
            b"identity",
            identity.descriptor_hash.as_bytes(),
        );
        frame(&mut hasher, b"edb-generation", edb_generation.as_bytes());
        frame(&mut hasher, b"journal-head", journal_head.as_bytes());
        // Fold the ordered committed deltas so a tampered delta payload (additions,
        // suppression rows, or step budget) is caught by the same CorruptCheckpoint gate.
        for delta in deltas {
            frame(
                &mut hasher,
                b"delta-additions",
                delta.additions_nquads.as_bytes(),
            );
            for row in &delta.retirement_nquads {
                frame(&mut hasher, b"delta-retirement", row.as_bytes());
            }
            frame(
                &mut hasher,
                b"delta-max-steps",
                &delta
                    .max_steps
                    .map_or_else(|| b"none".to_vec(), |steps| steps.to_le_bytes().to_vec()),
            );
            frame(&mut hasher, b"delta-end", b"");
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Mint a checkpoint, computing its content address over every field including the
    /// ordered committed deltas.
    #[must_use]
    pub fn new(
        identity: SessionIdentity,
        edb_generation: impl Into<String>,
        journal_head: impl Into<String>,
        deltas: Vec<CommittedDelta>,
    ) -> Self {
        let edb_generation = edb_generation.into();
        let journal_head = journal_head.into();
        let content_address =
            Self::compute_address(&identity, &edb_generation, &journal_head, &deltas);
        Self {
            identity,
            edb_generation,
            journal_head,
            deltas,
            content_address,
        }
    }

    /// Recompute the content address and compare it to the stored one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrityFault::CorruptCheckpoint`] when the recomputed address
    /// differs from the stored one — the checkpoint bytes (any field, including a stored
    /// delta payload) were tampered with.
    pub fn verify(&self) -> Result<(), IntegrityFault> {
        let computed = Self::compute_address(
            &self.identity,
            &self.edb_generation,
            &self.journal_head,
            &self.deltas,
        );
        if computed == self.content_address {
            Ok(())
        } else {
            Err(IntegrityFault::CorruptCheckpoint {
                expected_address: self.content_address.clone(),
                computed_address: computed,
            })
        }
    }
}
