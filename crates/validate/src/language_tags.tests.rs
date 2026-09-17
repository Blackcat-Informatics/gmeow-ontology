// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn is_internal_tag_basic() {
    assert!(is_internal_tag("x-gmeow-english"));
    assert!(is_internal_tag("x-gmeow-mandarin"));
    assert!(is_internal_tag("X-GMEOW-FRENCH"));
    assert!(is_internal_tag("x-gmeow-foo-bar"));
    assert!(!is_internal_tag("en"));
    assert!(!is_internal_tag("fr"));
    assert!(!is_internal_tag("x-gmeow-")); // empty suffix
    assert!(!is_internal_tag("xx-gmeow-no")); // wrong prefix
    assert!(!is_internal_tag("x-gmeow")); // no suffix segment
}

#[test]
fn marked_appends_fallback_marker() {
    assert_eq!(marked("Hello", false, "en"), "Hello");
    assert_eq!(marked("Hello", true, "en"), "Hello [fallback: en]");
    assert_eq!(marked("Bonjour", true, "fr"), "Bonjour [fallback: fr]");
}

#[test]
fn rank_language_carrier_wins() {
    let (r_en, _) = rank_language("x-gmeow-english");
    let (r_fr, _) = rank_language("x-gmeow-french");
    let (r_bcp, _) = rank_language("en");
    assert_eq!(r_en, 0);
    assert_eq!(r_fr, 1);
    assert_eq!(r_bcp, 1);
}

#[test]
fn rank_language_case_insensitive() {
    let (r, key) = rank_language("X-GMEOW-ENGLISH");
    assert_eq!(r, 0);
    assert_eq!(key, "x-gmeow-english");
}

#[test]
fn load_tag_map_parses_turtle() {
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
    let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
    assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
    assert_eq!(map.get("x-gmeow-french"), Some(&"fr".to_owned()));
}

#[test]
fn load_tag_map_ambiguous_err() {
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    lang:carrierTag "x-gmeow-english-alt" ;
    gmeow:bcp47Tag "en" .
"#;
    assert!(load_tag_map(ttl.as_bytes(), "turtle").is_err());
}

#[test]
fn load_tag_map_conflicting_duplicate_err() {
    // Two individuals mapping the SAME internal tag to DIFFERENT bcp47Tags is a
    // nondeterministic conflict and must hard-fail, not silently last-writer-win.
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishAlt a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en-GB" .
"#;
    let err = load_tag_map(ttl.as_bytes(), "turtle").expect_err("conflict must error");
    assert!(err.is::<crate::error::LanguageTag>());
    assert!(
        err.message().contains("conflicting bcp47Tag"),
        "{}",
        err.message()
    );
}

#[test]
fn load_tag_map_duplicate_identical_ok() {
    // The SAME internal tag → SAME bcp47Tag from two individuals is a harmless
    // duplicate, not a conflict.
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishCopy a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .
"#;
    let map = load_tag_map(ttl.as_bytes(), "turtle").expect("identical duplicate ok");
    assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
}

#[test]
fn load_tag_map_missing_tag_skipped() {
    // An individual with only one of the two required properties is silently
    // skipped (SHACL enforces completeness; we don't fabricate).
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" .
"#;
    let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
    assert!(map.is_empty(), "incomplete individual must be skipped");
}

#[test]
fn load_tag_map_formal_and_prog_language() {
    // The former gmeow:FormalLanguage / gmeow:ProgrammingLanguage subclasses are
    // retired: a formal or programming language is now a gmeow:Language
    // distinguished by lang:signSystemKind. Any gmeow:Language carrying a
    // lang:carrierTag + gmeow:bcp47Tag pair is picked up regardless of its kind.
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:Rust a gmeow:Language ;
    lang:signSystemKind lang:programmingLanguageKind ;
    lang:carrierTag "x-gmeow-rust" ;
    gmeow:bcp47Tag "en" .

gmeow:Prolog a gmeow:Language ;
    lang:signSystemKind lang:formalLanguageKind ;
    lang:carrierTag "x-gmeow-prolog" ;
    gmeow:bcp47Tag "en" .
"#;
    let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
    assert!(map.contains_key("x-gmeow-rust"));
    assert!(map.contains_key("x-gmeow-prolog"));
}

#[test]
fn load_tag_map_ntriples_format() {
    let nt = "\
<https://blackcatinformatics.ca/gmeow/English> \
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
<https://blackcatinformatics.ca/gmeow/Language> .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/lang/carrierTag> \
\"x-gmeow-english\" .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/gmeow/bcp47Tag> \
\"en\" .\n";
    let map = load_tag_map(nt.as_bytes(), "ntriples").expect("parse");
    assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
}

// ── shared fixtures ──────────────────────────────────────────────────────

/// A small internal→BCP-47 tag map: english, french, mandarin.
fn sample_tag_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("x-gmeow-english".to_owned(), "en".to_owned());
    m.insert("x-gmeow-french".to_owned(), "fr".to_owned());
    m.insert("x-gmeow-mandarin".to_owned(), "zh".to_owned());
    m
}

fn desc(lexical: &str, language: Option<&str>) -> LitDesc {
    LitDesc {
        lexical: lexical.to_owned(),
        language: language.map(str::to_owned),
    }
}

// ── resolve_lang_input ──────────────────────────────────────────────────

#[test]
fn resolve_lang_input_defaults_to_en() {
    let tm = sample_tag_map();
    let none = resolve_lang_input(None, &tm, None).expect("none");
    assert_eq!(none.requested, vec!["en".to_owned()]);
    let empty = resolve_lang_input(Some("   "), &tm, None).expect("empty");
    assert_eq!(empty.requested, vec!["en".to_owned()]);
}

#[test]
fn resolve_lang_input_accepts_public_bcp47() {
    let tm = sample_tag_map();
    let sel = resolve_lang_input(Some("fr"), &tm, None).expect("fr");
    assert_eq!(sel.requested, vec!["fr".to_owned()]);
}

#[test]
fn resolve_lang_input_accepts_internal_tag() {
    let tm = sample_tag_map();
    let sel = resolve_lang_input(Some("x-gmeow-french"), &tm, None).expect("internal");
    assert_eq!(sel.requested, vec!["fr".to_owned()]);
}

#[test]
fn resolve_lang_input_preserves_order_and_dedupes() {
    let tm = sample_tag_map();
    let sel = resolve_lang_input(Some("fr,en,fr,zh"), &tm, None).expect("list");
    assert_eq!(
        sel.requested,
        vec!["fr".to_owned(), "en".to_owned(), "zh".to_owned()]
    );
}

#[test]
fn resolve_lang_input_rejects_unknown_public_tag() {
    let tm = sample_tag_map();
    let err = resolve_lang_input(Some("de"), &tm, None).expect_err("unknown");
    assert_eq!(err.tag, "de");
    // Available is en-first then lexicographic.
    assert_eq!(err.available.first(), Some(&"en".to_owned()));
    assert!(err.available.contains(&"fr".to_owned()));
}

#[test]
fn resolve_lang_input_rejects_unknown_internal_tag() {
    let tm = sample_tag_map();
    let err = resolve_lang_input(Some("x-gmeow-klingon"), &tm, None).expect_err("unknown");
    assert_eq!(err.tag, "x-gmeow-klingon");
}

// ── mixed/upper-case internal-tag normalisation ──────────────────────────

#[test]
fn resolve_lang_input_mixed_case_internal_tag_resolves_same_as_lowercase() {
    // A mixed/upper-case internal tag must NOT raise UnknownLanguage; it must
    // resolve to the same BCP-47 value as the canonical lowercase form.
    let tm = sample_tag_map();

    let lower = resolve_lang_input(Some("x-gmeow-french"), &tm, None)
        .expect("lowercase internal tag must resolve");
    let upper = resolve_lang_input(Some("X-GMEOW-FRENCH"), &tm, None)
        .expect("UPPER-CASE internal tag must also resolve (Gap H2)");
    let mixed = resolve_lang_input(Some("X-Gmeow-French"), &tm, None)
        .expect("Mixed-Case internal tag must also resolve (Gap H2)");

    assert_eq!(
        lower.requested, upper.requested,
        "X-GMEOW-FRENCH must resolve to the same BCP-47 as x-gmeow-french"
    );
    assert_eq!(
        lower.requested, mixed.requested,
        "X-Gmeow-French must resolve to the same BCP-47 as x-gmeow-french"
    );
    assert_eq!(
        lower.requested,
        vec!["fr".to_owned()],
        "resolved tag must be fr"
    );
}

#[test]
fn retag_graph_mixed_case_internal_tag_retagged() {
    // A literal whose language tag is an upper/mixed-case internal tag must be
    // retagged to the public BCP-47 form, not left unchanged (bucket_key and
    // retagged_literal must both normalise the case).
    let tm = sample_tag_map();

    // Use X-GMEOW-ENGLISH (all caps) — maps to "en".
    let nt = nt_lang("https://e/s", "https://e/label", "Hello", "X-GMEOW-ENGLISH");
    let out = retag_graph(&nt, "ntriples", &tm).expect("retag must succeed for upper-case tag");
    let text = String::from_utf8(out).expect("utf8");
    assert!(
        text.contains("\"Hello\"@en"),
        "upper-case internal tag must be retagged to @en: {text}"
    );
    assert!(
        !text.contains("@X-GMEOW-ENGLISH"),
        "upper-case internal tag must not appear in output: {text}"
    );
}

#[test]
fn bucket_key_mixed_case_internal_tag_resolves() {
    // bucket_key must map a mixed-case internal tag to its BCP-47 bucket, not
    // leave it as an unmapped raw tag (which would fall through as an unknown
    // bucket and cause silent loss in select/filter paths).
    let tm = sample_tag_map();
    let lang = Some("X-Gmeow-Mandarin".to_owned());
    let key = bucket_key(&lang, &tm);
    assert_eq!(
        key, "zh",
        "mixed-case internal tag must resolve to its BCP-47 bucket key"
    );
}

#[test]
fn resolve_lang_input_respects_custom_available() {
    let tm = sample_tag_map();
    let avail = vec!["en".to_owned(), "fr".to_owned()];
    let sel = resolve_lang_input(Some("fr"), &tm, Some(&avail)).expect("custom");
    assert_eq!(sel.requested, vec!["fr".to_owned()]);
    assert_eq!(
        sel.available,
        BTreeSet::from(["en".to_owned(), "fr".to_owned()])
    );
}

#[test]
fn resolve_lang_input_rejects_tag_outside_custom_available() {
    let tm = sample_tag_map();
    let avail = vec!["en".to_owned(), "fr".to_owned()];
    // zh is in the tag_map but NOT in the custom available set → Err.
    let err = resolve_lang_input(Some("zh"), &tm, Some(&avail)).expect_err("outside");
    assert_eq!(err.tag, "zh");
}

// ── select_literal ──────────────────────────────────────────────────────

#[test]
fn select_literal_prefers_requested_language() {
    let tm = sample_tag_map();
    let literals = vec![
        desc("Bonjour", Some("x-gmeow-french")),
        desc("Hello", Some("x-gmeow-english")),
    ];
    let sel = select_literal(&literals, &["fr".to_owned()], &tm).expect("match");
    assert_eq!(sel.index, 0);
    assert_eq!(sel.retag_to, Some("fr".to_owned()));
    assert!(!sel.is_fallback);
}

#[test]
fn select_literal_falls_back_to_english() {
    let tm = sample_tag_map();
    let literals = vec![desc("Hello", Some("x-gmeow-english"))];
    let sel = select_literal(&literals, &["zh".to_owned()], &tm).expect("fallback");
    assert_eq!(sel.index, 0);
    assert_eq!(sel.retag_to, Some("en".to_owned()));
    assert!(sel.is_fallback);
}

#[test]
fn select_literal_prefers_internal_over_external_same_language() {
    let tm = sample_tag_map();
    // Two literals both land in the `en` public bucket: the internal-tagged one
    // (rank 0 carrier) must win over the external `en`-tagged one.
    let literals = vec![
        desc("external", Some("en")),
        desc("canonical", Some("x-gmeow-english")),
    ];
    let sel = select_literal(&literals, &["en".to_owned()], &tm).expect("match");
    assert_eq!(sel.index, 1);
    assert_eq!(sel.retag_to, Some("en".to_owned()));
}

// ── filter_literals ─────────────────────────────────────────────────────

#[test]
fn filter_literals_returns_all_requested_values() {
    let tm = sample_tag_map();
    let literals = vec![
        desc("a", Some("x-gmeow-french")),
        desc("b", Some("x-gmeow-french")),
        desc("c", Some("x-gmeow-english")),
    ];
    let sels = filter_literals(&literals, &["fr".to_owned()], &tm);
    assert_eq!(sels.len(), 2);
    let indices: BTreeSet<usize> = sels.iter().map(|s| s.index).collect();
    assert_eq!(indices, BTreeSet::from([0, 1]));
    assert!(sels.iter().all(|s| !s.is_fallback));
}

#[test]
fn filter_literals_falls_back_to_english() {
    let tm = sample_tag_map();
    let literals = vec![desc("Hello", Some("x-gmeow-english"))];
    let sels = filter_literals(&literals, &["zh".to_owned()], &tm);
    assert_eq!(sels.len(), 1);
    assert_eq!(sels[0].index, 0);
    assert!(sels[0].is_fallback);
}

// ── load_inverse_tag_map ────────────────────────────────────────────────

#[test]
fn load_inverse_tag_map_recovers_natural_tags() {
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
    let inv = load_inverse_tag_map(ttl.as_bytes(), "turtle").expect("parse");
    assert_eq!(inv.get("en"), Some(&"x-gmeow-english".to_owned()));
    assert_eq!(inv.get("fr"), Some(&"x-gmeow-french".to_owned()));
}

#[test]
fn load_inverse_tag_map_drops_ambiguous_bcp47() {
    // Two natural languages mapping to the SAME bcp47 `en` → `en` is dropped
    // (no fabrication), while the unambiguous `fr` survives.
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishUk a gmeow:Language ;
    lang:carrierTag "x-gmeow-english-uk" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
    let inv = load_inverse_tag_map(ttl.as_bytes(), "turtle").expect("parse");
    assert!(!inv.contains_key("en"), "ambiguous en must be dropped");
    assert_eq!(inv.get("fr"), Some(&"x-gmeow-french".to_owned()));
}

// ── graph passes ────────────────────────────────────────────────────────

/// Build a one-triple NT byte buffer: `<s> <p> "lex"@lang`.
fn nt_lang(subject: &str, predicate: &str, lexical: &str, lang: &str) -> Vec<u8> {
    format!("<{subject}> <{predicate}> \"{lexical}\"@{lang} .\n").into_bytes()
}

#[test]
fn retag_graph_to_internal_lifts_public_tags() {
    let mut inv = HashMap::new();
    inv.insert("en".to_owned(), "x-gmeow-english".to_owned());
    inv.insert("fr".to_owned(), "x-gmeow-french".to_owned());

    let mut nt = nt_lang("https://e/s", "https://e/label", "Hello", "en");
    nt.extend(nt_lang("https://e/s", "https://e/label", "Bonjour", "fr"));

    let out = retag_graph_to_internal(&nt, "ntriples", &inv).expect("retag");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("@x-gmeow-english"), "{text}");
    assert!(text.contains("@x-gmeow-french"), "{text}");
    assert!(!text.contains("\"Hello\"@en"), "{text}");
}

#[test]
fn filter_graph_keeps_only_selected_language() {
    let tm = sample_tag_map();
    let mut nt = nt_lang(
        "https://e/s",
        "https://e/label",
        "Bonjour",
        "x-gmeow-french",
    );
    nt.extend(nt_lang(
        "https://e/s",
        "https://e/label",
        "Hello",
        "x-gmeow-english",
    ));

    let preds = vec!["https://e/label".to_owned()];
    let out = filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("\"Bonjour\"@fr"), "{text}");
    assert!(
        !text.contains("Hello"),
        "english must be filtered out: {text}"
    );
}

#[test]
fn filter_graph_second_predicate_falls_back_to_english() {
    let tm = sample_tag_map();
    // Predicate `label` has fr+en; predicate `note` has ONLY en. A `fr` request
    // keeps fr on `label` and re-adds the en fallback on `note`.
    let mut nt = nt_lang(
        "https://e/s",
        "https://e/label",
        "Bonjour",
        "x-gmeow-french",
    );
    nt.extend(nt_lang(
        "https://e/s",
        "https://e/label",
        "Hello",
        "x-gmeow-english",
    ));
    nt.extend(nt_lang(
        "https://e/s",
        "https://e/note",
        "Note",
        "x-gmeow-english",
    ));

    let preds = vec!["https://e/label".to_owned(), "https://e/note".to_owned()];
    let out = filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("\"Bonjour\"@fr"), "{text}");
    assert!(
        text.contains("\"Note\"@en"),
        "english fallback on note: {text}"
    );
    assert!(!text.contains("\"Hello\""), "label english dropped: {text}");
}

#[test]
fn filter_graph_noop_is_byte_identical() {
    let tm = sample_tag_map();
    // The literal is already public `fr` and `fr` is requested → set-equality
    // skip leaves the group untouched, so the re-serialized output matches a
    // plain round-trip of the same input.
    let nt = nt_lang("https://e/s", "https://e/label", "Bonjour", "fr");
    let preds = vec!["https://e/label".to_owned()];
    let filtered = filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
    // A no-op filter must equal a plain parse→serialize round-trip.
    let roundtrip = retag_graph(&nt, "ntriples", &tm).expect("roundtrip");
    assert_eq!(filtered, roundtrip, "no-op filter must be byte-identical");
}

#[test]
fn retag_graph_preserves_typed_literal_and_bnode() {
    // A typed literal on a blank-node subject must survive retag_graph (a no-op
    // for it: no internal language tag) with no datatype loss and bnode intact.
    let tm = sample_tag_map();
    let nt = concat!(
        "_:b0 <https://e/count> \"5\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
        "_:b0 <https://e/label> \"Hi\"@x-gmeow-english .\n",
    )
    .as_bytes()
    .to_vec();

    let out = retag_graph(&nt, "ntriples", &tm).expect("retag");
    let text = String::from_utf8(out.clone()).expect("utf8");
    assert!(
        text.contains("XMLSchema#integer"),
        "typed literal datatype must survive: {text}"
    );
    assert!(text.contains("_:"), "blank node must survive: {text}");
    assert!(
        text.contains("@en"),
        "internal english retagged to public: {text}"
    );

    // Round-trip survives a re-parse with the datatype + bnode structure intact.
    let reparsed =
        parse_dataset(&out, "application/n-triples", None).expect("re-parse retagged output");
    let has_typed = reparsed.quad_refs().any(|qr| {
        matches!(
            qr.o,
            TermRef::Literal { datatype, .. }
                if dataset_iri(&reparsed, datatype) == "http://www.w3.org/2001/XMLSchema#integer"
        )
    });
    assert!(has_typed, "re-parsed typed literal must keep xsd:integer");
    let has_bnode = reparsed
        .quad_refs()
        .any(|qr| matches!(qr.s, TermRef::Blank { .. }));
    assert!(has_bnode, "re-parsed bnode subject must survive");
}

/// Resolve a `TermRef` datatype id to its IRI string (test helper).
fn dataset_iri(dataset: &purrdf::RdfDataset, datatype: purrdf::TermId) -> String {
    match dataset.resolve(datatype) {
        TermRef::Iri(iri) => iri.to_owned(),
        _ => String::new(),
    }
}

// ── reifier/annotation sync tests ───────────────────────────────────────

/// Build N-Triples bytes for a dataset that includes a quad, a reifier on it,
/// and an annotation on the reifier. Returns the serialized bytes.
fn build_nt_with_reifier(
    subject: &str,
    predicate: &str,
    lexical: &str,
    lang: &str,
    reifier_iri: &str,
    annotation_predicate: &str,
    annotation_value: &str,
) -> Vec<u8> {
    use purrdf::{
        RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple,
    };

    let subject_term = RdfTerm::iri(subject);
    let object_lit = RdfLiteral::language_tagged(lexical, lang);
    let object_term = RdfTerm::Literal(object_lit);
    let stmt = RdfTriple::new(subject_term.clone(), predicate, object_term.clone());

    let reifier_term = RdfTerm::iri(reifier_iri);
    let reifier = RdfReifier::new(reifier_term.clone(), stmt);
    let annotation = RdfAnnotation::new(
        reifier_term,
        annotation_predicate,
        RdfTerm::literal(RdfLiteral::simple(annotation_value)),
    );

    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(subject_term, predicate, object_term));
    builder.push_owned_reifier(&reifier);
    builder.push_owned_annotation(&annotation);

    let dataset = builder.freeze().expect("freeze");
    purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .expect("serialize")
}

#[test]
fn rewrite_graph_retag_updates_reifier_statement() {
    let tm = sample_tag_map();

    // Build a dataset: one quad with @x-gmeow-english, a reifier on it, and an
    // annotation on the reifier.
    let input = build_nt_with_reifier(
        "https://e/s",
        "https://e/label",
        "Hello",
        "x-gmeow-english",
        "https://e/r1",
        "https://e/confidence",
        "high",
    );

    let out = retag_graph(&input, "ntriples", &tm).expect("retag");
    let text = String::from_utf8(out.clone()).expect("utf8");

    // The base quad must be retagged to @en.
    assert!(text.contains("\"Hello\"@en"), "base quad retagged: {text}");
    assert!(
        !text.contains("@x-gmeow-english"),
        "internal tag must be gone: {text}"
    );

    // The reifier's statement object must also be updated to @en.
    // We check by parsing the output and inspecting owned_reifiers().
    let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");
    let reifier_stmt_updated = reparsed.owned_reifiers().any(|r| {
        if let purrdf::RdfTerm::Literal(lit) = &r.statement.object {
            lit.language.as_deref() == Some("en") && lit.lexical_form == "Hello"
        } else {
            false
        }
    });
    assert!(
        reifier_stmt_updated,
        "reifier statement object must be updated to @en; output:\n{text}"
    );

    // The annotation on <https://e/r1> must still be present.
    let has_annotation = reparsed
        .owned_annotations()
        .any(|a| a.reifier.to_string() == "<https://e/r1>");
    assert!(has_annotation, "annotation on r1 must survive: {text}");
}

#[test]
fn filter_graph_drops_reifier_for_dropped_literal() {
    use purrdf::{
        RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple,
    };

    // Dataset: two quads (English + French) on the same (s, p), with a reifier
    // only on the English quad.
    let subject_term = RdfTerm::iri("https://e/s");
    let pred = "https://e/label";
    let en_lit = RdfLiteral::language_tagged("Hello", "x-gmeow-english");
    let fr_lit = RdfLiteral::language_tagged("Bonjour", "x-gmeow-french");

    let en_stmt = RdfTriple::new(subject_term.clone(), pred, RdfTerm::Literal(en_lit.clone()));
    let reifier_term = RdfTerm::iri("https://e/r_en");
    let reifier = RdfReifier::new(reifier_term.clone(), en_stmt);
    let annotation = RdfAnnotation::new(
        reifier_term,
        "https://e/confidence",
        RdfTerm::literal(RdfLiteral::simple("high")),
    );

    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(
        subject_term.clone(),
        pred,
        RdfTerm::Literal(en_lit),
    ));
    builder.push_owned_quad(&RdfQuad::new(subject_term, pred, RdfTerm::Literal(fr_lit)));
    builder.push_owned_reifier(&reifier);
    builder.push_owned_annotation(&annotation);

    let dataset = builder.freeze().expect("freeze");
    let input = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .expect("serialize");

    let tm = sample_tag_map();
    let preds = vec![pred.to_owned()];
    // Request only French — English quad is dropped.
    let out = filter_graph(&input, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
    let text = String::from_utf8(out.clone()).expect("utf8");

    // French quad survives, retagged to @fr.
    assert!(text.contains("\"Bonjour\"@fr"), "french survives: {text}");
    // English quad is gone.
    assert!(!text.contains("Hello"), "english dropped: {text}");

    let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");

    // The reifier on the dropped English quad must not appear.
    let reifier_present = reparsed
        .owned_reifiers()
        .any(|r| r.reifier.to_string() == "<https://e/r_en>");
    assert!(
        !reifier_present,
        "reifier for dropped literal must be absent: {text}"
    );

    // The annotation on the dropped reifier must also be gone.
    let annotation_present = reparsed
        .owned_annotations()
        .any(|a| a.reifier.to_string() == "<https://e/r_en>");
    assert!(
        !annotation_present,
        "annotation for dropped reifier must be absent: {text}"
    );
}

#[test]
fn filter_graph_retag_updates_reifier_statement() {
    // Dataset: one quad with @x-gmeow-english, a reifier on it.
    // Request "en" → the quad is retagged to @en, and the reifier statement
    // must follow.
    let input = build_nt_with_reifier(
        "https://e/s",
        "https://e/label",
        "Hello",
        "x-gmeow-english",
        "https://e/r_en",
        "https://e/note",
        "tested",
    );

    let tm = sample_tag_map();
    let preds = vec!["https://e/label".to_owned()];
    let out = filter_graph(&input, "ntriples", &tm, &["en".to_owned()], &preds).expect("filter");
    let text = String::from_utf8(out.clone()).expect("utf8");

    // The quad must be retagged to @en.
    assert!(
        text.contains("\"Hello\"@en"),
        "quad retagged to @en: {text}"
    );
    assert!(
        !text.contains("@x-gmeow-english"),
        "internal tag gone: {text}"
    );

    let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");

    // The reifier's statement object must be @en, not @x-gmeow-english.
    let reifier_updated = reparsed.owned_reifiers().any(|r| {
        if let purrdf::RdfTerm::Literal(lit) = &r.statement.object {
            lit.language.as_deref() == Some("en") && lit.lexical_form == "Hello"
        } else {
            false
        }
    });
    assert!(
        reifier_updated,
        "reifier statement must be updated to @en: {text}"
    );
}

// ── public_literal / public_text ────────────────────────────────────────

/// Parse N-Triples bytes into a dataset for the graph-level selection tests.
fn parse_nt(nt: &[u8]) -> std::sync::Arc<purrdf::RdfDataset> {
    parse_dataset(nt, "application/n-triples", None).expect("parse")
}

#[test]
fn public_literal_retags_internal_carrier() {
    // An internal x-gmeow-english literal WITH a map entry wins and is retagged
    // to its public BCP-47 form.
    let tm = sample_tag_map();
    let nt = nt_lang("https://e/s", "https://e/label", "Hello", "x-gmeow-english");
    let ds = parse_nt(&nt);
    let lit = public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal present");
    assert_eq!(lit.lexical, "Hello");
    assert_eq!(lit.language, Some("en".to_owned()));
    assert_eq!(
        public_text(&ds, "https://e/s", "https://e/label", &tm),
        "Hello"
    );
}

#[test]
fn public_literal_prefers_carrier_over_other_internal() {
    // Two mapped internal literals: the x-gmeow-english carrier (rank 0) wins
    // over x-gmeow-french (rank 1) and is retagged to en.
    let tm = sample_tag_map();
    let mut nt = nt_lang(
        "https://e/s",
        "https://e/label",
        "Bonjour",
        "x-gmeow-french",
    );
    nt.extend(nt_lang(
        "https://e/s",
        "https://e/label",
        "Hello",
        "x-gmeow-english",
    ));
    let ds = parse_nt(&nt);
    let lit = public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal");
    assert_eq!(lit.lexical, "Hello");
    assert_eq!(lit.language, Some("en".to_owned()));
}

#[test]
fn public_literal_falls_back_to_external_tag_unchanged() {
    // No internal-mapped literal: the deterministic first (language, lexical)
    // candidate is returned with its original external tag preserved.
    let tm = sample_tag_map();
    let mut nt = nt_lang("https://e/s", "https://e/label", "Servus", "de");
    nt.extend(nt_lang("https://e/s", "https://e/label", "Hola", "es"));
    let ds = parse_nt(&nt);
    let lit = public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal");
    // (language, lexical) order: ("de","Servus") < ("es","Hola").
    assert_eq!(lit.lexical, "Servus");
    assert_eq!(lit.language, Some("de".to_owned()));
}

#[test]
fn public_literal_none_when_no_literal_object() {
    let tm = sample_tag_map();
    let nt = nt_lang("https://e/s", "https://e/label", "Hi", "x-gmeow-english");
    let ds = parse_nt(&nt);
    // Different predicate → no candidate.
    assert!(public_literal(&ds, "https://e/s", "https://e/other", &tm).is_none());
    assert_eq!(public_text(&ds, "https://e/s", "https://e/other", &tm), "");
}
