// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admission of an observed finite commit journal for temporal evaluation.
//!
//! The existing runtime transition hash is the only link authority. Admission
//! authenticates those links and the selected entry inventory; it does not claim
//! that a transition hash authenticates separately attached world observations.
//! Nothing here executes an operation or invents a committed event.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use gmeow_logic_compile::ir::Term;

use crate::runtime::{TransitionEntry, frame};

/// Operational admission envelope, separate from formula evaluation steps.
pub const MAX_JOURNAL_ENTRIES: usize = 65_536;

/// Explicit evidence about whether more committed events may follow this prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalBoundary {
    /// A currently observed prefix, with no claim that its last event is final.
    Open,
    /// Explicit finalization: this journal admits no subsequent event.
    Finalized,
}

/// One attributed input record identifying an existing runtime transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedEntry {
    /// Authored entry IRI, retained as a provenance anchor.
    pub identity: String,
    /// Authored predecessor entry IRI; absent only at genesis.
    pub predecessor: Option<String>,
    /// The exact runtime hash recipe and outcome discriminant.
    pub transition: TransitionEntry,
}

/// The complete selected prefix supplied by a caller. Entry order is immaterial.
#[derive(Debug, Clone)]
pub struct JournalObservation {
    /// Selected journal IRI.
    pub identity: String,
    /// The enactment occurrence whose commit history this journal describes.
    pub enactment: String,
    /// Explicit genesis anchor, in the runtime's lowercase BLAKE3 hexadecimal form.
    pub initial_head: String,
    /// IRI of the selected last entry, not its hash literal.
    pub head_entry: String,
    /// Every entry in the selected prefix, including genesis and the head.
    pub entries: Vec<ObservedEntry>,
    /// Explicit open/finalized evidence, never inferred from missing successors.
    pub boundary: JournalBoundary,
}

/// Malformed evidence and an operational admission bound are different failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// The supplied records cannot denote the declared finite commit chain.
    Invalid(String),
    /// The input was not evaluated because its declared admission envelope was hit.
    AdmissionLimit {
        /// Maximum admitted entry count.
        limit: usize,
    },
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(detail) => formatter.write_str(detail),
            Self::AdmissionLimit { limit } => {
                write!(
                    formatter,
                    "finite journal exceeds its {limit}-entry admission bound"
                )
            }
        }
    }
}

impl std::error::Error for JournalError {}

/// A nonempty, contiguous, hash-verified journal in causal order.
#[derive(Debug, Clone)]
pub struct FiniteJournal {
    identity: String,
    enactment: String,
    initial_head: String,
    entries: Vec<ObservedEntry>,
    identities: BTreeSet<String>,
    heads: BTreeSet<String>,
    boundary: JournalBoundary,
    prefix_hasher: blake3::Hasher,
}

fn iri(value: &str, field: &str) -> Result<(), JournalError> {
    Term::iri(value)
        .map(|_| ())
        .map_err(|error| JournalError::Invalid(format!("{field}: {}", error.message())))
}

fn hash(value: &str, field: &str) -> Result<(), JournalError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(JournalError::Invalid(format!(
            "{field} must be a canonical lowercase BLAKE3 digest"
        )));
    }
    Ok(())
}

fn validate_entry(entry: &ObservedEntry) -> Result<(), JournalError> {
    iri(&entry.identity, "journal entry")?;
    if let Some(predecessor) = &entry.predecessor {
        iri(predecessor, "journal predecessor")?;
    }
    let transition = &entry.transition;
    hash(&transition.prev_state_hash, "previous head")?;
    hash(&transition.delta_identity, "delta identity")?;
    hash(&transition.new_state_hash, "new head")?;
    let expected = TransitionEntry::advance(
        &transition.prev_state_hash,
        &transition.delta_identity,
        transition.outcome_tag,
    );
    if expected.new_state_hash != transition.new_state_hash {
        return Err(JournalError::Invalid(format!(
            "journal entry {} does not satisfy the runtime transition hash",
            entry.identity,
        )));
    }
    Ok(())
}

fn fold_entry(hasher: &mut blake3::Hasher, entry: &ObservedEntry) {
    frame(hasher, b"entry", entry.identity.as_bytes());
    frame(hasher, b"head", entry.transition.new_state_hash.as_bytes());
}

impl FiniteJournal {
    /// Authenticate a selected nonempty prefix without evaluating its formulas.
    ///
    /// # Errors
    /// Rejects malformed links, duplicate or disconnected records, cycles, forks,
    /// a wrong genesis anchor, and input outside the entry-count envelope.
    pub fn admit(observation: JournalObservation) -> Result<Self, JournalError> {
        Self::admit_with_limit(observation, MAX_JOURNAL_ENTRIES)
    }

    fn admit_with_limit(
        observation: JournalObservation,
        limit: usize,
    ) -> Result<Self, JournalError> {
        if observation.entries.len() > limit {
            return Err(JournalError::AdmissionLimit { limit });
        }
        iri(&observation.identity, "journal")?;
        iri(&observation.enactment, "enactment")?;
        iri(&observation.head_entry, "head entry")?;
        hash(&observation.initial_head, "initial head")?;
        if observation.entries.is_empty() {
            return Err(JournalError::Invalid(
                "finite temporal evaluation requires a nonempty journal".into(),
            ));
        }
        let mut inventory = BTreeMap::new();
        let mut heads = BTreeSet::from([observation.initial_head.clone()]);
        let mut successors = BTreeSet::new();
        for entry in observation.entries {
            validate_entry(&entry)?;
            if !heads.insert(entry.transition.new_state_hash.clone()) {
                return Err(JournalError::Invalid(
                    "journal repeats a committed head".into(),
                ));
            }
            if let Some(predecessor) = &entry.predecessor
                && !successors.insert(predecessor.clone())
            {
                return Err(JournalError::Invalid(
                    "selected journal inventory contains a fork".into(),
                ));
            }
            if inventory.insert(entry.identity.clone(), entry).is_some() {
                return Err(JournalError::Invalid(
                    "journal repeats an entry identity".into(),
                ));
            }
        }

        let mut selected = observation.head_entry;
        let mut entries = Vec::with_capacity(inventory.len());
        loop {
            let entry = inventory.remove(&selected).ok_or_else(|| {
                JournalError::Invalid(format!(
                    "journal predecessor {selected} is missing or cyclic"
                ))
            })?;
            let previous = entry.predecessor.clone();
            match &previous {
                Some(identity) => {
                    let predecessor = inventory.get(identity).ok_or_else(|| {
                        JournalError::Invalid(format!(
                            "journal predecessor {identity} is missing or cyclic"
                        ))
                    })?;
                    if entry.transition.prev_state_hash != predecessor.transition.new_state_hash {
                        return Err(JournalError::Invalid(
                            "journal predecessor edge disagrees with its hash link".into(),
                        ));
                    }
                }
                None if entry.transition.prev_state_hash != observation.initial_head => {
                    return Err(JournalError::Invalid(
                        "journal genesis does not extend its declared initial head".into(),
                    ));
                }
                None => {}
            }
            entries.push(entry);
            match previous {
                Some(previous) => selected = previous,
                None => break,
            }
        }
        if !inventory.is_empty() {
            return Err(JournalError::Invalid(
                "selected journal inventory contains disconnected entries".into(),
            ));
        }
        entries.reverse();
        let identities = entries.iter().map(|entry| entry.identity.clone()).collect();
        let mut prefix_hasher = blake3::Hasher::new();
        frame(&mut prefix_hasher, b"domain", b"gmeow-finite-journal-v1");
        frame(
            &mut prefix_hasher,
            b"journal",
            observation.identity.as_bytes(),
        );
        frame(
            &mut prefix_hasher,
            b"enactment",
            observation.enactment.as_bytes(),
        );
        frame(
            &mut prefix_hasher,
            b"genesis",
            observation.initial_head.as_bytes(),
        );
        for entry in &entries {
            fold_entry(&mut prefix_hasher, entry);
        }
        Ok(Self {
            identity: observation.identity,
            enactment: observation.enactment,
            initial_head: observation.initial_head,
            entries,
            identities,
            heads,
            boundary: observation.boundary,
            prefix_hasher,
        })
    }

    /// Append one observed commit atomically after authenticating its continuation.
    /// No event is synthesized, and failed admission leaves this prefix unchanged.
    ///
    /// # Errors
    /// Rejects finalization, a stale predecessor, duplicate identity/head, corrupt
    /// link, or an exhausted journal admission envelope.
    pub fn append(&mut self, entry: ObservedEntry) -> Result<(), JournalError> {
        if self.boundary == JournalBoundary::Finalized {
            return Err(JournalError::Invalid(
                "a finalized journal cannot be extended".into(),
            ));
        }
        if self.entries.len() >= MAX_JOURNAL_ENTRIES {
            return Err(JournalError::AdmissionLimit {
                limit: MAX_JOURNAL_ENTRIES,
            });
        }
        validate_entry(&entry)?;
        let current = self.entries.last().expect("admitted journal is nonempty");
        if entry.predecessor.as_deref() != Some(current.identity.as_str())
            || entry.transition.prev_state_hash != current.transition.new_state_hash
        {
            return Err(JournalError::Invalid(
                "observed commit does not extend the selected journal head".into(),
            ));
        }
        if self.identities.contains(&entry.identity)
            || self.heads.contains(&entry.transition.new_state_hash)
        {
            return Err(JournalError::Invalid(
                "observed commit repeats an existing entry or head".into(),
            ));
        }
        self.identities.insert(entry.identity.clone());
        self.heads.insert(entry.transition.new_state_hash.clone());
        fold_entry(&mut self.prefix_hasher, &entry);
        self.entries.push(entry);
        Ok(())
    }

    /// Apply explicit finalization evidence for exactly the currently observed head.
    /// Repeating the same finalization is idempotent; an unknown head is refused.
    ///
    /// # Errors
    /// Returns an error when the evidence names another prefix.
    pub fn finalize(&mut self, expected_head: &str) -> Result<(), JournalError> {
        if expected_head != self.head_hash() {
            return Err(JournalError::Invalid(
                "finalization evidence names a different journal head".into(),
            ));
        }
        self.boundary = JournalBoundary::Finalized;
        Ok(())
    }

    /// Journal identity, independent of prefix length.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The selected enactment occurrence.
    pub fn enactment(&self) -> &str {
        &self.enactment
    }

    /// The exact declared genesis anchor.
    pub fn initial_head(&self) -> &str {
        &self.initial_head
    }

    /// Authenticated entries in causal order, indexed from zero.
    pub fn entries(&self) -> &[ObservedEntry] {
        &self.entries
    }

    /// The exact current runtime head hash.
    pub fn head_hash(&self) -> &str {
        &self
            .entries
            .last()
            .expect("admitted journal is nonempty")
            .transition
            .new_state_hash
    }

    /// Explicitly selected future boundary.
    pub fn boundary(&self) -> JournalBoundary {
        self.boundary
    }

    /// Content identity of the selected prefix, scope, and finalization evidence.
    pub fn prefix_identity(&self) -> String {
        let mut hasher = self.prefix_hasher.clone();
        frame(
            &mut hasher,
            b"boundary",
            match self.boundary {
                JournalBoundary::Open => b"open",
                JournalBoundary::Finalized => b"finalized",
            },
        );
        hasher.finalize().to_hex().to_string()
    }
}

#[cfg(test)]
mod tests;
