// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reference-catalog data audits.
//!
//! These integration tests assert the *shape and coverage* of the GMEOW language
//! reference catalog (`imports/languages-reference.ttl`):
//!
//!   - every catalog natural language carries `rdfs:label`, `skos:definition`,
//!     `gmeow:languageTag`, `gmeow:bcp47Tag`, `gmeow:languageCode`, and a
//!     `skos:exactMatch` alignment to an external authority;
//!   - the catalog's ISO 639-1 two-letter `languageCode` set equals the complete
//!     184-entry ISO 639-1 code set;
//!   - writing systems referenced by the catalog carry `rdfs:label` +
//!     `skos:definition` and are typed `gmeow:WritingSystem`;
//!   - the named programming languages are typed `gmeow:ProgrammingLanguage`;
//!   - catalog natural languages link to Glottolog via `skos:exactMatch`;
//!   - `load_tag_map` is deterministic over the catalog and covers the core +
//!     catalog internal tags.
//!
//! The pure `load_tag_map` / `load_inverse_tag_map` / `retag_graph_to_internal`
//! *logic* is exercised by inline unit tests in
//! `crates/validate/src/language_tags.rs`; here we only assert CATALOG COVERAGE.
//! The ISO 639-1 sweep is a set-equality against the complete code set; the
//! per-language checks assert the presence of each required annotation.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use gmeow_rdf::{parse_dataset, TermRef};
use gmeow_validate::language_tags::{load_inverse_tag_map, load_tag_map, retag_graph_to_internal};

/// The GMEOW namespace prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
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

/// Absolute path to the repository root (`crates/validate/../../`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must be resolvable")
}

/// Read the reference-catalog Turtle bytes.
///
/// The catalog is self-contained for every Category-B assertion: each catalog
/// individual carries `rdfs:isDefinedBy <.../imports/languages-reference>`, and
/// the internal tags the tag-map test checks (english/french/mandarin/japanese/
/// arabic/hindi/python) are all defined here.
fn catalog_bytes() -> Vec<u8> {
    let path = repo_root().join("imports/languages-reference.ttl");
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Parse the catalog into a dataset (hard-fail on parse error).
fn catalog_dataset() -> std::sync::Arc<gmeow_rdf::RdfDataset> {
    parse_dataset(&catalog_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("catalog must parse as turtle: {e}"))
}

/// A flat, string-keyed projection of every triple in the dataset whose subject
/// and predicate are IRIs. The object is captured as either an IRI string or the
/// literal lexical form (no datatype/lang) — sufficient for the presence and
/// set-coverage assertions these audits perform.
struct Index {
    /// `(subject, predicate) -> set of object IRIs`.
    obj_iris: HashMap<(String, String), BTreeSet<String>>,
    /// `(subject, predicate) -> set of object literal lexical forms`.
    obj_lits: HashMap<(String, String), BTreeSet<String>>,
}

impl Index {
    fn build(dataset: &gmeow_rdf::RdfDataset) -> Self {
        let mut obj_iris: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        let mut obj_lits: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for qr in dataset.quad_refs() {
            let (TermRef::Iri(s), TermRef::Iri(p)) = (qr.s, qr.p) else {
                continue;
            };
            let key = (s.to_owned(), p.to_owned());
            match qr.o {
                TermRef::Iri(o) => {
                    obj_iris.entry(key).or_default().insert(o.to_owned());
                }
                TermRef::Literal { lexical, .. } => {
                    obj_lits.entry(key).or_default().insert(lexical.to_owned());
                }
                _ => {}
            }
        }
        Self { obj_iris, obj_lits }
    }

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
/// catalog natural language carries label + definition + languageTag + bcp47Tag +
/// languageCode + a `skos:exactMatch` alignment.
#[test]
fn reference_catalog_languages_are_annotated_and_aligned() {
    let dataset = catalog_dataset();
    let index = Index::build(&dataset);
    let languages = index.catalog_subjects_of_type(&format!("{GMEOW}Language"));
    assert!(
        !languages.is_empty(),
        "catalog must define gmeow:Language individuals"
    );

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

    let tag_prop = format!("{GMEOW}languageTag");
    let bcp_prop = format!("{GMEOW}bcp47Tag");
    for lang in &languages {
        assert!(
            index.has_any_lit(lang, RDFS_LABEL),
            "<{lang}> missing rdfs:label"
        );
        assert!(
            index.has_any_lit(lang, SKOS_DEFINITION),
            "<{lang}> missing skos:definition"
        );
        assert!(
            index.has_any_lit(lang, &tag_prop),
            "<{lang}> missing gmeow:languageTag"
        );
        assert!(
            index.has_any_lit(lang, &bcp_prop),
            "<{lang}> missing gmeow:bcp47Tag"
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

/// Mirror of `test_reference_catalog_writing_systems_are_annotated`: every writing
/// system referenced (via `gmeow:usesWritingSystem`) by a catalog language is typed
/// `gmeow:WritingSystem` and carries `rdfs:label` + `skos:definition`.
#[test]
fn reference_catalog_writing_systems_are_annotated() {
    let dataset = catalog_dataset();
    let index = Index::build(&dataset);
    let ws_type = format!("{GMEOW}WritingSystem");
    let uses_ws = format!("{GMEOW}usesWritingSystem");

    let languages = index.catalog_subjects_of_type(&format!("{GMEOW}Language"));
    let mut writing_systems: BTreeSet<String> = BTreeSet::new();
    for lang in &languages {
        if let Some(systems) = index.iris(lang, &uses_ws) {
            writing_systems.extend(systems.iter().cloned());
        }
    }
    assert!(
        !writing_systems.is_empty(),
        "no writing systems found for reference-catalog languages"
    );
    for ws in &writing_systems {
        assert!(
            index.has_iri(ws, RDF_TYPE, &ws_type),
            "<{ws}> missing gmeow:WritingSystem type"
        );
        assert!(
            index.has_any_lit(ws, RDFS_LABEL),
            "<{ws}> missing rdfs:label"
        );
        assert!(
            index.has_any_lit(ws, SKOS_DEFINITION),
            "<{ws}> missing skos:definition"
        );
    }
}

/// Mirror of `test_reference_catalog_programming_languages_typed`: the named
/// programming languages are typed `gmeow:ProgrammingLanguage`.
#[test]
fn reference_catalog_programming_languages_typed() {
    let dataset = catalog_dataset();
    let index = Index::build(&dataset);
    let prog_type = format!("{GMEOW}ProgrammingLanguage");
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
            index.has_iri(&iri, RDF_TYPE, &prog_type),
            "<{iri}> must be typed gmeow:ProgrammingLanguage"
        );
    }
}

/// Mirror of `test_reference_catalog_glottolog_alignments`: catalog-only natural
/// languages link to Glottolog via `skos:exactMatch`.
#[test]
fn reference_catalog_glottolog_alignments() {
    let dataset = catalog_dataset();
    let index = Index::build(&dataset);
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

/// Mirror of `test_language_tag_map_is_deterministic_and_covers_catalog`:
/// `load_tag_map` over the catalog is deterministic across two parses and covers
/// the core + catalog internal tags.
#[test]
fn language_tag_map_is_deterministic_and_covers_catalog() {
    let bytes = catalog_bytes();
    let map_a: HashMap<String, String> =
        load_tag_map(&bytes, "turtle").expect("first load_tag_map must succeed");
    let map_b: HashMap<String, String> =
        load_tag_map(&bytes, "turtle").expect("second load_tag_map must succeed");
    assert_eq!(map_a, map_b, "load_tag_map output must be deterministic");

    for internal_tag in [
        "x-gmeow-english",
        "x-gmeow-french",
        "x-gmeow-mandarin",
        "x-gmeow-japanese",
        "x-gmeow-arabic",
        "x-gmeow-hindi",
        "x-gmeow-python",
    ] {
        let bcp = map_a
            .get(internal_tag)
            .unwrap_or_else(|| panic!("missing tag mapping for {internal_tag}"));
        assert!(!bcp.is_empty(), "empty BCP-47 mapping for {internal_tag}");
    }
}

/// Catalog-data coverage: `load_inverse_tag_map` over the REAL catalog recovers the
/// three project translation targets — English, French, and Mandarin.
///
/// This is a DATA audit that the inline unit test in `language_tags.rs`
/// (`load_inverse_tag_map_recovers_natural_tags`) cannot substitute for: that test
/// uses a 2-language synthetic fixture and asserts the LOGIC is correct. This test
/// asserts that the CATALOG actually carries the three required mappings. An
/// authoring error (missing `bcp47Tag`, wrong tag, removed individual) would break
/// this test but leave the unit test green.
#[test]
fn inverse_tag_map_recovers_natural_internal_tags() {
    let bytes = catalog_bytes();
    let inv = load_inverse_tag_map(&bytes, "turtle")
        .expect("load_inverse_tag_map must succeed on the reference catalog");

    assert_eq!(
        inv.get("en"),
        Some(&"x-gmeow-english".to_owned()),
        "catalog inverse map must recover en → x-gmeow-english"
    );
    assert_eq!(
        inv.get("fr"),
        Some(&"x-gmeow-french".to_owned()),
        "catalog inverse map must recover fr → x-gmeow-french"
    );
    assert_eq!(
        inv.get("zh"),
        Some(&"x-gmeow-mandarin".to_owned()),
        "catalog inverse map must recover zh → x-gmeow-mandarin"
    );
}

/// Catalog round-trip: `retag_graph_to_internal` using the catalog's inverse map
/// converts `@en` and `@zh` literals to `@x-gmeow-english` and `@x-gmeow-mandarin`
/// respectively; verifies the catalog DATA drives the real graph-rewrite path.
///
/// This complements the unit-level `retag_graph_to_internal_lifts_public_tags` test
/// (which uses a synthetic 2-entry map) by asserting that the catalog-derived
/// inverse map actually produces the correct internal tags on a concrete N-Triples
/// graph — exercising the end-to-end catalog → inverse-map → retag path.
#[test]
fn retag_graph_to_internal_catalog_round_trip() {
    let bytes = catalog_bytes();
    let inv = load_inverse_tag_map(&bytes, "turtle")
        .expect("load_inverse_tag_map must succeed on the reference catalog");

    // Build a small N-Triples graph with one @en and one @zh literal.
    let nt = "<https://e/s> <https://e/label> \"Hello\"@en .\n\
              <https://e/s> <https://e/label> \"中文\"@zh .\n";

    let out = retag_graph_to_internal(nt.as_bytes(), "ntriples", &inv)
        .expect("retag_graph_to_internal must succeed");
    let text = String::from_utf8(out).expect("output must be valid UTF-8");

    assert!(
        text.contains("\"Hello\"@x-gmeow-english"),
        "@en must be retagged to @x-gmeow-english using catalog inverse map: {text}"
    );
    assert!(
        text.contains("\"中文\"@x-gmeow-mandarin"),
        "@zh must be retagged to @x-gmeow-mandarin using catalog inverse map: {text}"
    );
    assert!(
        !text.contains("\"Hello\"@en"),
        "original @en literal must not survive in output: {text}"
    );
    assert!(
        !text.contains("\"中文\"@zh"),
        "original @zh literal must not survive in output: {text}"
    );
}

/// Belt-and-braces coverage guard: the catalog defines the full ISO 639-1 sweep,
/// so the natural-language count is at least the 184-entry threshold the Python
/// sweep implies. Uses the same `>=` shape as a coverage floor (the set-equality
/// test above is the exact authority; this names a clearer failure if the catalog
/// shrinks). Each over/under is reported by subject for a helpful hard-fail.
#[test]
fn reference_catalog_language_count_meets_floor() {
    let dataset = catalog_dataset();
    let index = Index::build(&dataset);
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
