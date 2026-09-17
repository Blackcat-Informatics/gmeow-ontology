// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// Authored dictionary, pack and compression contracts consume authenticated producer
// receipts in the pipeline. These focused determinism controls use only synthetic values.
#[test]
fn synthetic_metric_serialization_is_deterministic() {
    let dictionary = GmnDictionary::default();
    let metrics = TokenMetrics {
        bytes_on_disk: 17,
        tokens_in_context: 3,
        ast_validity_rate: 1.0,
        roundtrip_loss: 0.0,
        compression_ratio: 0.25,
        glyph_density: 0.5,
        dictionary_hit_rate: 1.0,
        gmn_worst_case_tokens: 8,
        gmn_realistic_tokens: 3,
        turtle_best_case_tokens: 20,
        gmn_ascii_bytes: 12,
        gmn_nonascii_bytes: 5,
        turtle_bytes_on_disk: 68,
        jsonld_bytes_on_disk: 100,
        total_sources: 1,
        measured_sources: 1,
    };
    let first = ntriples_sorted(token_metrics_triples(&metrics, &dictionary));
    let second = ntriples_sorted(token_metrics_triples(&metrics, &dictionary));
    assert_eq!(first, second);
}

#[test]
fn synthetic_verbalizer_emission_is_deterministic() {
    let dictionary = GmnDictionary::default();
    let forms = vec![crate::gmn_verbalize::GmnOperatorForm {
        term_iri: "urn:synthetic:not".to_owned(),
        term_label: "not".to_owned(),
        gmn_glyph: "¬".to_owned(),
        fixity: crate::gmn_verbalize::FIXITY_PREFIX.to_owned(),
        arity: 1,
        sigil: String::new(),
    }];
    let first = gmn1_verbalizer_emission(&forms, &dictionary, "9")
        .expect("synthetic verbalizer")
        .expect("nonempty operator inventory");
    let second = gmn1_verbalizer_emission(&forms, &dictionary, "9")
        .expect("synthetic verbalizer")
        .expect("nonempty operator inventory");
    assert_eq!(first.artifacts[0].bytes, second.artifacts[0].bytes);
}
