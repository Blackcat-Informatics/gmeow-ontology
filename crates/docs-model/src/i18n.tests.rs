// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn expand_predicate_handles_curies_and_iris() {
    use crate::i18n_compile::expand_predicate;

    assert_eq!(
        expand_predicate("rdfs:label"),
        "http://www.w3.org/2000/01/rdf-schema#label"
    );
    assert_eq!(
        expand_predicate("skos:definition"),
        "http://www.w3.org/2004/02/skos/core#definition"
    );
    assert_eq!(
        expand_predicate("https://example.org/p"),
        "https://example.org/p"
    );
}

/// Real translations and language-independent notation remain admissible.
#[test]
fn translation_integrity_accepts_real_translations_and_invariants() {
    assert!(translation_has_integrity(
        "fr-CA",
        "Lifecycle state",
        "Etat du cycle de vie" // codespell:ignore vie
    ));
    assert!(translation_has_integrity(
        "zh-Hans",
        "Lifecycle state",
        "生命周期状态"
    ));
    assert!(translation_has_integrity("fr", "adoption", "adoption"));
    assert!(translation_has_integrity(
        "fr",
        "Creative Commons",
        "Creative Commons"
    ));
    assert!(translation_has_integrity("zh", "OWL 2 DL", "OWL 2 DL"));
}

/// Technical fragments must not hide copied or mixed English prose or missing Han.
#[test]
fn translation_integrity_rejects_copied_and_hybrid_english() {
    assert_eq!(
        translation_integrity_issue("fr", "Lifecycle state", "Lifecycle state"),
        Some("multi-word English source text was copied into msgstr")
    );
    assert_eq!(
        translation_integrity_issue(
            "fr",
            "A state in a lifecycle.",
            "Un etat. Use this construct only under the stated conditions."
        ),
        Some("msgstr retains multiple English prose tokens")
    );
    assert_eq!(
        translation_integrity_issue(
            "zh-Hans",
            "A state in a lifecycle.",
            "A state in a lifecycle."
        ),
        Some("multi-word English source text was copied into msgstr")
    );
    assert_eq!(
        translation_integrity_issue("zh-Hans", "Lifecycle state", "Lifecycle status"),
        Some("Chinese prose translation contains no Han characters")
    );
    assert_eq!(
        translation_integrity_issue("zh-Hans", "Permission", "Permission"),
        Some("Chinese prose translation contains no Han characters")
    );
    for text in [
        "The value is false.",
        "The R-hat must converge.",
        "red-hat",
        "Example: ex:record a logic:JournalBoundary .",
        "Illustration: ex:M math:manifoldStructureKind math:smoothStructure .",
        "INSERT DATA { ex: ex:p . }",
        "Example: INSERT DATA { ex: ex:p ex:o . }",
    ] {
        assert_eq!(
            translation_integrity_issue("fr", text, text),
            Some("multi-word English source text was copied into msgstr"),
            "{text}"
        );
    }
    for text in [
        "Un graphe. Use this <https://example.org/where/this> only.",
        "Un graphe. <where this>",
        "Un graphe. <https://example.org/where/this",
    ] {
        assert_eq!(
            translation_integrity_issue("fr", "A graph.", text),
            Some("msgstr retains multiple English prose tokens"),
            "{text}"
        );
    }
    assert_eq!(
        translation_integrity_issue(
            "zh-Hans",
            "Lifecycle state",
            "Lifecycle status <https://example.org/where/this>"
        ),
        Some("Chinese prose translation contains no Han characters")
    );
}

/// Preserved technical text is data; surrounding prose still requires translation.
#[test]
fn translation_integrity_admits_preserved_code_and_notation_literals() {
    // Each native grammar must preserve its own meaning of `/`: a SPARQL path
    // separator versus a character in a Turtle prefixed local name.
    let path_query = "SELECT ?value WHERE { : a:first/b:second ?value . }";
    let bindings = "PREFIX : <urn:default:>\nPREFIX a: <urn:first:>\nPREFIX b: <urn:second:>\n";
    assert!(
        purrdf::sparql::SparqlParser::new()
            .parse_query(&format!("{bindings}{path_query}"))
            .is_ok()
    );
    let slash_turtle = "ex: ex:p ex:a/b .";
    let document = format!("PREFIX ex: <urn:example:>\n{slash_turtle}");
    assert!(purrdf::parse_dataset(document.as_bytes(), "text/turtle", None).is_ok());
    // Faithful translations must keep SPARQL queries, wire values, and SPARQL
    // variables verbatim; the English keywords inside them (this/where/use/when)
    // must NOT be counted as copied English prose.
    assert!(translation_has_integrity(
        "fr",
        "Example: ex:g logic:sparqlTarget ex:focus, \"SELECT ?this WHERE { ?this ex:reviewed true }\" .",
        "Exemple : ex:g logic:sparqlTarget ex:focus, \"SELECT ?this WHERE { ?this ex:reviewed true }\" ."
    ));
    assert!(translation_has_integrity(
        "zh",
        "The logic:ProseField for a term's gmeow:useWhen. Wire value `use-when`.",
        "对应某词项 gmeow:useWhen 的 logic:ProseField。线路值 `use-when`。"
    ));
    assert!(translation_has_integrity(
        "fr",
        "lowered to a SELECT $this ?far … GROUP BY $this ?far HAVING(…)",
        "abaissée en un SELECT $this ?far … GROUP BY $this ?far HAVING(…)"
    ));
    for text in [
        "ex:record gmeow:evidenceCorpusStale false .",
        "ex:record gmeow:evidenceCorpusStale true .",
        "gmeow:pipeline-dataflow-mappings-compile-logic gmeow:flowReadsHandle <https://blackcatinformatics.ca/gmeow/graph/logic> .",
        "<urn:example:where:this>",
        "R-hat",
        "x-hat",
        "ex: a owl:Class .",
        "ex:s ex:p ex:o .",
        "é: ex:p ex:o .",
        "e\u{301}: ex:p ex:o .",
        "ex.part: ex:p ex:o .",
        "e·x: ex:p ex:o .",
        "ex: ex:p _:b .",
        "INSERT DATA { ex: ex:p ex:o . }",
        "DELETE WHERE { ex: ex:p ex:o . }",
        path_query,
        slash_turtle,
        ": a :Class .",
        "PREFIX logic: <https://blackcatinformatics.ca/logic/> SELECT ?record WHERE { ?record a logic:CorrespondenceSource . }",
    ] {
        for language in ["fr", "zh"] {
            assert!(
                translation_has_integrity(language, text, text),
                "{language}: {text}"
            );
        }
    }
    assert!(translation_has_integrity(
        "fr",
        "The graph <https://example.org/where/this>.",
        "Le graphe <https://example.org/where/this>."
    ));
    // But copied English PROSE outside any code span is still rejected.
    assert_eq!(
        translation_integrity_issue(
            "fr",
            "A guard `x`.",
            "Un garde `x`. Use this only when under the stated conditions."
        ),
        Some("msgstr retains multiple English prose tokens")
    );
}

#[test]
fn ui_string_falls_back_to_english() {
    let cat = UiCatalog::default();
    assert_eq!(ui_string("nav_home", "fr", &cat), "Home");
    assert_eq!(ui_string("nav_home", ENGLISH, &cat), "Home");
    assert_eq!(ui_string("category_class", "zh", &cat), "Classes");
}

#[test]
fn ui_string_uses_override_when_present() {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        ("fr".to_string(), "nav_home".to_string()),
        "Accueil".to_string(),
    );
    let cat = UiCatalog { overrides };
    assert_eq!(ui_string("nav_home", "fr", &cat), "Accueil");
    // Other languages / English still fall back.
    assert_eq!(ui_string("nav_home", "zh", &cat), "Home");
    assert_eq!(ui_string("nav_home", ENGLISH, &cat), "Home");
}

#[test]
fn ui_templates_key_count_is_pinned() {
    // 60 legacy nav/page/section/category/footer keys + 159 `body_*` chrome
    // keys routing the Markdown body renderers through the override catalog
    // (incl. the pipeline-stage attach surface: body_pipeline_attaches[_blob]).
    assert_eq!(UI_TEMPLATES.len(), 219);
}

#[test]
fn translations_lookup_indexes_by_full_predicate() {
    // Build a Translations index directly (without a catalog) to exercise
    // lookup + language detection semantics.
    let mut by_key = BTreeMap::new();
    by_key.insert(
        (
            "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            "fr".to_string(),
        ),
        TranslatedValue {
            value: "Fou".to_string(),
            fuzzy: false,
        },
    );
    // A fuzzy (unreviewed) value is stored but must NOT surface at lookup.
    by_key.insert(
        (
            "https://blackcatinformatics.ca/gmeow/Fuzzy".to_string(),
            "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            "fr".to_string(),
        ),
        TranslatedValue {
            value: "Approximatif".to_string(),
            fuzzy: true,
        },
    );
    let t = Translations {
        by_key,
        fixtures: BTreeMap::new(),
        languages: vec!["fr".to_string()],
        internal_tag: BTreeMap::new(),
    };
    assert_eq!(
        t.lookup(
            "https://blackcatinformatics.ca/gmeow/Foo",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "FR"
        ),
        Some("Fou")
    );
    // A stored fuzzy value falls back to English (lookup returns None).
    assert_eq!(
        t.lookup(
            "https://blackcatinformatics.ca/gmeow/Fuzzy",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "fr"
        ),
        None
    );
    // English carrier always returns None (model value is used).
    assert_eq!(
        t.lookup(
            "https://blackcatinformatics.ca/gmeow/Foo",
            "http://www.w3.org/2000/01/rdf-schema#label",
            ENGLISH
        ),
        None
    );
    assert_eq!(available_languages(&t), vec!["english", "fr"]);
    // Default internal-tag derivation.
    assert_eq!(t.internal_tag("fr"), "x-gmeow-fr");
    assert_eq!(t.internal_tag(ENGLISH), "x-gmeow-english");
}

#[test]
fn translations_serde_round_trip_is_identity() {
    let mut by_key = BTreeMap::new();
    by_key.insert(
        (
            "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            "fr".to_string(),
        ),
        TranslatedValue {
            value: "Fou".to_string(),
            fuzzy: false,
        },
    );
    let mut internal_tag = BTreeMap::new();
    internal_tag.insert("fr".to_string(), "x-gmeow-french".to_string());
    // `zh` appears in internal_tag / languages but has ZERO by_key rows — a
    // re-derived `languages`/`internal_tag` would drop it and break identity.
    internal_tag.insert("zh".to_string(), "x-gmeow-chinese".to_string());
    let original = Translations {
        by_key,
        fixtures: BTreeMap::new(),
        languages: vec!["fr".to_string(), "zh".to_string()],
        internal_tag,
    };
    let json = serde_json::to_string(&original).expect("serialize Translations");
    let restored: Translations = serde_json::from_str(&json).expect("deserialize Translations");
    assert_eq!(original, restored);
    // The `zh` internal tag survived despite having no translated entries.
    assert_eq!(restored.internal_tag("zh"), "x-gmeow-chinese");
}

// ── The `is_technical_invariant` structural facts, and their corpus proof ──

#[test]
fn a_bare_turtle_a_is_a_keyword_not_an_english_article() {
    // The whole point: Turtle's `rdf:type` shorthand fails every other notation
    // test, so before this the snippet below was classified as English prose.
    assert!(is_technical_invariant(
        "gmeow:paymentMethodCash a gmeow:PaymentMethod ."
    ));
    assert!(is_technical_invariant("ex:x a owl:Class ; rdfs:label ."));
}

#[test]
fn quoted_turtle_object_literals_are_data_not_english() {
    // A double-quoted span is a Turtle object literal — DATA. Stripping it leaves
    // notation to classify; when it leaves NOTHING, the input was pure data and is
    // an invariant (without that arm the `!tokens.is_empty()` guard would make the
    // function stricter for exactly the input the stripping is meant to admit).
    assert!(is_technical_invariant(
        "gmeow:me foaf:account \"@lillith@fosstodon.org\" ."
    ));
    assert!(is_technical_invariant("\"@lillith@fosstodon.org\""));
    assert!(
        !is_technical_invariant("   "),
        "blank input is not an invariant — there is nothing to be invariant about"
    );
}

#[test]
fn genuine_english_prose_is_still_caught() {
    // The negative controls the fix must not break: an indefinite article in front
    // of an ordinary noun is prose, not a Turtle `a` keyword.
    assert!(!is_technical_invariant("a Person"));
    assert!(!is_technical_invariant("a payment method for an account"));
    assert!(!is_technical_invariant(
        "A state in a lifecycle, never the lifecycle itself."
    ));
}

/// The repository root, from this crate's manifest.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the docs crate sits two levels under the repo root")
}

/// Every `slices/**/i18n/*.po` catalog path, sorted.
fn catalog_paths(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|x| x == "po") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("slices"), &mut out);
    out.sort();
    out
}

/// The classification BEFORE the Turtle-keyword / quoted-literal fix — kept here,
/// and only here, as the baseline the flip set is measured against.
fn legacy_is_technical_invariant(text: &str) -> bool {
    if text.eq_ignore_ascii_case("Creative Commons") {
        return true;
    }
    if text.contains("://") && !text.chars().any(char::is_whitespace) {
        return true;
    }
    let tokens: Vec<&str> = text
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':')))
        .filter(|token| !token.is_empty())
        .collect();
    !tokens.is_empty()
        && tokens.iter().all(|token| {
            token.chars().all(|ch| ch.is_ascii_digit())
                || token.contains(':')
                || (token.len() > 1
                    && token
                        .chars()
                        .filter(|ch| ch.is_ascii_alphabetic())
                        .all(|ch| ch.is_ascii_uppercase()))
        })
}

#[test]
fn synthesized_prefixes_does_not_panic_on_multibyte_delimiter_against_a_curie() {
    // CJK characters are legal prefix material, not delimiters. Preserve the
    // whole native prefix; an actual multibyte non-PN boundary fails lexing
    // rather than slicing into a UTF-8 codepoint or inventing a suffix prefix.
    let text = "线路值gmeow:useWhen";
    let tokens = purrdf::sparql::lexer::tokenize_turtle(text)
        .expect("CJK characters are legal prefix material");
    let prefixes = synthesized_prefixes(&tokens);
    assert!(
        prefixes.contains("PREFIX 线路值gmeow: "),
        "expected the whole Unicode prefix, got: {prefixes:?}"
    );
    assert!(purrdf::sparql::lexer::tokenize_turtle("×gmeow:useWhen").is_err());
}

/// Catalogue-wide notation admissions must have syntax independent of prose.
#[test]
fn every_reclassified_catalog_msgid_is_technical_notation() {
    // Compare every committed catalogue msgid with the frozen lexical baseline.
    // Newly admitted values must have Turtle, SPARQL, symbol-only or single-letter
    // hat syntax. Native parsers validate both languages. Baseline notation stays
    // admitted; baseline prose must retain its translation requirement.
    let root = repo_root();
    let paths = catalog_paths(&root);
    assert!(
        !paths.is_empty(),
        "the corpus proof needs the committed catalogs at {}",
        root.join("slices").display()
    );

    let mut msgids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for path in &paths {
        let text = std::fs::read_to_string(path).expect("catalog is readable");
        let entries =
            crate::i18n_compile::parse_po(&text, false).expect("committed catalogs parse");
        for entry in entries {
            if !entry.msgid.is_empty() {
                msgids.insert(entry.msgid);
            }
        }
    }

    let flipped: Vec<&String> = msgids
        .iter()
        .filter(|msgid| is_technical_invariant(msgid) && !legacy_is_technical_invariant(msgid))
        .collect();
    // The frozen lexical baseline can mistake prose introductions for namespaces.
    // No withdrawal may discard independently valid notation; every withdrawn
    // source must instead trigger the existing copied-prose refusal.
    let regressed: Vec<&String> = msgids
        .iter()
        .filter(|msgid| legacy_is_technical_invariant(msgid) && !is_technical_invariant(msgid))
        .collect();
    for msgid in &regressed {
        assert!(
            !parses_as_turtle(msgid)
                && !parses_as_sparql(msgid)
                && !ascii_words(msgid).is_empty()
                && !matches!(msgid.as_bytes(), [letter, b'-', b'h', b'a', b't'] if letter.is_ascii_alphabetic()),
            "independently valid baseline notation must remain admitted: {msgid}"
        );
        assert_eq!(
            translation_integrity_issue("fr", msgid, msgid),
            Some("multi-word English source text was copied into msgstr"),
            "withdrawn prose must still require translation: {msgid}"
        );
        println!("  WITHDRAWN → prose (native syntax refused): {msgid}");
    }
    println!(
        "{} baseline prose msgid(s) withdrawn; no independently valid notation withdrawn",
        regressed.len()
    );

    println!(
        "is_technical_invariant flip census: {} of {} distinct catalogue msgids across {} catalogs",
        flipped.len(),
        msgids.len(),
        paths.len()
    );
    // Every newly admitted value must belong to one of four syntax categories:
    //
    //  (a) valid Turtle under purrdf;
    //  (b) a valid SPARQL query under the native parser;
    //  (c) carrying NO ASCII word at all (a symbol-only literal such as `−∞`),
    //      admitted by the empty-after-stripping arm. English prose is ASCII
    //      words by construction, so a msgid with none cannot be prose.
    //  (d) one ASCII letter followed by `-hat`, the conventional notation spelling.
    //
    // All classes are ENUMERATED in the output, never quietly waved through.
    let mut symbol_only: Vec<&String> = Vec::new();
    let mut hat_notation: Vec<&String> = Vec::new();
    let mut sparql: Vec<&String> = Vec::new();
    let mut unexplained: Vec<&String> = Vec::new();
    for msgid in &flipped {
        if parses_as_turtle(msgid) {
            println!("  FLIPPED → invariant (valid Turtle): {msgid}");
        } else if parses_as_sparql(msgid) {
            println!("  FLIPPED → invariant (valid SPARQL): {msgid}");
            sparql.push(msgid);
        } else if ascii_words(msgid).is_empty() {
            println!("  FLIPPED → invariant (symbol-only, no ASCII word): {msgid}");
            symbol_only.push(msgid);
        } else if matches!(msgid.as_bytes(), [letter, b'-', b'h', b'a', b't'] if letter.is_ascii_alphabetic())
        {
            println!("  FLIPPED → invariant (single-letter hat notation): {msgid}");
            hat_notation.push(msgid);
        } else {
            unexplained.push(msgid);
        }
    }
    println!(
        "  {} flipped msgid(s) are valid Turtle; {} are valid SPARQL; {} are symbol-only; {} are single-letter hat notation",
        flipped.len() - sparql.len() - symbol_only.len() - hat_notation.len() - unexplained.len(),
        sparql.len(),
        symbol_only.len(),
        hat_notation.len()
    );
    assert!(
        unexplained.is_empty(),
        "every reclassified msgid must be native Turtle/SPARQL, carry no ASCII word or be single-letter hat notation; these match none: {unexplained:#?}"
    );
}

#[test]
fn corpus_english_prose_stays_non_invariant() {
    // The other half of the corpus proof: admitting notation must not admit PROSE.
    // Every committed msgid carrying three or more English function words is
    // English by construction, and must still be classified as prose.
    const FUNCTION_WORDS: &[&str] = &[
        "the", "of", "that", "is", "and", "for", "in", "to", "with", "this", "an", "its",
    ];
    let root = repo_root();
    let mut checked = 0usize;
    let mut leaked: Vec<String> = Vec::new();
    for path in catalog_paths(&root) {
        let text = std::fs::read_to_string(&path).expect("catalog is readable");
        let entries =
            crate::i18n_compile::parse_po(&text, false).expect("committed catalogs parse");
        for entry in entries {
            let words = ascii_words(&entry.msgid);
            let hits = words
                .iter()
                .filter(|w| FUNCTION_WORDS.contains(&w.as_str()))
                .count();
            if hits < 3 {
                continue;
            }
            checked += 1;
            if is_technical_invariant(&entry.msgid) {
                leaked.push(entry.msgid.clone());
            }
        }
    }
    assert!(
        checked > 100,
        "the negative control must see a real corpus, saw only {checked} prose msgids"
    );
    assert!(
        leaked.is_empty(),
        "English prose must never be classified as a technical invariant: {leaked:#?}"
    );
}

#[test]
fn ui_catalog_serde_round_trip_is_identity() {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        ("fr".to_string(), "nav_home".to_string()),
        "Accueil".to_string(),
    );
    overrides.insert(
        ("zh".to_string(), "nav_home".to_string()),
        "首页".to_string(),
    );
    let original = UiCatalog { overrides };
    let json = serde_json::to_string(&original).expect("serialize UiCatalog");
    let restored: UiCatalog = serde_json::from_str(&json).expect("deserialize UiCatalog");
    assert_eq!(original, restored);
    assert_eq!(ui_string("nav_home", "fr", &restored), "Accueil");
}
