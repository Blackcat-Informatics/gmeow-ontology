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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use gmeow_docs::i18n_compile::{
    authored_turtle_files, declared_homograph_sources, is_candidate_translation, language_from_po,
    parse_po,
};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;
use gmeow_validate::distinctiveness::skeleton;
use purrdf::slice::{ArtifactRole, SliceCatalog};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Committed logical path of the human-readable per-slice terminology glossary — the
/// Markdown projection of the `graph/lang-glossary-corpus` graph, grouped by slice then
/// language. A byte-decorated opaque fanout member (it carries a GENERATED banner and
/// section headers), reconstructed by the superset gate from the
/// [`REP_LANG_PROJECTIONS`](crate::stages::archive_blobs::REP_LANG_PROJECTIONS) archive:
/// it is natural-language term inventory, so it rides the `lang:` family's rep and is
/// primed by `gmeow:dictGmeowLangAstV1` rather than by the core-tier dictionary.
pub const GLOSSARY_TABLE_PATH: &str = "generated/catalog/glossary.md";

/// Committed logical path of the OntoLex `vartrans:translation` interop lowering — the
/// RDF-native terminology projection of `graph/lang-glossary-corpus`: one
/// `vartrans:Translation` per reviewed source→target crossing, relating the source and
/// target `ontolex:LexicalSense`, grouped in a per-language `vartrans:TranslationSet`.
/// An `.ttl` RDF output, so it rides as an RDF-fanout NAMED GRAPH (its fanout IRI is
/// [`crate::stages::superset::rdf_fanout_graph_iri`] of this path), NOT a byte-decorated
/// blob: `stage-export-glossary` emits the canonical Turtle fold and the snapshot carries
/// it into `graph/fanout/projections/glossary.vartrans.ttl`, which the superset gate
/// re-serializes byte-for-byte (unlike the non-RDF `.md`/`.tbx` blobs beside it).
pub const GLOSSARY_VARTRANS_PATH: &str = "generated/projections/glossary.vartrans.ttl";

/// Committed logical path of the TBX (ISO-30042 TermBase eXchange) interop lowering — the
/// standard terminology-interchange XML projection of `graph/lang-glossary-corpus`: one
/// `<termEntry>` per lexical concept, one `<langSet>` per language, one `<tig><term>` per
/// term. A byte-decorated opaque fanout member (an XML prolog + `<!-- GENERATED -->` banner)
/// riding `stage-export-glossary`, reconstructed by the superset gate from
/// [`REP_LANG_PROJECTIONS`](crate::stages::archive_blobs::REP_LANG_PROJECTIONS) — the
/// `lang:` family's own rep — for the same reason [`GLOSSARY_TABLE_PATH`] does.
pub const GLOSSARY_TBX_PATH: &str = "generated/projections/glossary.tbx";

use gmeow_ns::GMEOW_NS;
use gmeow_ns::LANG_NS;
use gmeow_ns::LOGIC_NS;
/// The OntoLex-Lemon core namespace — the RDF-native terminology structure the vartrans
/// lowering reuses (`ontolex:LexicalEntry`/`LexicalSense`/`Form`/`writtenRep`/`reference`).
const ONTOLEX_NS: &str = "http://www.w3.org/ns/lemon/ontolex#";
/// The OntoLex-Lemon variation-and-translation module namespace — the crossing itself
/// (`vartrans:Translation`/`translation`/`source`/`target`/`TranslationSet`/`trans`).
const VARTRANS_NS: &str = "http://www.w3.org/ns/lemon/vartrans#";
/// Dublin Core Terms — the `dct:language` tag on a target lexical entry.
const DCT_NS: &str = "http://purl.org/dc/terms/";
/// The example-instance base every minted vartrans projection individual lives under (the
/// forward `glossary → OntoLex vartrans` peer of the sibling `lang:` projection bases).
const VARTRANS_BASE: &str = "http://example.org/lang/glossary-vartrans/";
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
    let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())
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

// ── The human-readable glossary table (a byte-decorated `lang:`-family projection) ───────

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

// ── The two external interop lowerings (Principle-17 generated lowering artifacts) ──────
//
// Both project the SAME reviewed-crossing [`Entry`] list ([`build_entries`]) the graph folds
// — never a second `.po` parse — so the interchange files and the graph cannot drift. Both
// are LOSSY sound-under-approximations: they carry the term↔term crossing faithfully but drop
// the sense law-spine (the `logic:Correspondence` on the `lang:TranslationUnit`), the
// crossing's `logic:preservationKind`, the annotation-predicate provenance, and the homograph
// declaration; TBX additionally flattens the whole Frege-triangle sense structure and the term
// IRI grounding. Every drop is enumerated in the paired `lang:ProjectionEmission` record
// ([`build_lowering_corpus`]) so honest-lossy is a passing conformance state and silent-lossy
// is a red build (`lang:UndeclaredUnsupportedConstruct`).

/// Render the OntoLex `vartrans:translation` lowering as deterministic N-Triples/Turtle-subset
/// statements: each reviewed crossing becomes a `vartrans:Translation` relating a source
/// `ontolex:LexicalSense` (REUSING the carried `lang:Sense` identity, the OntoLex peer) to a
/// per-crossing target `ontolex:LexicalSense`, both `ontolex:reference`-ing the carried
/// `lang:LexicalConcept`, and grouped in a per-language `vartrans:TranslationSet`. Sorted +
/// deduped over full-IRI statements (a valid Turtle subset — no banner, no prefixes), so it
/// canonicalizes to the exact fanout fold in [`vartrans_fanout_ttl`] and is byte-reproducible
/// (no clock, no randomness).
fn render_vartrans_statements(entries: &[Entry]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for e in entries {
        let sense_local = localname(&e.sense_iri);
        let entry_local = localname(&e.entry_iri);
        // The source lexical entry/form/sense: keyed by the carried sense (language-independent),
        // so two languages of one term share one source sense — the OntoLex synonymy peer.
        let src_entry = format!("{VARTRANS_BASE}entry-src/{sense_local}");
        let src_form = format!("{VARTRANS_BASE}form-src/{sense_local}");
        let src_sense = e.sense_iri.clone();
        // The target lexical entry/form/sense + the crossing: keyed per crossing (a distinct
        // rendering per (term, language, predicate) crossing).
        let tgt_entry = format!("{VARTRANS_BASE}entry-tgt/{entry_local}");
        let tgt_form = format!("{VARTRANS_BASE}form-tgt/{entry_local}");
        let tgt_sense = format!("{VARTRANS_BASE}sense-tgt/{entry_local}");
        let translation = format!("{VARTRANS_BASE}translation/{entry_local}");
        let trans_set = format!("{VARTRANS_BASE}set/{}", e.lang);

        lines.push(triple(
            &src_entry,
            RDF_TYPE,
            &iri(ONTOLEX_NS, "LexicalEntry"),
        ));
        lines.push(triple(
            &src_entry,
            &iri(ONTOLEX_NS, "canonicalForm"),
            &src_form,
        ));
        lines.push(triple(&src_entry, &iri(ONTOLEX_NS, "sense"), &src_sense));
        lines.push(triple(&src_form, RDF_TYPE, &iri(ONTOLEX_NS, "Form")));
        lines.push(triple_langlit(
            &src_form,
            &iri(ONTOLEX_NS, "writtenRep"),
            &e.source,
            "en",
        ));
        lines.push(triple(
            &src_sense,
            RDF_TYPE,
            &iri(ONTOLEX_NS, "LexicalSense"),
        ));
        lines.push(triple(
            &src_sense,
            &iri(ONTOLEX_NS, "reference"),
            &e.concept_iri,
        ));

        lines.push(triple(
            &tgt_entry,
            RDF_TYPE,
            &iri(ONTOLEX_NS, "LexicalEntry"),
        ));
        lines.push(triple(
            &tgt_entry,
            &iri(ONTOLEX_NS, "canonicalForm"),
            &tgt_form,
        ));
        lines.push(triple(&tgt_entry, &iri(ONTOLEX_NS, "sense"), &tgt_sense));
        lines.push(triple_lit(&tgt_entry, &iri(DCT_NS, "language"), &e.lang));
        lines.push(triple(&tgt_form, RDF_TYPE, &iri(ONTOLEX_NS, "Form")));
        lines.push(triple_langlit(
            &tgt_form,
            &iri(ONTOLEX_NS, "writtenRep"),
            &e.translation,
            &e.lang,
        ));
        lines.push(triple(
            &tgt_sense,
            RDF_TYPE,
            &iri(ONTOLEX_NS, "LexicalSense"),
        ));
        lines.push(triple(
            &tgt_sense,
            &iri(ONTOLEX_NS, "reference"),
            &e.concept_iri,
        ));

        lines.push(triple(
            &translation,
            RDF_TYPE,
            &iri(VARTRANS_NS, "Translation"),
        ));
        lines.push(triple(
            &translation,
            &iri(VARTRANS_NS, "source"),
            &src_sense,
        ));
        lines.push(triple(
            &translation,
            &iri(VARTRANS_NS, "target"),
            &tgt_sense,
        ));
        lines.push(triple(
            &trans_set,
            RDF_TYPE,
            &iri(VARTRANS_NS, "TranslationSet"),
        ));
        lines.push(triple(&trans_set, &iri(VARTRANS_NS, "trans"), &translation));
    }
    lines.sort();
    lines.dedup();
    let mut out = String::new();
    for line in &lines {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The canonical Turtle fold of the OntoLex vartrans lowering — the EXACT bytes the superset
/// gate reconstructs from the `graph/fanout/projections/glossary.vartrans.ttl` named graph.
/// Emitted through the single wasm-clean canonical-Turtle renderer under the shared prefix
/// authority ([`crate::stages::superset::rdf_prefixes`]), so `committed file == named-graph
/// fold` holds by construction (identical prefix selection + renderer on both legs, exactly
/// like `generated/evals/scores.ttl`). No banner: an RDF file travels as RDF, never a blob.
fn vartrans_fanout_ttl(entries: &[Entry]) -> Result<Vec<u8>, gmeow_errors::Diag> {
    purrdf::turtle_normalize::canonical_turtle(
        render_vartrans_statements(entries).as_bytes(),
        &crate::stages::superset::rdf_prefixes(),
    )
    .map(String::into_bytes)
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-glossary".to_string(),
            message: format!("canonicalize {GLOSSARY_VARTRANS_PATH}: {e}"),
        })
    })
}

/// Render the TBX (ISO-30042 TermBase eXchange) lowering as deterministic XML: one
/// `<termEntry>` per lexical concept (the grouping key the glossary already folds on), one
/// `<langSet xml:lang=…>` per language (English source + each target rendering), one
/// `<tig><term>` per distinct term. Grouped and sorted by concept then language then term, XML
/// escaped, no clock — so the termbase is byte-reproducible.
fn render_tbx(entries: &[Entry]) -> String {
    // concept → language → sorted distinct terms. The English source rides the "en" langSet;
    // each target rendering rides its own language langSet.
    let mut concepts: BTreeMap<&str, BTreeMap<&str, BTreeSet<&str>>> = BTreeMap::new();
    for e in entries {
        let by_lang = concepts.entry(e.concept_iri.as_str()).or_default();
        by_lang.entry("en").or_default().insert(e.source.as_str());
        by_lang
            .entry(e.lang.as_str())
            .or_default()
            .insert(e.translation.as_str());
    }

    let mut out = String::from(TBX_HEADER);
    for (concept, by_lang) in &concepts {
        let id = format!("c-{}", localname(concept));
        out.push_str(&format!("      <termEntry id=\"{}\">\n", xml_attr(&id)));
        for (lang, terms) in by_lang {
            out.push_str(&format!(
                "        <langSet xml:lang=\"{}\">\n",
                xml_attr(lang)
            ));
            for term in terms {
                out.push_str("          <tig>\n");
                out.push_str(&format!("            <term>{}</term>\n", xml_text(term)));
                out.push_str("          </tig>\n");
            }
            out.push_str("        </langSet>\n");
        }
        out.push_str("      </termEntry>\n");
    }
    out.push_str(TBX_FOOTER);
    out
}

/// The XML prolog + `<!-- GENERATED -->` banner + `<martif>` head of the TBX termbase.
const TBX_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- GENERATED by gmeow lang-glossary — DO NOT EDIT. -->\n<martif type=\"TBX\" xml:lang=\"en\">\n  <martifHeader>\n    <fileDesc>\n      <sourceDesc>\n        <p>Projected from the GMEOW per-slice terminology glossary (graph/lang-glossary-corpus): one termEntry per lexical concept, one langSet per language, one tig per term. A LOSSY sound under-approximation — the Frege-triangle sense structure, the crossing's logic:Correspondence law-spine and preservation kind, the predicate provenance, the homograph declaration, and the term IRI grounding are dropped. The honest drop list rides the lang:ProjectionEmission \"TBX (ISO 30042)\" record in graph/lang-projection-corpus.</p>\n      </sourceDesc>\n    </fileDesc>\n  </martifHeader>\n  <text>\n    <body>\n";

/// The `<martif>` tail of the TBX termbase.
const TBX_FOOTER: &str = "    </body>\n  </text>\n</martif>\n";

/// Escape a value for XML element content (`&`, `<`, `>`).
fn xml_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escape a value for a double-quoted XML attribute (`&`, `<`, `>`, `"`).
fn xml_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A language-tagged N-Triples/Turtle literal object (`"text"@lang`).
fn triple_langlit(subject: &str, predicate: &str, text: &str, lang: &str) -> String {
    format!("<{subject}> <{predicate}> {}@{lang} .", nt_literal(text))
}

// ── The honest per-target loss ledger (the `lang:ProjectionEmission` records) ─────────────

/// The assembled glossary interop-lowering corpus: the two `lang:ProjectionEmission` records
/// (folded into `graph/lang-projection-corpus` by the mappings stage), the two honest lossy
/// loss-ledger rows, and the loss store their enumerated drops are interned into.
pub struct GlossaryLoweringCorpus {
    /// The deterministic, sorted N-Triples of the two `lang:ProjectionEmission` records —
    /// appended to `graph/lang-projection-corpus` alongside the sibling `lang:` emissions.
    pub emission_ntriples: Vec<u8>,
    /// One honest lossy `ProjectionResult` per target (`SoundUnderApproximation`). The rows
    /// carry only identity/judgment; their enumerated drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store each target's enumerated drops are interned into, keyed by target focus.
    /// The mappings stage unions it into the single report loss store.
    pub loss: LossLedger,
}

/// One glossary interop target: its human-readable emission name, its loss-ledger focus key,
/// whether its artifact is RDF, and the ordered list of constructs it drops (the honest
/// `lang:unsupportedConstruct` set — enumerated because both targets are lossy).
struct LoweringTarget {
    name: &'static str,
    ledger_key: &'static str,
    is_rdf: bool,
    drops: &'static [&'static str],
}

/// The two shipped glossary interop lowerings. Every drop is a construct the glossary corpus
/// carries that the target cannot: an undeclared drop trips `lang:UndeclaredUnsupportedConstruct`
/// (the overclaim floor, over bundle data), so honest-lossy is the only passing state.
const LOWERING_TARGETS: &[LoweringTarget] = &[
    LoweringTarget {
        name: "OntoLex vartrans",
        ledger_key: "glossary-lowering:ontolex-vartrans",
        is_rdf: true,
        // vartrans is RDF-native and keeps the sense/concept structure; it still drops the
        // crossing's law-spine and provenance.
        drops: &[
            "the lang:TranslationUnit crossing's logic:Correspondence law-spine (gmeow:glossaryUnit)",
            "the crossing's logic:preservationKind judgment",
            "the gmeow:glossaryPredicate annotation-predicate provenance",
            "the lang:DeclaredTerminologyHomograph declaration that split the concept",
        ],
    },
    LoweringTarget {
        name: "TBX (ISO 30042)",
        ledger_key: "glossary-lowering:tbx",
        is_rdf: false,
        // TBX is a flat concept/langSet/term termbase; it flattens the whole Frege triangle and
        // the term IRI grounding on top of the crossing law-spine and provenance.
        drops: &[
            "the OntoLex Frege-triangle sense structure (lang:Sense / lang:evokes / lang:LexicalConcept)",
            "the lang:TranslationUnit crossing's logic:Correspondence law-spine (gmeow:glossaryUnit)",
            "the crossing's logic:preservationKind judgment",
            "the gmeow:glossaryPredicate annotation-predicate provenance",
            "the lang:DeclaredTerminologyHomograph declaration that split the concept",
            "the gmeow:glossaryTerm IRI grounding (TBX terms are plain strings)",
        ],
    },
];

/// Build the two glossary interop lowerings' honest loss ledger: for each target, one
/// `lang:ProjectionEmission` record (folded into `graph/lang-projection-corpus`) declaring its
/// target name, the projected source senses, its `SoundUnderApproximation` preservation kind,
/// and every dropped construct — plus its `ProjectionResult` row + interned drops. Derived from
/// the SAME reviewed-crossing [`build_entries`] list the rendered artifacts and the graph
/// project, never a second parse.
pub fn build_lowering_corpus(root: &Path) -> Result<GlossaryLoweringCorpus, gmeow_errors::Diag> {
    let entries = build_entries(root)?;
    Ok(build_lowering_from_entries(&entries))
}

/// Fold the interop-lowering loss ledger from the in-memory [`Entry`] list (the shared
/// derivation, so the emission records, the rendered artifacts, and the glossary graph never
/// drift). Exposed to the test module and [`build_lowering_corpus`].
fn build_lowering_from_entries(entries: &[Entry]) -> GlossaryLoweringCorpus {
    // The distinct source senses the lowerings project FROM (a `lang:Sense`, the range
    // lang:projectsSource names). Sense-grain, sorted + deduped: two languages of one term
    // share one source sense, so the record names each source sense once.
    let mut senses: Vec<&str> = entries.iter().map(|e| e.sense_iri.as_str()).collect();
    senses.sort_unstable();
    senses.dedup();

    let mut lines: Vec<String> = Vec::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    let mut loss = LossLedger::new();

    for target in LOWERING_TARGETS {
        let emission_iri = example(
            "glossary-projection-emission",
            &digest16("glossary-projection-emission", target.name),
        );
        lines.push(triple(
            &emission_iri,
            RDF_TYPE,
            &iri(LANG_NS, "ProjectionEmission"),
        ));
        lines.push(triple_lit(
            &emission_iri,
            &iri(LANG_NS, "projectionTargetName"),
            target.name,
        ));
        for sense in &senses {
            lines.push(triple(
                &emission_iri,
                &iri(LANG_NS, "projectsSource"),
                sense,
            ));
        }
        // NOT Exact — both lowerings drop structure; a sound under-approximation (the emitted
        // term↔term crossings are entailed, but the projection is incomplete).
        lines.push(triple(
            &emission_iri,
            &iri(LOGIC_NS, "preservationKind"),
            &PreservationKind::SoundUnder.iri(),
        ));
        for drop in target.drops {
            lines.push(triple_lit(
                &emission_iri,
                &iri(LANG_NS, "unsupportedConstruct"),
                drop,
            ));
        }

        // The honest lossy ledger row + its interned drops (the same overclaim-floored
        // plumbing the sibling `lang:` lowerings use). SoundUnder with a non-empty residue
        // clears assert_no_overclaim.
        let drops_owned: Vec<String> = target.drops.iter().map(|d| (*d).to_owned()).collect();
        loss.record_projection_drops(
            target.ledger_key,
            PreservationKind::SoundUnder,
            &drops_owned,
            &drops_owned,
        );
        ledger.push(ProjectionResult {
            target: target.ledger_key.to_owned(),
            content: format!(
                "glossary crossings lowered to {}: sound under-approximation (term↔term carried; \
                 {} construct(s) dropped)",
                target.name,
                target.drops.len()
            ),
            is_rdf: target.is_rdf,
            preservation: PreservationKind::SoundUnder,
            complexity: "n/a".to_owned(),
        });
    }

    lines.sort();
    lines.dedup();
    let mut emission_ntriples = lines.join("\n");
    emission_ntriples.push('\n');
    GlossaryLoweringCorpus {
        emission_ntriples: emission_ntriples.into_bytes(),
        ledger,
        loss,
    }
}

// ── Stage impl ───────────────────────────────────────────────────────────────────────────

/// The `stage-export-glossary` export leaf: the committed human-readable terminology
/// glossary table, projected from the SAME reviewed `.po` fold `graph/lang-glossary-corpus`
/// carries (via [`build_entries`]) — never a second parse, so the table and the graph
/// cannot drift. A source-reading leaf like [`crate::stages::matrix::MatrixStage`]
/// (`consumes() == []`): it reads the authored catalogs + homograph declarations directly.
/// Its two NON-RDF surfaces are tarred into the `lang:` family's archive by
/// `stage-archive-blobs`; the `.vartrans.ttl` rides its RDF-fanout named graph.
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
        // The two external terminology-interchange lowerings (Principle-17 generated
        // lowerings), projected from the SAME reviewed-crossing entry list. The OntoLex
        // vartrans lowering is an `.ttl` RDF output: it rides as the canonical fold of the
        // RDF-fanout named graph `graph/fanout/projections/glossary.vartrans.ttl` (the
        // snapshot's `rdf_fanout_members` reads these bytes off this product and the superset
        // gate re-serializes them byte-for-byte), NOT an opaque blob. The TBX termbase is
        // non-RDF XML, so it is a byte-decorated archive member beside the readable
        // `.md` table — both on the `lang:` family's own rep, not the general opaque one.
        artifacts.insert(
            GLOSSARY_VARTRANS_PATH.to_string(),
            vartrans_fanout_ttl(&entries)?,
        );
        artifacts.insert(
            GLOSSARY_TBX_PATH.to_string(),
            render_tbx(&entries).into_bytes(),
        );
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
        // in-memory (the committed generated/catalog/glossary.md is emitted by `make check`
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
    fn vartrans_lowering_projects_crossings_over_reused_ontolex_structure() {
        // The OntoLex vartrans lowering projects the SAME reviewed-crossing entry list, reusing
        // the carried lang:Sense (as the ontolex:LexicalSense peer) and lang:LexicalConcept.
        let entries = build_entries(&repo_root()).expect("build entries");
        // The full-IRI statement body (the pre-canonicalization Turtle subset): no banner —
        // an RDF file travels as RDF, folded from a named graph, never a byte-decorated blob.
        let ttl = render_vartrans_statements(&entries);
        assert!(
            !ttl.contains("# GENERATED"),
            "vartrans carries NO banner (it is an RDF-fanout named-graph fold): {}",
            &ttl[..ttl.len().min(80)]
        );
        // Real vartrans + OntoLex structure.
        assert!(ttl.contains(&iri(VARTRANS_NS, "Translation")));
        assert!(ttl.contains(&iri(VARTRANS_NS, "TranslationSet")));
        assert!(ttl.contains(&iri(VARTRANS_NS, "source")));
        assert!(ttl.contains(&iri(VARTRANS_NS, "target")));
        assert!(ttl.contains(&iri(ONTOLEX_NS, "LexicalSense")));
        assert!(ttl.contains(&iri(ONTOLEX_NS, "writtenRep")));
        // The carried sense + concept identities are REUSED (the lowering references, never
        // re-mints, the OntoLex structure the glossary corpus already carries).
        let one = &entries[0];
        assert!(
            ttl.contains(&format!("<{}> ", one.sense_iri)),
            "the vartrans source sense reuses the carried lang:Sense IRI"
        );
        assert!(
            ttl.contains(&format!("<{}> .", one.concept_iri)),
            "the vartrans senses reference the carried lang:LexicalConcept IRI"
        );
        // A known reviewed crossing rides the lowering as a lang-tagged writtenRep.
        assert!(
            ttl.contains("\"Existence d'entité\"@fr"),
            "the EntityExistence French rendering must be a fr writtenRep"
        );
        // Deterministic (no clock, no randomness).
        assert_eq!(ttl, render_vartrans_statements(&entries));
        // The canonical fanout fold is deterministic and preserves the crossing content: it
        // parses back to a graph carrying the same vartrans/OntoLex structure. The committed
        // `generated/projections/glossary.vartrans.ttl` IS these bytes (byte-reconstructible
        // from the named graph by the superset gate — mirrors evals/scores.ttl).
        let fold = vartrans_fanout_ttl(&entries).expect("canonicalize vartrans fold");
        assert_eq!(
            fold,
            vartrans_fanout_ttl(&entries).expect("canonicalize vartrans fold (b)"),
            "the vartrans fanout fold must be byte-deterministic"
        );
        let fold_text = String::from_utf8(fold).expect("utf8");
        assert!(
            !fold_text.contains("# GENERATED"),
            "the canonical fold carries no banner"
        );
        assert!(
            fold_text.contains("Existence d'entité"),
            "the canonical fold carries the EntityExistence French rendering"
        );
    }

    #[test]
    fn vartrans_fanout_path_is_an_rdf_fanout_class_with_an_identity_graph_iri() {
        // The committed vartrans path is registered as an RDF-fanout class (routed to a named
        // graph, never an opaque blob), and its fanout graph IRI is an identity in both
        // directions — the SAME invariant the gate + the snapshot rely on to fold it back.
        // RDF-fanout membership is derived from the authored gmeow:fanoutExtracts rows of the
        // pipeline-slice source (the single data authority that replaced the retired
        // is_rdf_fanout_class hand-list), so load the source and consult RdfFanoutClasses.
        let module_ttl = std::fs::read(repo_root().join("slices/core/pipeline/module.ttl"))
            .expect("read pipeline module.ttl");
        let source =
            purrdf::parse_dataset(&module_ttl, "text/turtle", None).expect("parse pipeline source");
        assert!(
            crate::stages::superset::RdfFanoutClasses::from_source(&source)
                .expect("build RdfFanoutClasses from the pipeline source")
                .contains(GLOSSARY_VARTRANS_PATH),
            "the vartrans .ttl must be an RDF-fanout class, not an opaque blob member"
        );
        let iri = crate::stages::superset::rdf_fanout_graph_iri(GLOSSARY_VARTRANS_PATH)
            .expect("vartrans path resolves to a fanout graph IRI");
        assert_eq!(
            iri,
            "https://blackcatinformatics.ca/gmeow/graph/fanout/projections/glossary.vartrans.ttl"
        );
        assert_eq!(
            crate::stages::superset::rdf_fanout_path_for_graph_iri(&iri).as_deref(),
            Some(GLOSSARY_VARTRANS_PATH),
            "the fanout graph IRI inverts back to the committed path (identity both directions)"
        );
    }

    #[test]
    fn tbx_lowering_projects_concepts_and_escapes_xml() {
        let entries = build_entries(&repo_root()).expect("build entries");
        let tbx = render_tbx(&entries);
        assert!(tbx.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(tbx.contains("<martif type=\"TBX\" xml:lang=\"en\">"));
        assert!(tbx.trim_end().ends_with("</martif>"));
        assert!(
            tbx.contains("<termEntry id=\"c-"),
            "one termEntry per concept"
        );
        assert!(
            tbx.contains("<langSet xml:lang=\"en\">"),
            "the English source langSet"
        );
        assert!(
            tbx.contains("<langSet xml:lang=\"fr\">"),
            "a French target langSet"
        );
        assert!(
            tbx.contains("<term>Existence d'entité</term>"),
            "the EntityExistence French rendering must ride a fr tig"
        );
        // Deterministic.
        assert_eq!(tbx, render_tbx(&entries));
        // XML escaping is total: a term with markup metacharacters is escaped, never raw.
        let escaped = xml_text("a & b < c > d");
        assert_eq!(escaped, "a &amp; b &lt; c &gt; d");
        assert_eq!(xml_attr("x\"y"), "x&quot;y");
    }

    #[test]
    fn glossary_lowerings_fold_two_honest_lossy_emissions() {
        use gmeow_logic_compile::projections::assert_no_overclaim;

        let corpus = build_lowering_corpus(&repo_root()).expect("build lowering corpus");
        let nt = String::from_utf8(corpus.emission_ntriples.clone()).expect("utf8");

        // Exactly two ProjectionEmission records, one per target.
        let record_count = nt
            .matches(&format!("<{}> .", iri(LANG_NS, "ProjectionEmission")))
            .count();
        assert_eq!(record_count, 2, "one lang:ProjectionEmission per target");
        assert!(nt.contains("\"OntoLex vartrans\""));
        assert!(nt.contains("\"TBX (ISO 30042)\""));

        // Each emission declares a NON-Exact preservation kind and enumerates ≥1 drop, and
        // names its source senses — the shape gate + UndeclaredUnsupportedConstruct floor.
        assert!(nt.contains(&PreservationKind::SoundUnder.iri()));
        assert!(!nt.contains(&PreservationKind::Exact.iri()));
        assert!(nt.contains(&iri(LANG_NS, "unsupportedConstruct")));
        assert!(nt.contains(&iri(LANG_NS, "projectsSource")));

        // Two honest lossy ledger rows, each clearing the overclaim floor (SoundUnder with a
        // non-empty enumerated residue).
        assert_eq!(corpus.ledger.len(), 2);
        for row in &corpus.ledger {
            assert_eq!(row.preservation, PreservationKind::SoundUnder);
            let residue_owned = corpus.loss.projection_drops_for(&row.target);
            assert!(
                !residue_owned.is_empty(),
                "a lossy lowering must intern its enumerated drops: {}",
                row.target
            );
            let residue: Vec<&str> = residue_owned.iter().map(String::as_str).collect();
            assert_no_overclaim(&row.target, row.preservation, &residue)
                .unwrap_or_else(|e| panic!("overclaim floor violated: {e}"));
        }

        // Every named source is a carried glossary sense (the lang:Sense projectsSource range).
        let entries = build_entries(&repo_root()).expect("entries");
        let sense_marker = format!("<{}> <", iri(LANG_NS, "projectsSource"));
        for line in nt.lines() {
            if let Some(idx) = line.find(&sense_marker) {
                let obj = line[idx + sense_marker.len()..]
                    .trim_end_matches(" .")
                    .trim_end_matches('>');
                assert!(
                    entries.iter().any(|e| e.sense_iri == obj),
                    "projectsSource {obj} is not a carried glossary sense"
                );
            }
        }

        // Deterministic emission bytes.
        assert_eq!(
            corpus.emission_ntriples,
            build_lowering_corpus(&repo_root())
                .expect("b")
                .emission_ntriples
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
