// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn term(iri: &str) -> TermValue {
    TermValue::iri(iri)
}

// The atomic-term dictionary's own native dedup/lookup controls (display isolation,
// language-tag case significance, non-inserting lookup) is asserted where the
// dictionary lives — `gmeow_term_arena::interner`. What remains here is what this
// module owns: insertion order, `TermId` determinism, Skolemization, and quad dedup.

// ── (1) insertion-order iteration ─────────────────────────────────────────

#[test]
fn facts_iterate_in_insertion_order() {
    let mut set = TypedFactSet::new();
    set.push_quad(
        &term("http://ex/a"),
        "http://ex/knows",
        &term("http://ex/b"),
        "w",
    );
    set.push_quad(
        &term("http://ex/a"),
        "http://ex/knows",
        &term("http://ex/c"),
        "w",
    );
    set.push_quad(
        &term("http://ex/b"),
        "http://ex/likes",
        &term("http://ex/c"),
        "w",
    );

    let rendered: Vec<String> = set
        .facts()
        .map(|f| {
            let args: Vec<&str> = f
                .args
                .iter()
                .map(|&id| set.interner().display_of(id))
                .collect();
            format!("{}({})", f.predicate, args.join(", "))
        })
        .collect();
    assert_eq!(
        rendered,
        vec![
            "http://ex/knows(<http://ex/a>, <http://ex/b>, \"w\")",
            "http://ex/knows(<http://ex/a>, <http://ex/c>, \"w\")",
            "http://ex/likes(<http://ex/b>, <http://ex/c>, \"w\")",
        ],
    );
}

// ── (2) TermId determinism across identical build sequences ──────────────

#[test]
fn facts_term_ids_deterministic_across_identical_builds() {
    let build = || {
        let mut set = TypedFactSet::new();
        set.push_quad(
            &term("http://ex/a"),
            "http://ex/knows",
            &TermValue::lang_literal("hi", "en"),
            "http://world/W",
        );
        set.push_quad(
            &TermValue::blank("b0"),
            "http://ex/knows",
            &term("http://ex/a"),
            "http://world/W",
        );
        set
    };
    let s1 = build();
    let s2 = build();

    let facts1: Vec<&TypedFact> = s1.facts().collect();
    let facts2: Vec<&TypedFact> = s2.facts().collect();
    assert_eq!(facts1, facts2, "identical builds must mint identical ids");
    assert_eq!(s1.interner().len(), s2.interner().len());
    for f in &facts1 {
        for &id in &f.args {
            assert_eq!(
                s1.interner().display_of(id),
                s2.interner().display_of(id),
                "slot contents must match across identical builds"
            );
        }
    }
}

// ── (3) push_quad Skolemizes blanks via skolem_iri ────────────────────────

#[test]
fn facts_sha1_hex_matches_python_recipe_shape() {
    // sha1(b"...").hexdigest(): 40 lowercase hex chars.
    let h = sha1_hex("b0");
    assert_eq!(h.len(), 40, "SHA1 hex must be 40 characters");
    assert!(
        h.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA1 hex must be hex"
    );
    assert_eq!(skolem_iri("b0"), format!("{SKOLEM_PREFIX}{h}"));
}

#[test]
fn facts_push_quad_skolemizes_blank_terms() {
    let mut set = TypedFactSet::new();
    assert!(set.push_quad(
        &TermValue::blank("b0"),
        "http://ex/knows",
        &TermValue::blank("b1"),
        "http://world/W",
    ));

    let fact = set.facts().next().expect("one fact pushed");
    assert_eq!(fact.args.len(), 3);
    // Subject/object are the stable Skolem IRIs (default scope keeps the
    // bare label, so the digests are over "b0"/"b1" exactly).
    assert_eq!(
        set.interner().resolve(fact.args[0]),
        &TermValue::Iri(skolem_iri("b0"))
    );
    assert_eq!(
        set.interner().resolve(fact.args[1]),
        &TermValue::Iri(skolem_iri("b1"))
    );
    // The world is a plain string literal.
    assert_eq!(
        set.interner().resolve(fact.args[2]),
        &TermValue::simple_literal("http://world/W")
    );
}

// ── (4) dedup of an identical pushed quad ─────────────────────────────────

#[test]
fn facts_push_quad_dedups_identical_quad() {
    let mut set = TypedFactSet::new();
    let a = term("http://ex/a");
    let b = term("http://ex/b");
    assert!(set.push_quad(&a, "http://ex/knows", &b, "http://world/W"));
    assert!(
        !set.push_quad(&a, "http://ex/knows", &b, "http://world/W"),
        "identical quad must dedup"
    );
    assert_eq!(set.facts().count(), 1);
    assert_eq!(set.interner().len(), 3);

    // Same triple in a DIFFERENT world is a distinct fact.
    assert!(set.push_quad(&a, "http://ex/knows", &b, "http://world/X"));
    assert_eq!(set.facts().count(), 2);
}
