// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-arena native term identity. Dense handles follow insertion order and never
//! cross an arena boundary. Rendering is lazy and never participates in lookup;
//! datatype value equality is a separate reasoning operation.

use std::hash::BuildHasher;
use std::sync::OnceLock;

use hashbrown::HashTable;
use purrdf::TermValue;

use crate::display::term_display;
use crate::id::TermId;

/// Fixed-seed lookup hash for string-keyed DAG nodes. Never a persisted identity.
#[inline]
pub fn surface_hash(surface: &str) -> u64 {
    foldhash::fast::FixedState::default().hash_one(surface)
}

/// Borrowed IRI probes share the native dictionary without constructing an owned
/// `TermValue`. Other terms use PurRDF's complete structural Hash implementation.
#[derive(Hash)]
enum Probe<'a> {
    Iri(&'a str),
    Value(&'a TermValue),
}

fn value_hash(value: &TermValue) -> u64 {
    let probe = match value {
        TermValue::Iri(iri) => Probe::Iri(iri),
        _ => Probe::Value(value),
    };
    foldhash::fast::FixedState::default().hash_one(probe)
}

/// A per-arena dictionary keyed by exact native values, with lazy output text.
#[derive(Debug, Clone, Default)]
pub struct TermInterner {
    /// Handles only: keys are borrowed from the single owned term vector.
    by_value: HashTable<TermId>,
    terms: Vec<TermValue>,
    displays: Vec<OnceLock<String>>,
}

impl TermInterner {
    /// A fresh, empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a native value, cloning only when it has not been seen before.
    pub fn intern(&mut self, term: &TermValue) -> TermId {
        let hash = value_hash(term);
        if let Some(&id) = self
            .by_value
            .find(hash, |&id| self.terms[id.index()] == *term)
        {
            return id;
        }
        let id = TermId::from_index(self.terms.len());
        self.terms.push(term.clone());
        self.displays.push(OnceLock::new());
        let terms = &self.terms;
        self.by_value
            .insert_unique(hash, id, |&id| value_hash(&terms[id.index()]));
        id
    }

    /// Look up an exact native value without inserting or rendering it.
    pub fn lookup(&self, term: &TermValue) -> Option<TermId> {
        self.by_value
            .find(value_hash(term), |&id| self.terms[id.index()] == *term)
            .copied()
    }

    /// Probe a borrowed IRI without allocating a term or its display surface.
    pub fn lookup_iri(&self, iri: &str) -> Option<TermId> {
        let hash = foldhash::fast::FixedState::default().hash_one(Probe::Iri(iri));
        self.by_value
            .find(
                hash,
                |&id| matches!(&self.terms[id.index()], TermValue::Iri(value) if value == iri),
            )
            .copied()
    }

    /// Resolve a handle to its native value.
    ///
    /// # Panics
    /// Panics if the handle was not minted by this interner.
    pub fn resolve(&self, id: TermId) -> &TermValue {
        self.terms.get(id.index()).unwrap_or_else(|| {
            panic!("TermId {id:?} was not minted by this interner (len {}): TermIds must never cross interner boundaries", self.terms.len())
        })
    }

    /// Render only at an output boundary, retaining the result for later output.
    ///
    /// # Panics
    /// Panics if the handle was not minted by this interner.
    pub fn display_of(&self, id: TermId) -> &str {
        let term = self.resolve(id);
        self.displays[id.index()].get_or_init(|| term_display(term))
    }

    /// The number of distinct native terms interned.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the dictionary holds no terms.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

#[path = "interner.tests.rs"]
#[cfg(test)]
mod tests;
