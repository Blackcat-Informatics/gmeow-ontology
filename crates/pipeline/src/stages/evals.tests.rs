// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn evals_blake2s_matches_hashlib() {
    // hashlib.blake2s(data, digest_size=4).hexdigest() — canonical Python.
    assert_eq!(blake2s::hex(b"", 4), "36e9d246");
    assert_eq!(blake2s::hex(b"abc", 4), "df7101d3");
    assert_eq!(blake2s::hex(b"openai/gpt-4.1", 4), "7558b2f5");
}

#[test]
fn evals_slug_is_path_safe() {
    assert_eq!(slug("reference-baseline"), "reference-baseline");
    // lossy ids gain a content-hash suffix
    assert!(slug("openai/gpt-4.1").starts_with("openai-gpt-4-1-"));
}

/// Shift-left: drive the SAME native structural lint `make validate`/`make
/// check` run (`gmeow_validate::lint::structural_lint_dataset`) over this
/// generator's real output for a small synthetic scorecard set, so a
/// missing/incorrect A-Box annotation on a minted `ev:model-*` /
/// `ev:assessment-*` individual reds HERE — a fast `cargo nextest -p
/// gmeow-pipeline` — rather than only surfacing at the next expensive
/// whole-bundle SHACL validation (`make validate` / the pipeline
/// stage-validate).
#[test]
fn minted_individuals_satisfy_the_assertional_abox_contract() {
    use gmeow_validate::lint::{LintConfig, structural_lint_dataset};

    let cards = vec![Scorecard {
        model: "acme/test-model-1".to_string(),
        emitted: 4,
        valid: 4,
        scores: vec![
            ("schema-validity".to_string(), 1.0),
            ("grounding-precision".to_string(), 0.5),
            ("grounding-recall".to_string(), 0.5),
            ("hallucination-resistance".to_string(), 0.5),
            ("abstention-quality".to_string(), 1.0),
            ("calibration".to_string(), 0.8),
        ],
        notes: Vec::new(),
    }];

    let ttl = render_scores_ttl(&cards);
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
    // kernel slice; add it here (same pattern as `provenance_graph.rs`'s
    // `minted_individuals_satisfy_the_assertional_abox_contract` and
    // `release.rs`'s `minted_attestations_satisfy_the_assertional_contract`)
    // so the graphBoxRole-typing check has its declaration to resolve
    // against. `gmeow:` is already `@prefix`-declared by `render_scores_ttl`.
    let doc = format!("{ttl}\ngmeow:boxABox a gmeow:GraphBoxRole .\n");
    let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse the synthetic scores.ttl fragment");

    let cfg = LintConfig {
        namespace: GM.to_string(),
        ontology_iri: GM.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: Default::default(),
    };
    let report = structural_lint_dataset(&ds, &cfg);
    let errors = report.errors();
    let evals_errors: Vec<&String> = errors.iter().filter(|e| e.contains(EV)).collect();
    assert!(
        evals_errors.is_empty(),
        "every minted evals individual must satisfy the A-Box annotation \
             contract (rdfs:label / skos:definition / rdfs:isDefinedBy / \
             gmeow:graphBoxRole): {evals_errors:?}"
    );
}
