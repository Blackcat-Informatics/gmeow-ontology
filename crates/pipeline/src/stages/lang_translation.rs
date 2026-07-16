// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The live `lang:TranslationUnit` corpus producer (Principle 15 consumer wiring).
//!
//! The multilingual documentation `.po` catalogs (`slices/**/i18n/<lang>.po`) pair an
//! English carrier string (`msgid`) with a per-language rendering (`msgstr`). This
//! module TYPES those pairs as first-class `lang:TranslationUnit` crossings, each
//! carrying exactly one `logic:Correspondence` node with an HONESTLY-computed
//! preservation judgment — never a fabricated `logic:ExactPreservation`:
//!
//!   * **Present `msgstr`** — a translated but machine-unanalyzed surface pair. The
//!     carried correspondence is `logic:ValidationOnly` (present, but sense/register
//!     preservation is NOT machine-verified), on the honest weakest sensible rungs
//!     (`logic:RelatedMatch` relation, `logic:AffineCorrespondence` morphism class,
//!     `logic:Vague` determinacy, `logic:mnemomorphic false`). Residue: an explicit
//!     "unanalyzed surface pair" note.
//!   * **Empty `msgstr`** — an untranslated gap, untranslatability-as-data. The carried
//!     correspondence is `logic:Unsupported` (the legalization floor) on
//!     `logic:BridgeView`, and the unit is marked `lang:translationGap`. The residue is
//!     the untranslated English carrier text plus a no-witness note, so the Unsupported
//!     ledger row is NON-EMPTY (it satisfies the overclaim gate: the construct is carried
//!     and flagged, never silently dropped).
//!
//! Every unit is emitted BOTH as RDF (into the carrier's
//! `graph/lang-translation-corpus` named graph) AND as a `ProjectionResult` row folded
//! into the loss ledger (`generated/logic/projection-report.ttl`). A per-language
//! `lang:Translation` document rolls up its units; its preservation is the DERIVED
//! weakest-dominates join of its units' judgments (surfaced in the ledger, never minted
//! as a document-level RDF flag — that would trip the fabricated-roll-up gate).
//!
//! Surface text lives on the `lang:SurfaceForm` nodes, never on the crossing itself
//! (the unit stays clean of surface-stratum predicates), so the corpus never trips the
//! `lang:SurfaceLeakInContentKey` native gate. All identities are content-addressed and
//! the N-Triples are sorted + deduped, so the corpus is byte-reproducible (no clock, no
//! randomness).

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use gmeow_docs::i18n_compile::{language_from_po, parse_po};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::slice::{ArtifactRole, SliceCatalog};

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// The example-instance base every minted corpus IRI lives under — the same base the
/// `lang:` competency queries scope with `STRSTARTS(STR(?unit), "http://example.org/lang/")`.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// The assembled corpus: the sorted N-Triples graph plus the per-unit + per-document
/// loss-ledger rows.
pub struct LangTranslationCorpus {
    /// The deterministic, sorted, byte-stable N-Triples graph
    /// (`graph/lang-translation-corpus`).
    pub ntriples: Vec<u8>,
    /// One `ProjectionResult` per translation unit plus one per document roll-up. The rows
    /// carry only identity/judgment; their drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store every row's drops are interned into (keyed by target focus). The mappings
    /// stage unions it into the single report loss store.
    pub loss: LossLedger,
}

/// One typed translation crossing derived from a single `.po` entry.
struct Unit {
    unit_iri: String,
    corr_iri: String,
    en_surface: String,
    tgt_surface: String,
    en_sign_system: String,
    tgt_sign_system: String,
    /// The `lang:Script` individual the English source surface is written in (always Latin).
    en_script: String,
    /// The `lang:Script` individual the target surface is written in (from the catalog lang).
    tgt_script: String,
    msgid: String,
    msgstr: String,
    lang: String,
    /// The `<term-iri>|<predicate-curie>` provenance key (for residue attribution).
    key: String,
    /// `true` when the target is present (translated); `false` for an untranslated gap.
    present: bool,
}

/// Build the live translation corpus by typing every `.po` catalog pair under `root`.
///
/// Iterates the slice catalog's `ArtifactRole::TranslationCatalog` artifacts and, via
/// [`parse_po`] (which keeps EVERY entry, including untranslated gaps), types each
/// `msgctxt = "<term-iri>|<predicate-curie>"` entry as a `lang:TranslationUnit`.
pub fn build_corpus(root: &Path) -> Result<LangTranslationCorpus, gmeow_errors::Diag> {
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!("lang-translation slice catalog: {e}"),
                })
            })?;

    let mut units: Vec<Unit> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::TranslationCatalog {
                continue;
            }
            // A translation catalog is required input: invalid UTF-8 is a HARD FAIL, never
            // a silent lossy repair that would corrupt the surface text it carries.
            let text = std::str::from_utf8(&artifact.content).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!(
                        "lang-translation: translation catalog '{}' is not valid UTF-8: {e}",
                        artifact.logical_path
                    ),
                })
            })?;
            let lang = language_from_po(text)?.unwrap_or_default();
            let lang = lang.trim().to_string();
            // A catalog with no BCP-47 language header, or the English carrier itself,
            // is not a translation crossing.
            if lang.is_empty() || lang.eq_ignore_ascii_case("en") {
                continue;
            }
            for entry in &parse_po(text, false)? {
                // The header entry (empty msgctxt) is not a crossing; a malformed
                // msgctxt without the `<term-iri>|<predicate>` separator is skipped.
                if entry.msgctxt.is_empty() {
                    continue;
                }
                if !entry.msgctxt.contains('|') {
                    continue;
                }
                // Resolve the target surface's script now (fallible): an unknown catalog
                // language HARD-FAILS rather than minting a materially-underspecified surface.
                let tgt_script = script_for_lang(&lang)?;
                units.push(build_unit(
                    &entry.msgctxt,
                    &entry.msgid,
                    &entry.msgstr,
                    &lang,
                    tgt_script,
                ));
            }
        }
    }

    // Deterministic ordering by the content-addressed unit IRI, so the ledger rows are
    // reproducible independent of catalog discovery order.
    units.sort_by(|a, b| a.unit_iri.cmp(&b.unit_iri));

    let ntriples = emit_ntriples(&units);
    let mut loss = LossLedger::new();
    let mut ledger: Vec<ProjectionResult> = units
        .iter()
        .map(|u| unit_ledger_row(u, &mut loss))
        .collect();
    ledger.extend(document_ledger_rows(&units, &mut loss));

    Ok(LangTranslationCorpus {
        ntriples,
        ledger,
        loss,
    })
}

/// Derive one typed crossing from a `.po` entry: content-addressed IRIs for the unit,
/// its carried correspondence, and both surface forms. `tgt_script` is the pre-resolved
/// `lang:Script` local name for the target surface; the English source is always Latin.
fn build_unit(msgctxt: &str, msgid: &str, msgstr: &str, lang: &str, tgt_script: &str) -> Unit {
    let present = !msgstr.is_empty();
    let unit_key = format!("{msgctxt}\u{1f}{lang}");
    Unit {
        unit_iri: unit_iri(msgctxt, lang),
        corr_iri: example("translation-correspondence", &digest16("corr", &unit_key)),
        en_surface: example(
            "surface-form",
            &digest16("surface", &format!("english\u{1f}{msgctxt}\u{1f}{msgid}")),
        ),
        tgt_surface: target_surface_iri(msgctxt, msgstr, lang),
        en_sign_system: example("sign-system", "english"),
        tgt_sign_system: example("sign-system", lang),
        en_script: iri(LANG_NS, "latinScript"),
        tgt_script: iri(LANG_NS, tgt_script),
        msgid: msgid.to_string(),
        msgstr: msgstr.to_string(),
        lang: lang.to_string(),
        key: msgctxt.to_string(),
        present,
    }
}

/// The content-addressed `lang:TranslationUnit` IRI for a `.po` entry keyed by its `msgctxt`
/// and catalog language — the SAME identity [`build_unit`] mints. Exposed so the docs-page
/// roll-up (`lang_docs_rendering`) can point `lang:rollsUpFrom` at the real units without
/// re-deriving the content-addressing scheme (single source of truth).
pub(crate) fn unit_iri(msgctxt: &str, lang: &str) -> String {
    example(
        "translation-unit",
        &digest16("unit", &format!("{msgctxt}\u{1f}{lang}")),
    )
}

/// The content-addressed target `lang:SurfaceForm` IRI for a `.po` entry — the produced form
/// a docs-page rendering realizes. The SAME identity [`build_unit`] mints for `tgt_surface`,
/// exposed so `lang_docs_rendering` can point `lang:renderingForm` at the real surfaces.
pub(crate) fn target_surface_iri(msgctxt: &str, msgstr: &str, lang: &str) -> String {
    example(
        "surface-form",
        &digest16("surface", &format!("{lang}\u{1f}{msgctxt}\u{1f}{msgstr}")),
    )
}

/// Resolve the `lang:Script` individual (local name) for a surface written in a BCP-47
/// language. Script is material identity a surface hash needs, so an unknown language is a
/// HARD FAIL (no silent default): a newly-added catalog forces an explicit script mapping
/// and its `lang:Script` individual in `slices/grounding/lang/module.ttl`.
pub(crate) fn script_for_lang(lang: &str) -> Result<&'static str, gmeow_errors::Diag> {
    let primary = lang
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match primary.as_str() {
        "en" | "fr" => Ok("latinScript"),
        "zh" => Ok("hanScript"),
        _ => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-translation: no lang:Script mapping for BCP-47 language '{lang}'; add \
                 its lang:Script individual to slices/grounding/lang/module.ttl and extend \
                 script_for_lang"
            ),
        })),
    }
}

/// The honest preservation kind for a crossing: a present pair is `ValidationOnly` (the
/// surface pair exists but its sense/register preservation is NOT machine-verified — an
/// honest label, never Exact); an untranslated gap is `Unsupported` (the legalization
/// floor, carried and flagged).
fn preservation_of(present: bool) -> PreservationKind {
    if present {
        PreservationKind::ValidationOnly
    } else {
        PreservationKind::Unsupported
    }
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole corpus.
fn emit_ntriples(units: &[Unit]) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();

    // Per-language document roll-ups (`lang:Translation`), rolling up their units. The
    // document carries NO preservation triple — its adequacy is the derived join of its
    // units' judgments (surfaced in the ledger), never a minted document-level flag.
    let mut docs: BTreeMap<String, String> = BTreeMap::new();
    for unit in units {
        docs.entry(unit.lang.clone())
            .or_insert_with(|| example("translation", &unit.lang));
    }
    for doc_iri in docs.values() {
        lines.push(triple(doc_iri, RDF_TYPE, &iri(LANG_NS, "Translation")));
        lines.push(triple(
            doc_iri,
            &iri(LANG_NS, "translationMethod"),
            &iri(LANG_NS, "methodHuman"),
        ));
    }

    for unit in units {
        let kind = preservation_of(unit.present);

        // ── the crossing (kept clean of surface-stratum predicates) ──
        lines.push(triple(
            &unit.unit_iri,
            RDF_TYPE,
            &iri(LANG_NS, "TranslationUnit"),
        ));
        lines.push(triple(
            &unit.unit_iri,
            &iri(LANG_NS, "translationSource"),
            &unit.en_surface,
        ));
        lines.push(triple(
            &unit.unit_iri,
            &iri(LANG_NS, "translationTarget"),
            &unit.tgt_surface,
        ));
        lines.push(triple(
            &unit.unit_iri,
            &iri(LANG_NS, "translationMethod"),
            &iri(LANG_NS, "methodHuman"),
        ));
        lines.push(triple(
            &unit.unit_iri,
            &iri(LANG_NS, "translationCorrespondence"),
            &unit.corr_iri,
        ));
        // The document roll-up edge (derived adequacy, never asserted independently).
        let doc_iri = example("translation", &unit.lang);
        lines.push(triple(
            &doc_iri,
            &iri(LANG_NS, "rollsUpFrom"),
            &unit.unit_iri,
        ));
        // A gap marks the unit and points at the correspondence carrying the residue on
        // its mnemomorphic witness (the mark says only that a gap exists).
        if !unit.present {
            lines.push(triple(
                &unit.unit_iri,
                &iri(LANG_NS, "translationGap"),
                &unit.corr_iri,
            ));
        }

        // ── the carried logic:Correspondence law-spine ──
        lines.push(triple(
            &unit.corr_iri,
            RDF_TYPE,
            &iri(LOGIC_NS, "Correspondence"),
        ));
        lines.push(triple(
            &unit.corr_iri,
            &iri(LOGIC_NS, "preservationKind"),
            &kind.iri(),
        ));
        lines.push(triple(
            &unit.corr_iri,
            &iri(LOGIC_NS, "correspondenceRelation"),
            &iri(LOGIC_NS, "RelatedMatch"),
        ));
        lines.push(triple(
            &unit.corr_iri,
            &iri(LOGIC_NS, "morphismClass"),
            &iri(
                LOGIC_NS,
                if unit.present {
                    "AffineCorrespondence"
                } else {
                    "BridgeView"
                },
            ),
        ));
        lines.push(triple(
            &unit.corr_iri,
            &iri(LOGIC_NS, "hasDeterminacy"),
            &iri(LOGIC_NS, "Vague"),
        ));
        // No verified source witness for an unanalyzed surface pair — honest false.
        lines.push(triple_typed(
            &unit.corr_iri,
            &iri(LOGIC_NS, "mnemomorphic"),
            "false",
            XSD_BOOLEAN,
        ));

        // ── the two surface forms (surface text lives HERE, never on the unit) ──
        // Each carries the material identity a stable surface hash needs — script,
        // Unicode normalization, collation locale — so the corpus satisfies
        // lang:SurfaceMaterialShape and never trips lang:UnhashableSurface.
        for (surface, sign_system, script, locale, text) in [
            (
                &unit.en_surface,
                &unit.en_sign_system,
                &unit.en_script,
                "en",
                &unit.msgid,
            ),
            (
                &unit.tgt_surface,
                &unit.tgt_sign_system,
                &unit.tgt_script,
                unit.lang.as_str(),
                &unit.msgstr,
            ),
        ] {
            lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "SurfaceForm")));
            lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "UnanalyzedProse")));
            lines.push(triple(surface, &iri(LANG_NS, "inSignSystem"), sign_system));
            lines.push(triple(surface, &iri(LANG_NS, "inScript"), script));
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
        }

        // ── the minted sign systems (typed, matching the fixture convention) ──
        lines.push(triple(
            &unit.en_sign_system,
            RDF_TYPE,
            &iri(LANG_NS, "SignSystem"),
        ));
        lines.push(triple(
            &unit.tgt_sign_system,
            RDF_TYPE,
            &iri(LANG_NS, "SignSystem"),
        ));
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

/// The per-unit loss-ledger row. Its preservation is the SAME kind the carried
/// correspondence declares; the residue is non-empty so an `Unsupported` gap row is
/// carried and flagged (the overclaim gate rejects a silent floor).
fn unit_ledger_row(unit: &Unit, loss: &mut LossLedger) -> ProjectionResult {
    let kind = preservation_of(unit.present);
    let short = digest16("unit", &format!("{}\u{1f}{}", unit.key, unit.lang));
    let actual_drops = if unit.present {
        vec![format!(
            "unanalyzed surface pair: sense/register preservation not machine-verified \
             ({lang}, {key})",
            lang = unit.lang,
            key = unit.key,
        )]
    } else {
        vec![
            format!(
                "untranslatable residue ({lang}, {key}): no target witness form; the English \
                 carrier crosses unrealized",
                lang = unit.lang,
                key = unit.key,
            ),
            format!("english carrier: {}", unit.msgid),
        ]
    };
    let target = format!("lang-translation:{short}");
    // Attribute the residue to the DOCUMENTED gmeow: term whose English prose this unit
    // translates — the term IRI of the `<term-iri>|<predicate>` provenance key — so the
    // translation loss lands on that term's per-term projection-loss table. A non-gmeow key
    // (or a malformed one) leaves the drops whole-program; never fabricated onto a term.
    let source_term = unit
        .key
        .split_once('|')
        .map(|(term_iri, _)| term_iri)
        .filter(|t| t.starts_with(crate::gmeow_ns::GMEOW_NS))
        .map(str::to_owned);
    let attributed: Vec<(String, Option<String>)> = actual_drops
        .into_iter()
        .map(|note| (note, source_term.clone()))
        .collect();
    loss.record_projection_drops_attributed(&target, kind, &[], &attributed);
    ProjectionResult {
        target,
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: "n/a".to_string(),
    }
}

/// One roll-up ledger row per language document. Its preservation is the DERIVED
/// weakest-dominates join of the document's units (any `Unsupported` gap dominates), so
/// the ledger records the computed roll-up rather than a fabricated document-level flag.
fn document_ledger_rows(units: &[Unit], loss: &mut LossLedger) -> Vec<ProjectionResult> {
    let mut by_lang: BTreeMap<String, Vec<&Unit>> = BTreeMap::new();
    for unit in units {
        by_lang.entry(unit.lang.clone()).or_default().push(unit);
    }
    by_lang
        .into_iter()
        .map(|(lang, members)| {
            let kind = weakest_dominates(members.iter().map(|u| preservation_of(u.present)));
            let target = format!("lang-translation-doc:{lang}");
            let actual_drops = vec![format!(
                "document roll-up over {n} translation unit(s); weakest-dominates join = \
                 logic:{kind}",
                n = members.len(),
                kind = kind.as_str(),
            )];
            loss.record_projection_drops(&target, kind, &[], &actual_drops);
            ProjectionResult {
                target,
                content: String::new(),
                is_rdf: false,
                preservation: kind,
                complexity: "n/a".to_string(),
            }
        })
        .collect()
}

/// The weakest-dominates join over a document's unit judgments: the weakest member kind
/// dominates the roll-up (an `Unsupported` gap — the legalization floor — dominates any
/// `ValidationOnly` unit). `PreservationKind` derives `Ord` in STRONGEST-FIRST declaration
/// order (`Exact` least … `Unsupported` greatest), so the weakest kind is the `max`, NOT
/// the `min` — the join is order-independent and total over the whole lattice. An empty
/// document (no units) cannot occur — every document is minted from at least one unit — but
/// `ValidationOnly` is the honest floor for that vacuous case.
fn weakest_dominates(kinds: impl Iterator<Item = PreservationKind>) -> PreservationKind {
    kinds.max().unwrap_or(PreservationKind::ValidationOnly)
}

// ── N-Triples helpers ───────────────────────────────────────────────────────────

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

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn corpus_types_present_pairs_as_validation_only() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");

        // The live fr/zh lifecycle catalogs are fully translated, so every unit is a
        // present pair carried as logic:ValidationOnly — never a fabricated Exact.
        assert!(
            nt.contains(&iri(LANG_NS, "TranslationUnit")),
            "corpus must type lang:TranslationUnit instances"
        );
        assert!(
            nt.contains(&iri(LOGIC_NS, "ValidationOnly")),
            "present pairs carry logic:ValidationOnly"
        );
        assert!(
            !nt.contains(&iri(LOGIC_NS, "ExactPreservation")),
            "an unanalyzed surface pair must never claim logic:ExactPreservation"
        );
        // The crossing is clean of surface-stratum predicates (no SurfaceLeakInContentKey).
        for unit in ["translationSource", "translationTarget"] {
            assert!(nt.contains(&iri(LANG_NS, unit)), "missing lang:{unit}");
        }
        // Surface text lives on the SurfaceForm, NEVER inline on a crossing subject
        // (the unit or its correspondence). A blanket EXAMPLE_BASE prefix check is vacuous
        // — crossing subjects are minted as `<EXAMPLE_BASE>translation-unit/<digest>`, so
        // the base is never immediately followed by `surfaceText` — so assert directly that
        // no crossing-subject line carries the predicate.
        let surface_text = iri(LANG_NS, "surfaceText");
        for line in nt.lines() {
            let is_crossing_subject = line
                .starts_with(&format!("<{EXAMPLE_BASE}translation-unit/"))
                || line.starts_with(&format!("<{EXAMPLE_BASE}translation-correspondence/"));
            assert!(
                !(is_crossing_subject && line.contains(&surface_text)),
                "surface text must live on the SurfaceForm, never inline on a crossing subject: {line}"
            );
        }
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let a = build_corpus(&repo_root()).expect("build a").ntriples;
        let b = build_corpus(&repo_root()).expect("build b").ntriples;
        assert_eq!(a, b, "corpus N-Triples must be deterministic");
    }

    #[test]
    fn ledger_rows_are_present_and_ledger_targets_are_novel() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target.starts_with("lang-translation:")),
            "per-unit ledger rows must be present"
        );
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target.starts_with("lang-translation-doc:")),
            "per-document roll-up ledger rows must be present"
        );
        // Every row satisfies the overclaim contract: a floor (Unsupported) row carries a
        // non-empty residue; no row is Exact.
        for row in &corpus.ledger {
            assert_ne!(row.preservation, PreservationKind::Exact);
            if row.preservation == PreservationKind::Unsupported {
                assert!(
                    !corpus.loss.projection_drops_for(&row.target).is_empty(),
                    "an Unsupported row must carry a non-empty residue"
                );
            }
        }
    }

    #[test]
    fn gap_path_is_unsupported_with_nonempty_residue() {
        // A synthetic gap (empty msgstr) is typed Unsupported on the floor rung with the
        // English carrier carried as residue — untranslatability-as-data.
        let unit = build_unit(
            "https://blackcatinformatics.ca/gmeow/Foo|rdfs:label",
            "Foo",
            "",
            "fr",
            "latinScript",
        );
        assert!(!unit.present);
        let mut loss = LossLedger::new();
        let row = unit_ledger_row(&unit, &mut loss);
        assert_eq!(row.preservation, PreservationKind::Unsupported);
        assert!(
            loss.projection_drops_for(&row.target)
                .iter()
                .any(|d| d.contains("Foo"))
        );
    }

    #[test]
    fn weakest_dominates_is_the_order_independent_weakest_join() {
        use PreservationKind::{Exact, Unsupported, ValidationOnly};
        // Weakest = max under strongest-first Ord, regardless of iteration order.
        assert_eq!(
            weakest_dominates([ValidationOnly, Exact].into_iter()),
            ValidationOnly,
            "a weaker ValidationOnly must dominate a stronger Exact"
        );
        assert_eq!(
            weakest_dominates([Exact, ValidationOnly].into_iter()),
            ValidationOnly,
            "the join must be order-independent"
        );
        // Any Unsupported gap (the floor) dominates the whole document.
        assert_eq!(
            weakest_dominates([ValidationOnly, Unsupported, Exact].into_iter()),
            Unsupported
        );
        // A document of all-Exact units rolls up Exact; the vacuous case floors at ValidationOnly.
        assert_eq!(weakest_dominates([Exact, Exact].into_iter()), Exact);
        assert_eq!(weakest_dominates(std::iter::empty()), ValidationOnly);
    }

    #[test]
    fn corpus_surface_forms_carry_material_identity() {
        // Every lang:SurfaceForm the live corpus mints declares the material identity
        // lang:SurfaceMaterialShape requires — script, Unicode normalization, collation
        // locale — so the corpus never trips lang:UnhashableSurface.
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");
        for pred in ["inScript", "unicodeNormalization", "collationLocale"] {
            assert!(
                nt.contains(&iri(LANG_NS, pred)),
                "surface forms must declare lang:{pred}"
            );
        }
        // The zh catalog forces a non-Latin script individual to be referenced.
        assert!(
            nt.contains(&iri(LANG_NS, "hanScript")),
            "the zh catalog surface must be written in lang:hanScript"
        );
        // Every SurfaceForm line-block is complete: as many inScript triples as SurfaceForm
        // types, so no surface is emitted materially underspecified.
        let surface_forms = nt
            .matches(&format!("<{}> .", iri(LANG_NS, "SurfaceForm")))
            .count();
        let scripts = nt
            .matches(&format!(" <{}> ", iri(LANG_NS, "inScript")))
            .count();
        assert_eq!(
            surface_forms, scripts,
            "every lang:SurfaceForm must carry exactly one lang:inScript"
        );
    }

    #[test]
    fn script_for_lang_maps_known_and_hard_fails_unknown() {
        assert_eq!(script_for_lang("en").unwrap(), "latinScript");
        assert_eq!(script_for_lang("fr").unwrap(), "latinScript");
        assert_eq!(script_for_lang("zh").unwrap(), "hanScript");
        assert_eq!(script_for_lang("zh-Hans").unwrap(), "hanScript");
        // An unmapped language is a HARD FAIL, never a silent default surface.
        let err = script_for_lang("qtz").expect_err("unknown language must hard-fail");
        assert!(format!("{err}").contains("no lang:Script mapping"));
    }
}
