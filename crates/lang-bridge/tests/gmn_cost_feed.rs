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
//!
//! The same generic cross-check also covers the factored qualifier-slot
//! aliases (`m`/`ek`/`bd` — modality, evidentiality-kind, and `@p`-record boundary), since
//! they carry `gmeow:gmnGlyphTokenCost` + `gmeow:gmnCodepoints` on their
//! `gmeow:GmnDictionaryEntry` individuals exactly like a script glyph does. What the generic
//! cross-check does NOT prove is the razor's half-(a) discharge — that each alias is
//! genuinely CHEAPER than the full canonical IRI it dealiases, the reason the dictionary
//! bijection exists at all — so [`qualifier_marker_aliases_cost_less_than_full_iri`] measures
//! that inequality explicitly, per marker.

use gmeow_lang_bridge::gmn_glyph_token_cost;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue, parse_dataset};

const GMN_GLYPH_TOKEN_COST: &str = "https://blackcatinformatics.ca/gmeow/gmnGlyphTokenCost";
const GMN_CODEPOINTS: &str = "https://blackcatinformatics.ca/gmeow/gmnCodepoints";
const QUANTITY_VALUE: &str = "https://blackcatinformatics.ca/math/quantityValue";
const GMN_FORM_DENOTATION: &str = "https://blackcatinformatics.ca/gmeow/gmnFormDenotation";

const LEFT_WHITE_BRACKET: &str = "U+27E6";
const RIGHT_WHITE_BRACKET: &str = "U+27E7";

/// The declared factored qualifier-slot aliases: `(alias, full canonical IRI)` pairs
/// for every marker admitted under razor half (a) — measured cost reduction
/// (`design/LANG-GMN.md`, "The measured token-cost razor"). None is admitted under half (b)
/// (the ambiguity-class discharge implemented by the GMN-1 codec/gate):
/// every marker here pays its way on the measured half alone, so no marker needs a
/// fires-without/absent-with fixture pair.
const QUALIFIER_MARKER_ALIASES: &[(&str, &str)] = &[
    // `m` (modality) — standpoint slice's gmeow:ModalForce.
    (
        "nec",
        "https://blackcatinformatics.ca/gmeow/modalForceNecessary",
    ),
    (
        "act",
        "https://blackcatinformatics.ca/gmeow/modalForceActual",
    ),
    (
        "poss",
        "https://blackcatinformatics.ca/gmeow/modalForcePossible",
    ),
    (
        "cf",
        "https://blackcatinformatics.ca/gmeow/modalForceCounterfactual",
    ),
    // `ek` (evidentiality-kind) — observations slice's gmeow:ObservationMethod.
    (
        "dir",
        "https://blackcatinformatics.ca/gmeow/methodDirectObservation",
    ),
    (
        "inst",
        "https://blackcatinformatics.ca/gmeow/methodInstrumentalReading",
    ),
    (
        "rmt",
        "https://blackcatinformatics.ca/gmeow/methodRemoteSensing",
    ),
    (
        "cmp",
        "https://blackcatinformatics.ca/gmeow/methodComputationalModel",
    ),
    (
        "exj",
        "https://blackcatinformatics.ca/gmeow/methodExpertJudgement",
    ),
    ("srv", "https://blackcatinformatics.ca/gmeow/methodSurvey"),
    (
        "strm",
        "https://blackcatinformatics.ca/gmeow/methodStreaming",
    ),
    // `bd` (boundary, `@p` records only) — logic slice's logic:OccurrentBoundary.
    ("open", "https://blackcatinformatics.ca/logic/Open"),
    ("closed", "https://blackcatinformatics.ca/logic/Closed"),
];

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
        .trim()
        .split(' ')
        .filter(|group| !group.is_empty())
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
        checked >= 18,
        "the token-cost feed cross-checked {checked} glyphs; the worked plane plus the Task-5 \
         qualifier-slot aliases author at least eighteen (5 script glyphs + 13 \
         qualifier-slot dictionary entries)"
    );
}

/// The razor's half-(a) discharge (`design/LANG-GMN.md`, "The measured token-cost razor"):
/// every declared qualifier-slot alias must cost strictly fewer `cl100k_base` tokens
/// than the full canonical IRI it dealiases — the cost the alternative of inlining or
/// separately asserting the full term would pay, and the reason the dictionary bijection
/// exists at all. A marker that failed this inequality would not be paying its way and would
/// have to be justified under half (b) instead (an executable fires-without/absent-with
/// fixture pair tied to a named `lang:Gmn*` failure class, run through the shipped codec/gate)
/// or dropped.
#[test]
fn qualifier_marker_aliases_cost_less_than_full_iri() {
    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        let alias_cost = gmn_glyph_token_cost(alias);
        let iri_cost = gmn_glyph_token_cost(full_iri);
        assert!(
            alias_cost < iri_cost,
            "qualifier-slot alias {alias:?} measures {alias_cost} tokens, which must be \
             strictly cheaper than its full canonical IRI {full_iri:?} ({iri_cost} tokens) — \
             the razor's half-(a) measured-cost-reduction discharge"
        );
    }
}

/// Every declared qualifier-slot alias is actually present in the authored bundle as a
/// `gmeow:GmnDictionaryEntry` binding the expected term to the expected alias string — the
/// razor discharge above is meaningless unless the marker it measures is the one the carrier
/// ships.
#[test]
fn qualifier_marker_aliases_are_authored_dictionary_entries() {
    const DICTIONARY_ENTRY_TERM: &str =
        "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryTerm";
    const DICTIONARY_ENTRY_ALIAS: &str =
        "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryAlias";

    let ds = load_lang_module();
    let term_pred = ds
        .term_id_by_value(&TermValue::iri(DICTIONARY_ENTRY_TERM))
        .expect("gmeow:gmnDictionaryEntryTerm is used in the bundle");
    let alias_pred = ds
        .term_id_by_value(&TermValue::iri(DICTIONARY_ENTRY_ALIAS))
        .expect("gmeow:gmnDictionaryEntryAlias is used in the bundle");

    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        let term_id = ds
            .term_id_by_value(&TermValue::iri(*full_iri))
            .unwrap_or_else(|| panic!("{full_iri} is used in the bundle"));
        let found = ds
            .quads_for_pattern(None, Some(term_pred), Some(term_id), GraphMatch::Any)
            .any(|q| {
                ds.quads_for_pattern(Some(q.s), Some(alias_pred), None, GraphMatch::Any)
                    .any(|a| literal_lexical(&ds, a.o).as_deref() == Some(*alias))
            });
        assert!(
            found,
            "no gmeow:GmnDictionaryEntry binds {full_iri} to the alias {alias:?}"
        );
    }
}

/// The declared closing-the-loop check: the codec must actually EMIT each qualifier
/// marker's authored alias, verbatim, when a GMN-0 quad uses the term the alias
/// dealiases — not merely carry a dictionary entry that happens to agree with it. Reads
/// `gmeow:gmnDictV3` through `GmnDictionary::from_dataset` (the same "compiled carrier"
/// load path the codec's own writer uses — never a hardcoded parallel alias table) and
/// asserts the emitted GMN-1 text contains the exact `<slot>: <alias>` token the
/// dictionary declares, for every declared qualifier marker.
#[test]
fn codec_emits_the_authored_qualifier_marker_aliases_verbatim() {
    use gmeow_lang_bridge::{Gmn0Model, GmnDictionary, gmn1_write};
    use purrdf::RdfDatasetBuilder;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    let ds = load_lang_module();
    let dict = GmnDictionary::from_dataset(&ds).expect("dict-v3 loads from the carrier");

    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri(&format!("{GMEOW}probeSubject"));
        let p = b.intern_iri(&format!("{GMEOW}probePredicate"));
        let o = b.intern_iri(full_iri);
        b.push_quad(s, p, o, None);
        let model = Gmn0Model::from_dataset(&b.freeze().expect("freeze"));
        let doc = gmn1_write(&model, &dict)
            .unwrap_or_else(|e| panic!("codec must write a probe quad for {full_iri}: {e}"));
        assert!(
            doc.text.contains(&format!(": {alias}")),
            "the codec must emit the authored alias {alias:?} verbatim for {full_iri}, got:\n{}",
            doc.text
        );
    }
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
