// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Multilingual support for the documentation model (PyO3-free).
//!
//! GMEOW authors English with the `@x-gmeow-english` language tag and stores
//! translations in per-slice gettext catalogs `slices/<g>/<slice>/i18n/<lang>.po`.
//! The slice catalog classifies those files as
//! [`ArtifactRole::TranslationCatalog`], so their bytes are already available
//! on each [`purrdf::slice::SliceRecord`]. This module
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

use purrdf::slice::{ArtifactRole, SliceCatalog};

/// The English authoring carrier key (the model's own values).
pub const ENGLISH: &str = "english";

// ── Translation-integrity checks ──────────────────────────────────────────────

/// Return a stable reason when a non-English translation is not credible enough
/// to enter a generated graph or count as translated coverage.
///
/// This is deliberately a conservative lexical guard, not a language model or a
/// claim of translation correctness. It catches the two mechanically verifiable
/// corruption classes that have appeared in committed catalogs: copied multi-word
/// English source text and target strings that retain several unmistakably English
/// prose tokens. Chinese prose must additionally contain at least one Han character.
/// Single-word cognates and notation-only technical invariants remain admissible.
pub fn translation_integrity_issue(
    language: &str,
    msgid: &str,
    msgstr: &str,
) -> Option<&'static str> {
    let source = msgid.trim();
    let target = msgstr.trim();
    if target.is_empty() {
        return Some("empty translation");
    }

    let language = language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase();
    if language == "en" || language == ENGLISH {
        return None;
    }

    let source_words = ascii_words(source);
    if source.eq_ignore_ascii_case(target)
        && source_words.len() > 1
        && !is_technical_invariant(source)
    {
        return Some("multi-word English source text was copied into msgstr");
    }

    let english_leaks = ascii_words(target)
        .iter()
        .filter(|word| is_english_prose_token(word))
        .count();
    if english_leaks >= 2 {
        return Some("msgstr retains multiple English prose tokens");
    }

    if matches!(language.as_str(), "zh" | "cmn")
        && !source.is_empty()
        && !is_technical_invariant(source)
        && !target.chars().any(is_han)
    {
        return Some("Chinese prose translation contains no Han characters");
    }

    None
}

/// Whether a catalog value passes the deterministic integrity guard.
pub fn translation_has_integrity(language: &str, msgid: &str, msgstr: &str) -> bool {
    translation_integrity_issue(language, msgid, msgstr).is_none()
}

fn ascii_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn is_technical_invariant(text: &str) -> bool {
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

fn is_english_prose_token(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "this"
            | "that"
            | "these"
            | "those"
            | "when"
            | "where"
            | "which"
            | "whose"
            | "every"
            | "only"
            | "under"
            | "with"
            | "without"
            | "from"
            | "into"
            | "must"
            | "should"
            | "would"
            | "cannot"
            | "never"
            | "instead"
            | "broader"
            | "narrower"
            | "related"
            | "construct"
            | "stated"
            | "conditions"
            | "use"
            | "assert"
            | "preserve"
            | "declared"
            | "merely"
            | "scope"
            | "truth"
    )
}

fn is_han(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

// ── Translations index ─────────────────────────────────────────────────────────

/// A translated literal value plus its review state. `fuzzy` = machine-seeded and
/// not yet human-reviewed; such a value is carried through the index but treated as
/// not-yet-live at lookup (English fallback), matching the coverage measure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranslatedValue {
    pub value: String,
    pub fuzzy: bool,
}

/// All translated ontology literals, indexed by `(term_iri, predicate_full_iri,
/// bcp47_lang)`. Built from every slice's [`ArtifactRole::TranslationCatalog`]
/// artifact. Also carries the BCP-47 → internal-tag map (e.g. `fr` →
/// `x-gmeow-french`) so consumers can compute the archive prefix `create_docs`
/// expects.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(into = "TranslationsDto", from = "TranslationsDto")]
pub struct Translations {
    /// `(term_iri, predicate_full_iri, lang) -> translated_value` (with review state).
    by_key: BTreeMap<(String, String, String), TranslatedValue>,
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
    fuzzy: bool,
}

impl From<Translations> for TranslationsDto {
    fn from(t: Translations) -> Self {
        Self {
            by_key: t
                .by_key
                .into_iter()
                .map(|((iri, predicate, lang), tv)| TranslationEntryDto {
                    iri,
                    predicate,
                    lang,
                    value: tv.value,
                    fuzzy: tv.fuzzy,
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
                .map(|e| {
                    (
                        (e.iri, e.predicate, e.lang),
                        TranslatedValue {
                            value: e.value,
                            fuzzy: e.fuzzy,
                        },
                    )
                })
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
        use crate::i18n_compile::{expand_predicate, language_from_po, parse_po};

        let mut by_key: BTreeMap<(String, String, String), TranslatedValue> = BTreeMap::new();
        let mut langs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::TranslationCatalog {
                    continue;
                }
                let text = String::from_utf8_lossy(&artifact.content);
                // A present translation catalog is required input: a malformed one is a
                // HARD FAIL, never a silent skip that would drop measured coverage.
                let language = language_from_po(&text).unwrap_or_else(|e| {
                    panic!("i18n catalog for slice {owner} failed to parse: {e}")
                });
                let Some(language) = language else {
                    continue;
                };
                if language.is_empty() || language.eq_ignore_ascii_case("en") {
                    continue;
                }
                let entries = parse_po(&text, false).unwrap_or_else(|e| {
                    panic!("i18n catalog for slice {owner} failed to parse: {e}")
                });
                langs.insert(language.clone());
                for entry in &entries {
                    if entry.msgctxt.is_empty()
                        || !translation_has_integrity(&language, &entry.msgid, &entry.msgstr)
                    {
                        continue;
                    }
                    let Some((term_iri, predicate)) = entry.msgctxt.split_once('|') else {
                        continue;
                    };
                    let predicate = expand_predicate(predicate);
                    by_key.insert(
                        (term_iri.to_string(), predicate, language.clone()),
                        TranslatedValue {
                            value: entry.msgstr.clone(),
                            fuzzy: entry.fuzzy,
                        },
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
    /// Every constructed value is treated as reviewed (non-fuzzy).
    pub fn from_entries(
        entries: impl IntoIterator<Item = ((String, String, String), String)>,
        languages: impl IntoIterator<Item = String>,
    ) -> Self {
        let by_key: BTreeMap<(String, String, String), TranslatedValue> = entries
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    TranslatedValue {
                        value,
                        fuzzy: false,
                    },
                )
            })
            .collect();
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
    /// key always returns `None` so the model's own value is used, and a stored
    /// fuzzy (unreviewed) value also returns `None` — it is not yet live.
    pub fn lookup(&self, iri: &str, predicate: &str, lang: &str) -> Option<&str> {
        if lang == ENGLISH {
            return None;
        }
        self.by_key
            .get(&(iri.to_string(), predicate.to_string(), lang.to_string()))
            .filter(|v| !v.fuzzy)
            .map(|v| v.value.as_str())
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

/// Map a BCP-47 code to its internal `x-gmeow-*` carrier tag by reading the
/// carrier varieties in the lang module: the internal tag rides `lang:carrierTag`
/// on a `lang:LanguageVariety`, and its public BCP-47 code is DERIVED over the
/// model (never authored per language) — the variety's `lang:varietyOf` parent
/// sign system carries the ISO 639 primary subtag as `skos:notation` (script
/// suppressed for the carriers), matching the tag the `bcp47` projection folds.
fn internal_tag_map(catalog: &SliceCatalog) -> BTreeMap<String, String> {
    use crate::model::parse_turtle_lenient;
    use crate::store::Object;

    const CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
    const VARIETY_OF: &str = "https://blackcatinformatics.ca/lang/varietyOf";
    const SKOS_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";

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

            // For each carrier variety (subject with a lang:carrierTag), derive the
            // BCP-47 code from its lang:varietyOf parent's skos:notation and map the
            // (lowercased) code to the internal carrier tag.
            for (subject, object) in store.pattern_subjects_objects(CARRIER_TAG) {
                let Some(subject) = subject.as_named() else {
                    continue;
                };
                let Object::Literal(internal) = object else {
                    continue;
                };
                let Some(parent) = store.named_objects(subject, VARIETY_OF).into_iter().next()
                else {
                    continue;
                };
                if let Some(bcp) = store.first_literal(&parent, SKOS_NOTATION) {
                    out.entry(bcp.to_ascii_lowercase()).or_insert(internal);
                }
            }
        }
    }
    out
}

// ── UI-chrome string table ──────────────────────────────────────────────────────

/// The English UI-chrome strings. The nav/page/section/category/footer group was
/// ported verbatim from the legacy Python `_ONTOLOGY_DOCS_TEMPLATES` table; the
/// `body_*` group routes the Markdown body renderers' structural chrome (section
/// headings, inline labels, approximate-match caveats, and status prose) through
/// the same override catalog, so no user-facing chrome is hardcoded English.
/// Sorted by key for determinism.
pub const UI_TEMPLATES: &[(&str, &str)] = &[
    // Body chrome (heading / label / caveat / status strings emitted by the
    // Markdown body renderers; localizable via the same override catalog).
    ("body_about", "About"),
    ("body_academic_surface", "Formal & academic"),
    ("body_advice_avoid_for_consumers", "Avoid for consumers"),
    ("body_advice_avoid_when", "Avoid when"),
    ("body_advice_example", "Example"),
    ("body_advice_how_to_use", "How to use"),
    ("body_advice_scope", "Scope"),
    ("body_advice_use_for_consumers", "Use for consumers"),
    ("body_advice_use_when", "Use when"),
    ("body_alignments", "Alignments"),
    ("body_applies_to", "Applies to"),
    ("body_artifacts", "Artifacts"),
    ("body_at_a_glance", "At a glance"),
    ("body_badge_legend", "Badge legend"),
    ("body_box_role", "Box role"),
    ("body_browse", "Browse"),
    ("body_build_pipeline", "Build pipeline"),
    ("body_by_term_count", "By term count"),
    ("body_canonical_ir", "Canonical IR"),
    (
        "body_caveat_broad",
        "broader match — the external term is more general",
    ),
    (
        "body_caveat_close",
        "approximate match (close) — not an exact equivalence",
    ),
    (
        "body_caveat_disclosure_post",
        "for the per-target structural drops.",
    ),
    (
        "body_caveat_disclosure_pre",
        "An approximate match is a lossy projection — the external term is not an exact \
         equivalent. See the [preservation loss ledger]",
    ),
    (
        "body_caveat_edoal_fno_pre",
        "These cross-vocabulary crosswalks are also lowered to EDOAL (and, for transformation \
         correspondences, FnO) — sound but lossy under-approximations of the canonical \
         correspondence. See the [preservation loss ledger]",
    ),
    (
        "body_caveat_narrow",
        "narrower match — the external term is more specific",
    ),
    (
        "body_caveat_related",
        "related match — associative, not an equivalence",
    ),
    ("body_changelog", "Changelog"),
    ("body_changelog_added", "Added"),
    ("body_changelog_changed", "Changed"),
    ("body_citation", "Citation"),
    ("body_cite_this_page", "Cite this page"),
    ("body_competency_questions", "Competency questions"),
    ("body_compiler_diagnostics", "Compiler diagnostics"),
    ("body_compiler_products", "Compiler products"),
    (
        "body_completeness_distribution",
        "Completeness distribution",
    ),
    ("body_concern_not_found", "Concern not found."),
    ("body_concerns", "Concerns"),
    ("body_conformance_examples", "Conformance examples"),
    ("body_conformance_fixtures", "Conformance fixtures"),
    ("body_constraints", "Constraints"),
    ("body_coverage_by_dimension", "Coverage by dimension"),
    ("body_coverage_by_slice", "Coverage by slice"),
    ("body_definition", "Definition"),
    ("body_dependency_graph", "Dependency graph"),
    ("body_derivation_graph", "Derivation graph"),
    ("body_developer_surface", "For developers"),
    (
        "body_diagnostics_none",
        "No diagnostics recorded against this term in the current build.",
    ),
    (
        "body_diagnostics_you_might_hit",
        "Diagnostics you might hit",
    ),
    ("body_dl_axioms", "Description-Logic axioms"),
    ("body_documentation_coverage", "Documentation coverage"),
    ("body_documentation_health", "Documentation health"),
    ("body_domain", "Domain"),
    ("body_enforced_constraints", "What GMEOW enforces"),
    ("body_example_files", "Example files"),
    ("body_example_syntaxes", "Example in multiple syntaxes"),
    ("body_examples", "Examples"),
    ("body_examples_using_this_term", "Examples using this term"),
    ("body_expected_rows", "Expected rows"),
    ("body_external_ontologies", "External ontologies"),
    ("body_formalized_by", "Formalized by"),
    ("body_framework_distribution", "Framework distribution"),
    ("body_frameworks", "Frameworks"),
    ("body_getting_started", "Getting started"),
    ("body_glossary", "RDF-to-developer glossary"),
    ("body_goal", "Goal"),
    ("body_grammar_not_found", "Grammar not found."),
    ("body_grammar_source", "Grammar source"),
    (
        "body_health_heatmap_legend",
        "Per-slice coverage of each documentation dimension — green ≥ 80%, amber ≥ 50%, \
         red below.",
    ),
    ("body_i_want_to", "I want to…"),
    ("body_integrity_constraints", "Integrity constraints"),
    ("body_label_added_in", "Added in"),
    ("body_label_alignment_density", "Alignment density"),
    ("body_label_cite_ontology", "Cite the ontology"),
    ("body_label_cite_slice", "Cite the slice"),
    ("body_label_content_address", "Content address"),
    ("body_label_do", "Do"),
    ("body_label_dont", "Don't"),
    ("body_label_help_link", "Help link"),
    ("body_label_orphan_terms", "Orphan terms"),
    ("body_label_permalink", "Permalink"),
    ("body_label_rule_code", "Rule code"),
    ("body_label_severity", "Severity"),
    ("body_label_status", "Status"),
    ("body_label_violation_code", "Violation code"),
    ("body_learning_path_not_found", "Learning path not found."),
    ("body_learning_paths", "Learning paths"),
    ("body_linkage", "Linkage"),
    ("body_linkages", "Linkages"),
    ("body_logic_and_reasoning", "Logic & Reasoning"),
    ("body_logic_stereotypes", "Logic stereotypes"),
    ("body_maturity_by_slice", "Maturity by slice"),
    (
        "body_maturity_legend",
        "Each slice's earned documentation-maturity floor (projected from its \
         coverage), the bounded coverage fraction against the FULL intent, any \
         claimed tier, and the dimensions still standing between it and the next \
         tier.",
    ),
    ("body_neighborhood", "Neighborhood"),
    (
        "body_no_competency_questions",
        "No competency questions are declared in any slice.",
    ),
    (
        "body_no_conformance_fixtures",
        "No conformance fixtures are declared in any slice.",
    ),
    (
        "body_no_enforced_constraints",
        "No validation rules are declared in the constraint catalog.",
    ),
    (
        "body_no_learning_paths",
        "No learning paths are declared in the guides slice.",
    ),
    (
        "body_no_logic_stereotypes",
        "No logic stereotypes are declared yet.",
    ),
    (
        "body_no_notation_grammars",
        "No notation grammars are declared in the lang slice.",
    ),
    (
        "body_no_pipeline",
        "No build pipeline was discovered for this model.",
    ),
    ("body_no_query_text", "No query text available."),
    (
        "body_no_recipes",
        "No recipes are declared in the guides slice.",
    ),
    (
        "body_no_verify_queries",
        "No verification queries are declared in any slice.",
    ),
    (
        "body_no_worked_instances",
        "No worked math instances are declared in any slice.",
    ),
    (
        "body_no_worked_preservation_examples",
        "No authored preservation examples are declared in any slice.",
    ),
    ("body_notation_grammars", "Notation grammars"),
    ("body_openapi_fragment", "OpenAPI schema"),
    ("body_other_equivalences", "Other equivalences"),
    ("body_part_of", "Part of"),
    ("body_pipeline_attaches", "Attaches to the carrier"),
    ("body_pipeline_attaches_blob", "blob-rep lane"),
    ("body_pipeline_capabilities", "Capabilities and resources"),
    ("body_pipeline_consumed_by", "Consumed by"),
    ("body_pipeline_consumes", "Consumes"),
    ("body_pipeline_diagram", "Pipeline diagram"),
    ("body_pipeline_flowing_graphs", "Flowing graphs"),
    ("body_pipeline_implementation", "Implementation"),
    ("body_pipeline_stage", "Build-pipeline stage"),
    ("body_pipeline_stages", "Stages"),
    ("body_pipeline_success_mode", "Success mode"),
    ("body_preservation_loss_ledger", "Preservation loss ledger"),
    ("body_profiles", "Profiles"),
    ("body_projection_surface", "Projection surface"),
    ("body_projects_toward", "Projects toward"),
    ("body_provenance", "Provenance"),
    ("body_query", "Query"),
    ("body_quickstart", "Quickstart"),
    ("body_range", "Range"),
    ("body_read_next", "Read next"),
    ("body_reasoning", "Reasoning"),
    ("body_reasoning_consistent", "consistent"),
    ("body_reasoning_inconsistent", "**inconsistent**"),
    (
        "body_reasoning_not_evaluated",
        "**Not evaluated** — satisfiability is a class notion; the reasoner decides none \
         for this term.",
    ),
    (
        "body_reasoning_satisfiable",
        "**Satisfiable** — the native DL reasoner found this class consistent.",
    ),
    ("body_reasoning_status", "Reasoning status"),
    (
        "body_reasoning_unsatisfiable",
        "**Unsatisfiable** — the native DL reasoner proved this class necessarily empty \
         (`rdfs:subClassOf owl:Nothing`).",
    ),
    (
        "body_reasoning_unsatisfiable_because",
        "Unsatisfiable because",
    ),
    ("body_recipe_not_found", "Recipe not found."),
    ("body_recipes", "Recipes"),
    ("body_related_terms", "Related terms"),
    ("body_schema_fragment", "Use this term without RDF"),
    ("body_slice_not_found", "Slice not found."),
    ("body_slices", "Slices"),
    ("body_stability", "Stability"),
    ("body_super_classes", "Super-classes"),
    ("body_super_properties", "Super-properties"),
    ("body_term_entailments", "Inferred facts"),
    ("body_term_not_found", "Term not found."),
    (
        "body_term_projection_degradation",
        "How this term degrades under projection",
    ),
    (
        "body_term_projection_degradation_none",
        "Carried exactly by every projection — no per-shape property-path drops are \
         recorded against this term in the current build.",
    ),
    ("body_terms_used", "Terms used"),
    ("body_tested_by", "Tested by"),
    ("body_usage_advice", "Usage Advice"),
    ("body_vocabulary_by_category", "Vocabulary by category"),
    ("body_what_is_this", "What is this?"),
    ("body_where_to_go_next", "Where to go next"),
    ("body_worked_instances", "Worked instances"),
    (
        "body_worked_preservation_examples",
        "Worked preservation examples",
    ),
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
        use crate::i18n_compile::{language_from_po, parse_po};

        let mut overrides: BTreeMap<(String, String), String> = BTreeMap::new();
        // An absent `i18n/` dir means no overrides; a present-but-unreadable/unparsable
        // template file is a HARD FAIL.
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
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("UI template catalog {} failed to read: {e}", path.display())
            });
            let language = language_from_po(&text).unwrap_or_else(|e| {
                panic!(
                    "UI template catalog {} failed to parse: {e}",
                    path.display()
                )
            });
            let Some(language) = language.filter(|l| !l.is_empty()) else {
                continue;
            };
            let entries = parse_po(&text, false).unwrap_or_else(|e| {
                panic!(
                    "UI template catalog {} failed to parse: {e}",
                    path.display()
                )
            });
            for entry in &entries {
                // Fuzzy (machine-seeded, unreviewed) UI overrides are not inserted; they
                // fall back to English, consistent with rendering and the coverage measure.
                if entry.fuzzy || !translation_has_integrity(&language, &entry.msgid, &entry.msgstr)
                {
                    continue;
                }
                let Some(key) = entry.msgctxt.strip_prefix("ontology-docs-template|") else {
                    continue;
                };
                overrides.insert((language.clone(), key.to_string()), entry.msgstr.clone());
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
    if lang != ENGLISH
        && let Some(value) = catalog.overrides.get(&(lang.to_string(), key.to_string()))
    {
        return value.as_str();
    }
    ui_default(key)
}

#[cfg(test)]
mod tests {
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
        // 60 legacy nav/page/section/category/footer keys + 158 `body_*` chrome
        // keys routing the Markdown body renderers through the override catalog
        // (incl. the pipeline-stage attach surface: body_pipeline_attaches[_blob]).
        assert_eq!(UI_TEMPLATES.len(), 218);
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
