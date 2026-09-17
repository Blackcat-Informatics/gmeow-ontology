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
}

#[test]
fn translation_integrity_admits_preserved_code_and_notation_literals() {
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
        languages: vec!["fr".to_string()],
        internal_tag: BTreeMap::new(),
    };
    assert_eq!(
        t.lookup(
            "https://blackcatinformatics.ca/gmeow/Foo",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "fr"
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

/// `@prefix` declarations for every `prefix:` token `text` uses, so a bare CURIE
/// snippet can be parsed as a self-contained Turtle document.
fn synthesized_prefixes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (i, &c) in bytes.iter().enumerate() {
        if c != b':' {
            continue;
        }
        // `://` is an IRI scheme, not a CURIE prefix.
        if text[i..].starts_with("://") {
            continue;
        }
        // `rfind` on a `char` predicate returns the byte offset of the START of a
        // (possibly multi-byte) matching char; `j + 1` would land mid-codepoint for
        // any non-ASCII delimiter and panic on the slice below. Walk `char_indices`
        // instead and step past the FULL matched char's byte length.
        let start = text[..i]
            .char_indices()
            .rev()
            .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-'))
            .map_or(0, |(j, ch)| j + ch.len_utf8());
        let name = &text[start..i];
        if name.is_empty() || name.starts_with(|ch: char| ch.is_ascii_digit()) {
            continue;
        }
        names.insert(name.to_owned());
    }
    let mut out = String::new();
    for name in names {
        out.push_str(&format!("@prefix {name}: <urn:x-gmeow-test:{name}#> .\n"));
    }
    out
}

#[test]
fn synthesized_prefixes_does_not_panic_on_multibyte_delimiter_against_a_curie() {
    // The zh catalogues place CJK directly against a CURIE with no ASCII
    // separator, e.g. "线路值gmeow:useWhen". `rfind` returns the byte offset of
    // the START of the (3-byte) CJK char immediately before the prefix; naive
    // `+ 1` arithmetic lands one byte into that codepoint and panics the slice.
    let text = "线路值gmeow:useWhen";
    let prefixes = synthesized_prefixes(text);
    assert!(
        prefixes.contains("@prefix gmeow: "),
        "expected the `gmeow` prefix to be extracted cleanly, got: {prefixes:?}"
    );
}

/// Whether `text` is well-formed Turtle SYNTAX under the repo's own parser —
/// either as a standalone statement (or set of statements), or as a single term
/// in object position (which is what a bare literal or bare CURIE is).
fn parses_as_turtle(text: &str) -> bool {
    let prefixes = synthesized_prefixes(text);
    let as_document = format!("{prefixes}{text}\n");
    if purrdf::parse_dataset(as_document.as_bytes(), "text/turtle", None).is_ok() {
        return true;
    }
    let as_object = format!("{prefixes}<urn:x-gmeow-test:s> <urn:x-gmeow-test:p> {text} .\n");
    purrdf::parse_dataset(as_object.as_bytes(), "text/turtle", None).is_ok()
}

#[test]
fn every_msgid_the_fix_reclassifies_is_real_turtle_notation() {
    // CORPUS-WIDE, not a sample of three: sweep every committed catalogue entry,
    // emit the exact set of msgids whose classification FLIPS, and prove each one
    // is genuinely Turtle notation under purrdf — the repo's own parser, never an
    // eyeball or a regex. A hand-picked negative cannot see a hole; this can.
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
    // Direction check: the fix only ever ADMITS notation; nothing that used to be
    // an invariant may stop being one.
    let regressed: Vec<&String> = msgids
        .iter()
        .filter(|msgid| legacy_is_technical_invariant(msgid) && !is_technical_invariant(msgid))
        .collect();
    assert!(
        regressed.is_empty(),
        "the fix must only ADMIT notation, never withdraw it; withdrew: {regressed:#?}"
    );

    println!(
        "is_technical_invariant flip census: {} of {} distinct catalogue msgids across {} catalogs",
        flipped.len(),
        msgids.len(),
        paths.len()
    );
    // Every flip must land in ONE of two structurally-checkable classes; anything
    // else is a hole in the fix and reds this test:
    //
    //  (a) valid Turtle under purrdf — the snippets the fix exists to admit;
    //  (b) carrying NO ASCII word at all (a symbol-only literal such as `−∞`),
    //      admitted by the empty-after-stripping arm. English prose is ASCII
    //      words by construction, so a msgid with none cannot be prose.
    //
    // Both classes are ENUMERATED in the output, never quietly waved through.
    let mut symbol_only: Vec<&String> = Vec::new();
    let mut unexplained: Vec<&String> = Vec::new();
    for msgid in &flipped {
        if parses_as_turtle(msgid) {
            println!("  FLIPPED → invariant (valid Turtle): {msgid}");
        } else if ascii_words(msgid).is_empty() {
            println!("  FLIPPED → invariant (symbol-only, no ASCII word): {msgid}");
            symbol_only.push(msgid);
        } else {
            unexplained.push(msgid);
        }
    }
    println!(
        "  {} flipped msgid(s) are valid Turtle; {} are symbol-only",
        flipped.len() - symbol_only.len(),
        symbol_only.len()
    );
    assert!(
        unexplained.is_empty(),
        "every reclassified msgid must be valid Turtle under purrdf or carry no ASCII word at all; these are neither: {unexplained:#?}"
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
