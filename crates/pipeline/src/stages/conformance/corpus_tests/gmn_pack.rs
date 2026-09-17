// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only contracts over the selected native language projection and its controls.

use std::collections::BTreeSet;

use crate::stages::lang_projection::contract_fixtures;
use crate::stages::lang_projection::gmn_pack::{EmissionWitness, Observations};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const METRICS: &str = "https://blackcatinformatics.ca/gmeow/gmnTokenMetricsCurrent";

fn observations() -> &'static Observations {
    contract_fixtures::pack()
}

fn artifact(suffix: &str) -> &'static str {
    contract_fixtures::product(suffix)
}

fn assert_exact(witness: &EmissionWitness) {
    assert!(witness.is_rdf, "{} is an RDF product", witness.path);
    assert!(
        gmeow_lang_bridge::is_exact_correspondence(&witness.correspondence),
        "{} carries exact correspondence",
        witness.path
    );
    assert!(
        witness.round_trip,
        "{} measures an exact round trip",
        witness.path
    );
    let (get, put) = witness
        .leg_pair
        .as_ref()
        .expect("exact product carries its logical legs");
    assert!(
        gmeow_lang_bridge::exact_round_trip_holds(get, put),
        "{} carries inverse logical legs",
        witness.path
    );
    assert!(
        witness.source_matches_artifact,
        "{} rides the corpus verbatim",
        witness.path
    );
}

fn witness(suffix: &str) -> &'static EmissionWitness {
    let observed = observations();
    let path = format!("gmn1/v{}/{suffix}", observed.major);
    observed
        .emissions
        .iter()
        .find(|emission| emission.path == path)
        .expect("selected production emission witness")
}

fn gmn1_metrics_record_native_values_and_version() {
    let observed = observations();
    assert_exact(witness("token-metrics.ttl"));
    let ttl = artifact("token-metrics.ttl");
    assert!(ttl.contains(&format!("<{METRICS}> <{RDF_TYPE}> <{GMEOW}Measurement>")));
    for metric in [
        "bytes_on_disk",
        "tokens_in_context",
        "ast_validity_rate",
        "roundtrip_loss",
        "compression_ratio",
        "glyph_density",
        "dictionary_hit_rate",
    ] {
        let subject = format!("{METRICS}/{metric}");
        assert!(ttl.contains(&format!(
            "<{subject}> <{RDF_TYPE}> <https://blackcatinformatics.ca/math/Quantity>"
        )));
        assert!(ttl.contains(&format!(
            "<{subject}> <https://blackcatinformatics.ca/math/quantityValue>"
        )));
    }
    assert!(ttl.contains(&format!(
        "<{METRICS}> <{}> ",
        gmeow_lang_bridge::gmn_migrate::PRED_GMN_SCHEMA_VERSION
    )));
    assert!(observed.metrics.measured_sources >= observed.grounding_metrics.measured_sources);
    assert!(observed.metrics.compression_gate_holds);
    for (metric, value) in [
        ("bytes_on_disk", observed.metrics.bytes_on_disk),
        ("tokens_in_context", observed.metrics.tokens_in_context),
        (
            "gmn_worst_case_tokens",
            observed.metrics.gmn_worst_case_tokens,
        ),
        (
            "gmn_realistic_tokens",
            observed.metrics.gmn_realistic_tokens,
        ),
        (
            "turtle_best_case_tokens",
            observed.metrics.turtle_best_case_tokens,
        ),
    ] {
        assert!(ttl.contains(&format!("<{METRICS}/{metric}> <https://blackcatinformatics.ca/math/quantityValue> \"{value}\"")), "native {metric} is the shipped value");
    }
}

fn gmn_beats_turtle_under_grounding_byte_fallback_worst_case() {
    let observed = observations();
    let metrics = &observed.grounding_metrics;
    assert!(metrics.measured_sources > 0);
    for slice in ["lang", "math", "logic"] {
        assert!(
            observed
                .grounding_sources
                .iter()
                .any(|path| path.starts_with(&format!("slices/grounding/{slice}/examples/")))
        );
    }
    assert!(observed.grounding_sources.iter().all(|path| {
        ["lang", "math", "logic"]
            .iter()
            .any(|slice| path.starts_with(&format!("slices/grounding/{slice}/examples/")))
    }));
    assert_eq!(
        metrics.gmn_worst_case_tokens,
        metrics.gmn_ascii_bytes.div_ceil(4) + metrics.gmn_nonascii_bytes
    );
    assert!(
        metrics.bytes_on_disk > metrics.gmn_worst_case_tokens,
        "all-byte fallback is the rejected pessimistic bound"
    );
    assert!(metrics.compression_gate_holds);
    assert!(metrics.gmn_worst_case_tokens < metrics.turtle_best_case_tokens);
    assert!(metrics.gmn_realistic_tokens < metrics.turtle_best_case_tokens);
    assert!(metrics.bytes_on_disk < metrics.turtle_bytes_on_disk);
    assert!(metrics.bytes_on_disk < metrics.jsonld_bytes_on_disk);
}

fn artifacts_are_keyed_by_resolved_dialect_version() {
    let observed = observations();
    let prefix = format!("gmn1/v{}/", observed.major);
    assert!(!observed.paths.is_empty());
    assert!(observed.paths.iter().all(|path| path.starts_with(&prefix)));
    assert!(
        observed
            .paths
            .iter()
            .any(|path| path.ends_with("/conformance-pack.ttl"))
    );
    assert!(
        observed
            .paths
            .iter()
            .any(|path| path.ends_with("/gmn-grounding-glyphs.gmn"))
    );
    assert_ne!(observed.major, observed.controls.bumped_major);
    assert_eq!(observed.controls.bumped_major, "7");
    assert!(!observed.controls.bumped_paths.is_empty());
    assert!(
        observed
            .controls
            .bumped_paths
            .iter()
            .all(|path| path.starts_with("gmn1/v7/"))
    );
    assert!(
        observed
            .controls
            .bumped_paths
            .iter()
            .any(|path| path.ends_with("/conformance-pack.ttl"))
    );
    assert!(
        observed
            .controls
            .bumped_paths
            .iter()
            .any(|path| path.ends_with("/gmn-grounding-glyphs.gmn"))
    );
}

fn assert_digest(digest: &str) {
    let hex = digest
        .strip_prefix("blake3:")
        .expect("algorithm-tagged native digest");
    assert_eq!(hex.len(), 64);
    assert!(
        hex.bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
}

fn gmn1_conformance_pack_certifies_the_actual_ecosystem() {
    let observed = observations();
    assert_exact(witness("conformance-pack.ttl"));
    let ttl = artifact("conformance-pack.ttl");
    assert!(ttl.contains(&format!(
        "<{GMEOW}gmnPackCurrent> <{RDF_TYPE}> <{GMEOW}GmnConformancePack>"
    )));
    assert_digest(&observed.expected_codebook_digest);
    assert_digest(&observed.expected_pack_root);
    for (subject, predicate, expected) in [
        (
            "gmnPackCurrent",
            "gmnPackRoot",
            &observed.expected_pack_root,
        ),
        (
            "gmnCodebookCurrent",
            "gmnCodebookDigest",
            &observed.expected_codebook_digest,
        ),
        (
            "gmnGrammar",
            "gmnGrammarDigest",
            &observed.expected_grammar_leaf,
        ),
    ] {
        assert!(
            ttl.contains(&format!(
                "<{GMEOW}{subject}> <{GMEOW}{predicate}> \"{expected}\" ."
            )),
            "actual {predicate} matches independently reduced native inputs"
        );
    }
    for (subject, predicate, expected) in &observed.ecosystem_leaves {
        assert_eq!(expected.len(), 64);
        assert!(
            expected
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        assert!(ttl.contains(&format!(
            "<{GMEOW}{subject}> <{GMEOW}{predicate}> \"{expected}\" ."
        )));
        assert!(ttl.contains(&format!(
            "<{GMEOW}gmnPackCurrent> <{GMEOW}references> <{GMEOW}{subject}> ."
        )));
    }
    assert_eq!(observed.ecosystem_leaves.len(), 4);
    for subject in ["gmnCodebookCurrent", "gmnDictV3", "gmnGrammar"] {
        assert!(ttl.contains(&format!(
            "<{GMEOW}gmnPackCurrent> <{GMEOW}references> <{GMEOW}{subject}> ."
        )));
    }
}

fn verbalizer_pairs_are_bidirectional_injective_and_fixity_driven() {
    let observed = observations();
    assert_exact(witness("verbalizations.ttl"));
    let control = &observed.controls.verbalizer;
    assert_exact(&control.emission);
    assert_eq!(
        control.emission.path,
        format!("gmn1/v{}/verbalizations.ttl", observed.major)
    );
    for ttl in [artifact("verbalizations.ttl"), control.text.as_str()] {
        assert!(ttl.contains("<https://blackcatinformatics.ca/lang/TranslationUnit>"));
        assert!(ttl.contains("<https://blackcatinformatics.ca/lang/translationCorrespondence>"));
        assert!(ttl.contains("<https://blackcatinformatics.ca/logic/ExactPreservation>"));
        assert!(ttl.contains(&format!(
            "<{GMEOW}gmnVerbalizationsCurrent> <{}> ",
            gmeow_lang_bridge::gmn_migrate::PRED_GMN_SCHEMA_VERSION
        )));
    }
    assert_eq!(control.pairs.len(), 5);
    let nls: BTreeSet<_> = control.pairs.iter().map(|pair| &pair.nl).collect();
    assert_eq!(nls.len(), control.pairs.len());
    let contains: Vec<_> = control
        .pairs
        .iter()
        .filter(|pair| pair.label == "contains")
        .collect();
    assert_eq!(contains.len(), 2);
    assert!(contains.iter().all(|pair| pair.nl.contains('⟪')));
    assert!(control.round_trip);
    assert!(control.pairs.iter().all(|pair| pair.inverse_recovers_form));
    assert_ne!(control.text, observed.controls.perturbed_text);
}

fn one_glyph_under_two_sigils_emits_and_measures_exact() {
    let observed = observations();
    let control = &observed.controls.scoped;
    assert_exact(&control.emission);
    assert!(control.round_trip);
    assert!(control.pairs.iter().all(|pair| pair.inverse_recovers_form));
    let surfaces: Vec<_> = control
        .pairs
        .iter()
        .map(|pair| pair.surface.as_str())
        .collect();
    assert_eq!(surfaces, ["@ℒ arg1 → arg2", "@μ arg1 → arg2"]);
    assert!(observed.controls.erased_scope_error.contains("injectivity"));
}

/// One nextest process authenticates the selected language inputs once. Every
/// original named contract still runs, and one panic never skips its siblings.
#[test]
fn language_projection_contracts_share_one_authenticated_action() {
    use crate::stages::lang_projection::gmn_gate_tests as gates;
    use crate::stages::lang_projection::grammar_corpus_tests as grammar;

    macro_rules! contract {
        ($function:path) => {
            (stringify!($function), $function as fn())
        };
    }
    let contracts: [(&str, fn()); 14] = [
        contract!(grammar::gate3_turtle_grammar_round_trips_isomorphically),
        contract!(grammar::gate3_gts_grammar_round_trips_isomorphically),
        contract!(grammar::ebnf_target_emits_exact_round_tripping_grammar),
        contract!(grammar::abnf_target_is_honest_lossy_for_char_class_grammars),
        contract!(gmn1_metrics_record_native_values_and_version),
        contract!(gmn_beats_turtle_under_grounding_byte_fallback_worst_case),
        contract!(artifacts_are_keyed_by_resolved_dialect_version),
        contract!(gmn1_conformance_pack_certifies_the_actual_ecosystem),
        contract!(verbalizer_pairs_are_bidirectional_injective_and_fixity_driven),
        contract!(one_glyph_under_two_sigils_emits_and_measures_exact),
        contract!(gates::codebook_digest_gate_is_clean_over_the_real_tree),
        contract!(gates::codebook_digest_gate_reds_on_the_mismatch_fixture),
        contract!(gates::pack_root_check_is_clean_over_the_real_tree),
        contract!(gates::shipped_gmn1_projections_all_read_clean),
    ];
    let selected = contracts.len();
    let names: BTreeSet<_> = contracts.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names.len(),
        selected,
        "all fourteen named language contracts are distinct"
    );
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if let Err(payload) = std::panic::catch_unwind(contract) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "non-string panic".to_owned());
            failures.push(format!("{name}: {detail}"));
        }
    }
    eprintln!(
        "language projection contracts: {selected} executed, {} failed",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {selected} language projection contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
