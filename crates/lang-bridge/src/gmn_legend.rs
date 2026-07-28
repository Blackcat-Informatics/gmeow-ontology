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
}
