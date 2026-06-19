// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native SLME module extractor (issue #695).

use super::*;

const EX: &str = "http://example.org/";

fn iri(local: &str) -> String {
    format!("{EX}{local}")
}

/// Run an extraction and return the module Turtle string.
fn run(ttl: &str, seeds: &[&str], method: &str) -> ModuleResult {
    let terms: Vec<String> = seeds.iter().map(|s| iri(s)).collect();
    extract_module(ttl, &terms, method).expect("extraction must not fail")
}

/// True iff the module Turtle contains a `<local-s> <pred> <local-o>` edge. We test
/// against the canonical full IRIs (substring) so the prefix-style serializer output
/// is matched robustly.
fn has_edge(module: &str, s: &str, pred_iri: &str, o: &str) -> bool {
    module.contains(&iri(s)) && module.contains(pred_iri) && module.contains(&iri(o))
}

// ── Test 1: atomic subClassOf chain (BOT) ────────────────────────────────────────

#[test]
fn bot_keeps_subclass_chain_and_drops_unrelated() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf ex:B .
ex:B rdfs:subClassOf ex:C .
ex:D rdfs:subClassOf ex:E .
"#
    );
    let result = run(&ttl, &["A"], "BOT");
    let m = &result.module_ttl;
    // A⊑B kept (A∈Σ), then B pulled in → B⊑C kept (and C pulled in).
    assert!(m.contains(&iri("A")), "A⊑B must be kept: {m}");
    assert!(m.contains(&iri("B")), "B must be in module: {m}");
    assert!(m.contains(&iri("C")), "B⊑C must be kept (C pulled in): {m}");
    // D⊑E is unrelated to {A} and must NOT appear.
    assert!(!m.contains(&iri("D")), "D⊑E must be dropped: {m}");
    assert!(!m.contains(&iri("E")), "E must be dropped: {m}");
    // Three kept axioms? No — A⊑B and B⊑C = 2 named-subject triples.
    assert_eq!(
        result.selected_axiom_count, 2,
        "expected exactly A⊑B and B⊑C"
    );
}

// ── Test 2: declarations + labels ────────────────────────────────────────────────

#[test]
fn keeps_seeded_declaration_and_label_drops_unseeded() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Seeded a owl:Class ;
    rdfs:label "Seeded class" .
ex:Other a owl:Class ;
    rdfs:label "Other class" .
"#
    );
    let result = run(&ttl, &["Seeded"], "STAR");
    let m = &result.module_ttl;
    assert!(
        m.contains(&iri("Seeded")),
        "Seeded declaration must be kept: {m}"
    );
    assert!(m.contains("Seeded class"), "Seeded label must be kept: {m}");
    // The non-seeded class's label (and declaration) must be dropped.
    assert!(
        !m.contains("Other class"),
        "Other's label must be dropped: {m}"
    );
    assert!(!m.contains(&iri("Other")), "Other must be dropped: {m}");
}

// ── Test 3: disjointWith only when both endpoints in Σ ───────────────────────────

#[test]
fn disjoint_kept_only_when_both_in_sigma() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
ex:A owl:disjointWith ex:B .
"#
    );
    // Only A seeded → disjoint not kept (B not in Σ; disjoint never grows Σ).
    let one = run(&ttl, &["A"], "STAR");
    assert!(
        !one.module_ttl.contains(&iri("B")),
        "disjoint must be dropped with only one endpoint in Σ: {}",
        one.module_ttl
    );
    assert_eq!(one.selected_axiom_count, 0);

    // Both A and B seeded → disjoint kept.
    let both = run(&ttl, &["A", "B"], "STAR");
    assert!(
        has_edge(
            &both.module_ttl,
            "A",
            "http://www.w3.org/2002/07/owl#disjointWith",
            "B"
        ),
        "disjoint must be kept with both endpoints in Σ: {}",
        both.module_ttl
    );
    assert_eq!(both.selected_axiom_count, 1);
}

// ── Test 4: conservative-keep of a Restriction blank-node closure ────────────────

#[test]
fn conservative_keep_restriction_closure_and_warns() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf [
    a owl:Restriction ;
    owl:onProperty ex:p ;
    owl:someValuesFrom ex:B
] .
"#
    );
    let result = run(&ttl, &["A"], "STAR");
    let m = &result.module_ttl;
    // The blank-node closure (Restriction, onProperty, someValuesFrom) must all be
    // pulled in because A∈Σ and the subClassOf object is a blank node.
    assert!(
        m.contains("Restriction"),
        "Restriction type triple must be kept: {m}"
    );
    assert!(m.contains(&iri("p")), "onProperty target must be kept: {m}");
    assert!(
        m.contains(&iri("B")),
        "someValuesFrom target must be kept: {m}"
    );
    // A conservative-keep warning must be emitted, tooled "slme".
    let warned = result
        .findings
        .iter()
        .any(|f| f.code == "slme.conservative-keep" && f.tool.as_deref() == Some("slme"));
    assert!(
        warned,
        "expected a slme.conservative-keep warning: {:?}",
        result.findings
    );
}

// ── Test 5: BOT vs TOP directional closure ───────────────────────────────────────

#[test]
fn bot_vs_top_directional_subclass() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf ex:B .
"#
    );
    // BOT seeded at {A}: keep iff C(=A)∈Σ → kept.
    let bot_a = run(&ttl, &["A"], "BOT");
    assert!(
        has_edge(
            &bot_a.module_ttl,
            "A",
            "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            "B"
        ),
        "BOT seeded at A must keep A⊑B: {}",
        bot_a.module_ttl
    );

    // BOT seeded at {B}: keep iff A∈Σ → A∉Σ → dropped.
    let bot_b = run(&ttl, &["B"], "BOT");
    assert_eq!(
        bot_b.selected_axiom_count, 0,
        "BOT seeded at B must NOT keep A⊑B: {}",
        bot_b.module_ttl
    );

    // TOP seeded at {B}: keep iff D(=B)∈Σ → kept.
    let top_b = run(&ttl, &["B"], "TOP");
    assert!(
        has_edge(
            &top_b.module_ttl,
            "A",
            "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            "B"
        ),
        "TOP seeded at B must keep A⊑B: {}",
        top_b.module_ttl
    );

    // TOP seeded at {A}: keep iff B∈Σ → B∉Σ → dropped.
    let top_a = run(&ttl, &["A"], "TOP");
    assert_eq!(
        top_a.selected_axiom_count, 0,
        "TOP seeded at A must NOT keep A⊑B: {}",
        top_a.module_ttl
    );
}

// ── Test 6: multilingual labels are NOT deduped by the canonical key ─────────────

#[test]
fn keeps_all_language_tagged_labels_on_a_seeded_term() {
    // Regression: term_sort_key must include the language tag, else "x"@en and
    // "x"@fr collide on the triple key and one translation is silently dropped.
    // GMEOW is multilingual, so this must hold.
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A a owl:Class ;
    rdfs:label "Thing"@en ;
    rdfs:label "Chose"@fr ;
    rdfs:label "事物"@zh .
"#
    );
    let result = run(&ttl, &["A"], "STAR");
    let m = &result.module_ttl;
    assert!(m.contains("@en"), "English label dropped: {m}");
    assert!(m.contains("@fr"), "French label dropped: {m}");
    assert!(m.contains("@zh"), "Chinese label dropped: {m}");
    // 1 declaration + 3 labels = 4 named-subject triples.
    assert_eq!(
        result.selected_axiom_count, 4,
        "all three labels + the declaration must be kept: {m}"
    );
}

// ── Extra: unknown method warns and falls back to STAR ───────────────────────────

#[test]
fn unknown_method_warns_and_uses_star() {
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf ex:B .
"#
    );
    let result = run(&ttl, &["A"], "WAT");
    assert_eq!(result.method, Method::Star);
    assert!(result
        .findings
        .iter()
        .any(|f| f.code == "slme.unknown-method"));
}

// ── Test 7: predicate ∈ Σ keeps the whole assertion (bug B regression) ───────────

#[test]
fn predicate_in_sigma_keeps_assertion() {
    // ex:rel is the seed; ex:Lonely1 ex:rel ex:Lonely2 must appear in the module
    // even though neither Lonely1 nor Lonely2 is in Σ.
    // This would have been dropped before fix B.
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
ex:rel a owl:ObjectProperty .
ex:Lonely1 ex:rel ex:Lonely2 .
"#
    );
    let result = run(&ttl, &["rel"], "STAR");
    let m = &result.module_ttl;
    assert!(
        m.contains(&iri("Lonely1")) && m.contains(&iri("rel")) && m.contains(&iri("Lonely2")),
        "predicate-in-Σ assertion must be kept: {m}"
    );
}

// ── Test 8: bnode predicate collected into Σ (bug A regression) ──────────────────

#[test]
fn bnode_predicate_collected_into_sigma() {
    // ex:bnodeProp appears only as owl:onProperty inside a blank-node restriction.
    // With fix A, collect_named_iris_in_closure now picks up predicates, so seeding
    // {bnodeProp} must pull in the BnodeClass + its equivalentClass restriction.
    // This would have been dropped before fix A.
    let ttl = format!(
        r#"@prefix ex: <{EX}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
ex:BnodeClass a owl:Class ;
    owl:equivalentClass [
        a owl:Restriction ;
        owl:onProperty ex:bnodeProp ;
        owl:someValuesFrom ex:SomeTarget
    ] .
ex:bnodeProp a owl:ObjectProperty .
ex:SomeTarget a owl:Class .
"#
    );
    let result = run(&ttl, &["bnodeProp"], "STAR");
    let m = &result.module_ttl;
    assert!(
        m.contains(&iri("BnodeClass")),
        "BnodeClass must be pulled into module via bnode predicate collection: {m}"
    );
    assert!(
        m.contains(&iri("bnodeProp")),
        "bnodeProp must be in the module: {m}"
    );
    assert!(
        m.contains("Restriction"),
        "Restriction bnode must be in the module: {m}"
    );
}
