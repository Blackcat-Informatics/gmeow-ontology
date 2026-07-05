// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The total prose-lift corpus producer (Gate 1: every `@x-gmeow-english` literal is a
//! reachable `lang:SurfaceForm`).
//!
//! # Extraction universe
//!
//! Every DISTINCT `@x-gmeow-english`-tagged literal across the SOURCE slices: this parses
//! every `text/turtle` artifact the [`SliceCatalog`] discovers under `slices/` (module,
//! shapes, manifest, example, counter-example, and Turtle mapping artifacts) and collects
//! each language-tagged literal whose tag is `x-gmeow-english` into a `BTreeSet<String>`
//! (deterministic, deduplicated by material identity). The `.po` translation catalogs are
//! not Turtle and carry no `@x-gmeow-english` RDF literal, so the English canon is exactly
//! the Turtle literal set — never re-derived from the translations.
//!
//! # What each literal becomes
//!
//! Each distinct literal is interned (in sorted order) as one `lang:SurfaceForm`, typed
//! `lang:UnanalyzedProse` at `lang:rawLevel`, addressed by its material
//! [`SurfaceForm::surface_key`] via [`digest16`]. A surface whose text byte-length exceeds
//! [`DOCUMENT_SCALE_BYTES`] carries its bytes BY REFERENCE — a content-addressed
//! `lang:surfaceBlob "blake3:<hex>"` handle whose bytes ride the bundle blob channel
//! ([`build_surface_blobs`]) — rather than inline `lang:surfaceText`, so document-scale
//! payloads never inflate the graph; smaller surfaces stay inline. The surface carries a
//! `logic:candidateSourceHash` computed by [`candidate_source_hash`] over the RAW literal
//! text — byte-identical to what the obligations gate recomputes — so the prose-hash
//! discipline resolves THROUGH the lifted surface. The `lang:unicodeNormalization` frame is
//! declared HONESTLY (the form the bytes are actually in), and the hashed input is never
//! normalized, so the declared frame never lies and the hash never drifts.
//!
//! The surface CARRIES a `logic:Correspondence` (the [`PlainTextBridge`] law spine): the
//! surface round-trip re-emits the bytes verbatim, an exact isomorphism whose lens laws are
//! discharged. Nothing is dropped — the unanalyzed status is a recorded graded level, not a
//! projection loss — so the corpus folds exactly one honest `logic:ExactPreservation` ledger
//! row. All identities are content-addressed and the N-Triples are sorted + deduped, so the
//! corpus is byte-reproducible (no clock, no randomness).

use std::collections::BTreeSet;
use std::path::Path;

use purrdf::gts_compose::BlobRow;
use purrdf::slice::SliceCatalog;
use purrdf::{parse_dataset, DatasetView, GraphMatch, TermRef};

use gmeow_lang_bridge::emit::{digest16, ntriples_sorted};
use gmeow_lang_bridge::{
    exact_surface_correspondence, normalization_label, Bridge, PlainTextBridge,
};
use gmeow_lang_form::SurfaceForm;
use gmeow_logic::obligations::candidate_source_hash;
use gmeow_logic_compile::ir::{Correspondence, PreservationKind};
use gmeow_logic_compile::projections::ProjectionResult;

use crate::error::PipelineError;

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// The example-instance base every minted corpus IRI lives under — the same base the
/// translation corpus and the `lang:` competency queries scope with `STRSTARTS(...)`.
const EXAMPLE_BASE: &str = "http://example.org/lang/";
/// The internal English carrier language tag every source-prose literal is written under.
const ENGLISH_TAG: &str = "x-gmeow-english";

/// The document-scale threshold, in bytes: a surface whose text byte-length EXCEEDS
/// this holds its bytes by reference (`lang:surfaceBlob`) rather than inline
/// (`lang:surfaceText`), so the RDF never inlines document-scale payload bytes (the
/// blob-by-reference doctrine — the graph carries a handle and origin, never multi-KB
/// payloads that grow without bound with the document).
///
/// This is ONE hard-coded, documented constant — never a tunable knob. 4096 bytes is a
/// 4 KiB page: prose fields (labels, definitions, competency questions) sit comfortably
/// below it and stay inline for direct reading, while a genuine document-scale surface (a
/// lifted docs page, a treebank text, a whole section) crosses it and is held by
/// content-addressed reference. The native `lang:InlineBlobPayload` gate in
/// `crates/validate` shares this exact value; the two MUST stay in sync.
const DOCUMENT_SCALE_BYTES: usize = 4096;

/// The bundle blob-channel representation label for a document-scale surface's bytes.
/// Like the per-slice `doc-guide` guide-blob channel, this only tags the channel — the
/// blob is resolved by its content-addressed digest, not by rep.
const REP_LANG_SURFACE: &str = "lang-surface-blob";

/// The assembled prose-lift corpus: the sorted, byte-stable N-Triples graph plus the single
/// honest loss-ledger row (nothing is dropped — the round-trip is exact).
pub struct LangFormCorpus {
    /// The deterministic, sorted, byte-stable N-Triples graph (`graph/lang-form-corpus`).
    pub ntriples: Vec<u8>,
    /// The one `ProjectionResult` row for the whole corpus (an exact surface round-trip).
    pub ledger: Vec<ProjectionResult>,
}

/// One distinct English source literal interned as a raw surface plus its carried
/// surface-round-trip correspondence.
struct Prose {
    /// The raw literal text (byte-identical to the source RDF literal).
    text: String,
    /// The content-addressed `lang:SurfaceForm` IRI.
    surface_iri: String,
    /// The content-addressed `logic:Correspondence` IRI (the carried law-spine node).
    corr_iri: String,
    /// The `lang:Script` individual IRI the surface is written in.
    script_iri: String,
    /// The HONEST Unicode normalization-form label the bytes are actually in.
    normalization: String,
    /// The `sha256:`-prefixed `logic:candidateSourceHash`, byte-identical to the obligations
    /// gate's recomputation over the same raw text.
    source_hash: String,
    /// The carried `logic:Correspondence` (the [`PlainTextBridge`] law spine) whose facets
    /// the corpus re-emits under [`Self::corr_iri`].
    correspondence: Correspondence,
}

/// Build the total prose-lift corpus by interning every distinct `@x-gmeow-english` literal
/// under `root` as a reachable raw `lang:SurfaceForm`.
pub fn build_corpus(root: &Path) -> Result<LangFormCorpus, PipelineError> {
    let texts = collect_english_literals(root)?;

    let mut proses: Vec<Prose> = Vec::with_capacity(texts.len());
    for text in &texts {
        proses.push(build_prose(text)?);
    }
    // Deterministic ordering by the content-addressed surface IRI (the texts already arrive
    // sorted, but sort the interned rows explicitly so the ledger + graph are reproducible).
    proses.sort_by(|a, b| a.surface_iri.cmp(&b.surface_iri));

    let ntriples = emit_ntriples(&proses);
    let ledger = vec![corpus_ledger_row(&proses)];
    Ok(LangFormCorpus { ntriples, ledger })
}

/// The bundle blob rows backing every document-scale surface's `lang:surfaceBlob`
/// reference: for each distinct `@x-gmeow-english` literal whose byte-length exceeds
/// [`DOCUMENT_SCALE_BYTES`], one [`BlobRow`] carrying the raw bytes, keyed (by the gts
/// writer's `digest_string`) under the SAME `blake3:<hex>` digest the corpus emits — so
/// adding the same bytes resolves the reference and no document-scale payload rides
/// inline in the graph. Recomputed from `root` (the guide-blob pattern: blobs are rebuilt
/// in the carrier, independent of the stage product), so the set is a pure function of the
/// sources. Deterministic (sorted by bytes) and — until a source literal actually crosses
/// the threshold — empty, exactly as the total-prose corpus is all-inline today.
pub fn build_surface_blobs(root: &Path) -> Result<Vec<BlobRow>, PipelineError> {
    let texts = collect_english_literals(root)?;
    let mut blobs: Vec<BlobRow> = texts
        .iter()
        .filter(|text| text.len() > DOCUMENT_SCALE_BYTES)
        .map(|text| BlobRow {
            data: text.clone().into_bytes(),
            media_type: "text/plain; charset=utf-8".to_string(),
            rep: REP_LANG_SURFACE.to_string(),
        })
        .collect();
    blobs.sort_by(|a, b| a.data.cmp(&b.data));
    Ok(blobs)
}

/// The content-addressed `blake3:<hex>` blob reference for a surface's bytes — the SAME
/// digest the gts writer's `digest_string` assigns the registered [`BlobRow`] (mirroring
/// the per-slice `gmeow:guideBlob` anchor), so the emitted reference resolves to the
/// registered bytes.
fn surface_blob_digest(text: &str) -> String {
    format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex())
}

/// Collect every DISTINCT `@x-gmeow-english` literal across the source slices' Turtle
/// artifacts. Deterministic (a `BTreeSet`), deduplicated by material identity.
fn collect_english_literals(root: &Path) -> Result<BTreeSet<String>, PipelineError> {
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| PipelineError::Stage {
                stage: "stage-mappings".to_string(),
                message: format!("lang-form slice catalog: {e}"),
            })?;

    let mut texts: BTreeSet<String> = BTreeSet::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            // The English canon lives in the Turtle sources; only Turtle carries an
            // `@x-gmeow-english` RDF literal (the `.po` catalogs are not Turtle).
            if artifact.media_type != "text/turtle" {
                continue;
            }
            let ds = parse_dataset(&artifact.content, "text/turtle", None).map_err(|e| {
                PipelineError::Parse(format!(
                    "lang-form RDF parse of {}: {e}",
                    artifact.logical_path
                ))
            })?;
            for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
                if let TermRef::Literal {
                    lexical,
                    language: Some(lang),
                    ..
                } = ds.resolve(q.o)
                {
                    if lang == ENGLISH_TAG {
                        texts.insert(lexical.to_owned());
                    }
                }
            }
        }
    }
    Ok(texts)
}

/// Intern one distinct literal as a raw surface plus its carried surface-round-trip
/// correspondence. Degenerate (empty / whitespace-only / control-char) literals still lift;
/// only non-UTF-8 is a hard fail (it cannot occur for a parsed RDF literal, but the guard
/// stays honest).
fn build_prose(text: &str) -> Result<Prose, PipelineError> {
    // Drive the shared plain-text bridge: verify the surface round-trip re-emits the bytes
    // verbatim before minting anything (never a silent lossy repair).
    let lifted = PlainTextBridge
        .lift(text.as_bytes())
        .map_err(|d| PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-form: plain-text lift hard-failed on a source literal ({}): {}",
                d.failure_class.as_str(),
                d.construct
            ),
        })?;
    if PlainTextBridge.emit(&lifted) != text.as_bytes() {
        return Err(PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: "lang-form: plain-text surface round-trip is not byte-exact".to_string(),
        });
    }

    // Resolve the script individual from the language tag, then frame the surface with the
    // material identity a stable hash needs.
    let script_local = script_for_tag(ENGLISH_TAG)?;
    let surface = SurfaceForm {
        text: text.to_owned(),
        script: script_local.to_owned(),
        encoding: "UTF-8".to_owned(),
        normalization: normalization_label(text).to_owned(),
        collation: "en".to_owned(),
    };
    let surface_key = surface.surface_key();

    Ok(Prose {
        text: text.to_owned(),
        surface_iri: example("lang-surface", &digest16("lang-surface", &surface_key)),
        corr_iri: example(
            "lang-form-correspondence",
            &digest16("lang-form-corr", &surface_key),
        ),
        script_iri: iri(LANG_NS, script_local),
        normalization: surface.normalization.clone(),
        // Hash the RAW literal text (no NFC transform) so the value coincides byte-for-byte
        // with the obligations gate's `candidate_source_hash`.
        source_hash: candidate_source_hash(text),
        correspondence: exact_surface_correspondence(&surface),
    })
}

/// Resolve the `lang:Script` individual (local name) for a source language tag, structured
/// as a lookup so adding a language is a one-line data add. An unknown tag is a HARD FAIL,
/// never a silently-underspecified surface. The universe is `x-gmeow-english` only, so this
/// never triggers today.
fn script_for_tag(tag: &str) -> Result<&'static str, PipelineError> {
    match tag {
        "x-gmeow-english" => Ok("latinScript"),
        _ => Err(PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-form: no lang:Script mapping for language tag '{tag}'; add its \
                 lang:Script individual to slices/grounding/lang/module.ttl and extend \
                 script_for_tag"
            ),
        }),
    }
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole corpus.
fn emit_ntriples(proses: &[Prose]) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();

    for prose in proses {
        // ── the raw surface (the only stratum where byte identity is load-bearing) ──
        lines.push(triple(
            &prose.surface_iri,
            RDF_TYPE,
            &iri(LANG_NS, "SurfaceForm"),
        ));
        lines.push(triple(
            &prose.surface_iri,
            RDF_TYPE,
            &iri(LANG_NS, "UnanalyzedProse"),
        ));
        // Document-scale surfaces hold their bytes BY REFERENCE (blob-by-reference
        // doctrine): a surface whose text byte-length exceeds DOCUMENT_SCALE_BYTES emits
        // a content-addressed `lang:surfaceBlob "blake3:<hex>"` handle instead of inline
        // `lang:surfaceText`, and its bytes ride the bundle blob channel (registered by
        // `build_surface_blobs`). Small surfaces stay inline for direct reading. The
        // digest is content-addressed, so the reference is deterministic and the emitted
        // graph never inlines document-scale payload bytes.
        if prose.text.len() > DOCUMENT_SCALE_BYTES {
            lines.push(triple_lit(
                &prose.surface_iri,
                &iri(LANG_NS, "surfaceBlob"),
                &surface_blob_digest(&prose.text),
            ));
        } else {
            lines.push(triple_lit(
                &prose.surface_iri,
                &iri(LANG_NS, "surfaceText"),
                &prose.text,
            ));
        }
        lines.push(triple(
            &prose.surface_iri,
            &iri(LANG_NS, "inScript"),
            &prose.script_iri,
        ));
        lines.push(triple_lit(
            &prose.surface_iri,
            &iri(LANG_NS, "encoding"),
            "UTF-8",
        ));
        lines.push(triple_lit(
            &prose.surface_iri,
            &iri(LANG_NS, "unicodeNormalization"),
            &prose.normalization,
        ));
        lines.push(triple_lit(
            &prose.surface_iri,
            &iri(LANG_NS, "collationLocale"),
            "en",
        ));
        lines.push(triple(
            &prose.surface_iri,
            &iri(LANG_NS, "analysisLevel"),
            &iri(LANG_NS, "rawLevel"),
        ));
        // The prose-hash the obligations gate resolves THROUGH this surface (byte-identical).
        lines.push(triple_lit(
            &prose.surface_iri,
            &iri(LOGIC_NS, "candidateSourceHash"),
            &prose.source_hash,
        ));
        // THE attachment point of the law-spine: the surface carries exactly one
        // logic:Correspondence for its round-trip.
        lines.push(triple(
            &prose.surface_iri,
            &iri(LANG_NS, "surfaceCorrespondence"),
            &prose.corr_iri,
        ));

        // ── the carried logic:Correspondence law-spine (facets read off the bridge) ──
        let c = &prose.correspondence;
        lines.push(triple(
            &prose.corr_iri,
            RDF_TYPE,
            &iri(LOGIC_NS, "Correspondence"),
        ));
        // The surface round-trip re-emits the bytes verbatim: an exact preservation, carried
        // WITH the mnemomorphic witness (never Exact-with-no-witness — the overclaim floor).
        lines.push(triple(
            &prose.corr_iri,
            &iri(LOGIC_NS, "preservationKind"),
            &PreservationKind::Exact.iri(),
        ));
        lines.push(triple(
            &prose.corr_iri,
            &iri(LOGIC_NS, "correspondenceRelation"),
            &iri(LOGIC_NS, c.relation.as_str()),
        ));
        lines.push(triple(
            &prose.corr_iri,
            &iri(LOGIC_NS, "morphismClass"),
            &c.morphism_class.iri(),
        ));
        if let Some(det) = c.determinacy {
            lines.push(triple(
                &prose.corr_iri,
                &iri(LOGIC_NS, "hasDeterminacy"),
                &det.iri(),
            ));
        }
        lines.push(triple_typed(
            &prose.corr_iri,
            &iri(LOGIC_NS, "mnemomorphic"),
            if c.mnemomorphic { "true" } else { "false" },
            XSD_BOOLEAN,
        ));
    }

    ntriples_sorted(lines)
}

/// The single honest ledger row for the whole corpus: the surface stratum round-trips
/// exactly, so nothing is dropped (`actual_drops`/`lossy_drops` empty). The unanalyzed
/// status is recorded on each surface as a graded level, not charged as a loss.
fn corpus_ledger_row(proses: &[Prose]) -> ProjectionResult {
    ProjectionResult {
        target: "lang-form".to_string(),
        // The lift count is a descriptive summary, NOT a dropped item — a declared
        // ExactPreservation projection carries empty `actual_drops`/`lossy_drops` (a
        // non-empty drop set under an exact claim is the overclaim floor).
        content: format!(
            "total prose lift: {n} distinct @x-gmeow-english literal(s) interned as raw \
             lang:SurfaceForm; surface round-trip exact, nothing dropped",
            n = proses.len(),
        ),
        is_rdf: false,
        preservation: PreservationKind::Exact,
        complexity: "n/a".to_string(),
        lossy_drops: Vec::new(),
        actual_drops: Vec::new(),
    }
}

// ── N-Triples helpers (kept self-contained, mirroring the translation producer) ────────

fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

fn example(segment: &str, id: &str) -> String {
    format!("{EXAMPLE_BASE}{segment}/{id}")
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
    fn gate1_every_distinct_english_literal_has_a_surface_form() {
        use std::collections::HashSet;

        let root = repo_root();
        let universe = collect_english_literals(&root).expect("collect universe");
        assert!(
            !universe.is_empty(),
            "the source bundle must carry @x-gmeow-english prose"
        );
        let corpus = build_corpus(&root).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");

        // Index every emitted lang:surfaceText OBJECT literal ONCE (set membership, not a
        // per-literal scan of the whole graph) so the coverage check is linear in the corpus
        // size. Each surfaceText line is `<surface> <surfaceText> "escaped" .`; the object is
        // the segment after the predicate marker, minus the trailing ` .`.
        let pred_marker = format!("<{}> ", iri(LANG_NS, "surfaceText"));
        let emitted: HashSet<&str> = nt
            .lines()
            .filter_map(|line| {
                line.find(&pred_marker)
                    .map(|idx| &line[idx + pred_marker.len()..line.len() - 2])
            })
            .collect();
        // A document-scale surface holds its bytes BY REFERENCE (lang:surfaceBlob) rather
        // than inline, so index the emitted blob references the same way and resolve each
        // universe literal through whichever channel its byte-length selects.
        let blob_marker = format!("<{}> ", iri(LANG_NS, "surfaceBlob"));
        let blobs: HashSet<&str> = nt
            .lines()
            .filter_map(|line| {
                line.find(&blob_marker)
                    .map(|idx| &line[idx + blob_marker.len()..line.len() - 2])
            })
            .collect();
        for text in &universe {
            if text.len() > DOCUMENT_SCALE_BYTES {
                let digest = nt_literal(&surface_blob_digest(text));
                assert!(
                    blobs.contains(digest.as_str()),
                    "Gate 1: document-scale @x-gmeow-english literal has no lang:surfaceBlob \
                     reference: {text:?}"
                );
            } else {
                assert!(
                    emitted.contains(nt_literal(text).as_str()),
                    "Gate 1: distinct @x-gmeow-english literal has no lang:SurfaceForm: {text:?}"
                );
            }
        }
        // Exactly one lang:SurfaceForm per distinct literal — total, not partial.
        let surface_forms = nt
            .matches(&format!("<{}> .", iri(LANG_NS, "SurfaceForm")))
            .count();
        assert_eq!(
            surface_forms,
            universe.len(),
            "one lang:SurfaceForm per distinct @x-gmeow-english literal"
        );
        // Each surface is honest unanalyzed prose at the raw level, carrying its round-trip.
        assert!(nt.contains(&iri(LANG_NS, "UnanalyzedProse")));
        assert!(nt.contains(&iri(LANG_NS, "rawLevel")));
        assert!(nt.contains(&iri(LANG_NS, "surfaceCorrespondence")));
        // The carried law-spine is an exact isomorphism — never a fabricated approximation.
        assert!(nt.contains(&iri(LOGIC_NS, "Isomorphism")));
        assert!(nt.contains(&PreservationKind::Exact.iri()));
    }

    #[test]
    fn prose_hash_coincides_with_the_obligations_gate() {
        // The emitted logic:candidateSourceHash equals the obligations gate's recomputation
        // over the SAME raw byte string — the coincidence the prose-hash discipline needs.
        for text in ["A definition prose field.", "café", "", "   "] {
            let prose = build_prose(text).expect("build prose");
            assert_eq!(
                prose.source_hash,
                candidate_source_hash(text),
                "emitted prose-hash must equal the gate's recomputation for {text:?}"
            );
            let nt = String::from_utf8(emit_ntriples(&[build_prose(text).unwrap()])).unwrap();
            assert!(
                nt.contains(&candidate_source_hash(text)),
                "the corpus must emit the gate's candidate_source_hash for {text:?}"
            );
        }
    }

    #[test]
    fn prose_hash_resolves_for_both_nfc_and_nfd() {
        // The SAME visible text as NFC vs NFD are two DISTINCT surface literals with distinct
        // byte strings. The candidate-hash the corpus emits for each must match
        // candidate_source_hash of that exact byte string — so the lookup resolves for both.
        let nfc = "caf\u{e9}"; // "café" with precomposed é (NFC) codespell:ignore caf
        let nfd = "cafe\u{301}"; // "café" with e + combining acute (NFD)
        assert_ne!(nfc, nfd, "the two normalizations are distinct byte strings");

        let p_nfc = build_prose(nfc).expect("nfc");
        let p_nfd = build_prose(nfd).expect("nfd");

        // Distinct surface literals (distinct material identity → distinct content address).
        assert_ne!(p_nfc.surface_iri, p_nfd.surface_iri);
        // Each hash coincides with the gate over its exact byte string.
        assert_eq!(p_nfc.source_hash, candidate_source_hash(nfc));
        assert_eq!(p_nfd.source_hash, candidate_source_hash(nfd));
        assert_ne!(p_nfc.source_hash, p_nfd.source_hash);
        // The declared normalization frame is honest for each (never a blanket "NFC").
        assert_eq!(p_nfc.normalization, "NFC");
        assert_eq!(p_nfd.normalization, "NFD");
    }

    #[test]
    fn document_scale_surface_holds_bytes_by_reference() {
        // Small surfaces stay inline; a document-scale surface emits a content-addressed
        // lang:surfaceBlob reference and NEVER inlines its bytes.
        let short = "cats chase mice";
        let nt_short = String::from_utf8(emit_ntriples(&[build_prose(short).unwrap()])).unwrap();
        assert!(nt_short.contains(&iri(LANG_NS, "surfaceText")));
        assert!(!nt_short.contains(&iri(LANG_NS, "surfaceBlob")));

        let long = "x".repeat(DOCUMENT_SCALE_BYTES + 1);
        let nt_long = String::from_utf8(emit_ntriples(&[build_prose(&long).unwrap()])).unwrap();
        assert!(
            nt_long.contains(&surface_blob_digest(&long)),
            "a document-scale surface must carry its content-addressed blob reference"
        );
        assert!(!nt_long.contains(&iri(LANG_NS, "surfaceText")));
        // The document-scale payload never rides inline in the graph.
        assert!(!nt_long.contains(&long));
    }

    #[test]
    fn surface_blobs_resolve_every_document_scale_reference() {
        use std::collections::HashSet;
        let root = repo_root();
        let nt = String::from_utf8(build_corpus(&root).expect("corpus").ntriples).unwrap();
        let blobs = build_surface_blobs(&root).expect("blobs");

        // Every emitted lang:surfaceBlob reference in the corpus (the object literal,
        // stripped of its N-Triples quoting) is backed by exactly one registered blob whose
        // bytes hash to that same content-addressed digest — no dangling reference, no
        // orphan blob.
        let blob_marker = format!("<{}> ", iri(LANG_NS, "surfaceBlob"));
        let refs: HashSet<String> = nt
            .lines()
            .filter_map(|line| {
                line.find(&blob_marker)
                    .map(|idx| line[idx + blob_marker.len()..line.len() - 2].to_owned())
            })
            .collect();
        let digests: HashSet<String> = blobs
            .iter()
            .map(|b| nt_literal(&surface_blob_digest(std::str::from_utf8(&b.data).unwrap())))
            .collect();
        assert_eq!(
            refs, digests,
            "every surfaceBlob reference must be backed by a registered blob, and vice versa"
        );

        // Deterministic: the blob set is a pure function of the sources.
        let again = build_surface_blobs(&root).expect("blobs again");
        let data: Vec<&Vec<u8>> = blobs.iter().map(|b| &b.data).collect();
        let data2: Vec<&Vec<u8>> = again.iter().map(|b| &b.data).collect();
        assert_eq!(data, data2, "build_surface_blobs must be deterministic");
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let root = repo_root();
        let a = build_corpus(&root).expect("build a").ntriples;
        let b = build_corpus(&root).expect("build b").ntriples;
        assert_eq!(a, b, "corpus N-Triples must be deterministic");
    }

    #[test]
    fn corpus_ledger_is_one_exact_row_with_no_drops() {
        let corpus = build_corpus(&repo_root()).expect("build corpus");
        assert_eq!(corpus.ledger.len(), 1, "one corpus ledger row");
        let row = &corpus.ledger[0];
        assert_eq!(row.target, "lang-form");
        assert_eq!(row.preservation, PreservationKind::Exact);
        assert!(row.lossy_drops.is_empty(), "nothing is dropped");
        assert!(
            row.actual_drops.is_empty(),
            "an exact projection declares no dropped items (else the overclaim gate fires)"
        );
        assert!(
            row.content.contains("total prose lift"),
            "the lift count is a descriptive summary carried on content"
        );
    }

    #[test]
    fn script_for_tag_maps_english_and_hard_fails_unknown() {
        assert_eq!(script_for_tag("x-gmeow-english").unwrap(), "latinScript");
        let err = script_for_tag("qtz").expect_err("unknown tag must hard-fail");
        assert!(format!("{err}").contains("no lang:Script mapping"));
    }
}
