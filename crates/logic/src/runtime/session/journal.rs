// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The hash-linked transition journal — the event-sourced spine that makes a delta
//! structurally impossible to apply twice.
//!
//! Each committed transition is `(prev_state_hash, delta_identity, outcome_tag,
//! new_state_hash)` where `new_state_hash` is a framed BLAKE3 over the first three. A
//! committed delta advances `head` to its `new_state_hash`, so re-submitting the same
//! delta (whose `expected_head` is the now-stale prior hash) fails the transition
//! precondition. Because `head` is restored from a durable checkpoint's `journal_head`,
//! this guard survives crash/restart without an in-memory seen-set.

use crate::runtime::frame;

use super::outcome::OutcomeTag;

/// One committed transition in the hash-linked journal.
///
/// Construct via [`TransitionEntry::advance`]; `#[non_exhaustive]` so adding a field is
/// additive.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransitionEntry {
    /// The journal head this transition extended.
    pub prev_state_hash: String,
    /// The content address of the applied delta.
    pub delta_identity: String,
    /// The data-only classification of the transition's outcome.
    pub outcome_tag: OutcomeTag,
    /// The advanced journal head — the framed link over the three fields above.
    pub new_state_hash: String,
}

impl TransitionEntry {
    /// Compute the next transition entry that extends `prev_state_hash` by applying the
    /// delta `delta_identity` with the classified `outcome_tag`.
    ///
    /// `new_state_hash = frame(domain "gmeow-logic-transition-v1", prev_state_hash,
    /// delta_identity, outcome_tag-as-byte)`.
    #[must_use]
    pub fn advance(
        prev_state_hash: impl Into<String>,
        delta_identity: impl Into<String>,
        outcome_tag: OutcomeTag,
    ) -> Self {
        let prev_state_hash = prev_state_hash.into();
        let delta_identity = delta_identity.into();

        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, b"domain", b"gmeow-logic-transition-v1");
        frame(&mut hasher, b"prev", prev_state_hash.as_bytes());
        frame(&mut hasher, b"delta", delta_identity.as_bytes());
        frame(&mut hasher, b"outcome", &[outcome_tag.wire_byte()]);
        let new_state_hash = hasher.finalize().to_hex().to_string();

        Self {
            prev_state_hash,
            delta_identity,
            outcome_tag,
            new_state_hash,
        }
    }
}
