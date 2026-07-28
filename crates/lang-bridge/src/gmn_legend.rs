// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 **glyph legend**: the codebook's glyph inventory joined to each glyph's real
//! LLM-token cost.
//!
//! Two machine primitives make up the symbology plane's public face — WHICH glyphs the
//! codec may emit ([`GmnGlyphRegistry::glyph_tokens`](crate::GmnGlyphRegistry::glyph_tokens))
//! and WHAT each one costs on the token channel — and the legend is their join. It is the
//! surface a reader (human or agent) consults to learn the notation's alphabet before
//! reading a GMN-1 document.
//!
//! # Why it lives here and not in a shim
//!
//! The legend used to be composed inside `gmeow-gmn-wasm`, the browser shim: the pinned
//! cost table, the JSON shape, and the ordering all lived next to the `wasm_bindgen`
//! export. That made the docs widget the only consumer that could have one. The MCP tool
//! surface needs the identical legend, and it may not depend on a `cdylib` wasm shim — so
//! the composition moved HERE, into the crate that already owns both primitives, and the
//! shim became the thin marshal it always claimed to be. One implementation, two callers;
//! there is no second copy to drift.
//!
//! # Why the costs are pinned rather than measured
//!
//! [`gmn_glyph_token_cost`](crate::gmn_symbology::gmn_glyph_token_cost) measures the real
//! `cl100k_base` BPE cost, but it embeds a ~1.7 MB vocabulary that a wasm image cannot
//! afford — which is exactly why the shim takes this crate with `default-features =
//! false`, dropping the `glyph-cost` feature. [`GLYPH_TOKEN_COSTS`] therefore pins each
//! value, and this module is deliberately NOT feature-gated so a `glyph-cost`-free build
//! still gets a complete legend. The pin is kept honest by
//! [`assert_pinned_costs_match_the_real_bpe`], which is gated on the feature and asserts
//! the table equals the measured cost for EVERY glyph in a live registry (and carries no
//! stale entry). Wasm reads the pinned cost; native cross-checks it.
//!
//! No-optionality: a glyph the registry carries but the table does not price is a HARD
//! FAIL ([`crate::error::GmnUnpinnedGlyphCost`]) — never a silent zero and never a dropped
//! row.

use gmeow_errors::Diag;

use crate::error::GmnUnpinnedGlyphCost;
use crate::gmn1_codec::GmnGlyphRegistry;

/// The pinned real token cost of every glyph the codec may emit — each glyph's
/// `cl100k_base` BPE cost, the exact value
/// [`gmn_glyph_token_cost`](crate::gmn_symbology::gmn_glyph_token_cost) returns natively.
///
/// See the module docs for why this is pinned rather than measured, and
/// [`assert_pinned_costs_match_the_real_bpe`] for the anti-rot gate that keeps it true.
pub const GLYPH_TOKEN_COSTS: &[(&str, usize)] = &[
    ("*", 1),
    ("+", 1),
    ("^", 1),
    ("¬", 1),
    ("×", 1),
    ("γ", 1),
    ("π", 1),
    ("→", 1),
    ("⊑", 3),
];

/// The pinned token cost of `glyph`.
///
/// # Errors
///
/// [`GmnUnpinnedGlyphCost`] when `glyph` has no entry in [`GLYPH_TOKEN_COSTS`]. A missing
/// entry is a HARD FAIL, never a silent zero: the legend must carry every glyph's real
/// cost.
pub fn pinned_glyph_token_cost(glyph: &str) -> Result<usize, Diag> {
    GLYPH_TOKEN_COSTS
        .iter()
        .find(|(g, _)| *g == glyph)
        .map(|(_, cost)| *cost)
        .ok_or_else(|| {
            Diag::of_kind(GmnUnpinnedGlyphCost {
                glyph: glyph.to_owned(),
            })
        })
}

/// The pinned real token cost of every token the **GMN symbol audit** weighs — each
/// candidate glyph and each ASCII fallback the audited `gmeow:GmnSymbolCandidate`
/// inventory carries, at its exact `cl100k_base` BPE cost.
///
/// A SUPERSET of [`GLYPH_TOKEN_COSTS`], and a different question. That table prices the
/// glyphs the *codec may emit* — the shipped legend a reader consults. This one prices
/// both sides of the *comparison the audit makes*: a candidate glyph is only admissible
/// on the token-cost basis when it costs no more than the ASCII fallback it replaces, so
/// the fallback (`"is_subclass_of"`, `"mutual_information"`, …) must be priced too, and
/// those are ordinary words that no glyph registry carries.
///
/// Pinned for the same reason [`GLYPH_TOKEN_COSTS`] is: the audit runs inside
/// `gmeow-slice-quality`, which the browser reasoning segment links, and measuring would
/// drag `tiktoken-rs`' ~1.7 MB `cl100k_base` vocabulary into that image. The axis keeps
/// measuring exactly what it measured before — the numbers are the real BPE's, and
/// [`assert_pinned_audit_costs_match_the_real_bpe`] proves it on native.
///
/// No-optionality: a token the audit compares but this table does not price is a HARD
/// FAIL ([`crate::error::GmnUnpinnedGlyphCost`]) — never a silent zero, which would
/// invert the comparison and score an expensive glyph as a free win.
pub const GMN_SYMBOL_AUDIT_TOKEN_COSTS: &[(&str, usize)] = &[
    ("(", 1),
    ("*", 1),
    ("+", 1),
    ("+∞", 3),
    ("-", 1),
    ("<", 1),
    (">", 1),
    ("[", 1),
    ("^", 1),
    ("~", 1),
    ("¬", 1),
    ("·", 1),
    ("×", 1),
    ("÷", 2),
    ("ˈ", 2),
    ("→", 1),
    ("↔", 2),
    ("⇒", 3),
    ("⇝", 3),
    ("∀", 2),
    ("∁", 2),
    ("∂", 2),
    ("∃", 2),
    ("∈", 2),
    ("−", 1),
    ("−∞", 3),
    ("∖", 2),
    ("∘", 2),
    ("∧", 2),
    ("∨", 2),
    ("∩", 2),
    ("∪", 2),
    ("≤", 2),
    ("≥", 2),
    ("⊂", 3),
    ("⊃", 3),
    ("⊆", 3),
    ("⊇", 3),
    ("⊑", 3),
    ("⊕", 3),
    ("⊘", 3),
    ("⊛", 3),
    ("⌟", 3),
    ("⟦·⟧", 5),
    ("add", 1),
    ("æ", 1),
    ("amount", 1),
    ("and", 1),
    ("boundary", 1),
    ("ℂ", 2),
    ("closed_endpoint", 2),
    ("coboundary", 2),
    ("complement", 2),
    ("compose", 1),
    ("cplx", 2),
    ("cross", 1),
    ("curr", 1),
    ("D", 1),
    ("den", 1),
    ("dimvec(L=1,M=1,T=-2)", 12),
    ("div", 1),
    ("ds", 1),
    ("entropy", 1),
    ("ex", 1),
    ("extreal", 2),
    ("fa", 1),
    ("gamma", 1),
    ("geq", 2),
    ("gp", 1),
    ("grade", 1),
    ("gt", 1),
    ("H", 1),
    ("hlap", 2),
    ("I", 1),
    ("iff", 1),
    ("in", 1),
    ("int", 1),
    ("inter", 1),
    ("ipa_ae", 3),
    ("ipa_k", 2),
    ("ipa_t", 2),
    ("is_cplx", 3),
    ("is_int", 2),
    ("is_nat", 2),
    ("is_rat", 2),
    ("is_real", 2),
    ("is_subclass_of", 4),
    ("J", 1),
    ("k", 1),
    ("⟨·⟩ₖ", 6),
    ("kl_divergence", 4),
    ("L", 1),
    ("L¹M¹T⁻²", 8),
    ("lcon", 2),
    ("len", 1),
    ("leq", 2),
    ("lt", 1),
    ("lumin", 2),
    ("M", 1),
    ("mass", 1),
    ("mb", 1),
    ("measure", 1),
    ("mul", 1),
    ("mutual_information", 3),
    ("N", 1),
    ("ℕ", 2),
    ("nat", 1),
    ("neg", 1),
    ("neginf", 3),
    ("nerve", 2),
    ("not", 1),
    ("nt", 1),
    ("open_endpoint", 2),
    ("or", 1),
    ("pi", 1),
    ("posinf", 2),
    ("pow", 1),
    ("primary_stress", 3),
    ("propsubset", 3),
    ("propsupset", 3),
    ("ℚ", 2),
    ("ℝ", 2),
    ("ℝ̄", 4),
    ("rat", 1),
    ("real", 1),
    ("rev", 1),
    ("setminus", 2),
    ("sheaf_laplacian", 6),
    ("sub", 1),
    ("subseteq", 1),
    ("supseteq", 3),
    ("T", 1),
    ("t", 1),
    ("temp", 1),
    ("time", 1),
    ("to", 1),
    ("ungrammatical", 4),
    ("union", 1),
    ("vsa_bind", 3),
    ("vsa_bundle", 3),
    ("vsa_unbind", 4),
    ("wedge", 2),
    ("xl", 1),
    ("ℤ", 2),
    ("γ", 1),
    ("Δ", 2),
    ("δ", 1),
    ("Θ", 2),
    ("μ", 1),
    ("π", 1),
];

/// The pinned token cost of `token` for the GMN symbol audit.
///
/// # Errors
///
/// [`GmnUnpinnedGlyphCost`] when `token` has no entry in
/// [`GMN_SYMBOL_AUDIT_TOKEN_COSTS`]. A missing entry is a HARD FAIL, never a silent zero:
/// the audit compares two costs, and a zero would fabricate a token win.
pub fn pinned_symbol_audit_token_cost(token: &str) -> Result<usize, Diag> {
    GMN_SYMBOL_AUDIT_TOKEN_COSTS
        .iter()
        .find(|(t, _)| *t == token)
        .map(|(_, cost)| *cost)
        .ok_or_else(|| {
            Diag::of_kind(GmnUnpinnedGlyphCost {
                glyph: token.to_owned(),
            })
        })
}

/// The GMN-1 glyph legend for `registry`, as a deterministic JSON array of
/// `{ "glyph": <token>, "tokenCost": <n> }`.
///
/// The row order is [`GmnGlyphRegistry::glyph_tokens`]'s order (longest-match first, then
/// lexicographic) — already deterministic, because the registry is `BTreeMap`-backed — so
/// two calls over the same codebook produce byte-identical text. Glyph tokens are
/// JSON-escaped defensively even though no current glyph needs it.
///
/// The JSON is assembled by hand rather than through `serde_json`: this crate ships no
/// JSON encoder in its normal dependency set, and the shape is two scalar fields.
///
/// # Errors
///
/// [`GmnUnpinnedGlyphCost`] when the registry carries a glyph [`GLYPH_TOKEN_COSTS`] does
/// not price.
pub fn glyph_legend_json(registry: &GmnGlyphRegistry) -> Result<String, Diag> {
    let mut out = String::from("[");
    for (i, glyph) in registry.glyph_tokens().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let escaped = glyph.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!(
            "{{\"glyph\":\"{escaped}\",\"tokenCost\":{}}}",
            pinned_glyph_token_cost(glyph)?
        ));
    }
    out.push(']');
    Ok(out)
}

/// The anti-rot gate for [`GLYPH_TOKEN_COSTS`]: assert the pinned table prices EVERY glyph
/// `registry` carries with the value the real `cl100k_base` BPE reports, and carries no
/// stale entry the registry has dropped.
///
/// Exposed as a callable assertion rather than a `#[cfg(test)]` item because the caller
/// that HAS a codebook to build a registry from is not this crate — a `#[cfg(test)]` item
/// is invisible across a crate boundary, so the shim that embeds the codebook could not
/// reach it. Gated on `glyph-cost` because it is the only thing here that needs the
/// embedded tokenizer vocabulary.
///
/// # Panics
///
/// Panics (this is a test assertion) when a registry glyph is unpinned, when a pinned cost
/// disagrees with the measured BPE cost, or when the table prices a glyph the registry no
/// longer carries.
#[cfg(feature = "glyph-cost")]
pub fn assert_pinned_costs_match_the_real_bpe(registry: &GmnGlyphRegistry) {
    let glyphs = registry.glyph_tokens();
    assert!(
        !glyphs.is_empty(),
        "the codebook registry must carry at least one glyph for the pin to mean anything"
    );
    for glyph in &glyphs {
        let pinned = pinned_glyph_token_cost(glyph).unwrap_or_else(|e| {
            panic!("glyph {glyph:?} is in the registry but not in GLYPH_TOKEN_COSTS: {e}")
        });
        let real = crate::gmn_symbology::gmn_glyph_token_cost(glyph);
        assert_eq!(
            pinned, real,
            "pinned cost for glyph {glyph:?} is {pinned} but the real cl100k_base cost is \
             {real} — re-pin GLYPH_TOKEN_COSTS"
        );
    }
    for (glyph, _) in GLYPH_TOKEN_COSTS {
        assert!(
            glyphs.contains(glyph),
            "GLYPH_TOKEN_COSTS prices {glyph:?}, which the codebook registry no longer \
             carries — drop the stale entry"
        );
    }
}

/// The anti-rot gate for [`GMN_SYMBOL_AUDIT_TOKEN_COSTS`]: assert every pinned entry
/// equals the value the real `cl100k_base` BPE reports, and that `live` — the token
/// inventory the caller actually audits — is fully priced.
///
/// The twin of [`assert_pinned_costs_match_the_real_bpe`], and exposed for the same
/// reason: the crate that HAS the live inventory is `gmeow-slice-quality`, which reads it
/// out of a bundle graph, so a `#[cfg(test)]` item here would be invisible to it. Gated on
/// `glyph-cost`, so the vocabulary is reachable only from a native test lane.
///
/// # Panics
///
/// Panics (this is a test assertion) when a pinned cost disagrees with the measured BPE
/// cost, or when `live` names a token the table does not price.
#[cfg(feature = "glyph-cost")]
pub fn assert_pinned_audit_costs_match_the_real_bpe(live: &[&str]) {
    for (token, pinned) in GMN_SYMBOL_AUDIT_TOKEN_COSTS {
        let real = crate::gmn_symbology::gmn_glyph_token_cost(token);
        assert_eq!(
            *pinned, real,
            "pinned audit cost for token {token:?} is {pinned} but the real cl100k_base \
             cost is {real} — re-pin GMN_SYMBOL_AUDIT_TOKEN_COSTS"
        );
    }
    for token in live {
        pinned_symbol_audit_token_cost(token).unwrap_or_else(|e| {
            panic!(
                "the audited symbol inventory carries {token:?}, which \
                 GMN_SYMBOL_AUDIT_TOKEN_COSTS does not price: {e}"
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unpinned_glyph_is_a_named_hard_error() {
        let err = pinned_glyph_token_cost("⊗").expect_err("an unpinned glyph must not price");
        assert_eq!(err.code(), GmnUnpinnedGlyphCost::register());
        assert!(err.to_string().contains('⊗'), "{err}");
    }

    #[test]
    fn every_pinned_cost_is_positive() {
        for (glyph, cost) in GLYPH_TOKEN_COSTS {
            assert!(*cost > 0, "glyph {glyph:?} is pinned at a zero token cost");
        }
    }

    #[test]
    fn the_pinned_table_carries_no_duplicate_glyph() {
        let mut seen: Vec<&str> = GLYPH_TOKEN_COSTS.iter().map(|(g, _)| *g).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len(), "GLYPH_TOKEN_COSTS repeats a glyph");
    }

    #[test]
    fn an_unpinned_audit_token_is_a_named_hard_error() {
        let err = pinned_symbol_audit_token_cost("⊗")
            .expect_err("a token outside the audited inventory must not price");
        assert_eq!(err.code(), GmnUnpinnedGlyphCost::register());
        assert!(err.to_string().contains('⊗'), "{err}");
    }

    #[test]
    fn every_pinned_audit_cost_is_positive() {
        for (token, cost) in GMN_SYMBOL_AUDIT_TOKEN_COSTS {
            assert!(*cost > 0, "token {token:?} is pinned at a zero token cost");
        }
    }

    #[test]
    fn the_pinned_audit_table_carries_no_duplicate_token() {
        let mut seen: Vec<&str> = GMN_SYMBOL_AUDIT_TOKEN_COSTS
            .iter()
            .map(|(t, _)| *t)
            .collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            before,
            seen.len(),
            "GMN_SYMBOL_AUDIT_TOKEN_COSTS repeats a token"
        );
    }

    /// The audit table must be a genuine SUPERSET of the shipped codec legend: the two
    /// answer different questions, but every glyph the codec may emit is also a glyph the
    /// audit can be asked to weigh, and the two tables must never disagree about a cost.
    #[test]
    fn the_audit_table_is_a_consistent_superset_of_the_codec_legend() {
        for (glyph, cost) in GLYPH_TOKEN_COSTS {
            let audited = pinned_symbol_audit_token_cost(glyph).unwrap_or_else(|e| {
                panic!("GLYPH_TOKEN_COSTS prices {glyph:?} but the audit table does not: {e}")
            });
            assert_eq!(
                *cost, audited,
                "the two pinned tables disagree about {glyph:?}"
            );
        }
    }
}
