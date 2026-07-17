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
//! English source, target rendering, a sense anchor (`gmeow:glossaryConcept` →
//! `lang:LexicalConcept`), and the `lang:TranslationUnit` that holds the crossing's
//! `logic:Correspondence` law-spine (`gmeow:glossaryUnit`, the SAME content-addressed
//! identity [`crate::stages::lang_translation`] mints, so the two graphs join in
//! `gmeow.gts`).
//!
//! The glossary is a pure DERIVATION of the catalogs — never a second hand-authored
//! source (One Canonical Source). It is a term-grain VIEW distinct from the
//! translation corpus: it emits the term IRI and English source as queryable triples
//! and groups entries by a sense (`gmeow:glossaryConcept`) so the cross-batch
//! consistency invariant (`lang:GlossaryTermConsistencyConstraint`) can be stated over
//! it.
//!
//! ## Sense anchoring and the homograph escape
//!
//! Each entry's `gmeow:glossaryConcept` is minted from the English source skeleton, so
//! two entries whose source normalizes identically share one concept (and the fast
//! `make i18n-lint` gate requires their renderings to agree). A source explicitly
//! declared a `lang:DeclaredTerminologyHomograph` (via [`gmeow_docs::i18n_compile::declared_homograph_sources`],
//! the SAME loader the lint consults — no second source of truth) is instead minted a
//! DISTINCT concept per term, so its genuinely-distinct senses ride distinct concepts
//! and never trip the single-valued invariant.
//!
//! All identities are content-addressed and the N-Triples are sorted + deduped, so the
//! corpus is byte-reproducible (no clock, no randomness).

use std::path::Path;

use sha2::{Digest, Sha256};

use gmeow_docs::i18n_compile::{
    declared_homograph_sources, is_candidate_translation, language_from_po, parse_po,
};
use gmeow_validate::distinctiveness::skeleton;
use purrdf::slice::{ArtifactRole, SliceCatalog};

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
    unit_iri: String,
}

/// Build the per-slice terminology glossary corpus by folding every reviewed `.po`
/// catalog pair under `root` into its `(slice, language)` `gmeow:Glossary`.
pub fn build_corpus(root: &Path) -> Result<LangGlossaryCorpus, gmeow_errors::Diag> {
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
                    src_skel
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
                    unit_iri: crate::stages::lang_translation::unit_iri(&entry.msgctxt, &lang),
                });
            }
        }
    }

    entries.sort_by(|a, b| a.entry_iri.cmp(&b.entry_iri));
    Ok(LangGlossaryCorpus {
        ntriples: emit_ntriples(&entries),
    })
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
            &iri(GMEOW_NS, "glossaryUnit"),
            &e.unit_iri,
        ));

        // The sense anchor (the ontolex:LexicalConcept peer) — typed so the graph is
        // self-describing.
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
