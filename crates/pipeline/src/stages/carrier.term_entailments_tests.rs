// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The alternate bare-reifying spelling of the production fixture below. RDF 1.2
/// parses each quoted triple through a minted reifier bound in
/// `RdfDataset::owned_reifiers()`; the join must resolve that statement identically.
const EXPLANATIONS_TTL_REIFYING: &str = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .

<https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567> a gmeow:Derivation ;
   gmeow:concludes << <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> >> ;
   logic:derivationIdentifier \"https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567\" ;
   gmeow:hasPremise << <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Mammal> >> ;
   gmeow:viaRule <https://blackcatinformatics.ca/gmeow/rule/subclass-transitivity> ;
   gmeow:inferenceKind gmeow:Deduction ;
   rdfs:label \"derivation of an inferred axiom\"@en ;
   gmeow:inWorld <https://blackcatinformatics.ca/gmeow/world/default> .
";

/// A hand-built `reasoning-explanations.rdf12.ttl` fixture mirroring
/// `gmeow_logic::reason::artifacts::build_explanations_ttl`'s production shape: one
/// named, content-addressed derivation carrying canonical parenthesized
/// `<<( s p o )>>` triple terms, which parse inline as `RdfTerm::Triple`, plus its
/// exact derivation-identifier literal and firing-rule IRI.
const EXPLANATIONS_TTL: &str = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .

<https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567> a gmeow:Derivation ;
   gmeow:concludes <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> )>> ;
   logic:derivationIdentifier \"https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567\" ;
   gmeow:hasPremise <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Mammal> )>> ;
   gmeow:viaRule <https://blackcatinformatics.ca/gmeow/rule/subclass-transitivity> ;
   gmeow:inferenceKind gmeow:Deduction ;
   rdfs:label \"derivation of an inferred axiom\"@en ;
   gmeow:inWorld <https://blackcatinformatics.ca/gmeow/world/default> .
";

#[test]
fn term_entailments_from_explanations_populates_matching_term_only() {
    let mut term_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    term_iris.insert("https://blackcatinformatics.ca/gmeow/Cat".to_string());

    let digest = term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris)
        .expect("parse explanations fixture");

    // `Cat` is the conclusion's subject AND the premise's subject: matched once
    // (the join is a set, never a duplicate panel entry for one derivation).
    let entries = digest
        .get("https://blackcatinformatics.ca/gmeow/Cat")
        .expect("Cat must have a populated entailment panel");
    assert_eq!(entries.len(), 1, "one derivation ⇒ one panel entry");
    let entailment = &entries[0];
    assert!(
        entailment.conclusion.contains("rdfs:subClassOf"),
        "conclusion display: {}",
        entailment.conclusion
    );
    assert!(
        entailment.conclusion.contains("Animal"),
        "conclusion display: {}",
        entailment.conclusion
    );
    assert_eq!(entailment.premises.len(), 1);
    assert!(
        entailment.premises[0].contains("Mammal"),
        "premise display: {}",
        entailment.premises[0]
    );
    assert!(
        !entailment.rule.is_empty(),
        "the firing rule must never be a fabricated empty string"
    );

    // `Animal` and `Mammal` are documented terms too — a term appearing ONLY in an
    // object/premise-object position also gets the derivation's panel (any position
    // joins), so the same derivation lands on all three matched terms.
    let mut term_iris_all = term_iris.clone();
    term_iris_all.insert("https://blackcatinformatics.ca/gmeow/Animal".to_string());
    term_iris_all.insert("https://blackcatinformatics.ca/gmeow/Mammal".to_string());
    let digest_all =
        term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris_all)
            .expect("parse explanations fixture (wider term set)");
    assert!(digest_all.contains_key("https://blackcatinformatics.ca/gmeow/Animal"));
    assert!(digest_all.contains_key("https://blackcatinformatics.ca/gmeow/Mammal"));

    // A term absent from the derivation entirely gets no entry (honest absence,
    // never a fabricated empty panel).
    let mut term_iris_unrelated: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    term_iris_unrelated.insert("https://blackcatinformatics.ca/gmeow/Unrelated".to_string());
    let digest_unrelated =
        term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris_unrelated)
            .expect("parse explanations fixture (unrelated term)");
    assert!(digest_unrelated.is_empty());

    // The bare reifying spelling must resolve to the IDENTICAL digest as the
    // production parenthesized triple-term form.
    let digest_reifying =
        term_entailments_from_explanations(EXPLANATIONS_TTL_REIFYING.as_bytes(), &term_iris)
            .expect("parse reifying explanations fixture");
    assert_eq!(
        digest, digest_reifying,
        "parenthesized triple terms and bare reifiers must join identically"
    );
}

#[test]
fn term_entailments_preserves_every_conclusion_of_one_rule_firing() {
    let multi_conclusion = EXPLANATIONS_TTL.replacen(
            "   gmeow:concludes <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> )>> ;",
            "   gmeow:concludes <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> )>>,\
             <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://example.org/CompanionAnimal> )>> ;",
            1,
        );
    assert_ne!(multi_conclusion, EXPLANATIONS_TTL);
    let term_iris = [
        "https://blackcatinformatics.ca/gmeow/Cat".to_owned(),
        "https://blackcatinformatics.ca/gmeow/Animal".to_owned(),
        "https://example.org/CompanionAnimal".to_owned(),
        "https://blackcatinformatics.ca/gmeow/Mammal".to_owned(),
    ]
    .into_iter()
    .collect();

    let digest = term_entailments_from_explanations(multi_conclusion.as_bytes(), &term_iris)
        .expect("one content-addressed firing may carry every head conclusion");

    let cat = &digest["https://blackcatinformatics.ca/gmeow/Cat"];
    assert_eq!(
        cat.len(),
        2,
        "both conclusions must survive the identity join"
    );
    assert!(cat.iter().any(|entry| entry.conclusion.contains("Animal")));
    assert!(
        cat.iter()
            .any(|entry| entry.conclusion.contains("CompanionAnimal"))
    );
    assert!(cat.iter().all(|entry| entry.premises.len() == 1));

    assert_eq!(
        digest["https://blackcatinformatics.ca/gmeow/Animal"].len(),
        1
    );
    assert_eq!(digest["https://example.org/CompanionAnimal"].len(), 1);
    assert_eq!(
        digest["https://blackcatinformatics.ca/gmeow/Mammal"].len(),
        2,
        "the exact shared premise participates in each head's entailment"
    );
}

#[test]
fn term_entailments_rejects_an_unnamed_derivation() {
    let unnamed = EXPLANATIONS_TTL.replacen(
            "<https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567>",
            "[]",
            1,
        );
    let term_iris = ["https://blackcatinformatics.ca/gmeow/Cat".to_owned()]
        .into_iter()
        .collect();
    let err = term_entailments_from_explanations(unnamed.as_bytes(), &term_iris)
        .expect_err("an unnamed derivation must not lose its content identity");
    assert!(
        err.message().contains("content-addressed IRI"),
        "got: {err}"
    );
}

#[test]
fn term_entailments_rejects_incomplete_provenance() {
    let term_iris = ["https://blackcatinformatics.ca/gmeow/Cat".to_owned()]
        .into_iter()
        .collect();
    for (malformed, expected) in [
        (
            EXPLANATIONS_TTL.replacen(
                "gmeow:concludes",
                "<https://example.org/ignoredConclusion>",
                1,
            ),
            "has no conclusion",
        ),
        (
            EXPLANATIONS_TTL.replacen("gmeow:viaRule", "<https://example.org/ignoredRule>", 1),
            "has no firing rule",
        ),
        (
            EXPLANATIONS_TTL.replacen(
                "logic:derivationIdentifier",
                "<https://example.org/ignoredIdentifier>",
                1,
            ),
            "has no derivation identifier",
        ),
    ] {
        let err = term_entailments_from_explanations(malformed.as_bytes(), &term_iris)
            .expect_err("incomplete derivation provenance must fail closed");
        assert!(err.message().contains(expected), "got: {err}");
    }
}

#[test]
fn term_entailments_rejects_a_mismatched_derivation_identity() {
    let mismatched = EXPLANATIONS_TTL.replacen(
            "\"https://blackcatinformatics.ca/gmeow/derivation/0123456789abcdef0123456789abcdef01234567\"",
            "\"https://blackcatinformatics.ca/gmeow/derivation/ffffffffffffffffffffffffffffffffffffffff\"",
            1,
        );
    let term_iris = ["https://blackcatinformatics.ca/gmeow/Cat".to_owned()]
        .into_iter()
        .collect();
    let err = term_entailments_from_explanations(mismatched.as_bytes(), &term_iris)
        .expect_err("derivation identifier must repeat the exact resource IRI");
    assert!(err.message().contains("does not match"), "got: {err}");
}

#[test]
fn term_entailments_from_upstream_joins_and_hard_fails_on_missing_artifact() {
    let mut term_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    term_iris.insert("https://blackcatinformatics.ca/gmeow/Cat".to_string());

    // Positive: a synthetic `stage-reason` StageProduct carrying the explanations
    // artifact joins exactly like the pure function above.
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        crate::stages::reason::EXPLANATIONS_PATH.to_string(),
        EXPLANATIONS_TTL.as_bytes().to_vec(),
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-reason".to_string(),
        StageProduct::from_artifacts("stage-reason", artifacts),
    );
    let digest = term_entailments_from_upstream(&upstream, &term_iris)
        .expect("digest folds from synthetic upstream");
    assert!(digest.contains_key("https://blackcatinformatics.ca/gmeow/Cat"));

    // Missing the whole stage-reason product hard-fails (never a silent empty digest).
    assert!(
        term_entailments_from_upstream(&BTreeMap::new(), &term_iris).is_err(),
        "missing stage-reason product must hard-fail"
    );

    // A declared stage-reason product present but MISSING the explanations artifact
    // (e.g. a stale/partial product) hard-fails too — never silently treated as empty.
    let mut missing_artifact: BTreeMap<String, StageProduct> = BTreeMap::new();
    missing_artifact.insert(
        "stage-reason".to_string(),
        StageProduct::from_artifacts("stage-reason", BTreeMap::new()),
    );
    assert!(
        term_entailments_from_upstream(&missing_artifact, &term_iris).is_err(),
        "a stage-reason product missing the explanations artifact must hard-fail"
    );
}
