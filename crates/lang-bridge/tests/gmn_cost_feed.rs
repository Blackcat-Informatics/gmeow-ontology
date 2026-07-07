// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Cross-check the authored GMN glyph token-cost feed against the measurement, and pin the
//! ⟦·⟧ named-key ruling in the shipped bundle.
//!
//! The lang slice authors, for every GMN glyph, a `math:Quantity` token cost via
//! `gmeow:gmnGlyphTokenCost`. That authored value is only honest if it equals what the
//! crate-side [`gmeow_lang_bridge::gmn_glyph_token_cost`] primitive measures for the glyph's
//! rendering — otherwise the feed the machine-compression sibling consumes has silently
//! drifted from the real cost. This test parses the authored `module.ttl` and asserts the
//! equality glyph by glyph, so a hand-edited cost that diverges from the measurement fails
//! the gate. It also pins the ⟦·⟧ ruling: the denotation brackets, measured to fragment,
//! are dispositioned to the `den` named key and MUST NOT appear as graphemes of the GMN
//! script — the ruling's outcome, observable in the bundle.

use gmeow_lang_bridge::gmn_glyph_token_cost;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue, parse_dataset};

const GMN_GLYPH_TOKEN_COST: &str = "https://blackcatinformatics.ca/gmeow/gmnGlyphTokenCost";
const GMN_CODEPOINTS: &str = "https://blackcatinformatics.ca/gmeow/gmnCodepoints";
const QUANTITY_VALUE: &str = "https://blackcatinformatics.ca/math/quantityValue";
const GMN_FORM_DENOTATION: &str = "https://blackcatinformatics.ca/gmeow/gmnFormDenotation";

const LEFT_WHITE_BRACKET: &str = "U+27E6";
const RIGHT_WHITE_BRACKET: &str = "U+27E7";

/// Load the authored lang-slice `module.ttl` as an RDF dataset.
fn load_lang_module() -> std::sync::Arc<RdfDataset> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../slices/grounding/lang/module.ttl"
    );
    let bytes = std::fs::read(path).expect("lang module.ttl is readable");
    parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses as Turtle")
}

/// Reconstruct the rendered glyph string from a `"U+XXXX"`-style codepoint spelling
/// (space-separated groups for multi-codepoint glyphs).
fn glyph_from_codepoints(spelling: &str) -> String {
    spelling
        .split(' ')
        .map(|group| {
            let hex = group.strip_prefix("U+").expect("canonical U+ prefix");
            let cp = u32::from_str_radix(hex, 16).expect("hex codepoint");
            char::from_u32(cp).expect("valid scalar value")
        })
        .collect()
}

/// The lexical text of a term if it is a literal.
fn literal_lexical(ds: &RdfDataset, id: purrdf::TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
        _ => None,
    }
}

#[test]
fn authored_glyph_cost_matches_measurement() {
    let ds = load_lang_module();
    let cost_pred = ds
        .term_id_by_value(&TermValue::iri(GMN_GLYPH_TOKEN_COST))
        .expect("gmeow:gmnGlyphTokenCost is used in the bundle");
    let codepoints_pred = ds
        .term_id_by_value(&TermValue::iri(GMN_CODEPOINTS))
        .expect("gmeow:gmnCodepoints is used in the bundle");
    let qvalue_pred = ds
        .term_id_by_value(&TermValue::iri(QUANTITY_VALUE))
        .expect("math:quantityValue is used in the bundle");

    // Every glyph carrying a token-cost feed is cross-checked; the feed is non-empty.
    let mut checked = 0usize;
    let cost_quads: Vec<_> = ds
        .quads_for_pattern(None, Some(cost_pred), None, GraphMatch::Any)
        .collect();
    for q in cost_quads {
        let glyph_node = q.s;
        let quantity_node = q.o;

        // The glyph's codepoint-explicit spelling, reconstructed to its rendered string.
        let codepoints = ds
            .quads_for_pattern(
                Some(glyph_node),
                Some(codepoints_pred),
                None,
                GraphMatch::Any,
            )
            .find_map(|c| literal_lexical(&ds, c.o))
            .expect("a glyph carrying a token cost carries its gmeow:gmnCodepoints spelling");
        let glyph = glyph_from_codepoints(&codepoints);

        // The authored token-cost value on the referenced math:Quantity.
        let authored: usize = ds
            .quads_for_pattern(
                Some(quantity_node),
                Some(qvalue_pred),
                None,
                GraphMatch::Any,
            )
            .find_map(|c| literal_lexical(&ds, c.o))
            .expect("the token-cost quantity carries a math:quantityValue")
            .parse()
            .expect("the token-cost value is an integer");

        let measured = gmn_glyph_token_cost(&glyph);
        assert_eq!(
            authored, measured,
            "authored token cost {authored} for glyph {glyph:?} ({codepoints}) diverges from \
             the measured cost {measured}"
        );
        checked += 1;
    }
    assert!(
        checked >= 4,
        "the token-cost feed cross-checked {checked} glyphs; the worked plane authors at least four"
    );
}

#[test]
fn denotation_brackets_are_not_glyphs() {
    let ds = load_lang_module();
    let codepoints_pred = ds
        .term_id_by_value(&TermValue::iri(GMN_CODEPOINTS))
        .expect("gmeow:gmnCodepoints is used in the bundle");

    for q in ds.quads_for_pattern(None, Some(codepoints_pred), None, GraphMatch::Any) {
        if let Some(spelling) = literal_lexical(&ds, q.o) {
            assert!(
                !spelling
                    .split(' ')
                    .any(|g| g == LEFT_WHITE_BRACKET || g == RIGHT_WHITE_BRACKET),
                "⟦·⟧ (U+27E6/U+27E7) must not be a GMN grapheme — the measured ruling disposes \
                 the denotation term to the `den` named key, not the glyph table (found {spelling:?})"
            );
        }
    }

    // The ruling's positive half: the `den` named-key form is present in the bundle.
    assert!(
        ds.term_id_by_value(&TermValue::iri(GMN_FORM_DENOTATION))
            .is_some(),
        "the `den` named-key form (gmeow:gmnFormDenotation) must be authored as the denotation \
         term's disposition"
    );
}
