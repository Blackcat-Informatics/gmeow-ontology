// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Multilingual support for the documentation model (PyO3-free).
//!
//! GMEOW authors English with the `@x-gmeow-english` language tag and stores
//! translations in per-slice gettext catalogs `slices/<g>/<slice>/i18n/<lang>.po`.
//! The slice catalog classifies those files as
//! [`ArtifactRole::TranslationCatalog`](gmeow_slice::ArtifactRole::TranslationCatalog),
//! so their bytes are already available on each [`SliceRecord`]. This module
//! parses them into a [`Translations`] index and resolves per-language label /
//! definition values at render time, falling back to the English carrier.
//!
//! The `.po` entry shape is:
//!
//! ```text
//! msgctxt "https://blackcatinformatics.ca/gmeow/EntityExistence|rdfs:label"
//! msgid "Entity Existence"
//! msgstr "Existence d'entité"
//! ```
//!
//! The `msgctxt` is `"<term-iri>|<predicate-curie>"`. An empty `msgstr` means
//! untranslated (skipped). The `Language:` header gives the BCP-47 code (`fr`,
//! `zh`). Translation values are keyed by that BCP-47 code; the carrier English
//! value always lives in the model itself.
//!
//! UI-chrome strings (nav labels, page headings, category names) are ported from
//! the legacy Python `_ONTOLOGY_DOCS_TEMPLATES` table and exposed via
//! [`ui_string`]. They may be overridden per language by an optional
//! `ontology-docs-templates.<lang>.po` catalog (none exist yet — English is the
//! expected fallback).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use gmeow_slice::{ArtifactRole, SliceCatalog};

/// The English authoring carrier key (the model's own values).
pub const ENGLISH: &str = "english";

// ── Namespace constants for CURIE expansion in `.po` msgctxt predicates ─────────

const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Expand a CURIE-or-IRI predicate (as found in a `.po` `msgctxt`) to a full IRI.
///
/// Recognises the prefixes used by GMEOW's localizable predicates. A full IRI or
/// an unknown prefixed form is returned unchanged.
fn expand_predicate(predicate: &str) -> String {
    let Some((prefix, local)) = predicate.split_once(':') else {
        return predicate.to_string();
    };
    // A full IRI (scheme://…) has a `//` right after the colon — leave it.
    if local.starts_with("//") {
        return predicate.to_string();
    }
    let ns = match prefix {
        "rdfs" => RDFS_NS,
        "skos" => SKOS_NS,
        "dcterms" | "dct" => DCTERMS_NS,
        "gmeow" => GMEOW_NS,
        _ => return predicate.to_string(),
    };
    format!("{ns}{local}")
}

// ── gettext `.po` parsing ──────────────────────────────────────────────────────

/// One parsed `.po` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoEntry {
    /// The `msgctxt` value (may be empty for the header entry).
    pub msgctxt: String,
    /// The `msgid` (source / English) value.
    pub msgid: String,
    /// The `msgstr` (translated) value (empty = untranslated).
    pub msgstr: String,
}

/// A parsed `.po` catalog: its `Language:` header code and its entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PoCatalog {
    /// The BCP-47 code from the `Language:` header (e.g. `fr`), or empty.
    pub language: String,
    /// All entries (including any header entry with an empty `msgctxt`).
    pub entries: Vec<PoEntry>,
}

/// Reverse the gettext escape sequences in a quoted string body.
fn po_unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Extract the inner body of a leading-keyword PO line such as
/// `msgid "Hello \"x\""` → `Hello "x"` (unescaped), or a bare continuation
/// `"more text"`. Returns `None` when the line has no quoted body.
fn quoted_body(rest: &str) -> Option<String> {
    let trimmed = rest.trim();
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    Some(po_unescape(inner))
}

/// Parse a `.po` source string into a [`PoCatalog`].
///
/// Handles multi-line quoted strings (a keyword line followed by bare
/// `"continuation"` lines) and `\"`/`\n`/`\t` escapes. The `Language:` header is
/// read from the header entry's `msgstr` (the standard gettext location).
/// Comment lines (`#…`) are ignored.
pub fn parse_po(text: &str) -> PoCatalog {
    #[derive(Clone, Copy, PartialEq)]
    enum Field {
        None,
        Ctxt,
        Id,
        Str,
    }

    let mut entries: Vec<PoEntry> = Vec::new();
    let mut cur = PoEntry {
        msgctxt: String::new(),
        msgid: String::new(),
        msgstr: String::new(),
    };
    let mut have_ctxt = false;
    let mut have_id = false;
    let mut have_str = false;
    let mut field = Field::None;

    let flush = |entries: &mut Vec<PoEntry>,
                 cur: &mut PoEntry,
                 have_ctxt: &mut bool,
                 have_id: &mut bool,
                 have_str: &mut bool| {
        if *have_ctxt || *have_id || *have_str {
            entries.push(cur.clone());
        }
        *cur = PoEntry {
            msgctxt: String::new(),
            msgid: String::new(),
            msgstr: String::new(),
        };
        *have_ctxt = false;
        *have_id = false;
        *have_str = false;
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(
                &mut entries,
                &mut cur,
                &mut have_ctxt,
                &mut have_id,
                &mut have_str,
            );
            field = Field::None;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("msgctxt") {
            // A new msgctxt starts a fresh entry if the current one is complete.
            if have_id {
                flush(
                    &mut entries,
                    &mut cur,
                    &mut have_ctxt,
                    &mut have_id,
                    &mut have_str,
                );
            }
            if let Some(body) = quoted_body(rest) {
                cur.msgctxt = body;
                have_ctxt = true;
                field = Field::Ctxt;
            }
        } else if let Some(rest) = line.strip_prefix("msgid") {
            if let Some(body) = quoted_body(rest) {
                cur.msgid = body;
                have_id = true;
                field = Field::Id;
            }
        } else if let Some(rest) = line.strip_prefix("msgstr") {
            if let Some(body) = quoted_body(rest) {
                cur.msgstr = body;
                have_str = true;
                field = Field::Str;
            }
        } else if let Some(body) = quoted_body(line) {
            // Bare continuation string for the current field.
            match field {
                Field::Ctxt => cur.msgctxt.push_str(&body),
                Field::Id => cur.msgid.push_str(&body),
                Field::Str => cur.msgstr.push_str(&body),
                Field::None => {}
            }
        }
    }
    flush(
        &mut entries,
        &mut cur,
        &mut have_ctxt,
        &mut have_id,
        &mut have_str,
    );

    // The header entry has an empty msgid; its msgstr carries `Language: <code>\n`.
    let language = entries
        .iter()
        .find(|e| e.msgid.is_empty())
        .and_then(|e| language_from_header(&e.msgstr))
        .unwrap_or_default();

    PoCatalog { language, entries }
}

/// Extract the `Language:` code from a gettext header `msgstr` body.
fn language_from_header(header: &str) -> Option<String> {
    for line in header.split('\n') {
        if let Some(rest) = line.trim().strip_prefix("Language:") {
            let code = rest.trim();
            if !code.is_empty() {
                return Some(code.to_string());
            }
        }
    }
    None
}

// ── Translations index ─────────────────────────────────────────────────────────

/// All translated ontology literals, indexed by `(term_iri, predicate_full_iri,
/// bcp47_lang)`. Built from every slice's [`ArtifactRole::TranslationCatalog`]
/// artifact. Also carries the BCP-47 → internal-tag map (e.g. `fr` →
/// `x-gmeow-french`) so consumers can compute the archive prefix `create_docs`
/// expects.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(into = "TranslationsDto", from = "TranslationsDto")]
pub struct Translations {
    /// `(term_iri, predicate_full_iri, lang) -> translated_value`.
    by_key: BTreeMap<(String, String, String), String>,
    /// Sorted set of BCP-47 language codes seen across all catalogs.
    languages: Vec<String>,
    /// `bcp47 -> internal x-gmeow-* tag` (from the language slice).
    internal_tag: BTreeMap<String, String>,
}

/// JSON-friendly wire shape for [`Translations`]. The in-memory `by_key` index is
/// keyed by a `(iri, predicate, lang)` tuple, which `serde_json` cannot encode as
/// a map key — so it is flattened to a `Vec` of flat records. `languages` and
/// `internal_tag` are carried **verbatim** (not re-derived from `by_key`): a
/// language can appear in `internal_tag` with no translated entries, so
/// re-deriving would break round-trip identity.
#[derive(Serialize, Deserialize)]
struct TranslationsDto {
    by_key: Vec<TranslationEntryDto>,
    languages: Vec<String>,
    internal_tag: BTreeMap<String, String>,
}

/// One `(iri, predicate, lang) -> value` translation, flattened for serde.
#[derive(Serialize, Deserialize)]
struct TranslationEntryDto {
    iri: String,
    predicate: String,
    lang: String,
    value: String,
}

impl From<Translations> for TranslationsDto {
    fn from(t: Translations) -> Self {
        Self {
            by_key: t
                .by_key
                .into_iter()
                .map(|((iri, predicate, lang), value)| TranslationEntryDto {
                    iri,
                    predicate,
                    lang,
                    value,
                })
                .collect(),
            languages: t.languages,
            internal_tag: t.internal_tag,
        }
    }
}

impl From<TranslationsDto> for Translations {
    fn from(d: TranslationsDto) -> Self {
        Self {
            by_key: d
                .by_key
                .into_iter()
                .map(|e| ((e.iri, e.predicate, e.lang), e.value))
                .collect(),
            languages: d.languages,
            internal_tag: d.internal_tag,
        }
    }
}

impl Translations {
    /// Build the index from every translation catalog in the slice catalog, and
    /// the BCP-47 → internal-tag map read from the language slice's module.
    pub fn from_catalog(catalog: &SliceCatalog) -> Self {
        let mut by_key: BTreeMap<(String, String, String), String> = BTreeMap::new();
        let mut langs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::TranslationCatalog {
                    continue;
                }
                let text = String::from_utf8_lossy(&artifact.content);
                let parsed = parse_po(&text);
                if parsed.language.is_empty() || parsed.language.eq_ignore_ascii_case("en") {
                    continue;
                }
                langs.insert(parsed.language.clone());
                for entry in &parsed.entries {
                    if entry.msgctxt.is_empty() || entry.msgstr.is_empty() {
                        continue;
                    }
                    let Some((term_iri, predicate)) = entry.msgctxt.split_once('|') else {
                        continue;
                    };
                    let predicate = expand_predicate(predicate);
                    by_key.insert(
                        (term_iri.to_string(), predicate, parsed.language.clone()),
                        entry.msgstr.clone(),
                    );
                }
            }
        }

        let internal_tag = internal_tag_map(catalog);

        Self {
            by_key,
            languages: langs.into_iter().collect(),
            internal_tag,
        }
    }

    /// Construct a `Translations` index directly from `(iri, predicate, lang) ->
    /// value` triples and a set of languages. For tests / programmatic builders.
    pub fn from_entries(
        entries: impl IntoIterator<Item = ((String, String, String), String)>,
        languages: impl IntoIterator<Item = String>,
    ) -> Self {
        let by_key: BTreeMap<(String, String, String), String> = entries.into_iter().collect();
        let mut languages: Vec<String> = languages.into_iter().collect();
        languages.sort();
        languages.dedup();
        Self {
            by_key,
            languages,
            internal_tag: BTreeMap::new(),
        }
    }

    /// The translated value for `(iri, predicate, lang)`, or `None` when absent
    /// (the caller falls back to the English carrier value). The English carrier
    /// key always returns `None` so the model's own value is used.
    pub fn lookup(&self, iri: &str, predicate: &str, lang: &str) -> Option<&str> {
        if lang == ENGLISH {
            return None;
        }
        self.by_key
            .get(&(iri.to_string(), predicate.to_string(), lang.to_string()))
            .map(String::as_str)
    }

    /// All non-English BCP-47 language codes with at least one catalog (sorted).
    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    /// The internal `x-gmeow-*` tag for a BCP-47 code (e.g. `fr` →
    /// `x-gmeow-french`), defaulting to `x-gmeow-<code>` when the language slice
    /// declares no mapping. This is the gts archive-prefix the docs consumer
    /// (`create_docs`) selects on.
    pub fn internal_tag(&self, lang: &str) -> String {
        if lang == ENGLISH {
            return "x-gmeow-english".to_string();
        }
        self.internal_tag
            .get(lang)
            .cloned()
            .unwrap_or_else(|| format!("x-gmeow-{lang}"))
    }
}

/// All languages including the English carrier first, then translation languages
/// sorted. The order is deterministic.
pub fn available_languages(translations: &Translations) -> Vec<String> {
    let mut out = vec![ENGLISH.to_string()];
    out.extend(translations.languages().iter().cloned());
    out
}

/// Read the `gmeow:bcp47Tag` ↔ `gmeow:languageTag` pairs from the language
/// slice's module so a BCP-47 code maps to its internal `x-gmeow-*` tag.
fn internal_tag_map(catalog: &SliceCatalog) -> BTreeMap<String, String> {
    use crate::model::parse_turtle_lenient;
    use oxigraph::model::{GraphNameRef, NamedOrBlankNode, Term};

    const BCP47: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
    const LANG_TAG: &str = "https://blackcatinformatics.ca/gmeow/languageTag";

    let mut out: BTreeMap<String, String> = BTreeMap::new();

    for record in catalog.records() {
        let owner = &record.manifest.slice_iri;
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::Module {
                continue;
            }
            // A module that fails to parse is a hard fault; surface it loudly.
            let store = parse_turtle_lenient(&artifact.content)
                .unwrap_or_else(|e| panic!("module.ttl for slice {owner} failed to parse: {e}"));

            // For each subject with both a bcp47Tag and a languageTag, map the
            // (lowercased) bcp47 code to the internal tag.
            let bcp_pred = oxigraph::model::NamedNode::new_unchecked(BCP47);
            for quad in store
                .quads_for_pattern(
                    None,
                    Some(bcp_pred.as_ref()),
                    None,
                    Some(GraphNameRef::DefaultGraph),
                )
                .flatten()
            {
                let NamedOrBlankNode::NamedNode(subject) = &quad.subject else {
                    continue;
                };
                let Term::Literal(bcp) = &quad.object else {
                    continue;
                };
                let internal = first_literal(&store, subject.as_str(), LANG_TAG);
                if let Some(internal) = internal {
                    out.entry(bcp.value().to_ascii_lowercase())
                        .or_insert(internal);
                }
            }
        }
    }
    out
}

/// First literal value (lowest lexical form) for `subject predicate ?o`.
fn first_literal(store: &oxigraph::store::Store, subject: &str, predicate: &str) -> Option<String> {
    use oxigraph::model::{GraphNameRef, Term};
    let subject = oxigraph::model::NamedNode::new(subject).ok()?;
    let predicate = oxigraph::model::NamedNode::new_unchecked(predicate);
    let mut values: Vec<String> = store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .filter_map(|q| match q.object {
            Term::Literal(lit) => Some(lit.value().to_string()),
            _ => None,
        })
        .collect();
    values.sort();
    values.into_iter().next()
}

// ── UI-chrome string table ──────────────────────────────────────────────────────

/// The English UI-chrome strings, ported verbatim from the legacy Python
/// `_ONTOLOGY_DOCS_TEMPLATES` table (60 keys). These are the category labels,
/// nav items, page titles, section headings, footer, and accessibility strings
/// the renderer emits. Sorted by key for determinism.
pub const UI_TEMPLATES: &[(&str, &str)] = &[
    // Category labels
    ("category_class", "Classes"),
    ("category_datatype", "Datatypes"),
    ("category_individual", "Individuals"),
    ("category_property", "Properties"),
    // Footer
    ("footer_cite_prefix", "Cite as"),
    (
        "footer_generated",
        "Generated from the GMEOW ontology. Canonical source is RDF/OWL; this site is a \
         deterministic projection.",
    ),
    ("footer_license", "Ontology licensed CC BY 4.0"),
    // Accessibility / misc
    ("generated_documentation", "Generated documentation"),
    ("module", "Module"),
    // Site navigation
    ("nav_adoption", "Adoption"),
    ("nav_bibliography", "Bibliography"),
    ("nav_concerns", "Concerns"),
    ("nav_examples", "Examples"),
    ("nav_external", "External"),
    ("nav_four_boxes", "Four Boxes"),
    ("nav_getting_started", "Getting Started"),
    ("nav_home", "Home"),
    ("nav_integrity", "Integrity Constraints"),
    ("nav_learning_paths", "Learning Paths"),
    ("nav_linkages", "Linkages"),
    ("nav_logic", "Logic & Reasoning"),
    ("nav_rdf12", "RDF 1.2"),
    ("nav_recipes", "Recipes"),
    ("nav_reference", "Reference"),
    ("nav_slices", "Slices"),
    ("open_canonical_page", "Open the canonical reference page."),
    // Generic page titles
    ("page_about", "About GMEOW"),
    ("page_adoption_targets", "Adoption Targets"),
    ("page_changelog", "Changelog"),
    ("page_examples", "Examples"),
    ("page_external_ontologies", "External Ontologies"),
    ("page_external_terms", "External Terms"),
    ("page_getting_started", "Getting Started"),
    ("page_index", "Index"),
    ("page_learning_paths", "Learning Paths"),
    ("page_linkages", "Linkages"),
    ("page_quality_gates", "Quality Gates"),
    ("page_recipes", "Recipes"),
    ("page_reference", "Reference"),
    ("page_references", "References"),
    ("page_search", "Search"),
    ("page_slices", "Slices"),
    ("page_statements", "RDF 1.2 Statement Layer"),
    ("page_visualizations", "Visualizations"),
    // Section headings
    ("section_distribution", "Distribution"),
    ("section_export_docs", "Export the bundled docs"),
    (
        "section_external_vocabulary_coverage",
        "External Vocabulary Coverage",
    ),
    (
        "section_inspect_terms",
        "Inspect terms while reading examples",
    ),
    ("section_install", "Install"),
    ("section_pick_first_path", "Pick a first path"),
    ("section_profiles", "Profiles"),
    ("section_read_next", "Read Next"),
    (
        "section_read_slices",
        "Read slices as doctrine, not just reference",
    ),
    ("section_recipes", "Recipes"),
    ("section_reference", "Reference"),
    ("section_references", "References"),
    ("section_slices", "Slices"),
    ("section_start_here", "Start Here"),
    ("section_static_indexes", "Static Indexes"),
    ("skip_to_content", "Skip to content"),
];

/// Per-language UI-chrome overrides. Keyed by `(lang, key) -> translated`.
/// Built from optional `ontology-docs-templates.<lang>.po` catalogs; empty when
/// none are present (the English fallback is used everywhere).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(into = "UiCatalogDto", from = "UiCatalogDto")]
pub struct UiCatalog {
    overrides: BTreeMap<(String, String), String>,
}

/// JSON-friendly wire shape for [`UiCatalog`]: the `(lang, key)` tuple-keyed
/// overrides flattened to a `Vec` of flat records (`serde_json` cannot encode a
/// tuple map key).
#[derive(Serialize, Deserialize)]
struct UiCatalogDto {
    overrides: Vec<UiOverrideDto>,
}

/// One `(lang, key) -> value` UI-chrome override, flattened for serde.
#[derive(Serialize, Deserialize)]
struct UiOverrideDto {
    lang: String,
    key: String,
    value: String,
}

impl From<UiCatalog> for UiCatalogDto {
    fn from(c: UiCatalog) -> Self {
        Self {
            overrides: c
                .overrides
                .into_iter()
                .map(|((lang, key), value)| UiOverrideDto { lang, key, value })
                .collect(),
        }
    }
}

impl From<UiCatalogDto> for UiCatalog {
    fn from(d: UiCatalogDto) -> Self {
        Self {
            overrides: d
                .overrides
                .into_iter()
                .map(|o| ((o.lang, o.key), o.value))
                .collect(),
        }
    }
}

impl UiCatalog {
    /// Build the UI override catalog from an optional `i18n/` directory holding
    /// `ontology-docs-templates.<lang>.po` files. Absent files mean no overrides.
    ///
    /// The msgctxt of each entry is `"ontology-docs-template|<key>"` (matching the
    /// legacy POT format); the language is read from each catalog's header.
    pub fn from_dir(dir: &std::path::Path) -> Self {
        let mut overrides: BTreeMap<(String, String), String> = BTreeMap::new();
        let Ok(read) = std::fs::read_dir(dir) else {
            return Self { overrides };
        };
        let mut paths: Vec<std::path::PathBuf> = read
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("ontology-docs-templates.") && n.ends_with(".po"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let parsed = parse_po(&text);
            if parsed.language.is_empty() {
                continue;
            }
            for entry in &parsed.entries {
                if entry.msgstr.is_empty() {
                    continue;
                }
                let Some(key) = entry.msgctxt.strip_prefix("ontology-docs-template|") else {
                    continue;
                };
                overrides.insert(
                    (parsed.language.clone(), key.to_string()),
                    entry.msgstr.clone(),
                );
            }
        }
        Self { overrides }
    }
}

/// The English default (a `'static` table entry) for a UI-chrome key, or `""`
/// when the key is unknown (unknown keys never reach the live renderer).
pub(crate) fn ui_default(key: &str) -> &'static str {
    UI_TEMPLATES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

/// Resolve a UI-chrome string for `key` in `lang`: a per-language override when
/// present, else the `'static` English default.
pub fn ui_string<'a>(key: &str, lang: &str, catalog: &'a UiCatalog) -> &'a str {
    if lang != ENGLISH {
        if let Some(value) = catalog.overrides.get(&(lang.to_string(), key.to_string())) {
            return value.as_str();
        }
    }
    ui_default(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# a comment
msgid ""
msgstr ""
"Project-Id-Version: gmeow\n"
"Language: fr\n"
"Content-Type: text/plain; charset=UTF-8\n"

msgctxt "https://blackcatinformatics.ca/gmeow/Foo|rdfs:label"
msgid "Foo"
msgstr "Fou"

msgctxt "https://blackcatinformatics.ca/gmeow/Foo|skos:definition"
msgid "A foo."
msgstr ""
"Une longue "
"définition."

msgctxt "https://blackcatinformatics.ca/gmeow/Bar|rdfs:label"
msgid "Bar"
msgstr ""
"#;

    #[test]
    fn parse_po_reads_language_and_entries() {
        let cat = parse_po(SAMPLE);
        assert_eq!(cat.language, "fr");
        // header + 3 entries
        let non_header: Vec<_> = cat.entries.iter().filter(|e| !e.msgid.is_empty()).collect();
        assert_eq!(non_header.len(), 3);
    }

    #[test]
    fn parse_po_joins_multiline_msgstr() {
        let cat = parse_po(SAMPLE);
        let def = cat
            .entries
            .iter()
            .find(|e| e.msgctxt.ends_with("|skos:definition"))
            .unwrap();
        assert_eq!(def.msgstr, "Une longue définition.");
    }

    #[test]
    fn po_escapes_are_unescaped() {
        let text = "msgctxt \"x|rdfs:label\"\nmsgid \"a\"\nmsgstr \"line1\\nline2 \\\"q\\\"\"\n";
        let cat = parse_po(text);
        let e = cat.entries.iter().find(|e| !e.msgid.is_empty()).unwrap();
        assert_eq!(e.msgstr, "line1\nline2 \"q\"");
    }

    #[test]
    fn expand_predicate_handles_curies_and_iris() {
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
    fn ui_templates_has_sixty_keys() {
        assert_eq!(UI_TEMPLATES.len(), 60);
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
            "Fou".to_string(),
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
            "Fou".to_string(),
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
}
