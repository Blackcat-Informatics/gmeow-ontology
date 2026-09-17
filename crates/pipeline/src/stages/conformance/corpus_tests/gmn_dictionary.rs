// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only assertions over the selected producer's dictionary judgments.

use super::super::gmn_dictionary::{CHANNEL, Observations};

pub(super) fn observations() -> &'static Observations {
    static OBSERVATIONS: std::sync::OnceLock<Observations> = std::sync::OnceLock::new();
    OBSERVATIONS.get_or_init(|| {
        let bytes = crate::fixture::authenticated_artifact(
            &gmeow_conformance::paths::repo_root(),
            "stage-conformance",
            CHANNEL,
        )
        .expect("authenticated native dictionary observations");
        serde_json::from_slice(&bytes).expect("typed dictionary observations")
    })
}

#[test]
fn real_dictionary_loads_and_is_injective() {
    let observed = observations();
    assert!(
        observed.aliases.len() >= 30,
        "required authored dictionary inventory"
    );
    for (iri, alias) in [
        (
            "https://blackcatinformatics.ca/gmeow/modalForceNecessary",
            "nec",
        ),
        ("https://blackcatinformatics.ca/lang/Denotation", "den"),
        ("https://blackcatinformatics.ca/math/Division", "div"),
        ("https://blackcatinformatics.ca/logic/forall", "fa"),
        ("https://blackcatinformatics.ca/logic/Open", "open"),
        (
            "https://blackcatinformatics.ca/gmeow/modalForcePossible",
            "poss",
        ),
        (
            "https://blackcatinformatics.ca/gmeow/methodInstrumentalReading",
            "inst",
        ),
    ] {
        assert_eq!(observed.aliases.get(iri).map(String::as_str), Some(alias));
    }
    assert_eq!(
        observed.nec_reverse.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/modalForceNecessary")
    );
}

#[test]
fn out_of_window_envelope_header_is_rejected() {
    let observed = observations();
    assert_eq!(observed.acceptance, observed.resolved_acceptance);
    assert!(
        observed.current_header.is_ok(),
        "{:?}",
        observed.current_header
    );
    let error = observed
        .future_header
        .as_ref()
        .expect_err("future major must be rejected");
    assert_eq!(
        error.failure_class(),
        gmeow_lang_bridge::Gmn1Error::CLASS_UNDECLARED_DIALECT_VERSION
    );
}

#[test]
fn standalone_round_trips_a_ground_reification_over_the_real_dictionary() {
    let observed = observations();
    assert!(observed.per_claim.is_ok(), "{:?}", observed.per_claim);
    let report = observed
        .standalone
        .as_ref()
        .expect("ground reification inverts standalone");
    assert!(
        report.skipped.is_empty(),
        "no claim is blank-shared: {report:?}"
    );
    assert!(
        report
            .checked
            .contains(&"<https://blackcatinformatics.ca/gmeow/reifier1>".to_owned())
    );
}

#[test]
fn every_shipped_denotation_glyph_is_uts39_confusable_free_within_scope() {
    use gmeow_lang_bridge::gmn1_codec::decode_codepoint_sequence;
    use std::collections::BTreeMap;
    use unicode_security::skeleton;

    // These are raw source coordinates, not the dictionary registry's judgment.
    // The independent audit still catches a missed registry collision.
    let glyphs = &observations().glyphs;
    assert!(glyphs.len() >= 5, "required denotation-glyph inventory");
    let mut skeleton_to_glyph = BTreeMap::new();
    let mut audited = 0;
    for (iri, source) in glyphs {
        assert!(
            source.codepoints.len() <= 1,
            "ambiguous codepoints for {iri}"
        );
        assert!(source.scopes.len() <= 1, "ambiguous scope for {iri}");
        let Some(codepoints) = source.codepoints.first() else {
            continue;
        };
        let glyph = decode_codepoint_sequence(codepoints)
            .unwrap_or_else(|error| panic!("shipped glyph {iri} decodes: {}", error.0));
        let scope = source.scopes.first().cloned().unwrap_or_default();
        let key = (scope.clone(), skeleton(&glyph).collect::<String>());
        if let Some(prior) = skeleton_to_glyph.insert(key, glyph.clone()) {
            assert_eq!(
                prior, glyph,
                "shipped glyphs share a UTS #39 skeleton within scope {scope:?}"
            );
        }
        audited += 1;
    }
    assert!(audited >= 5, "required decoded denotation-glyph inventory");
}

#[test]
fn writer_and_reader_cover_all_thirteen_declared_sigils() {
    let observed = observations()
        .records
        .sigils
        .as_ref()
        .expect("all roles write and read");
    for sigil in [
        "@c", "@e", "@s", "@p", "@π", "@d", "@m", "@μ", "@λ", "@ℒ", "@err", "@patch", "@retract",
    ] {
        assert!(
            observed
                .text
                .lines()
                .any(|line| line.starts_with(&format!("{sigil}{{"))),
            "writer did not emit {sigil}: {}",
            observed.text
        );
    }
    assert!(
        observed.canonical_equal,
        "all thirteen sigils read back exactly"
    );
}

#[test]
fn real_writer_uses_scoped_grounding_glyphs_and_wrong_scope_hard_fails() {
    use gmeow_lang_bridge::Gmn1Error;
    let observed = observations()
        .records
        .scoped
        .as_ref()
        .expect("grounding glyphs write and read");
    let text = &observed.written.text;
    for fragment in ["@μ{s: π", "o: +", "@ℒ{", "p: ¬"] {
        assert!(text.contains(fragment), "missing {fragment}: {text}");
    }
    assert!(
        observed.written.canonical_equal,
        "glyph-bearing records round-trip"
    );
    assert_eq!(
        observed.fallback,
        Ok(true),
        "ASCII fallback semantic parity"
    );
    assert!(
        matches!(observed.wrong_scope, Err(Gmn1Error::Uncovered(_))),
        "a math-scoped glyph must not decode in logic scope"
    );
    assert!(
        matches!(observed.wrong_fallback_scope, Err(Gmn1Error::Uncovered(_))),
        "an adopted glyph's ASCII fallback preserves its sigil scope"
    );
}

#[test]
fn coverage_uses_grouped_process_record_context() {
    use gmeow_lang_bridge::{Gmn1ConstructCategory, QuadCoverage};
    let observed = observations()
        .records
        .process
        .as_ref()
        .expect("process record writes and reads");
    let text = &observed.written.text;
    assert!(text.contains("@p{"), "{text}");
    assert!(
        text.contains("o: math__Addition"),
        "process record must not use the math glyph: {text}"
    );
    assert!(!text.contains("o: +"), "{text}");
    assert!(
        matches!(
            observed.primary,
            Some(QuadCoverage::Covered {
                object: Gmn1ConstructCategory::IriPrefixMangled,
                ..
            })
        ),
        "coverage must use the grouped process sigil: {:?}",
        observed.primary
    );
    assert_eq!(
        observed.glyph_count, 0,
        "coverage cannot claim an un-emitted glyph"
    );
    assert!(
        observed.written.canonical_equal,
        "process record round-trips"
    );
}

#[test]
fn resolved_version_provenance_is_single_valued() {
    let observed = &observations().stamps;
    assert_eq!(observed.resolved, observed.acceptance_major);
    assert_eq!(observed.resolved, observed.dictionary_major);
    for record in [
        "https://blackcatinformatics.ca/gmeow/examples/lang/metricRowA",
        "https://blackcatinformatics.ca/gmeow/examples/lang/verbalizationRowB",
    ] {
        let values = observed
            .values
            .get(record)
            .expect("producer observed each record");
        assert_eq!(
            values.len(),
            1,
            "record {record} carries exactly one schema version"
        );
        assert_eq!(
            values[0], observed.resolved,
            "record {record} uses the resolved major"
        );
    }
    assert!(
        observed.repeated_quad_equal,
        "re-stamping yields the identical quad"
    );
}

#[test]
fn all_notation_views_agree_on_one_canonical_tree() {
    let observed = observations()
        .notation
        .as_ref()
        .expect("graph-derived glyph grammar parses");
    assert!(
        observed.production.starts_with("glyphToken ::= '"),
        "nonempty closed glyph production"
    );
    assert!(
        !observed.canonical.is_empty(),
        "at least one glyph production"
    );
    for formalism in ["Ebnf", "Abnf", "Gbnf", "Lark"] {
        let via = observed
            .views
            .get(formalism)
            .expect("every selected notation view")
            .as_ref()
            .expect("notation view reparses");
        assert_eq!(
            via, &observed.canonical,
            "{formalism}: canonical glyph tree agreement"
        );
    }
}
