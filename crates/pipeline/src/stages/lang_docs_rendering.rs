// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-typing the existing documentation "language trees" as reified crossings.
//!
//! The multilingual documentation is rendered from the per-slice gettext catalogs
//! (`slices/**/i18n/<lang>.po`): every non-English language tree is the English docs model
//! resolved through those catalogs. This module TYPES that already-existing output — it does
//! NOT re-render and does NOT build a second docs pipeline. Over the same `.po`-derived page
//! and language set the docs renderer walks, it emits three first-class crossing records:
//!
//!   * **One `lang:Rendering` (`lang:renderingDocsPage`) per non-English page.** A "page" is
//!     the ontology term (`msgctxt = "<term-iri>|<predicate>"`, so the page key is the
//!     term-IRI); the rendering `lang:renderedContent`s that term, `lang:renderingForm`s the
//!     translated target surfaces the `lang_translation` corpus already interned, is governed
//!     by the target language's `lang:renderingConvention` (a minted `lang:Orthography`), and
//!     carries a `lang:renderingPreservation` that is the DERIVED weakest-dominates join of
//!     the page's units — never a fabricated stronger grade.
//!   * **One `lang:Translation` per (English page, language) pairing.** It `lang:rollsUpFrom`
//!     the page's `lang:TranslationUnit`s (reusing the exact content-addressed identities
//!     `lang_translation` mints — [`unit_iri`]). The document-level judgment is the DERIVED
//!     join of those units' judgments, surfaced in the ledger and NEVER minted as a
//!     document-level RDF preservation flag (which would trip the `lang:AssertedRollup` gate).
//!   * **A declared `lang:translationGap` for the exec-docs English-only boundary.** The
//!     executable-docs surfaces (the offline SPARQL playground, the reasoned "try it"
//!     inference diffs, and the export substrate) are rendered ONLY in the English tree — see
//!     `crates/docs/src/render.rs` (`render_site_lang_exec` swaps in empty
//!     `ExecutableDocsData` for every non-English language). That asymmetry is recorded as a
//!     typed, marked gap (untranslatability-as-data), never a silent omission.
//!
//! Every record is emitted BOTH as RDF (into the carrier's `graph/lang-docs-rendering-corpus`
//! named graph) AND as a `ProjectionResult` row folded into the loss ledger. All identities
//! are content-addressed and the N-Triples are sorted + deduped, so the corpus is
//! byte-reproducible (no clock, no randomness).

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use gmeow_docs::i18n_compile::{language_from_po, live_translation_target, parse_po};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::slice::{ArtifactRole, SliceCatalog};

use crate::stages::lang_translation::{script_for_lang, target_surface_iri, unit_iri};

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// The example-instance base every minted corpus IRI lives under — the same base the
/// translation / form corpora and the `lang:` competency queries scope with `STRSTARTS(...)`.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// The prose the exec-docs English-only boundary carries as its untranslated English source
/// (the declared asymmetry, recorded as data rather than silently omitted).
const EXEC_DOCS_BOUNDARY: &str = "GMEOW executable documentation surfaces (the offline SPARQL \
     playground, the reasoned try-it inference diffs, and the export substrate) are rendered \
     only in the English documentation tree.";

/// The assembled docs-rendering corpus: the sorted, byte-stable N-Triples graph plus the
/// per-page + per-boundary loss-ledger rows.
pub struct LangDocsRenderingCorpus {
    /// The deterministic, sorted, byte-stable N-Triples graph
    /// (`graph/lang-docs-rendering-corpus`).
    pub ntriples: Vec<u8>,
    /// One `ProjectionResult` per page rendering + per page translation roll-up, plus one per
    /// exec-docs English-only boundary gap. The rows carry only identity/judgment; their drops
    /// live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store every row's drops are interned into (keyed by target focus). The mappings
    /// stage unions it into the single report loss store.
    pub loss: LossLedger,
}

/// One `.po` entry, reduced to what the docs re-typing needs (its `msgctxt` and target).
struct EntryRef {
    msgctxt: String,
    msgstr: String,
    present: bool,
}

/// A page's units grouped for one (term-page, language): the ontology term the page
/// documents plus the entries that render it in `lang`.
struct PageGroup {
    lang: String,
    term_iri: String,
    entries: Vec<EntryRef>,
}

/// Build the docs-rendering corpus by re-typing the `.po`-derived documentation language
/// trees under `root`. Iterates the slice catalog's `ArtifactRole::TranslationCatalog`
/// artifacts (the same catalogs the docs renderer resolves per-language values from), groups
/// each catalog's entries by their `msgctxt` term-IRI page key, and emits the page rendering
/// + roll-up records plus the exec-docs boundary gap.
pub fn build_corpus(root: &std::path::Path) -> Result<LangDocsRenderingCorpus, gmeow_errors::Diag> {
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!("lang-docs-rendering slice catalog: {e}"),
                })
            })?;

    // (lang, term-page IRI) -> the entries that render that page in that language. A
    // BTreeMap keeps the grouping deterministic independent of catalog discovery order.
    let mut pages: BTreeMap<(String, String), Vec<EntryRef>> = BTreeMap::new();
    // The distinct non-English languages seen — each contributes one exec-docs boundary gap.
    let mut langs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::TranslationCatalog {
                continue;
            }
            // A translation catalog is required input: invalid UTF-8 is a HARD FAIL, never a
            // silent lossy repair that would corrupt the surface text it carries.
            let text = std::str::from_utf8(&artifact.content).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!(
                        "lang-docs-rendering: translation catalog '{}' is not valid UTF-8: {e}",
                        artifact.logical_path
                    ),
                })
            })?;
            let lang = language_from_po(text)?.unwrap_or_default();
            let lang = lang.trim().to_string();
            if lang.is_empty() || lang.eq_ignore_ascii_case("en") {
                continue;
            }
            // Resolve the target script now (fallible): an unknown catalog language HARD-FAILS
            // rather than minting a materially-underspecified rendering.
            let _ = script_for_lang(&lang)?;
            langs.insert(lang.clone());
            for entry in &parse_po(text, false)? {
                if entry.msgctxt.is_empty() || !entry.msgctxt.contains('|') {
                    continue;
                }
                let Some((term_iri, _predicate)) = entry.msgctxt.split_once('|') else {
                    continue;
                };
                // A machine-seeded `#, fuzzy` entry is not a reviewed translation: it is
                // treated as not-yet-live (English fallback), identical to an untranslated
                // entry, so unreviewed content never surfaces as a live rendered form in the
                // gmeow.gts projection — matching `Translations::lookup` and the coverage axis.
                // The fuzzy-gating lives in the shared `live_translation_target` policy.
                let msgstr = live_translation_target(entry).to_string();
                pages
                    .entry((lang.clone(), term_iri.to_string()))
                    .or_default()
                    .push(EntryRef {
                        msgctxt: entry.msgctxt.clone(),
                        present: !msgstr.is_empty(),
                        msgstr,
                    });
            }
        }
    }

    let mut groups: Vec<PageGroup> = pages
        .into_iter()
        .map(|((lang, term_iri), mut entries)| {
            entries.sort_by(|a, b| a.msgctxt.cmp(&b.msgctxt));
            PageGroup {
                lang,
                term_iri,
                entries,
            }
        })
        .collect();
    groups.sort_by(|a, b| (&a.lang, &a.term_iri).cmp(&(&b.lang, &b.term_iri)));

    let langs: Vec<String> = langs.into_iter().collect();
    let ntriples = emit_ntriples(&groups, &langs)?;

    let mut loss = LossLedger::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    for group in &groups {
        ledger.push(rendering_ledger_row(group, &mut loss));
        ledger.push(translation_ledger_row(group, &mut loss));
    }
    for lang in &langs {
        ledger.push(exec_gap_ledger_row(lang, &mut loss));
    }

    Ok(LangDocsRenderingCorpus {
        ntriples,
        ledger,
        loss,
    })
}

/// The DERIVED weakest-dominates join over a page's unit judgments (any `Unsupported` gap
/// dominates any `ValidationOnly` present pair). `PreservationKind` derives `Ord` in
/// STRONGEST-FIRST declaration order, so the weakest kind is the `max`. A page always has at
/// least one entry; the vacuous case floors at `ValidationOnly`.
fn derived_kind(group: &PageGroup) -> PreservationKind {
    group
        .entries
        .iter()
        .map(|e| preservation_of(e.present))
        .max()
        .unwrap_or(PreservationKind::ValidationOnly)
}

/// The honest preservation kind for a docs unit: a present translation is `ValidationOnly`
/// (rendered but not machine-verified for sense/register), an untranslated entry is
/// `Unsupported` (the legalization floor).
fn preservation_of(present: bool) -> PreservationKind {
    if present {
        PreservationKind::ValidationOnly
    } else {
        PreservationKind::Unsupported
    }
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole corpus.
fn emit_ntriples(groups: &[PageGroup], langs: &[String]) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut lines: Vec<String> = Vec::new();

    for group in groups {
        let rendering = docs_rendering_iri(&group.lang, &group.term_iri);
        let translation = docs_translation_iri(&group.lang, &group.term_iri);
        let orthography = docs_orthography_iri(&group.lang);
        let sign_system = docs_sign_system_iri(&group.lang);
        let script = script_for_lang(&group.lang)?;
        let kind = derived_kind(group);

        // ── the non-English page as a lang:Rendering (renderingDocsPage) ──
        lines.push(triple(&rendering, RDF_TYPE, &iri(LANG_NS, "Rendering")));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingKind"),
            &iri(LANG_NS, "renderingDocsPage"),
        ));
        // The content the docs page renders IS the ontology term (distinct from the rendering
        // node and from the produced surfaces — never a self-identical crossing).
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderedContent"),
            &group.term_iri,
        ));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingConvention"),
            &orthography,
        ));
        // The produced form: the translated target surfaces the lang_translation corpus
        // already interned for this page's entries (reused by identity, not re-minted).
        for entry in &group.entries {
            lines.push(triple(
                &rendering,
                &iri(LANG_NS, "renderingForm"),
                &target_surface_iri(&entry.msgctxt, &entry.msgstr, &group.lang),
            ));
        }
        // The DERIVED per-page preservation facet onto the shared logic: judgment — the join
        // of the page's units, never a fabricated stronger grade.
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingPreservation"),
            &kind.iri(),
        ));

        // ── the target language's rendering convention (a minted lang:Orthography) ──
        lines.push(triple(&orthography, RDF_TYPE, &iri(LANG_NS, "Orthography")));
        lines.push(triple(
            &orthography,
            &iri(LANG_NS, "orthographyFor"),
            &sign_system,
        ));
        lines.push(triple(
            &orthography,
            &iri(LANG_NS, "usesScript"),
            &iri(LANG_NS, script),
        ));
        lines.push(triple(&sign_system, RDF_TYPE, &iri(LANG_NS, "SignSystem")));

        // ── the English pairing as a lang:Translation rolling up the page's units ──
        // NO preservation triple is minted on the document: its adequacy is the DERIVED join
        // surfaced in the ledger, so the lang:AssertedRollup gate never fires.
        lines.push(triple(&translation, RDF_TYPE, &iri(LANG_NS, "Translation")));
        lines.push(triple(
            &translation,
            &iri(LANG_NS, "translationMethod"),
            &iri(LANG_NS, "methodHuman"),
        ));
        for entry in &group.entries {
            lines.push(triple(
                &translation,
                &iri(LANG_NS, "rollsUpFrom"),
                &unit_iri(&entry.msgctxt, &group.lang),
            ));
        }
    }

    // ── the exec-docs English-only boundary: one declared gap per non-English language ──
    for lang in langs {
        emit_exec_gap(lang, script_for_lang(lang)?, &mut lines);
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

/// Emit the declared exec-docs English-only boundary as a marked `lang:translationGap`: a
/// reified `lang:TranslationUnit` whose English source is present, whose target is an empty
/// (untranslated) surface, carrying a `logic:Unsupported` correspondence — the honest floor,
/// carried and flagged rather than silently omitted. Mirrors the `lang_translation` gap path.
fn emit_exec_gap(lang: &str, script: &str, lines: &mut Vec<String>) {
    let unit = example("docs-execgap-unit", &digest16("execgap", lang));
    let corr = example(
        "docs-execgap-correspondence",
        &digest16("execgap-corr", lang),
    );
    let en_surface = example("surface-form", &digest16("surface", "execdocs-en"));
    let tgt_surface = example(
        "surface-form",
        &digest16("surface", &format!("{lang}\u{1f}execdocs\u{1f}")),
    );

    // The crossing (kept clean of surface-stratum predicates), marked as a gap.
    lines.push(triple(&unit, RDF_TYPE, &iri(LANG_NS, "TranslationUnit")));
    lines.push(triple(
        &unit,
        &iri(LANG_NS, "translationSource"),
        &en_surface,
    ));
    lines.push(triple(
        &unit,
        &iri(LANG_NS, "translationTarget"),
        &tgt_surface,
    ));
    lines.push(triple(
        &unit,
        &iri(LANG_NS, "translationMethod"),
        &iri(LANG_NS, "methodHuman"),
    ));
    lines.push(triple(
        &unit,
        &iri(LANG_NS, "translationCorrespondence"),
        &corr,
    ));
    lines.push(triple(&unit, &iri(LANG_NS, "translationGap"), &corr));

    // The carried logic:Correspondence law-spine — Unsupported (the floor), no witness.
    lines.push(triple(&corr, RDF_TYPE, &iri(LOGIC_NS, "Correspondence")));
    lines.push(triple(
        &corr,
        &iri(LOGIC_NS, "preservationKind"),
        &PreservationKind::Unsupported.iri(),
    ));
    lines.push(triple(
        &corr,
        &iri(LOGIC_NS, "correspondenceRelation"),
        &iri(LOGIC_NS, "RelatedMatch"),
    ));
    lines.push(triple(
        &corr,
        &iri(LOGIC_NS, "morphismClass"),
        &iri(LOGIC_NS, "BridgeView"),
    ));
    lines.push(triple(
        &corr,
        &iri(LOGIC_NS, "hasDeterminacy"),
        &iri(LOGIC_NS, "Vague"),
    ));
    lines.push(triple_typed(
        &corr,
        &iri(LOGIC_NS, "mnemomorphic"),
        "false",
        XSD_BOOLEAN,
    ));

    // The two surfaces (material identity per lang:SurfaceMaterialShape). The English source
    // carries the boundary prose; the target is an empty untranslated surface.
    for (surface, sys_lang, surface_script, locale, text) in [
        (
            &en_surface,
            "english",
            "latinScript",
            "en",
            EXEC_DOCS_BOUNDARY,
        ),
        (&tgt_surface, lang, script, lang, ""),
    ] {
        let sign_system = example("sign-system", sys_lang);
        lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "SurfaceForm")));
        lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "UnanalyzedProse")));
        lines.push(triple(surface, &iri(LANG_NS, "inSignSystem"), &sign_system));
        lines.push(triple(
            surface,
            &iri(LANG_NS, "inScript"),
            &iri(LANG_NS, surface_script),
        ));
        lines.push(triple_lit(
            surface,
            &iri(LANG_NS, "unicodeNormalization"),
            "NFC",
        ));
        lines.push(triple_lit(
            surface,
            &iri(LANG_NS, "collationLocale"),
            locale,
        ));
        lines.push(triple_lit(surface, &iri(LANG_NS, "surfaceText"), text));
        lines.push(triple(&sign_system, RDF_TYPE, &iri(LANG_NS, "SignSystem")));
    }
}

/// The DOCUMENTED gmeow: source term a per-page loss attributes to — the page's `term_iri`
/// when it is GMEOW-namespaced (so the translation/rendering loss lands on that term's
/// per-term projection-loss table), else `None` (a non-gmeow page stays whole-program).
fn gmeow_source_term(group: &PageGroup) -> Option<String> {
    group
        .term_iri
        .starts_with(crate::gmeow_ns::GMEOW_NS)
        .then(|| group.term_iri.clone())
}

/// One roll-up ledger row per page translation: its preservation is the DERIVED join of the
/// page's units, recorded so the ledger carries the computed roll-up rather than a fabricated
/// document-level flag.
fn translation_ledger_row(group: &PageGroup, loss: &mut LossLedger) -> ProjectionResult {
    let kind = derived_kind(group);
    let short = digest16("docs", &format!("{}\u{1f}{}", group.lang, group.term_iri));
    let target = format!("lang-docs-translation:{short}");
    let actual_drops = vec![(
        format!(
            "docs page translation roll-up ({lang}, {term}) over {n} unit(s); \
             weakest-dominates join = logic:{kind}",
            lang = group.lang,
            term = group.term_iri,
            n = group.entries.len(),
            kind = kind.as_str(),
        ),
        gmeow_source_term(group),
    )];
    loss.record_projection_drops_attributed(&target, kind, &[], &actual_drops);
    ProjectionResult {
        target,
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: "n/a".to_string(),
    }
}

/// One ledger row per page rendering: the DERIVED per-page preservation of realizing the
/// English term as a non-English docs page.
fn rendering_ledger_row(group: &PageGroup, loss: &mut LossLedger) -> ProjectionResult {
    let kind = derived_kind(group);
    let short = digest16("docs", &format!("{}\u{1f}{}", group.lang, group.term_iri));
    let target = format!("lang-docs-rendering:{short}");
    let actual_drops = vec![(
        format!(
            "docs page rendering ({lang}, {term}): {n} target surface(s); \
             derived preservation = logic:{kind}",
            lang = group.lang,
            term = group.term_iri,
            n = group.entries.len(),
            kind = kind.as_str(),
        ),
        gmeow_source_term(group),
    )];
    loss.record_projection_drops_attributed(&target, kind, &[], &actual_drops);
    ProjectionResult {
        target,
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: "n/a".to_string(),
    }
}

/// One ledger row per exec-docs English-only boundary: `Unsupported` with a non-empty residue
/// (the untranslated boundary prose), so the floor row is carried and flagged — untranslata-
/// bility-as-data satisfying the overclaim contract.
fn exec_gap_ledger_row(lang: &str, loss: &mut LossLedger) -> ProjectionResult {
    let target = format!("lang-docs-execgap:{lang}");
    let actual_drops = vec![
        format!(
            "exec-docs boundary ({lang}): executable documentation surfaces are rendered \
             only in the English tree; the {lang} tree carries no non-English rendering",
        ),
        format!("english carrier: {EXEC_DOCS_BOUNDARY}"),
    ];
    loss.record_projection_drops(&target, PreservationKind::Unsupported, &[], &actual_drops);
    ProjectionResult {
        target,
        content: String::new(),
        is_rdf: false,
        preservation: PreservationKind::Unsupported,
        complexity: "n/a".to_string(),
    }
}

// ── content-addressed identity helpers ─────────────────────────────────────────────

fn docs_rendering_iri(lang: &str, term_iri: &str) -> String {
    example(
        "docs-rendering",
        &digest16("docs-rendering", &format!("{lang}\u{1f}{term_iri}")),
    )
}

fn docs_translation_iri(lang: &str, term_iri: &str) -> String {
    example(
        "docs-translation",
        &digest16("docs-translation", &format!("{lang}\u{1f}{term_iri}")),
    )
}

fn docs_orthography_iri(lang: &str) -> String {
    example("docs-orthography", &digest16("docs-orthography", lang))
}

fn docs_sign_system_iri(lang: &str) -> String {
    example("sign-system", lang)
}

// ── N-Triples helpers (self-contained, mirroring the sibling lang: producers) ──────

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

fn triple_typed(subject: &str, predicate: &str, literal: &str, datatype: &str) -> String {
    format!(
        "<{subject}> <{predicate}> {}^^<{datatype}> .",
        nt_literal(literal)
    )
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
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn every_page_has_one_rendering_and_a_rollup_translation() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");

        // Discover the real (lang, page) universe the SAME way the corpus does, so this is a
        // genuine coverage check over the docs language trees, not a tautology.
        let catalog = SliceCatalog::discover(
            &repo_root().join("slices"),
            crate::gmeow_ns::gmeow_slice_vocab(),
        )
        .expect("discover catalog");
        let mut pages: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::TranslationCatalog {
                    continue;
                }
                let text = std::str::from_utf8(&artifact.content).unwrap();
                let lang = language_from_po(text).unwrap().unwrap_or_default();
                let lang = lang.trim().to_string();
                if lang.is_empty() || lang.eq_ignore_ascii_case("en") {
                    continue;
                }
                for entry in &parse_po(text, false).unwrap() {
                    if let Some((term, _)) = entry.msgctxt.split_once('|') {
                        pages.insert((lang.clone(), term.to_string()));
                    }
                }
            }
        }
        assert!(
            !pages.is_empty(),
            "the docs language trees must carry non-English pages"
        );

        for (lang, term) in &pages {
            let rendering = docs_rendering_iri(lang, term);
            let translation = docs_translation_iri(lang, term);
            // Exactly one renderingDocsPage rendering per non-English page.
            assert!(
                nt.contains(&triple(
                    &rendering,
                    &iri(LANG_NS, "renderingKind"),
                    &iri(LANG_NS, "renderingDocsPage")
                )),
                "page ({lang}, {term}) has no lang:Rendering renderingDocsPage"
            );
            // Its paired Translation rolls up at least one real translation unit.
            assert!(
                nt.contains(&triple(
                    &translation,
                    RDF_TYPE,
                    &iri(LANG_NS, "Translation")
                )),
                "page ({lang}, {term}) has no paired lang:Translation"
            );
            let rolls = nt
                .lines()
                .filter(|l| l.starts_with(&format!("<{translation}> <{}rollsUpFrom>", LANG_NS)))
                .count();
            assert!(rolls >= 1, "translation ({lang}, {term}) rolls up no units");
        }

        // Every renderingDocsPage edge is exactly the count of distinct pages — total, not
        // partial (one rendering per non-English page, never more).
        let docs_page_edges = nt
            .matches(&format!(
                " <{}renderingKind> <{}renderingDocsPage> .",
                LANG_NS, LANG_NS
            ))
            .count();
        assert_eq!(
            docs_page_edges,
            pages.len(),
            "exactly one renderingDocsPage rendering per distinct docs page"
        );
    }

    #[test]
    fn document_judgment_is_derived_never_asserted() {
        // The document-level lang:Translation carries NO preservation triple: its adequacy is
        // the DERIVED join of its units (surfaced in the ledger), so lang:AssertedRollup never
        // fires. Assert no lang:Translation subject carries logic:preservationKind or
        // lang:renderingPreservation, and that the ledger roll-up equals the units' join.
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");
        for line in nt.lines() {
            let is_translation_subject =
                line.starts_with(&format!("<{EXAMPLE_BASE}docs-translation/"));
            assert!(
                !(is_translation_subject
                    && (line.contains(&iri(LOGIC_NS, "preservationKind"))
                        || line.contains(&iri(LANG_NS, "renderingPreservation")))),
                "a document-level lang:Translation must never assert a preservation flag: {line}"
            );
        }
        // The derived roll-up ledger row equals the weakest-dominates join of the page's units
        // (present fr/zh catalogs are fully translated → ValidationOnly, never a stronger Exact).
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target.starts_with("lang-docs-translation:")
                    && r.preservation == PreservationKind::ValidationOnly),
            "a fully-translated page rolls up as the derived logic:ValidationOnly"
        );
        for row in &corpus.ledger {
            assert_ne!(
                row.preservation,
                PreservationKind::Exact,
                "no docs re-typing row may claim logic:ExactPreservation"
            );
        }
    }

    #[test]
    fn empty_msgstr_page_yields_a_translation_gap() {
        // A synthetic page with an untranslated entry rolls up as Unsupported (the floor) and
        // the exec-docs boundary path proves the gap marker is emitted as data. Build a group
        // with one present and one empty entry and check the derived kind is the weakest.
        let group = PageGroup {
            lang: "fr".to_string(),
            term_iri: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            entries: vec![
                EntryRef {
                    msgctxt: "https://blackcatinformatics.ca/gmeow/Foo|rdfs:label".to_string(),
                    msgstr: "Fou".to_string(),
                    present: true,
                },
                EntryRef {
                    msgctxt: "https://blackcatinformatics.ca/gmeow/Foo|skos:definition".to_string(),
                    msgstr: String::new(),
                    present: false,
                },
            ],
        };
        assert_eq!(
            derived_kind(&group),
            PreservationKind::Unsupported,
            "a page with an untranslated entry rolls up as the Unsupported floor"
        );
        // The exec-docs boundary is always emitted as a marked lang:translationGap over the
        // real repo catalogs — untranslatability-as-data, not a silent omission.
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");
        assert!(
            nt.contains(&iri(LANG_NS, "translationGap")),
            "the exec-docs boundary must be recorded as a lang:translationGap"
        );
    }

    #[test]
    fn exec_docs_boundary_is_a_declared_gap() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");
        // The English-only boundary prose is carried as the untranslated source (data, not a
        // silent omission), and its correspondence sits on the Unsupported floor.
        assert!(
            nt.contains(EXEC_DOCS_BOUNDARY),
            "the exec-docs boundary prose must be carried as recorded data"
        );
        assert!(
            nt.contains(&iri(LOGIC_NS, "Unsupported")),
            "the exec-docs boundary gap sits on the logic:Unsupported floor"
        );
        // A ledger row per non-English language, on the floor, with a non-empty residue.
        let gap_rows: Vec<_> = corpus
            .ledger
            .iter()
            .filter(|r| r.target.starts_with("lang-docs-execgap:"))
            .collect();
        assert!(
            !gap_rows.is_empty(),
            "an exec-docs gap ledger row per language"
        );
        for row in &gap_rows {
            assert_eq!(row.preservation, PreservationKind::Unsupported);
            assert!(
                !corpus.loss.projection_drops_for(&row.target).is_empty(),
                "an Unsupported floor row must carry a non-empty residue"
            );
        }
    }

    #[test]
    fn rollup_targets_are_real_translation_units() {
        // Every lang:rollsUpFrom target the docs re-typing emits is a real lang:TranslationUnit
        // in the lang_translation corpus — the roll-up references live units by identity, never
        // a re-derived scheme that could drift.
        let docs = build_corpus(&repo_root()).expect("docs corpus");
        let trans = crate::stages::lang_translation::build_corpus(&repo_root())
            .expect("translation corpus");
        let docs_nt = String::from_utf8(docs.ntriples).expect("utf8");
        let trans_nt = String::from_utf8(trans.ntriples).expect("utf8");

        let rolls_marker = format!("<{}rollsUpFrom> <", LANG_NS);
        let mut checked = 0usize;
        for line in docs_nt.lines() {
            let Some(idx) = line.find(&rolls_marker) else {
                continue;
            };
            let rest = &line[idx + rolls_marker.len()..];
            let unit = &rest[..rest.find('>').expect("closing bracket")];
            assert!(
                trans_nt.contains(&triple(unit, RDF_TYPE, &iri(LANG_NS, "TranslationUnit"))),
                "rollsUpFrom target {unit} is not a real lang:TranslationUnit"
            );
            checked += 1;
        }
        assert!(
            checked >= 1,
            "at least one rollsUpFrom edge must be checked"
        );
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let a = build_corpus(&repo_root()).expect("build a").ntriples;
        let b = build_corpus(&repo_root()).expect("build b").ntriples;
        assert_eq!(a, b, "corpus N-Triples must be deterministic");
    }
}
