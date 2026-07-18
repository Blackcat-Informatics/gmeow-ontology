// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-slice terminology-glossary corpus producer — the term-grain slice of the
//! translation corpus (Principle 15 consumer wiring).
//!
//! The multilingual documentation `.po` catalogs (`slices/**/i18n/<lang>.po`) pair an
//! English source literal (`msgid`) with a per-language rendering (`msgstr`) keyed by
//! `msgctxt = "<term-iri>|<predicate-curie>"`. This module folds every REVIEWED
//! (non-fuzzy, present) pair into a first-class `gmeow:Glossary` — one per `(slice,
//! language)` — of `gmeow:GlossaryEntry` records, each carrying its term, predicate,
//! English source, target rendering, the Frege-triangle sense structure (a `lang:Sense`
//! on `gmeow:glossarySense` that `lang:evokes` the entry's `lang:LexicalConcept` grouping
//! key on `gmeow:glossaryConcept`), and the `lang:TranslationUnit` that holds the
//! crossing's `logic:Correspondence` law-spine (`gmeow:glossaryUnit`, the SAME
//! content-addressed identity [`crate::stages::lang_translation`] mints, so the two graphs
//! join in `gmeow.gts`).
//!
//! The glossary is a pure DERIVATION of the catalogs — never a second hand-authored
//! source (One Canonical Source). It is a term-grain VIEW distinct from the
//! translation corpus: it emits the term IRI and English source as queryable triples
//! and groups entries by a lexical concept (`gmeow:glossaryConcept`) so the cross-batch
//! consistency invariant (`lang:GlossaryTermConsistencyConstraint`) can be stated over
//! it.
//!
//! ## The Frege-triangle sense structure and the homograph escape
//!
//! Each entry carries the real OntoLex Frege triangle, not a flat concept skeleton: a
//! `lang:Sense` (`gmeow:glossarySense`) that `lang:evokes` the entry's
//! `lang:LexicalConcept` (`gmeow:glossaryConcept`). The concept remains the grouping key
//! the consistency invariant reads: it is minted from the English source skeleton, so two
//! entries whose source normalizes identically share one concept (and the fast `make
//! i18n-lint` gate requires their renderings to agree). The sense is minted per `(term,
//! source-skeleton)` and is INDEPENDENT of target language, so two DISTINCT terms sharing
//! one source skeleton (a class and its property twin) become two distinct senses that
//! both evoke the one shared concept — the exact OntoLex synonymy model (synonymy derived
//! from two senses evoking one concept, never asserted flat) — while the SAME term's fr
//! and zh renderings collapse onto one sense (one way of meaning, many language
//! renderings). A source explicitly declared a `lang:DeclaredTerminologyHomograph` (via
//! [`gmeow_docs::i18n_compile::declared_homograph_sources`], the SAME loader the lint
//! consults — no second source of truth) is instead minted a DISTINCT concept per term,
//! so its genuinely-distinct senses evoke distinct concepts and never trip the
//! single-valued invariant.
//!
//! All identities are content-addressed and the N-Triples are sorted + deduped, so the
//! corpus is byte-reproducible (no clock, no randomness).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use gmeow_docs::i18n_compile::{
    authored_turtle_files, declared_homograph_sources, is_candidate_translation, language_from_po,
    parse_po,
};
use gmeow_validate::distinctiveness::skeleton;
use purrdf::slice::{ArtifactRole, SliceCatalog};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Committed logical path of the human-readable per-slice terminology glossary — the
/// Markdown projection of the `graph/lang-glossary-corpus` graph, grouped by slice then
/// language. A byte-decorated opaque fanout member (it carries a GENERATED banner and
/// section headers), reconstructed by the superset gate from the `REP_GENERATED` archive.
pub const GLOSSARY_TABLE_PATH: &str = "generated/catalog/glossary.md";

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The example-instance base every minted glossary IRI lives under — shared with the
/// sibling `lang:` corpora so the competency queries scope with the same `STRSTARTS`.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// The assembled glossary corpus: the sorted, byte-stable N-Triples graph
/// (`graph/lang-glossary-corpus`).
pub struct LangGlossaryCorpus {
    /// The deterministic, sorted, byte-stable N-Triples graph.
    pub ntriples: Vec<u8>,
}

/// One term-grain glossary entry derived from a single reviewed `.po` pair.
struct Entry {
    glossary_iri: String,
    slice_iri: String,
    lang: String,
    entry_iri: String,
    term: String,
    predicate: String,
    source: String,
    translation: String,
    concept_iri: String,
    sense_iri: String,
    unit_iri: String,
}

/// Build the per-slice terminology glossary corpus by folding every reviewed `.po`
/// catalog pair under `root` into its `(slice, language)` `gmeow:Glossary`.
pub fn build_corpus(root: &Path) -> Result<LangGlossaryCorpus, gmeow_errors::Diag> {
    let entries = build_entries(root)?;
    Ok(LangGlossaryCorpus {
        ntriples: emit_ntriples(&entries),
    })
}

/// Fold every reviewed `.po` catalog pair under `root` into the sorted list of term-grain
/// [`Entry`] records — the SINGLE derivation both the N-Triples corpus ([`emit_ntriples`])
/// and the human-readable table ([`render_glossary_table`]) project, so the graph and the
/// table can never drift (they share one in-memory list, never two parses of the `.po`).
fn build_entries(root: &Path) -> Result<Vec<Entry>, gmeow_errors::Diag> {
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!("lang-glossary slice catalog: {e}"),
                })
            })?;
    // The ontology-resident homograph escape — the SAME loader the make i18n-lint gate
    // consults (One Canonical Source), read from authored TTL only.
    let homographs = declared_homograph_sources(root);

    let mut entries: Vec<Entry> = Vec::new();
    for record in catalog.records() {
        let slice_iri = record.manifest.slice_iri.clone();
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::TranslationCatalog {
                continue;
            }
            // A translation catalog is required input: invalid UTF-8 is a HARD FAIL,
            // never a silent lossy repair.
            let text = std::str::from_utf8(&artifact.content).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!(
                        "lang-glossary: translation catalog '{}' is not valid UTF-8: {e}",
                        artifact.logical_path
                    ),
                })
            })?;
            let lang = language_from_po(text)?.unwrap_or_default();
            let lang = lang.trim().to_string();
            // The English carrier catalog (or a header-less file) is not a glossary.
            if lang.is_empty() || lang.eq_ignore_ascii_case("en") {
                continue;
            }
            for entry in &parse_po(text, false)? {
                // Only reviewed (non-fuzzy, present) pairs are AGREED terminology; the
                // header entry and malformed msgctxts are not crossings.
                if entry.msgctxt.is_empty() || !entry.msgctxt.contains('|') {
                    continue;
                }
                if !is_candidate_translation(entry) {
                    continue;
                }
                let (term, predicate) = entry.msgctxt.split_once('|').unwrap();
                let src_skel = skeleton(&entry.msgid);
                // Homograph split: a declared homograph rides a per-term concept (its
                // distinct senses stay distinct); every other source shares one concept
                // per source skeleton.
                let concept_key = if homographs.contains(&src_skel) {
                    format!("{src_skel}\u{1f}{term}")
                } else {
                    src_skel.clone()
                };
                entries.push(Entry {
                    glossary_iri: example(
                        "glossary",
                        &digest16("glossary", &format!("{slice_iri}\u{1f}{lang}")),
                    ),
                    slice_iri: slice_iri.clone(),
                    lang: lang.clone(),
                    entry_iri: example(
                        "glossary-entry",
                        &digest16("entry", &format!("{}\u{1f}{lang}", entry.msgctxt)),
                    ),
                    term: term.to_string(),
                    predicate: predicate.to_string(),
                    source: entry.msgid.clone(),
                    translation: entry.msgstr.clone(),
                    concept_iri: example("glossary-concept", &digest16("concept", &concept_key)),
                    // The Frege-triangle sense: keyed by (term, source-skeleton) and
                    // language-independent, so distinct terms sharing one source mint
                    // distinct senses that both evoke the one concept (synonymy derived),
                    // while one term's renderings across languages share one sense.
                    sense_iri: example(
                        "glossary-sense",
                        &digest16("sense", &format!("{term}\u{1f}{src_skel}")),
                    ),
                    unit_iri: crate::stages::lang_translation::unit_iri(&entry.msgctxt, &lang),
                });
            }
        }
    }

    entries.sort_by(|a, b| a.entry_iri.cmp(&b.entry_iri));
    Ok(entries)
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole glossary corpus.
fn emit_ntriples(entries: &[Entry]) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();
    for e in entries {
        // The glossary container (one per (slice, language)) — deduped by the sort below.
        lines.push(triple(
            &e.glossary_iri,
            RDF_TYPE,
            &iri(GMEOW_NS, "Glossary"),
        ));
        lines.push(triple(
            &e.glossary_iri,
            &iri(GMEOW_NS, "glossarySlice"),
            &e.slice_iri,
        ));
        lines.push(triple_lit(
            &e.glossary_iri,
            &iri(GMEOW_NS, "glossaryLanguage"),
            &e.lang,
        ));
        lines.push(triple(
            &e.glossary_iri,
            &iri(GMEOW_NS, "glossaryEntry"),
            &e.entry_iri,
        ));

        // The term-grain entry.
        lines.push(triple(
            &e.entry_iri,
            RDF_TYPE,
            &iri(GMEOW_NS, "GlossaryEntry"),
        ));
        lines.push(triple(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossaryTerm"),
            &e.term,
        ));
        lines.push(triple_lit(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossaryPredicate"),
            &e.predicate,
        ));
        lines.push(triple_lit(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossarySource"),
            &e.source,
        ));
        lines.push(triple_lit(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossaryTranslation"),
            &e.translation,
        ));
        lines.push(triple(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossaryConcept"),
            &e.concept_iri,
        ));
        lines.push(triple(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossarySense"),
            &e.sense_iri,
        ));
        lines.push(triple(
            &e.entry_iri,
            &iri(GMEOW_NS, "glossaryUnit"),
            &e.unit_iri,
        ));

        // The Frege triangle: the entry's sense (the ontolex:LexicalSense peer) evokes the
        // lexical concept (the ontolex:LexicalConcept peer). Both are typed so the graph is
        // self-describing; two distinct senses evoking one concept ARE the synonymy the
        // consistency invariant groups on gmeow:glossaryConcept.
        lines.push(triple(&e.sense_iri, RDF_TYPE, &iri(LANG_NS, "Sense")));
        lines.push(triple(
            &e.sense_iri,
            &iri(LANG_NS, "evokes"),
            &e.concept_iri,
        ));
        lines.push(triple(
            &e.concept_iri,
            RDF_TYPE,
            &iri(LANG_NS, "LexicalConcept"),
        ));
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

// ── N-Triples helpers (mirroring the sibling lang: corpora) ──────────────────────

fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

fn example(segment: &str, id: &str) -> String {
    format!("{EXAMPLE_BASE}{segment}/{id}")
}

/// A stable 16-hex-char content address over a domain-separated key.
fn digest16(domain: &str, key: &str) -> String {
    let digest = Sha256::digest(format!("{domain}\u{1f}{key}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}

fn triple_lit(subject: &str, predicate: &str, literal: &str) -> String {
    format!("<{subject}> <{predicate}> {} .", nt_literal(literal))
}

/// Escape a string as an N-Triples quoted literal (UTF-8 passes through verbatim).
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

// ── The human-readable glossary table (a byte-decorated `REP_GENERATED` projection) ──────

/// The GENERATED banner + prose head of the glossary table. Its presence (a comment
/// banner + Markdown section headers, not graph data) is why the artifact rides an opaque
/// archive member rather than a canonical named-graph fold.
const TABLE_HEADER: &str = "<!-- GENERATED by gmeow lang-glossary — DO NOT EDIT. -->\n\n# Terminology glossary\n\nPer-slice bilingual terminology derived from the reviewed `slices/**/i18n/<lang>.po`\ncatalogs — the human-readable projection of the `graph/lang-glossary-corpus` graph in\n`gmeow.gts`. One row per reviewed source→target crossing: the term, the annotation\npredicate, the English source, the target rendering, and the lexical concept it groups\nunder (two rows sharing one concept are the synonymy the cross-batch consistency\ninvariant reads).\n";

/// The trailing local segment of an IRI (after the final `/` or `#`) — the readable name
/// for the term and concept columns. A lossy, sanctioned projection detail.
fn localname(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string()
}

/// Escape a value for a single Markdown table cell: collapse every newline/tab to a space
/// (a cell is one line) and escape the column separator. Deterministic and total.
fn cell(value: &str) -> String {
    let collapsed: String = value
        .chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect();
    collapsed.replace('|', "\\|")
}

/// Render the human-readable glossary table from the SAME [`Entry`] list the N-Triples
/// corpus folds ([`build_entries`]) — grouped by slice then language, sorted and
/// deterministic (no clock, no randomness). A pure projection of the corpus (Principle
/// 4/17): lossy (localnames, whitespace-collapsed cells) but byte-reconstructible from the
/// bundle, and provably non-drifting from the graph because both read one in-memory list.
fn render_glossary_table(entries: &[Entry]) -> String {
    // Stable presentation order: slice, then language, then term, predicate, source.
    let mut rows: Vec<&Entry> = entries.iter().collect();
    rows.sort_by(|a, b| {
        (&a.slice_iri, &a.lang, &a.term, &a.predicate, &a.source).cmp(&(
            &b.slice_iri,
            &b.lang,
            &b.term,
            &b.predicate,
            &b.source,
        ))
    });

    let mut out = String::from(TABLE_HEADER);
    let mut cur_slice: Option<&str> = None;
    let mut cur_lang: Option<&str> = None;
    let mut slice_count = 0usize;
    let mut group_count = 0usize;
    for e in &rows {
        if cur_slice != Some(e.slice_iri.as_str()) {
            cur_slice = Some(&e.slice_iri);
            cur_lang = None;
            slice_count += 1;
            out.push_str(&format!(
                "\n## slice: {} (`{}`)\n",
                localname(&e.slice_iri),
                e.slice_iri
            ));
        }
        if cur_lang != Some(e.lang.as_str()) {
            cur_lang = Some(&e.lang);
            group_count += 1;
            out.push_str(&format!(
                "\n### language: {lang}\n\n| term | predicate | English source | {lang} rendering | concept |\n|---|---|---|---|---|\n",
                lang = e.lang
            ));
        }
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            cell(&localname(&e.term)),
            cell(&e.predicate),
            cell(&e.source),
            cell(&e.translation),
            cell(&localname(&e.concept_iri))
        ));
    }

    out.push_str(&format!(
        "\n**{} entries** across {group_count} per-slice glossary group(s) in {slice_count} slice(s).\n",
        rows.len()
    ));
    out
}

// ── Stage impl ───────────────────────────────────────────────────────────────────────────

/// The `stage-export-glossary` export leaf: the committed human-readable terminology
/// glossary table, projected from the SAME reviewed `.po` fold `graph/lang-glossary-corpus`
/// carries (via [`build_entries`]) — never a second parse, so the table and the graph
/// cannot drift. A source-reading leaf like [`crate::stages::matrix::MatrixStage`]
/// (`consumes() == []`): it reads the authored catalogs + homograph declarations directly
/// and folds one opaque `REP_GENERATED` member the sink carries into `gmeow.gts`.
pub struct GlossaryTableStage;

impl Stage for GlossaryTableStage {
    fn id(&self) -> &str {
        "stage-export-glossary"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "lang_glossary_table.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // `consumes() == []`: the table is a pure fold of the reviewed `.po` catalogs
        // (`slices/**/i18n/*.po`) and the homograph escape's authored TTL. Declare BOTH
        // read surfaces so any edit — a new/changed translation, a flipped homograph
        // declaration — busts the cache. This is the SAME input set `build_entries`
        // reads (the `.po` via the slice catalog, the homographs via `authored_turtle_files`).
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(groups) = std::fs::read_dir(root.join("slices")) {
            for group in groups.flatten() {
                if let Ok(names) = std::fs::read_dir(group.path()) {
                    for name in names.flatten() {
                        if let Ok(pos) = std::fs::read_dir(name.path().join("i18n")) {
                            for po in pos.flatten() {
                                let p = po.path();
                                if p.extension().is_some_and(|x| x == "po") {
                                    files.push(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        files.extend(authored_turtle_files(root));
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let entries = build_entries(input.root)?;
        let md = render_glossary_table(&entries);
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(GLOSSARY_TABLE_PATH.to_string(), md.into_bytes());
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn corpus_emits_glossary_entries_for_reviewed_pairs() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");
        assert!(
            nt.contains(&iri(GMEOW_NS, "Glossary")),
            "corpus must type gmeow:Glossary containers"
        );
        assert!(
            nt.contains(&iri(GMEOW_NS, "GlossaryEntry")),
            "corpus must type gmeow:GlossaryEntry records"
        );
        // Every entry carries its term-grain payload and its sense anchor.
        for pred in [
            "glossarySlice",
            "glossaryLanguage",
            "glossaryTerm",
            "glossarySource",
            "glossaryTranslation",
            "glossaryConcept",
            "glossarySense",
            "glossaryUnit",
        ] {
            assert!(
                nt.contains(&iri(GMEOW_NS, pred)),
                "corpus must emit gmeow:{pred}"
            );
        }
        assert!(
            nt.contains(&iri(LANG_NS, "LexicalConcept")),
            "each glossary concept is typed lang:LexicalConcept"
        );
        // The Frege triangle: each entry's sense is typed lang:Sense and evokes its concept.
        assert!(
            nt.contains(&iri(LANG_NS, "Sense")),
            "each glossary sense is typed lang:Sense"
        );
        assert!(
            nt.contains(&iri(LANG_NS, "evokes")),
            "each glossary sense lang:evokes its lexical concept"
        );
        // A known reviewed lang-slice term rendered in French appears as a source/translation.
        assert!(
            nt.contains("\"Composed Form\""),
            "the ComposedForm English source must be present"
        );
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let a = build_corpus(&repo_root()).expect("build a").ntriples;
        let b = build_corpus(&repo_root()).expect("build b").ntriples;
        assert_eq!(a, b, "glossary corpus N-Triples must be deterministic");
    }

    #[test]
    fn glossary_table_is_deterministic_and_projects_the_corpus() {
        // The table is the human-readable projection of the SAME entry list the N-Triples
        // corpus folds — rendered on the production path, byte-deterministic (no clock, no
        // randomness), and provably non-drifting from the graph. Mirrors matrix.rs's
        // "module-status.md drifted from committed" drift test, but renders-and-compares
        // in-memory (the committed generated/catalog/glossary.md is emitted by `make sync`
        // at G6; the renderer + this test fully specify its bytes).
        let entries = build_entries(&repo_root()).expect("build entries");
        let table = render_glossary_table(&entries);
        assert_eq!(
            table,
            render_glossary_table(&entries),
            "glossary table render must be deterministic"
        );
        // The GENERATED banner + slice/language grouping headers (why it is byte-decorated).
        assert!(
            table.starts_with("<!-- GENERATED by gmeow lang-glossary"),
            "table carries the GENERATED banner: {}",
            &table[..table.len().min(80)]
        );
        assert!(table.contains("## slice: "), "grouped by slice");
        assert!(table.contains("### language: fr"), "grouped by language");
        // EXACTLY one body row per reviewed corpus crossing — the table cannot carry a row
        // the graph does not, nor drop one it does (they share one in-memory Entry list).
        let body_rows = table
            .lines()
            .filter(|l| l.starts_with("| ") && !l.starts_with("| term "))
            .count();
        assert_eq!(
            body_rows,
            entries.len(),
            "one glossary table row per corpus entry"
        );
        // A known reviewed lang-slice term appears with its English source and French
        // rendering — the table derives from the real reviewed `.po` catalogs.
        assert!(
            table.contains("Entity Existence"),
            "the EntityExistence English source must appear"
        );
        assert!(
            table.contains("Existence d'entité"),
            "the EntityExistence French rendering must appear"
        );
    }

    #[test]
    fn glossary_table_escapes_pipes_and_collapses_newlines() {
        // Cell escaping is total and deterministic: a column separator is escaped and any
        // newline/tab collapses to a space, so a multi-line skos:definition never breaks
        // the Markdown table.
        assert_eq!(cell("a | b"), "a \\| b");
        assert_eq!(cell("line1\nline2\tx"), "line1 line2 x");
        assert_eq!(
            localname("https://blackcatinformatics.ca/gmeow/EntityExistence"),
            "EntityExistence"
        );
    }

    #[test]
    fn glossary_unit_joins_the_translation_corpus() {
        // gmeow:glossaryUnit points at the SAME content-addressed lang:TranslationUnit the
        // translation corpus mints, so the two graphs join in gmeow.gts.
        let glossary = String::from_utf8(build_corpus(&repo_root()).expect("g").ntriples).unwrap();
        let translation = String::from_utf8(
            crate::stages::lang_translation::build_corpus(&repo_root())
                .expect("t")
                .ntriples,
        )
        .unwrap();
        // Pick a glossaryUnit object and confirm the translation corpus types it as a unit.
        let unit = glossary
            .lines()
            .find_map(|l| {
                l.contains(&iri(GMEOW_NS, "glossaryUnit")).then(|| {
                    l.rsplit("> <")
                        .next()
                        .and_then(|s| s.strip_suffix("> ."))
                        .unwrap_or("")
                        .trim_start_matches('<')
                        .to_string()
                })
            })
            .expect("at least one glossaryUnit edge");
        assert!(
            translation.contains(&format!(
                "<{unit}> <{RDF_TYPE}> <{}TranslationUnit>",
                LANG_NS
            )),
            "glossaryUnit {unit} must be a lang:TranslationUnit in the translation corpus"
        );
    }
}
