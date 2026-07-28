// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The arena's atomic-term dictionary: display surface → dense [`TermId`].
//!
//! # Determinism (non-negotiable)
//!
//! - The interner is **per-arena** (never global): [`TermId`]s are meaningless outside
//!   the interner that minted them, are assigned in insertion order, and are NEVER
//!   serialized or hashed for provenance — the content key is the persistent identity.
//! - Interning is keyed on the [`term_display`](crate::display::term_display) surface, which
//!   preserves the historical dedup semantics **byte-exactly**: two terms are one
//!   `TermId` exactly when their display surfaces are byte-equal (so an `xsd:string`
//!   literal and a lang-less `rdf:langString` literal collapse, and language tags stay
//!   case-sensitive because `term_display` preserves the stored tag verbatim). The N3
//!   surface is NEVER a dedup key — it lowercases language tags.

use std::hash::BuildHasher;

use hashbrown::HashTable;
use purrdf::TermValue;

use crate::display::term_display;
use crate::id::TermId;

/// Fixed-seed hash of a borrowed surface, for every borrowed-key probe in this crate
/// (the atom dictionary's display key and the DAG's content key).
///
/// The seed is fixed (`FixedState::default()`) and never persisted — determinism comes
/// from insertion order and the content key, never from this hash. ONE definition: the
/// dictionary and the DAG probe the same way.
#[inline]
pub fn surface_hash(surface: &str) -> u64 {
    foldhash::fast::FixedState::default().hash_one(surface)
}

/// A per-arena term dictionary: display surface → dense [`TermId`].
///
/// IDs are assigned in insertion order.  The dedup key is the
/// [`term_display`](crate::display::term_display) surface (byte-exact preservation of the
/// historical string-keyed semantics); the FIRST-seen `TermValue` for each surface is
/// the one stored and resolved.
#[derive(Debug, Clone, Default)]
pub struct TermInterner {
    /// Display surface → id, for O(1) intern/lookup.
    ///
    /// A hashbrown [`HashTable`] storing the [`TermId`] ONLY: the display bytes
    /// live once in `displays` (the side arena), so a borrowed-key (`&str`) probe
    /// resolves an entry with a cheap `displays[id.index()]` slice read — the
    /// eq/hash closure NEVER re-renders `term_display`, and `intern` never stores
    /// the display `String` twice.
    by_display: HashTable<TermId>,
    /// First-seen `TermValue` per id, in insertion order (slot = id index).
    terms: Vec<TermValue>,
    /// Cached display surface per id, in lockstep with `terms` — the side arena the
    /// [`by_display`](Self::by_display) probe resolves against.
    displays: Vec<String>,
}

impl TermInterner {
    /// A fresh, empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `term`, returning its id — minting a new insertion-ordered id if
    /// its display surface is new, else the existing id (first-seen value wins).
    pub fn intern(&mut self, term: &TermValue) -> TermId {
        let display = term_display(term);
        let hash = surface_hash(&display);
        // Borrowed-key probe: resolve each candidate id to its display slice in the
        // side arena — no re-render, no owned-key clone.
        let displays = &self.displays;
        if let Some(&id) = self
            .by_display
            .find(hash, |&id| displays[id.index()] == display)
        {
            return id;
        }
        let id = TermId::from_index(self.terms.len());
        self.terms.push(term.clone());
        // Move the display bytes into the side arena ONCE (never stored twice).
        self.displays.push(display);
        let displays = &self.displays;
        self.by_display
            .insert_unique(hash, id, |&id| surface_hash(&displays[id.index()]));
        id
    }

    /// The id of the term with this [`term_display`](crate::display::term_display) surface, if
    /// already interned; never inserts.
    ///
    /// The display surface IS the interner key (see the module doctrine), so a
    /// surface-keyed lookup is the primitive — probes that hold a `TermValue`
    /// pass `&term_display(term)`.
    pub fn lookup(&self, display: &str) -> Option<TermId> {
        let hash = surface_hash(display);
        self.by_display
            .find(hash, |&id| self.displays[id.index()] == display)
            .copied()
    }

    /// The first-seen `TermValue` for `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not minted by this interner — `TermId`s are per-set,
    /// and resolving a foreign id is a programming error, never a data state.
    pub fn resolve(&self, id: TermId) -> &TermValue {
        self.terms.get(id.index()).unwrap_or_else(|| {
            panic!(
                "TermId {id:?} was not minted by this interner (len {}): \
                 TermIds are per-set handles and must never cross interner boundaries",
                self.terms.len()
            )
        })
    }

    /// The cached display surface for `id` (same panic contract as [`Self::resolve`]).
    ///
    /// Hot joins compare the interned [`TermId`] itself. This cached lexical surface
    /// is materialized only when a binding or output crosses back to text, avoiding a
    /// repeated `term_display` call at that boundary.
    pub fn display_of(&self, id: TermId) -> &str {
        self.displays.get(id.index()).unwrap_or_else(|| {
            panic!(
                "TermId {id:?} was not minted by this interner (len {}): \
                 TermIds are per-set handles and must never cross interner boundaries",
                self.displays.len()
            )
        })
    }

    /// The number of distinct terms interned.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the dictionary holds no terms.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::display::RDF_LANG_STRING;

    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

    /// `xsd:string` and `rdf:langString`-without-lang both display as `"a"` — they MUST
    /// collapse to one id (historical string-keyed semantics), first-seen value wins.
    #[test]
    fn interner_dedups_on_display_surface() {
        let mut interner = TermInterner::new();

        let plain = TermValue::simple_literal("a");
        let langless = TermValue::Literal {
            lexical_form: "a".to_owned(),
            datatype: RDF_LANG_STRING.to_owned(),
            language: None,
            direction: None,
        };

        let id_plain = interner.intern(&plain);
        let id_langless = interner.intern(&langless);
        assert_eq!(id_plain, id_langless, "byte-equal surfaces must collapse");
        assert_eq!(interner.len(), 1);

        match interner.resolve(id_plain) {
            TermValue::Literal { datatype, .. } => assert_eq!(datatype, XSD_STRING),
            other => panic!("expected Literal, got {other:?}"),
        }

        let tagged = TermValue::lang_literal("a", "en");
        let id_tagged = interner.intern(&tagged);
        assert_ne!(id_plain, id_tagged);
        assert_eq!(interner.len(), 2);
    }

    /// The language tag's CASE is significant: `term_display` never lowercases it.
    #[test]
    fn interner_lang_tag_case_is_significant() {
        let raw = |lang: &str| TermValue::Literal {
            lexical_form: "a".to_owned(),
            datatype: RDF_LANG_STRING.to_owned(),
            language: Some(lang.to_owned()),
            direction: None,
        };
        let mut interner = TermInterner::new();
        let id_upper = interner.intern(&raw("EN"));
        let id_lower = interner.intern(&raw("en"));
        assert_ne!(id_upper, id_lower, "lang tag case must stay significant");
        assert_eq!(interner.len(), 2);
        assert_eq!(interner.display_of(id_upper), "\"a\"@EN");
        assert_eq!(interner.display_of(id_lower), "\"a\"@en");
    }

    /// `lookup` is a pure probe — it never inserts.
    #[test]
    fn interner_lookup_never_inserts() {
        let mut interner = TermInterner::new();
        let a = TermValue::iri("http://ex/a");
        let a_display = term_display(&a);
        assert_eq!(interner.lookup(&a_display), None);
        assert!(interner.is_empty(), "lookup must not insert");
        let id = interner.intern(&a);
        assert_eq!(interner.lookup(&a_display), Some(id));
        assert_eq!(interner.len(), 1);
    }
}
