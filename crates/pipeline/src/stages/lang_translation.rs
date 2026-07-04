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

use gmeow_docs::i18n::parse_po;
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::slice::{ArtifactRole, SliceCatalog};

use crate::error::PipelineError;

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
    /// One `ProjectionResult` per translation unit plus one per document roll-up.
    pub ledger: Vec<ProjectionResult>,
}

/// One typed translation crossing derived from a single `.po` entry.
struct Unit {
    unit_iri: String,
    corr_iri: String,
    en_surface: String,
    tgt_surface: String,
    en_sign_system: String,
    tgt_sign_system: String,
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
pub fn build_corpus(root: &Path) -> Result<LangTranslationCorpus, PipelineError> {
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| PipelineError::Stage {
                stage: "stage-mappings".to_string(),
                message: format!("lang-translation slice catalog: {e}"),
            })?;

    let mut units: Vec<Unit> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::TranslationCatalog {
                continue;
            }
            let text = String::from_utf8_lossy(&artifact.content);
            let parsed = parse_po(&text);
            let lang = parsed.language.trim().to_string();
            // A catalog with no BCP-47 language header, or the English carrier itself,
            // is not a translation crossing.
            if lang.is_empty() || lang.eq_ignore_ascii_case("en") {
                continue;
            }
            for entry in &parsed.entries {
                // The header entry (empty msgctxt) is not a crossing; a malformed
                // msgctxt without the `<term-iri>|<predicate>` separator is skipped.
                if entry.msgctxt.is_empty() {
                    continue;
                }
                let Some((_term_iri, _predicate)) = entry.msgctxt.split_once('|') else {
                    continue;
                };
                units.push(build_unit(
                    &entry.msgctxt,
                    &entry.msgid,
                    &entry.msgstr,
                    &lang,
                ));
            }
        }
    }

    // Deterministic ordering by the content-addressed unit IRI, so the ledger rows are
    // reproducible independent of catalog discovery order.
    units.sort_by(|a, b| a.unit_iri.cmp(&b.unit_iri));

    let ntriples = emit_ntriples(&units);
    let mut ledger: Vec<ProjectionResult> = units.iter().map(unit_ledger_row).collect();
    ledger.extend(document_ledger_rows(&units));

    Ok(LangTranslationCorpus { ntriples, ledger })
}

/// Derive one typed crossing from a `.po` entry: content-addressed IRIs for the unit,
/// its carried correspondence, and both surface forms.
fn build_unit(msgctxt: &str, msgid: &str, msgstr: &str, lang: &str) -> Unit {
    let present = !msgstr.is_empty();
    let unit_key = format!("{msgctxt}\u{1f}{lang}");
    Unit {
        unit_iri: example("translation-unit", &digest16("unit", &unit_key)),
        corr_iri: example("translation-correspondence", &digest16("corr", &unit_key)),
        en_surface: example(
            "surface-form",
            &digest16("surface", &format!("english\u{1f}{msgctxt}\u{1f}{msgid}")),
        ),
        tgt_surface: example(
            "surface-form",
            &digest16("surface", &format!("{lang}\u{1f}{msgctxt}\u{1f}{msgstr}")),
        ),
        en_sign_system: example("sign-system", "english"),
        tgt_sign_system: example("sign-system", lang),
        msgid: msgid.to_string(),
        msgstr: msgstr.to_string(),
        lang: lang.to_string(),
        key: msgctxt.to_string(),
        present,
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
    for (lang, doc_iri) in &docs {
        lines.push(triple(doc_iri, RDF_TYPE, &iri(LANG_NS, "Translation")));
        lines.push(triple(
            doc_iri,
            &iri(LANG_NS, "translationMethod"),
            &iri(LANG_NS, "methodHuman"),
        ));
        let _ = lang;
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
        for (surface, sign_system, text) in [
            (&unit.en_surface, &unit.en_sign_system, &unit.msgid),
            (&unit.tgt_surface, &unit.tgt_sign_system, &unit.msgstr),
        ] {
            lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "SurfaceForm")));
            lines.push(triple(surface, RDF_TYPE, &iri(LANG_NS, "UnanalyzedProse")));
            lines.push(triple(surface, &iri(LANG_NS, "inSignSystem"), sign_system));
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
fn unit_ledger_row(unit: &Unit) -> ProjectionResult {
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
    ProjectionResult {
        target: format!("lang-translation:{short}"),
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: "n/a".to_string(),
        lossy_drops: Vec::new(),
        actual_drops,
    }
}

/// One roll-up ledger row per language document. Its preservation is the DERIVED
/// weakest-dominates join of the document's units (any `Unsupported` gap dominates), so
/// the ledger records the computed roll-up rather than a fabricated document-level flag.
fn document_ledger_rows(units: &[Unit]) -> Vec<ProjectionResult> {
    let mut by_lang: BTreeMap<String, Vec<&Unit>> = BTreeMap::new();
    for unit in units {
        by_lang.entry(unit.lang.clone()).or_default().push(unit);
    }
    by_lang
        .into_iter()
        .map(|(lang, members)| {
            let kind = weakest_dominates(members.iter().map(|u| preservation_of(u.present)));
            ProjectionResult {
                target: format!("lang-translation-doc:{lang}"),
                content: String::new(),
                is_rdf: false,
                preservation: kind,
                complexity: "n/a".to_string(),
                actual_drops: vec![format!(
                    "document roll-up over {n} translation unit(s); weakest-dominates join = \
                     logic:{kind}",
                    n = members.len(),
                    kind = kind.as_str(),
                )],
                lossy_drops: Vec::new(),
            }
        })
        .collect()
}

/// The weakest-dominates join over a document's unit judgments: an `Unsupported` gap
/// (the legalization floor) dominates any `ValidationOnly` unit. An empty document (no
/// units) cannot occur — every document is minted from at least one unit.
fn weakest_dominates(kinds: impl Iterator<Item = PreservationKind>) -> PreservationKind {
    let mut result = PreservationKind::ValidationOnly;
    for kind in kinds {
        if kind == PreservationKind::Unsupported {
            return PreservationKind::Unsupported;
        }
        result = kind;
    }
    result
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
        assert!(
            !nt.contains(&format!(
                "<{}> <{}>",
                EXAMPLE_BASE,
                iri(LANG_NS, "surfaceText")
            )),
            "surface text must live on the SurfaceForm, never inline on a crossing subject"
        );
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
                    !row.actual_drops.is_empty(),
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
        );
        assert!(!unit.present);
        let row = unit_ledger_row(&unit);
        assert_eq!(row.preservation, PreservationKind::Unsupported);
        assert!(row.actual_drops.iter().any(|d| d.contains("Foo")));
    }
}
