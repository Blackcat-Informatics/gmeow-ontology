// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed-fact bridge: dictionary-interned facts on the native substrate.
//!
//! # Doctrine
//!
//! The reasoning core exchanges facts as **typed values**, never as fact-string
//! text: a [`TypedFactSet`] holds [`TypedFact`]s whose arguments are [`TermId`]s
//! into a per-set [`TermInterner`] over native [`TermValue`]s.  The Nemo adapter
//! is the **sole stringifier** — it renders typed facts to Nemo's ground-fact
//! syntax at the last moment, inside its own module, and decodes every result
//! row back to `TermValue` before it leaves the adapter.  Nothing else in the
//! crate formats or parses fact strings.
//!
//! # Determinism (non-negotiable)
//!
//! - The interner is **per-set** (never global): `TermId`s are meaningless
//!   outside the set that minted them, are assigned in insertion order, and are
//!   NEVER serialized or hashed for provenance — the provenance recipes in
//!   [`crate::provenance`] stay the single source of truth, fed by `TermValue`.
//! - Interning is keyed on the [`term_display`] surface, which preserves the
//!   historical dedup semantics **byte-exactly**: two terms are one `TermId`
//!   exactly when their display surfaces are byte-equal (so an `xsd:string`
//!   literal and a lang-less `rdf:langString` literal collapse, and language
//!   tags stay case-sensitive because `term_display` preserves the stored tag
//!   verbatim).  [`crate::provenance::term_n3`] is NEVER a dedup key — it
//!   lowercases language tags.
//! - Facts iterate in insertion order with O(1) dedup on
//!   `(predicate, args)`, mirroring the columnar store's discipline
//!   ([`crate::physical`]).
//!
//! # Skolemization
//!
//! [`TypedFactSet::push_quad`] Skolemizes blank-node subjects/objects to stable
//! IRIs (`{SKOLEM_PREFIX}{sha1_hex(qualified_label)}`) before interning, so the
//! engine only ever joins over IRIs and literals.  Skolemization is a semantic
//! operation of the bridge, not a codec concern — it lives here.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;

use purrdf::TermValue;
use sha1::{Digest, Sha1};

use crate::provenance::term_display;

// ── Skolemization primitives ─────────────────────────────────────────────────

/// Prefix for Skolem IRIs derived from blank-node identifiers.
///
/// Matches the retired Python oracle: `{NAMESPACE}skolem/{sha1_hex(bnode_id_utf8)}`.
pub(crate) const SKOLEM_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/skolem/";

/// Compute the SHA-1 hex digest of a UTF-8 string — matching the Python recipe
/// `sha1(str(bnode).encode("utf-8")).hexdigest()`.
pub(crate) fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Skolemize a blank-node identifier to a stable IRI string.
pub(crate) fn skolem_iri(bnode_id: &str) -> String {
    format!("{}{}", SKOLEM_PREFIX, sha1_hex(bnode_id))
}

/// Skolemize a term for the engine: blank nodes become stable Skolem IRIs;
/// every other term passes through unchanged (by reference, no clone).
fn skolemize(term: &TermValue) -> std::borrow::Cow<'_, TermValue> {
    match term {
        TermValue::Blank { label, scope } => {
            std::borrow::Cow::Owned(TermValue::Iri(skolem_iri(&scope.qualify_label(label))))
        }
        other => std::borrow::Cow::Borrowed(other),
    }
}

// ── TermId ────────────────────────────────────────────────────────────────────

/// A dense per-set term handle minted by a [`TermInterner`].
///
/// `TermId`s are assigned in insertion order within ONE interner and carry no
/// meaning outside it.  They are never serialized and never hashed for
/// provenance — content-addressed IDs are always minted from the `TermValue`
/// surfaces (see [`crate::provenance`]).
///
/// `NonZeroU32` keeps `Option<TermId>` pointer-width for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TermId(NonZeroU32);

impl TermId {
    /// The zero-based slot index this id addresses in its interner.
    #[inline]
    fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }

    /// Mint the id for zero-based slot `index` (interner-internal).
    #[inline]
    fn from_index(index: usize) -> Self {
        let raw = u32::try_from(index + 1)
            .expect("TermInterner overflow: more than u32::MAX - 1 distinct terms in one set");
        Self(NonZeroU32::new(raw).expect("index + 1 is nonzero by construction"))
    }
}

// ── TermInterner ──────────────────────────────────────────────────────────────

/// A per-set term dictionary: display surface → dense [`TermId`].
///
/// IDs are assigned in insertion order.  The dedup key is the [`term_display`]
/// surface (byte-exact preservation of the historical string-keyed semantics);
/// the FIRST-seen `TermValue` for each surface is the one stored and resolved.
#[derive(Debug, Clone, Default)]
pub(crate) struct TermInterner {
    /// Display surface → id, for O(1) intern/lookup.
    by_display: HashMap<String, TermId>,
    /// First-seen `TermValue` per id, in insertion order (slot = id index).
    terms: Vec<TermValue>,
    /// Cached display surface per id, in lockstep with `terms`.
    displays: Vec<String>,
}

impl TermInterner {
    /// A fresh, empty interner.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Intern `term`, returning its id — minting a new insertion-ordered id if
    /// its display surface is new, else the existing id (first-seen value wins).
    pub(crate) fn intern(&mut self, term: &TermValue) -> TermId {
        let display = term_display(term);
        if let Some(&id) = self.by_display.get(&display) {
            return id;
        }
        let id = TermId::from_index(self.terms.len());
        self.terms.push(term.clone());
        self.displays.push(display.clone());
        self.by_display.insert(display, id);
        id
    }

    /// The id of `term` if it is already interned; never inserts.
    pub(crate) fn lookup(&self, term: &TermValue) -> Option<TermId> {
        self.by_display.get(&term_display(term)).copied()
    }

    /// The first-seen `TermValue` for `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not minted by this interner — `TermId`s are per-set,
    /// and resolving a foreign id is a programming error, never a data state.
    pub(crate) fn resolve(&self, id: TermId) -> &TermValue {
        self.terms.get(id.index()).unwrap_or_else(|| {
            panic!(
                "TermId {id:?} was not minted by this interner (len {}): \
                 TermIds are per-set handles and must never cross interner boundaries",
                self.terms.len()
            )
        })
    }

    /// The cached display surface for `id` (same panic contract as [`Self::resolve`]).
    pub(crate) fn display_of(&self, id: TermId) -> &str {
        self.displays.get(id.index()).unwrap_or_else(|| {
            panic!(
                "TermId {id:?} was not minted by this interner (len {}): \
                 TermIds are per-set handles and must never cross interner boundaries",
                self.displays.len()
            )
        })
    }

    /// The number of distinct terms interned.
    pub(crate) fn len(&self) -> usize {
        self.terms.len()
    }
}

// ── TypedFact ─────────────────────────────────────────────────────────────────

/// One typed fact: a relation name applied to interned term arguments.
///
/// The predicate stays an un-interned IRI `String` deliberately: it is the
/// relation NAME — the key every ruleset, index, and adapter in this crate
/// addresses relations by — not a term occurring in argument position, so
/// interning it would perturb nothing but the deterministic surfaces that key
/// on it.  Do not "fix" this asymmetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypedFact {
    /// The relation name (a full predicate IRI, un-bracketed).
    pub predicate: String,
    /// The argument terms, as ids into the owning set's interner.
    pub args: Vec<TermId>,
}

// ── TypedFactSet ──────────────────────────────────────────────────────────────

/// An insertion-ordered, deduped set of [`TypedFact`]s with its own interner.
///
/// This is the engine-facing EDB form: facts iterate in insertion order, dedup
/// is O(1) on `(predicate, args)`, and the interner travels with the set (ids
/// are meaningless outside it).
#[derive(Debug, Clone, Default)]
pub(crate) struct TypedFactSet {
    /// The set's term dictionary.
    interner: TermInterner,
    /// Facts in insertion order.
    facts: Vec<TypedFact>,
    /// Dedup keys `(predicate, args)` for O(1) membership.
    keys: HashSet<(String, Vec<TermId>)>,
}

impl TypedFactSet {
    /// A fresh, empty set.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Intern `term` into this set's dictionary, returning its id.
    pub(crate) fn intern(&mut self, term: &TermValue) -> TermId {
        self.interner.intern(term)
    }

    /// Push `predicate(args…)` if not already present; return `true` if inserted.
    ///
    /// `args` must be ids minted by THIS set's interner.
    pub(crate) fn push_fact(&mut self, predicate: &str, args: Vec<TermId>) -> bool {
        let key = (predicate.to_owned(), args.clone());
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.insert(key);
        self.facts.push(TypedFact {
            predicate: predicate.to_owned(),
            args,
        });
        true
    }

    /// Push one world-scoped quad as an arity-3 fact
    /// `predicate(subject, object, world)`; return `true` if newly inserted.
    ///
    /// Blank-node subjects/objects are Skolemized to stable IRIs before
    /// interning.  The world is interned as a plain string literal — the same
    /// treatment the Nemo fact encoding gives the world position (a string
    /// constant, not an IRI term).
    pub(crate) fn push_quad(
        &mut self,
        subject: &TermValue,
        predicate_iri: &str,
        object: &TermValue,
        world: &str,
    ) -> bool {
        let s = self.interner.intern(&skolemize(subject));
        let o = self.interner.intern(&skolemize(object));
        let w = self.interner.intern(&TermValue::simple_literal(world));
        self.push_fact(predicate_iri, vec![s, o, w])
    }

    /// The set's interner (for resolving fact arguments).
    pub(crate) fn interner(&self) -> &TermInterner {
        &self.interner
    }

    /// The facts, in insertion order.
    pub(crate) fn facts(&self) -> impl Iterator<Item = &TypedFact> {
        self.facts.iter()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

    fn term(iri: &str) -> TermValue {
        TermValue::iri(iri)
    }

    /// A literal built field-by-field so the language tag case is preserved
    /// exactly as given (the `lang_literal` constructor lowercases it).
    fn raw_lang_literal(lex: &str, lang: &str) -> TermValue {
        TermValue::Literal {
            lexical_form: lex.to_owned(),
            datatype: RDF_LANG_STRING.to_owned(),
            language: Some(lang.to_owned()),
            direction: None,
        }
    }

    // ── (1) display-surface dedup byte-parity ─────────────────────────────────

    #[test]
    fn facts_intern_dedups_on_display_surface() {
        let mut interner = TermInterner::new();

        // xsd:string and rdf:langString-without-lang both display as `"a"` —
        // they MUST collapse to one id (historical string-keyed semantics).
        let plain = TermValue::simple_literal("a");
        let langless = TermValue::Literal {
            lexical_form: "a".to_owned(),
            datatype: RDF_LANG_STRING.to_owned(),
            language: None,
            direction: None,
        };
        assert_eq!(term_display(&plain), "\"a\"");
        assert_eq!(term_display(&langless), "\"a\"");

        let id_plain = interner.intern(&plain);
        let id_langless = interner.intern(&langless);
        assert_eq!(id_plain, id_langless, "byte-equal surfaces must collapse");
        assert_eq!(interner.len(), 1);

        // First-seen value wins: the stored term is the xsd:string literal.
        match interner.resolve(id_plain) {
            TermValue::Literal { datatype, .. } => assert_eq!(datatype, XSD_STRING),
            other => panic!("expected Literal, got {other:?}"),
        }

        // A lang-TAGGED literal displays with its tag and stays distinct.
        let tagged = TermValue::lang_literal("a", "en");
        assert_eq!(term_display(&tagged), "\"a\"@en");
        let id_tagged = interner.intern(&tagged);
        assert_ne!(id_plain, id_tagged);
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn facts_lang_tag_case_is_significant() {
        // term_display preserves the stored language tag verbatim (it never
        // lowercases), so `"a"@EN` and `"a"@en` are DISTINCT dedup surfaces.
        let upper = raw_lang_literal("a", "EN");
        let lower = raw_lang_literal("a", "en");
        assert_eq!(term_display(&upper), "\"a\"@EN");
        assert_eq!(term_display(&lower), "\"a\"@en");

        let mut interner = TermInterner::new();
        let id_upper = interner.intern(&upper);
        let id_lower = interner.intern(&lower);
        assert_ne!(id_upper, id_lower, "lang tag case must stay significant");
        assert_eq!(interner.len(), 2);
        assert_eq!(interner.display_of(id_upper), "\"a\"@EN");
        assert_eq!(interner.display_of(id_lower), "\"a\"@en");
    }

    #[test]
    fn facts_lookup_never_inserts() {
        let mut interner = TermInterner::new();
        let a = term("http://ex/a");
        assert_eq!(interner.lookup(&a), None);
        assert_eq!(interner.len(), 0, "lookup must not insert");
        let id = interner.intern(&a);
        assert_eq!(interner.lookup(&a), Some(id));
        assert_eq!(interner.len(), 1);
    }

    // ── (2) insertion-order iteration ─────────────────────────────────────────

    #[test]
    fn facts_iterate_in_insertion_order() {
        let mut set = TypedFactSet::new();
        set.push_quad(
            &term("http://ex/a"),
            "http://ex/knows",
            &term("http://ex/b"),
            "w",
        );
        set.push_quad(
            &term("http://ex/a"),
            "http://ex/knows",
            &term("http://ex/c"),
            "w",
        );
        set.push_quad(
            &term("http://ex/b"),
            "http://ex/likes",
            &term("http://ex/c"),
            "w",
        );

        let rendered: Vec<String> = set
            .facts()
            .map(|f| {
                let args: Vec<&str> = f
                    .args
                    .iter()
                    .map(|&id| set.interner().display_of(id))
                    .collect();
                format!("{}({})", f.predicate, args.join(", "))
            })
            .collect();
        assert_eq!(
            rendered,
            vec![
                "http://ex/knows(<http://ex/a>, <http://ex/b>, \"w\")",
                "http://ex/knows(<http://ex/a>, <http://ex/c>, \"w\")",
                "http://ex/likes(<http://ex/b>, <http://ex/c>, \"w\")",
            ],
        );
    }

    // ── (3) TermId determinism across identical build sequences ──────────────

    #[test]
    fn facts_term_ids_deterministic_across_identical_builds() {
        let build = || {
            let mut set = TypedFactSet::new();
            set.push_quad(
                &term("http://ex/a"),
                "http://ex/knows",
                &TermValue::lang_literal("hi", "en"),
                "http://world/W",
            );
            set.push_quad(
                &TermValue::blank("b0"),
                "http://ex/knows",
                &term("http://ex/a"),
                "http://world/W",
            );
            set
        };
        let s1 = build();
        let s2 = build();

        let facts1: Vec<&TypedFact> = s1.facts().collect();
        let facts2: Vec<&TypedFact> = s2.facts().collect();
        assert_eq!(facts1, facts2, "identical builds must mint identical ids");
        assert_eq!(s1.interner().len(), s2.interner().len());
        for f in &facts1 {
            for &id in &f.args {
                assert_eq!(
                    s1.interner().display_of(id),
                    s2.interner().display_of(id),
                    "slot contents must match across identical builds"
                );
            }
        }
    }

    // ── (4) push_quad Skolemizes blanks via skolem_iri ────────────────────────

    #[test]
    fn facts_sha1_hex_matches_python_recipe_shape() {
        // sha1(b"...").hexdigest(): 40 lowercase hex chars.
        let h = sha1_hex("b0");
        assert_eq!(h.len(), 40, "SHA1 hex must be 40 characters");
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA1 hex must be hex"
        );
        assert_eq!(skolem_iri("b0"), format!("{SKOLEM_PREFIX}{h}"));
    }

    #[test]
    fn facts_push_quad_skolemizes_blank_terms() {
        let mut set = TypedFactSet::new();
        assert!(set.push_quad(
            &TermValue::blank("b0"),
            "http://ex/knows",
            &TermValue::blank("b1"),
            "http://world/W",
        ));

        let fact = set.facts().next().expect("one fact pushed");
        assert_eq!(fact.args.len(), 3);
        // Subject/object are the stable Skolem IRIs (default scope keeps the
        // bare label, so the digests are over "b0"/"b1" exactly).
        assert_eq!(
            set.interner().resolve(fact.args[0]),
            &TermValue::Iri(skolem_iri("b0"))
        );
        assert_eq!(
            set.interner().resolve(fact.args[1]),
            &TermValue::Iri(skolem_iri("b1"))
        );
        // The world is a plain string literal (Nemo string-constant treatment).
        assert_eq!(
            set.interner().resolve(fact.args[2]),
            &TermValue::simple_literal("http://world/W")
        );
    }

    // ── (5) dedup of an identical pushed quad ─────────────────────────────────

    #[test]
    fn facts_push_quad_dedups_identical_quad() {
        let mut set = TypedFactSet::new();
        let a = term("http://ex/a");
        let b = term("http://ex/b");
        assert!(set.push_quad(&a, "http://ex/knows", &b, "http://world/W"));
        assert!(
            !set.push_quad(&a, "http://ex/knows", &b, "http://world/W"),
            "identical quad must dedup"
        );
        assert_eq!(set.facts().count(), 1);
        assert_eq!(set.interner().len(), 3);

        // Same triple in a DIFFERENT world is a distinct fact.
        assert!(set.push_quad(&a, "http://ex/knows", &b, "http://world/X"));
        assert_eq!(set.facts().count(), 2);
    }
}
