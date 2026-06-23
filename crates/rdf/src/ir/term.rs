// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed term identity and interned-term storage for the immutable IR (#819 C1).
//!
//! These types realize the normative C0 identity contract (see
//! `docs/design/819-rdf-ir-dataflow.md`, *Appendix C0*):
//!
//! - [`TermId`] is opaque and **local to one frozen `RdfDataset`** — never
//!   serialized, never merge-stable, never meaningful across datasets (C0.8).
//! - Literal identity is defined by the IR, not a backend (C0.1): the datatype is
//!   always expanded (`xsd:string` / `rdf:langString`), the language tag is
//!   lowercased for the key, base direction participates in identity, and the
//!   lexical spelling is preserved verbatim.
//! - Blank-node scope participates in the interning key (C0.2).
//! - Triple terms are identified structurally by their resolved `(s, p, o)` (C0.3).

use std::num::NonZeroU32;

use crate::RdfTextDirection;

/// The `xsd:string` datatype IRI — the default datatype of a plain literal (C0.1).
pub(crate) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The `rdf:langString` datatype IRI — the default datatype of a language-tagged
/// literal (C0.1).
pub(crate) const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// Opaque term identity, LOCAL to one frozen `RdfDataset`. Deliberately NOT
/// `Serialize`/`Deserialize`, not merge-stable, not meaningful across datasets
/// (C0.8). Any consumer needing a durable identifier MUST resolve the term to its
/// RDF value rather than retaining a `TermId`.
///
/// # Layout (#837 P3a)
///
/// The inner value is a [`NonZeroU32`] holding `dense_index + 1`, so the all-zero
/// bit pattern is free for the [`Option`] niche: `Option<TermId>` is **4 bytes**
/// (not 8), which shrinks [`QuadRow`](crate::ir::dataset) from 20 to 16 — ~20% off
/// the quad table — because the absent-graph slot (`g: Option<TermId>`) no longer
/// needs a discriminant word. `#[repr(transparent)]` keeps the FFI layout a plain
/// `u32`. Id `0` is reserved as the niche sentinel and is never minted. The `+1`
/// offset is confined entirely to [`index`](TermId::index) /
/// [`from_index`](TermId::from_index); every other site addresses terms through
/// those two methods and is offset-agnostic, so allocation order — and therefore
/// the `Ord` sort used at freeze — is preserved exactly.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct TermId(NonZeroU32);

impl TermId {
    /// The dense index this id addresses in the interner's term table.
    ///
    /// Crate-internal: the inner value is private precisely so a `TermId` cannot
    /// be forged or compared across datasets by external code.
    pub(crate) fn index(self) -> usize {
        // The stored value is `index + 1` (id 0 is the niche sentinel), so the
        // dense index is one less. Never underflows: the inner is `>= 1`.
        (self.0.get() - 1) as usize
    }

    /// Construct a `TermId` from a dense table index.
    ///
    /// Crate-internal: only the interner mints ids, in allocation order. Hard-fails
    /// (rather than wrapping) if `index` is `u32::MAX`, since `index + 1` would
    /// overflow the id space — the table is bounded at `u32::MAX - 1` terms.
    pub(crate) fn from_index(index: u32) -> Self {
        let raw = index
            .checked_add(1)
            .expect("term table index exceeds u32::MAX - 1 entries");
        // `raw = index + 1 >= 1`, so the `NonZeroU32` invariant always holds.
        Self(NonZeroU32::new(raw).expect("index + 1 is always >= 1"))
    }
}

// The NonZeroU32 niche is the load-bearing P3a invariant (#837): it is *why*
// `Option<TermId>` — and the `g` graph slot of every quad row — costs no extra
// word. These compile-time assertions fail the build if the niche ever regresses.
const _: () = assert!(std::mem::size_of::<TermId>() == 4);
const _: () = assert!(std::mem::size_of::<Option<TermId>>() == 4);

/// Blank-node scope. Participates in the interning key (C0.2): two blank nodes
/// from different scopes are distinct even with the same label; two blank nodes in
/// the same scope with the same label are the same node. `0` = default/global
/// scope; `> 0` = a per-segment scope assigned by the streaming importer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BlankScope(pub u32);

impl BlankScope {
    /// The default/global blank-node scope.
    pub const DEFAULT: Self = Self(0);

    /// The raw scope ordinal.
    #[inline]
    pub fn ordinal(self) -> u32 {
        self.0
    }

    /// Render a blank node's owned-model label, qualifying it deterministically by
    /// scope so two same-label blanks from DIFFERENT scopes never collapse into one
    /// owned blank for legacy consumers (compat bridge / oxigraph / SHACL).
    ///
    /// The DEFAULT scope keeps the bare label verbatim, so real single-scope data is
    /// byte-unchanged; a non-default scope `n` qualifies as `"{label}.s{n}"` (C0.2).
    #[inline]
    pub fn qualify_label(self, label: &str) -> std::borrow::Cow<'_, str> {
        if self == Self::DEFAULT {
            std::borrow::Cow::Borrowed(label)
        } else {
            std::borrow::Cow::Owned(format!("{label}.s{}", self.0))
        }
    }
}

impl Default for BlankScope {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// An interned literal. The identity key per C0.1: datatype is ALWAYS expanded to
/// an interned IRI [`TermId`]; the language tag is lowercased; base direction is in
/// the key; and the lexical spelling is preserved verbatim.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct InternedLiteral {
    /// The lexical form, byte-for-byte as authored — never canonicalized (C0.1).
    pub lexical_form: Box<str>,
    /// The expanded datatype, always present (`xsd:string` / `rdf:langString`
    /// expanded at intern time), stored as the id of its interned IRI term.
    pub datatype: TermId,
    /// The language tag, lowercased for the identity key (C0.1).
    pub language: Option<Box<str>>,
    /// The RDF 1.2 base direction; distinct directions are distinct literals.
    pub direction: Option<RdfTextDirection>,
}

/// An interned term — the storage form behind a [`TermId`]. Crate-private: the IR
/// exposes terms through resolved views, never this internal representation.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum InternedTerm {
    /// An IRI, by its full string.
    Iri(Box<str>),
    /// A blank node, identified by `(label, scope)` (C0.2).
    Blank { label: Box<str>, scope: BlankScope },
    /// A literal, identified per C0.1.
    Literal(InternedLiteral),
    /// A triple term (RDF 1.2 quoted triple), identified structurally by its
    /// resolved `(s, p, o)` (C0.3).
    Triple { s: TermId, p: TermId, o: TermId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_id_index_round_trips() {
        // `u32::MAX` is no longer a valid index (the stored value is `index + 1`,
        // so the last addressable index is `u32::MAX - 1`).
        for raw in [0u32, 1, 42, u32::MAX - 1] {
            let id = TermId::from_index(raw);
            assert_eq!(id.index(), raw as usize);
        }
    }

    #[test]
    fn term_id_option_uses_the_nonzero_niche() {
        // The whole point of P3a: `Option<TermId>` rides the NonZeroU32 niche.
        assert_eq!(std::mem::size_of::<Option<TermId>>(), 4);
        assert_eq!(std::mem::size_of::<TermId>(), 4);
    }

    #[test]
    #[should_panic(expected = "exceeds u32::MAX - 1")]
    fn term_id_from_index_rejects_u32_max() {
        // `index + 1` would overflow the id space; the mint hard-fails (#837).
        let _ = TermId::from_index(u32::MAX);
    }

    #[test]
    fn blank_scope_default_is_zero() {
        assert_eq!(BlankScope::default(), BlankScope(0));
        assert_eq!(BlankScope::DEFAULT, BlankScope(0));
    }

    #[test]
    fn datatype_constants_are_the_expected_iris() {
        assert_eq!(XSD_STRING, "http://www.w3.org/2001/XMLSchema#string");
        assert_eq!(
            RDF_LANG_STRING,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
        );
    }

    #[test]
    fn interned_literal_equality_includes_direction() {
        let a = InternedLiteral {
            lexical_form: "x".into(),
            datatype: TermId::from_index(0),
            language: None,
            direction: Some(RdfTextDirection::Ltr),
        };
        let mut b = a.clone();
        assert_eq!(a, b);
        b.direction = Some(RdfTextDirection::Rtl);
        assert_ne!(a, b);
    }
}
