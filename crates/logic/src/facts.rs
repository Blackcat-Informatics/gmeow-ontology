// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed-fact bridge: dictionary-interned facts on the native substrate.
//!
//! # Doctrine
//!
//! The reasoning core exchanges facts as **typed values**, never as fact-string
//! text: a [`TypedFactSet`] holds [`TypedFact`]s whose arguments are [`TermId`]s
//! into a per-set [`TermInterner`] over native [`TermValue`]s. Nothing in the
//! production reasoning path formats or parses fact strings.
//!
//! The atomic-term dictionary itself ([`TermInterner`]) is the shared term arena's — it
//! moved to [`gmeow_term_arena`] with the DAG it feeds, so a front-end can intern terms
//! without linking this runtime — and is re-exported here, never redefined.
//!
//! # Determinism (non-negotiable)
//!
//! - The interner is **per-set** (never global): `TermId`s are meaningless
//!   outside the set that minted them, are assigned in insertion order, and are
//!   NEVER serialized or hashed for provenance — the provenance recipes in
//!   [`crate::provenance`] stay the single source of truth, fed by `TermValue`.
//! - Interning is keyed on the [`term_display`](gmeow_term_arena::engine::term_display)
//!   surface, which preserves the
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

use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::HashTable;
use purrdf::TermValue;
use sha1::{Digest, Sha1};

/// The shared arena's per-set atomic-term dictionary: display surface → dense
/// [`TermId`]. ONE definition lives in [`gmeow_term_arena::engine`]; this is the
/// crate-wide name the fact set and the store address terms by (greenfield — there is no
/// second, ad-hoc interner here).
pub(crate) use gmeow_term_arena::engine::TermInterner;

/// The engine's branded per-interner term handle.
///
/// `TermId` is the `Term`-branded [`Id`](crate::physical::id::Id): ONE definition of the
/// niche-ID lives in the shared term arena, re-exported through [`crate::physical::id`],
/// and this is the crate-wide name the interner/store address terms by (greenfield —
/// there is no second, ad-hoc `TermId` here).
pub(crate) use crate::physical::id::TermId;

/// The engine's branded per-store predicate-IRI handle.
///
/// `PredId` is the [`Pred`](crate::physical::id::Pred)-branded
/// [`Id`](crate::physical::id::Id) — the dense key a [`crate::physical::store::RelationStore`]
/// / [`crate::rule_ir::FactStore`] addresses relations by instead of the owned predicate
/// `String`.  Interned once at first insert; resolved back to its IRI surface only at
/// emission / diagnostic edges (a sorted sweep resolves each `PredId` to its string and
/// sorts LEXICALLY — never by `PredId` mint order).
pub(crate) use crate::physical::id::PredId;

/// Fixed-seed hash of a surface, for every borrowed-key probe here.
///
/// ONE definition, shared with the term arena's dictionary and DAG probes: the seed is
/// fixed (`FixedState::default()`) and never persisted — determinism comes from
/// insertion order and the sorted commit, never from this hash.
use gmeow_term_arena::engine::surface_hash as display_hash;

/// Fixed-seed hash of a `(predicate, args)` dedup key, for [`TypedFactSet`]'s
/// borrowed-key probe (never clones the key to hash it).
#[inline]
fn fact_key_hash(predicate: &str, args: &[TermId]) -> u64 {
    let mut hasher = foldhash::fast::FixedState::default().build_hasher();
    predicate.hash(&mut hasher);
    args.hash(&mut hasher);
    hasher.finish()
}

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

// ── PredInterner ────────────────────────────────────────────────────────────────

/// A per-store predicate-IRI dictionary: predicate surface → dense [`PredId`].
///
/// The columnar [`crate::physical::store::RelationStore`] and the ternary
/// [`crate::rule_ir::FactStore`] key their relations / predicate buckets by `PredId`
/// (a `Copy` niche integer) instead of an owned predicate `String`, so a lookup /
/// insert never clones the IRI to probe.  IDs are assigned in first-seen insertion
/// order; the surface is resolved back only at emission / diagnostic edges, where the
/// resolved strings are sorted LEXICALLY (never by `PredId` mint order).
///
/// Mirrors [`TermInterner`]'s borrowed-key discipline: the surface bytes live once in
/// `names` (the side arena), and a `&str` probe resolves a candidate id to its slice —
/// the eq/hash closure never re-allocates.
#[derive(Debug, Clone, Default)]
pub(crate) struct PredInterner {
    /// Predicate surface → id, for O(1) intern/lookup (holds the `PredId` only).
    by_name: HashTable<PredId>,
    /// First-seen predicate IRI per id, in insertion order (slot = id index) — the
    /// side arena the [`by_name`](Self::by_name) probe resolves against.
    names: Vec<String>,
}

impl PredInterner {
    /// A fresh, empty predicate interner.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Intern `name`, returning its id — minting a new insertion-ordered id if the
    /// predicate surface is new, else the existing id.
    pub(crate) fn intern(&mut self, name: &str) -> PredId {
        let hash = display_hash(name);
        let names = &self.names;
        if let Some(&id) = self.by_name.find(hash, |&id| names[id.index()] == name) {
            return id;
        }
        let id = PredId::from_index(self.names.len());
        self.names.push(name.to_owned());
        let names = &self.names;
        self.by_name
            .insert_unique(hash, id, |&id| display_hash(&names[id.index()]));
        id
    }

    /// The id of the predicate with this surface, if already interned; never inserts.
    pub(crate) fn lookup(&self, name: &str) -> Option<PredId> {
        let hash = display_hash(name);
        self.by_name
            .find(hash, |&id| self.names[id.index()] == name)
            .copied()
    }

    /// Every interned predicate surface, in mint order (slot order).
    ///
    /// Callers that need a deterministic sweep resolve + sort LEXICALLY; this returns
    /// the raw mint-ordered names, never itself an emission-order source.
    pub(crate) fn names(&self) -> &[String] {
        &self.names
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
    /// Dedup index for O(1) membership on `(predicate, args)`.
    ///
    /// A hashbrown [`HashTable`] holding the ROW INDEX into `facts`: the key
    /// `(predicate, args)` lives once in the fact itself, so a borrowed-key probe
    /// resolves via `facts[i]` and no owned `(String, Vec<TermId>)` key is ever
    /// cloned — an owned key would double every fact's storage.
    keys: HashTable<usize>,
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
        let hash = fact_key_hash(predicate, &args);
        // Borrowed-key membership probe: compare against the stored fact in place,
        // allocating NOTHING on a hit.
        let facts = &self.facts;
        if self
            .keys
            .find(hash, |&i| {
                let f = &facts[i];
                f.predicate == predicate && f.args == args
            })
            .is_some()
        {
            return false;
        }
        // Miss: own the key exactly once, as the fact itself.
        let idx = self.facts.len();
        self.facts.push(TypedFact {
            predicate: predicate.to_owned(),
            args,
        });
        let facts = &self.facts;
        self.keys.insert_unique(hash, idx, |&i| {
            let f = &facts[i];
            fact_key_hash(&f.predicate, &f.args)
        });
        true
    }

    /// Push one world-scoped quad as an arity-3 fact
    /// `predicate(subject, object, world)`; return `true` if newly inserted.
    ///
    /// Blank-node subjects/objects are Skolemized to stable IRIs before
    /// interning. The world is interned as a plain string literal rather than an
    /// IRI term.
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

    /// Whether the set holds no facts.
    pub(crate) fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn term(iri: &str) -> TermValue {
        TermValue::iri(iri)
    }

    // The atomic-term dictionary's own dedup/lookup parity (display-surface collapse,
    // language-tag case significance, non-inserting lookup) is asserted where the
    // dictionary lives — `gmeow_term_arena::interner`. What remains here is what this
    // module owns: insertion order, `TermId` determinism, Skolemization, and quad dedup.

    // ── (1) insertion-order iteration ─────────────────────────────────────────

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

    // ── (2) TermId determinism across identical build sequences ──────────────

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

    // ── (3) push_quad Skolemizes blanks via skolem_iri ────────────────────────

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
        // The world is a plain string literal.
        assert_eq!(
            set.interner().resolve(fact.args[2]),
            &TermValue::simple_literal("http://world/W")
        );
    }

    // ── (4) dedup of an identical pushed quad ─────────────────────────────────

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
