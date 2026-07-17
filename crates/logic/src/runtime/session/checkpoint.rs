// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed, integrity-checked, identity-gated [`Checkpoint`]s.
//!
//! A checkpoint stores the [`SessionIdentity`], the authorized EDB generation, and the
//! journal head. It carries NO circuit state: `restore` deterministically
//! re-materializes from the authorized EDB (approach a — the iteration history is
//! fragile across solver bumps and has no measured perf need). Identical checkpoints
//! collide by `content_address` (content-addressed).

use crate::runtime::frame;

use super::identity::SessionIdentity;
use super::outcome::IntegrityFault;

/// A durable, content-addressed session checkpoint.
///
/// Construct via [`Checkpoint::new`]; `#[non_exhaustive]` so adding a field is additive.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Checkpoint {
    /// The seven-axis identity the checkpoint was minted under.
    pub identity: SessionIdentity,
    /// The authorized EDB data-generation (`urn:blake3:` address).
    pub edb_generation: String,
    /// The journal head at checkpoint time — the durable double-apply precondition
    /// anchor a restart resumes from.
    pub journal_head: String,
    /// Framed-BLAKE3 content address over the three fields above.
    pub content_address: String,
}

impl Checkpoint {
    /// Compute the content address for `(identity, edb_generation, journal_head)`.
    fn compute_address(
        identity: &SessionIdentity,
        edb_generation: &str,
        journal_head: &str,
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
        hasher.finalize().to_hex().to_string()
    }

    /// Mint a checkpoint, computing its content address.
    #[must_use]
    pub fn new(
        identity: SessionIdentity,
        edb_generation: impl Into<String>,
        journal_head: impl Into<String>,
    ) -> Self {
        let edb_generation = edb_generation.into();
        let journal_head = journal_head.into();
        let content_address = Self::compute_address(&identity, &edb_generation, &journal_head);
        Self {
            identity,
            edb_generation,
            journal_head,
            content_address,
        }
    }

    /// Recompute the content address and compare it to the stored one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrityFault::CorruptCheckpoint`] when the recomputed address
    /// differs from the stored one — the checkpoint bytes were tampered with.
    pub fn verify(&self) -> Result<(), IntegrityFault> {
        let computed =
            Self::compute_address(&self.identity, &self.edb_generation, &self.journal_head);
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
