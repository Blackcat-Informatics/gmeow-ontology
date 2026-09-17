// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Coverage assertions consume the producer's original reference-catalog index.

use super::super::language_catalog::{CHANNEL, Index};
use std::collections::{BTreeSet, HashSet};

/// The GMEOW namespace prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The lang: grounding namespace prefix.
const LANG: &str = "https://blackcatinformatics.ca/lang/";
/// The catalog ontology IRI used as the `rdfs:isDefinedBy` object.
const CATALOG_IRI: &str = "https://blackcatinformatics.ca/gmeow/imports/languages-reference";
/// Glottolog languoid IRI base for `skos:exactMatch` alignments.
const GLOTTOLOG_BASE: &str = "https://glottolog.org/resource/languoid/id/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";

/// The complete ISO 639-1 two-letter code set (184 entries, stable since 2000).
const EXPECTED_ISO639_1_CODES: &[&str] = &[
    "aa", "ab", "ae", "af", "ak", "am", "an", "ar", "as", "av", "ay", "az", "ba", "be", "bg", "bi",
    "bm", "bn", "bo", "br", "bs", "ca", "ce", "ch", "co", "cr", "cs", "cu", "cv", "cy", "da", "de",
    "dv", "dz", "ee", "el", "en", "eo", "es", "et", "eu", "fa", "ff", "fi", "fj", "fo", "fr", "fy",
    "ga", "gd", "gl", "gn", "gu", "gv", "ha", "he", "hi", "ho", "hr", "ht", "hu", "hy", "hz", "ia",
    "id", "ie", "ig", "ii", "ik", "io", "is", "it", "iu", "ja", "jv", "ka", "kg", "ki", "kj", "kk",
    "kl", "km", "kn", "ko", "kr", "ks", "ku", "kv", "kw", "ky", "la", "lb", "lg", "li", "ln", "lo",
    "lt", "lu", "lv", "mg", "mh", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my", "na", "nb", "nd",
    "ne", "ng", "nl", "nn", "no", "nr", "nv", "ny", "oc", "oj", "om", "or", "os", "pa", "pi", "pl",
    "ps", "pt", "qu", "rm", "rn", "ro", "ru", "rw", "sa", "sc", "sd", "se", "sg", "sh", "si", "sk",
    "sl", "sm", "sn", "so", "sq", "sr", "ss", "st", "su", "sv", "sw", "ta", "te", "tg", "th", "ti",
    "tk", "tl", "tn", "to", "tr", "ts", "tt", "tw", "ty", "ug", "uk", "ur", "uz", "ve", "vi", "vo",
    "wa", "wo", "xh", "yi", "yo", "za", "zh", "zu",
];

fn index() -> &'static Index {
    static INDEX: std::sync::OnceLock<Index> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let bytes = crate::fixture::authenticated_artifact(
            &gmeow_conformance::paths::repo_root(),
            "stage-conformance",
            CHANNEL,
        )
        .expect("authenticated original language-reference observations");
        bincode::deserialize(&bytes).expect("typed language-reference index")
    })
}

impl Index {
    fn iris(&self, subject: &str, predicate: &str) -> Option<&BTreeSet<String>> {
        self.obj_iris
            .get(&(subject.to_owned(), predicate.to_owned()))
    }

    fn lits(&self, subject: &str, predicate: &str) -> Option<&BTreeSet<String>> {
        self.obj_lits
            .get(&(subject.to_owned(), predicate.to_owned()))
    }

    fn has_iri(&self, subject: &str, predicate: &str, object: &str) -> bool {
        self.iris(subject, predicate)
            .is_some_and(|set| set.contains(object))
    }

    fn has_any_lit(&self, subject: &str, predicate: &str) -> bool {
        self.lits(subject, predicate).is_some_and(|s| !s.is_empty())
    }

    /// Subjects that are `rdf:type <class>` AND `rdfs:isDefinedBy <CATALOG_IRI>`.
    fn catalog_subjects_of_type(&self, class: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for ((s, p), objs) in &self.obj_iris {
            if p == RDF_TYPE
                && objs.contains(class)
                && self.has_iri(s, RDFS_IS_DEFINED_BY, CATALOG_IRI)
            {
                out.insert(s.clone());
            }
        }
        out
    }
}

/// Mirror of `test_reference_catalog_languages_are_annotated_and_aligned`:
/// the catalog ISO 639-1 code set equals the complete 184-entry set, and every
/// catalog natural language carries label + definition + languageCode + a
/// `skos:exactMatch` alignment — and authors NO `gmeow:bcp47Tag` (retired to a
/// generated projection by the lang: graft).
#[test]
fn reference_catalog_languages_are_annotated_and_aligned() {
    let index = index();
    let all_languages = index.catalog_subjects_of_type(&format!("{GMEOW}Language"));
    assert!(
        !all_languages.is_empty(),
        "catalog must define gmeow:Language individuals"
    );

    // Programming languages are gmeow:Language too (distinguished by
    // lang:signSystemKind lang:programmingLanguageKind). The ISO 639-1 sweep and
    // the registry-annotation checks below are about NATURAL languages, so filter
    // the programming languages out.
    let sign_kind = format!("{LANG}signSystemKind");
    let prog_kind = format!("{LANG}programmingLanguageKind");
    let languages: BTreeSet<String> = all_languages
        .iter()
        .filter(|l| !index.has_iri(l, &sign_kind, &prog_kind))
        .cloned()
        .collect();

    // ISO 639-1 two-letter languageCode set equals the complete code set
    // (Python: `catalog_iso1_codes == EXPECTED_ISO639_1_CODES`).
    let code_prop = format!("{GMEOW}languageCode");
    let mut catalog_iso1: BTreeSet<String> = BTreeSet::new();
    for lang in &languages {
        if let Some(codes) = index.lits(lang, &code_prop) {
            for code in codes {
                if code.chars().count() == 2 {
                    catalog_iso1.insert(code.clone());
                }
            }
        }
    }
    let expected: BTreeSet<String> = EXPECTED_ISO639_1_CODES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let missing: Vec<&String> = expected.difference(&catalog_iso1).collect();
    let unexpected: Vec<&String> = catalog_iso1.difference(&expected).collect();
    assert!(
        catalog_iso1 == expected,
        "ISO 639-1 code set mismatch: missing={missing:?}; unexpected={unexpected:?}"
    );

    let bcp_prop = format!("{GMEOW}bcp47Tag");
    for lang in &languages {
        // The three project translation targets (English/French/Mandarin) are
        // unified with the grounding lang: sign systems (lang:english/french/
        // mandarin), which carry their rdfs:label + skos:definition in
        // slices/grounding/lang/module.ttl; the catalog only ENRICHES them with
        // codes, alignments and appellations. So label/definition are asserted
        // for the catalog-owned (gmeow:-namespace) languages only.
        if lang.starts_with(GMEOW) {
            assert!(
                index.has_any_lit(lang, RDFS_LABEL),
                "<{lang}> missing rdfs:label"
            );
            assert!(
                index.has_any_lit(lang, SKOS_DEFINITION),
                "<{lang}> missing skos:definition"
            );
        }
        // Regression gate: the lang: graft retired `gmeow:bcp47Tag` as an authored
        // property — it is now a GENERATED projection derived from carrier variety
        // structure. No catalog language may author it (an undefined-but-authored
        // predicate would otherwise slip past the namespace-only coverage gate).
        assert!(
            !index.has_any_lit(lang, &bcp_prop),
            "<{lang}> authors gmeow:bcp47Tag, but it is retired as an authored property \
             (generated projection only)"
        );
        assert!(
            index.has_any_lit(lang, &code_prop),
            "<{lang}> missing gmeow:languageCode"
        );
        assert!(
            index
                .iris(lang, SKOS_EXACT_MATCH)
                .is_some_and(|s| !s.is_empty()),
            "<{lang}> missing skos:exactMatch alignment"
        );
    }
}

/// Post-graft twin of `test_reference_catalog_writing_systems_are_annotated`:
/// scripts are grounded as `lang:Script` (ISO 15924 on `skos:notation`), and the
/// language↔script binding is a `lang:Orthography` (`lang:orthographyFor` +
/// `lang:usesScript`). Every `lang:Script` DEFINED IN THE CATALOG carries
/// `rdfs:label` + `skos:definition` + `skos:notation`, and the catalog mints at
/// least one `lang:Orthography` bound to a catalog language and a script.
#[test]
fn reference_catalog_writing_systems_are_annotated() {
    let index = index();
    let script_type = format!("{LANG}Script");
    let skos_notation = "http://www.w3.org/2004/02/skos/core#notation";

    // Catalog-defined scripts (the reused lang:latinScript / lang:hanScript are
    // defined in slices/grounding/lang/module.ttl, not here).
    let scripts = index.catalog_subjects_of_type(&script_type);
    assert!(
        !scripts.is_empty(),
        "catalog must define lang:Script individuals"
    );
    for ws in &scripts {
        assert!(
            index.has_any_lit(ws, RDFS_LABEL),
            "<{ws}> missing rdfs:label"
        );
        assert!(
            index.has_any_lit(ws, SKOS_DEFINITION),
            "<{ws}> missing skos:definition"
        );
        assert!(
            index.has_any_lit(ws, skos_notation),
            "<{ws}> missing skos:notation (ISO 15924)"
        );
    }

    // The language↔script binding is now a lang:Orthography.
    let orthographies = index.catalog_subjects_of_type(&format!("{LANG}Orthography"));
    assert!(
        !orthographies.is_empty(),
        "catalog must mint lang:Orthography bindings for its languages' scripts"
    );
    let orthography_for = format!("{LANG}orthographyFor");
    let uses_script = format!("{LANG}usesScript");
    for orth in &orthographies {
        assert!(
            index
                .iris(orth, &orthography_for)
                .is_some_and(|s| !s.is_empty()),
            "<{orth}> missing lang:orthographyFor"
        );
        assert!(
            index
                .iris(orth, &uses_script)
                .is_some_and(|s| !s.is_empty()),
            "<{orth}> missing lang:usesScript"
        );
    }
}

/// Post-graft twin of `test_reference_catalog_programming_languages_typed`: the
/// removed `gmeow:ProgrammingLanguage` subclass is retired; a programming language
/// is a `gmeow:Language` distinguished by
/// `lang:signSystemKind lang:programmingLanguageKind`.
#[test]
fn reference_catalog_programming_languages_typed() {
    let index = index();
    let lang_type = format!("{GMEOW}Language");
    let sign_kind = format!("{LANG}signSystemKind");
    let prog_kind = format!("{LANG}programmingLanguageKind");
    // The exact IRI list checked by the Python case.
    for local in [
        "langPython",
        "langRust",
        "langJavaScript",
        "langTypeScript",
        "langJava",
    ] {
        let iri = format!("{GMEOW}{local}");
        assert!(
            index.has_iri(&iri, RDF_TYPE, &lang_type),
            "<{iri}> must be typed gmeow:Language"
        );
        assert!(
            index.has_iri(&iri, &sign_kind, &prog_kind),
            "<{iri}> must carry lang:signSystemKind lang:programmingLanguageKind"
        );
    }
}

/// Mirror of `test_reference_catalog_glottolog_alignments`: catalog-only natural
/// languages link to Glottolog via `skos:exactMatch`.
#[test]
fn reference_catalog_glottolog_alignments() {
    let index = index();
    for local in ["langJapanese", "langArabic", "langHindi", "langSpanish"] {
        let iri = format!("{GMEOW}{local}");
        assert!(
            index.has_iri(&iri, RDFS_IS_DEFINED_BY, CATALOG_IRI),
            "<{iri}> must be defined by the reference catalog"
        );
        let glottos: Vec<&String> = index
            .iris(&iri, SKOS_EXACT_MATCH)
            .into_iter()
            .flatten()
            .filter(|m| m.starts_with(GLOTTOLOG_BASE))
            .collect();
        assert!(
            !glottos.is_empty(),
            "<{iri}> missing Glottolog skos:exactMatch"
        );
    }
}

/// Belt-and-braces coverage guard: the catalog defines the full ISO 639-1 sweep,
/// so the natural-language count is at least the 184-entry threshold the Python
/// sweep implies. Uses the same `>=` shape as a coverage floor (the set-equality
/// test above is the exact authority; this names a clearer failure if the catalog
/// shrinks). Each over/under is reported by subject for a helpful hard-fail.
#[test]
fn reference_catalog_language_count_meets_floor() {
    let index = index();
    let languages = index.catalog_subjects_of_type(&format!("{GMEOW}Language"));
    // The ISO 639-1 codes alone require >= 184 distinct natural languages.
    let floor = EXPECTED_ISO639_1_CODES.len();
    assert!(
        languages.len() >= floor,
        "catalog natural-language count {} is below the ISO 639-1 floor of {floor}; \
         present subjects: {:?}",
        languages.len(),
        languages.iter().collect::<HashSet<_>>()
    );
}
