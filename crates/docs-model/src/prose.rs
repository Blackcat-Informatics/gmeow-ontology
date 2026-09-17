// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SINGLE definition of GMEOW's deterministic authored-prose predicates.
//!
//! Two consumers read DIFFERENT inputs but must apply the SAME predicate:
//!
//! - [`crate::coverage`] scores `dimProseQuality` over the typed [`crate::model::DocsModel`]
//!   (feeding `axisDocMaturity`);
//! - `gmeow_slice_quality::axes::prose_axis` scores `axisProseQuality` over the raw
//!   slice RDF dataset.
//!
//! Those two INPUT sets legitimately differ (see the module doc on
//! [`crate::coverage`]); the PREDICATE must not. It lived twice, and the copies
//! drifted — the slice-quality copy grew a boilerplate carve-out and an
//! `rdfs:isDefinedBy` guard the docs copy never got, so the same definition could
//! be "boundary-stating" for one score and not the other. This module is the one
//! definition; the STRICT semantics is the definition (a laxer twin behind a flag
//! would just be the drift again, opt-in).
//!
//! Every predicate here is a DETERMINISTIC structural fact — a pure function of a
//! string, present/absent, never a corpus-tuned threshold and never a reasoner.
//! Each is deliberately CONSERVATIVE: it prefers a false negative to a false
//! positive, because every caller feeds a ratchet-gated score where a false
//! positive silently inflates the tier while a false negative only under-credits.

/// True if `word` occurs in `corpus` at identifier/word boundaries — the char on
/// each side is neither an ASCII alphanumeric nor `_`/`-`.
///
/// Keeps an INCIDENTAL substring (`"whenever"` containing `"never"`, `"NOTE"`
/// containing `"not"`, `FooBar` containing `Foo`) from counting as a real
/// occurrence. Phrase words (e.g. `"rather than"`) match as a contiguous span,
/// their outer ends boundary-checked.
#[must_use]
pub fn word_at_boundary(corpus: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    corpus.match_indices(word).any(|(idx, _)| {
        let before = corpus[..idx].chars().next_back();
        let after = corpus[idx + word.len()..].chars().next();
        before.is_none_or(|c| !is_ident(c)) && after.is_none_or(|c| !is_ident(c))
    })
}

/// The term-agnostic definitional coat that says nothing distinguishing.
///
/// This exact family was mechanically appended to hundreds of definitions; it
/// contains the cue `"not"` yet draws no boundary between THIS term and any other,
/// so crediting it would let a bulk edit inflate a ratchet-gated score corpus-wide.
const BOILERPLATE_COAT: &str =
    "not an interchangeable alias for a broader, narrower, or merely related construct";

/// Negation cues that signal a boundary-stating ("what it is NOT") definition.
const BOUNDARY_CUES: &[&str] = &[
    "not",
    "never",
    "nor",
    "cannot",
    "rather than",
    "as opposed to",
    "instead of",
    "unlike",
    "distinct from",
];

/// True if a definition states a boundary ("what it is NOT") via a negation cue,
/// matched at word boundaries ([`word_at_boundary`]) on the lowercased text, once
/// the term-agnostic boilerplate coat ([`BOILERPLATE_COAT`]) is removed.
///
/// The coat is EXCISED from the text before scanning, not treated as a
/// whole-definition veto: it can neither credit the axis (its own cues are gone
/// with it) nor deny credit to a genuine boundary cue phrased elsewhere in the
/// same definition.
#[must_use]
pub fn states_boundary(def: &str) -> bool {
    let d = def.to_lowercase().replace(BOILERPLATE_COAT, " ");
    BOUNDARY_CUES.iter().any(|cue| word_at_boundary(&d, cue))
}

/// True if `s` carries a turtle CURIE token (`prefix:local`): a `:` with a name
/// char before it and an alphanumeric/`_` after.
///
/// Deliberately conservative — it rejects a bare prose colon (`"section 3: ..."`)
/// and a full-IRI scheme (`<http://…>`, whose `:` is followed by `/`), so a
/// definition without a real term reference is not mistaken for a worked triple.
#[must_use]
pub fn has_curie(s: &str) -> bool {
    let bytes = s.as_bytes();
    let is_name = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    for (i, &c) in bytes.iter().enumerate() {
        if c != b':' {
            continue;
        }
        let before = i.checked_sub(1).map(|j| bytes[j]);
        let after = bytes.get(i + 1).copied();
        if before.is_some_and(is_name)
            && after.is_some_and(|a| a.is_ascii_alphanumeric() || a == b'_')
        {
            return true;
        }
    }
    false
}

/// A worked triple names a term via a CURIE (`prefix:local`) and carries turtle
/// statement structure (the `a` type keyword or a `; , .` terminator).
///
/// `term rdfs:isDefinedBy slice` is EXCLUDED: it is ownership metadata, not an
/// example of the term in use. Counting it lets a generated provenance inventory
/// pose as hundreds of worked examples.
#[must_use]
pub fn is_worked_triple(example: &str) -> bool {
    !example.contains("rdfs:isDefinedBy")
        && has_curie(example)
        && (word_at_boundary(example, "a")
            || example.contains(" ;")
            || example.contains(" .")
            || example.contains(" ,")
            || example.ends_with('.')
            || example.ends_with(';'))
}

#[path = "prose.tests.rs"]
#[cfg(test)]
mod tests;
