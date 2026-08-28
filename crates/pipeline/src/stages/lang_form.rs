// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The total prose-lift corpus producer (Gate 1: every `@x-gmeow-english` literal is a
//! reachable `lang:SurfaceForm`).
//!
//! # Extraction universe
//!
//! Every DISTINCT `@x-gmeow-english`-tagged literal across the SOURCE slices. The universe
//! is drawn from the SHARED, already-discovered in-memory [`SliceCatalog`] the mappings
//! stage holds (the same catalog the correspondence lowerings consume) — NOT a second disk
//! walk. Its artifact bytes are already resident, so this parses every `text/turtle`
//! artifact the catalog carries (module, shapes, manifest, example, counter-example, and
//! Turtle mapping artifacts) straight from memory and collects each language-tagged literal
//! whose tag is `x-gmeow-english` into a `BTreeSet<String>` (deterministic, deduplicated by
//! material identity). Because the extraction universe IS the in-memory source the pipeline
//! composes from, "total lift" is total over what actually composes into the bundle — never
//! a fresh, independent re-read of `slices/`. The `.po` translation catalogs are not Turtle
//! and carry no `@x-gmeow-english` RDF literal, so the English canon is exactly the Turtle
//! literal set — never re-derived from the translations.
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

use std::collections::{BTreeMap, BTreeSet};

use purrdf::gts_compose::BlobRow;
use purrdf::slice::SliceCatalog;
use purrdf::{DatasetView, GraphMatch, TermRef, parse_dataset};

use gmeow_lang_bridge::emit::{digest16, ntriples_sorted};
use gmeow_lang_bridge::{
    Bridge, PlainTextBridge, exact_surface_correspondence, normalization_label,
};
use gmeow_lang_form::SurfaceForm;
use gmeow_logic::obligations::candidate_source_hash;
use gmeow_logic_compile::ir::{Correspondence, PreservationKind};
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;

use gmeow_ns::LANG_NS;
use gmeow_ns::LOGIC_NS;
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
    /// The loss store the row's drops are interned into (empty for this Exact corpus). The
    /// mappings stage unions it into the single report loss store.
    pub loss: LossLedger,
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
    /// The `lang:SignSystem` individual IRI the surface is situated in (`lang:inSignSystem`)
    /// — the carrier variety the source tag resolves to (e.g. `lang:gmeowEnglish`), read off
    /// the SAME data-driven `lang:carrierTag` edge the script binding joins through.
    sign_system_iri: String,
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
/// carried by the shared in-memory source [`SliceCatalog`] as a reachable raw
/// `lang:SurfaceForm`. The catalog is the one the mappings stage already discovered (its
/// artifact bytes are resident), so the universe is a projection of the composed source —
/// never a second disk walk. `None` (no `slices/` tree) yields an empty corpus.
pub fn build_corpus(catalog: Option<&SliceCatalog>) -> Result<LangFormCorpus, gmeow_errors::Diag> {
    let texts = collect_english_literals(catalog)?;

    let mut proses: Vec<Prose> = Vec::with_capacity(texts.len());
    if !texts.is_empty() {
        // Resolve the carrier tag to its lang:Script AND its sign-system individual ONCE
        // from the parsed source ontology (never a hard-coded Rust match) — the whole
        // corpus is authored under the single English carrier tag, so this is one lookup,
        // not one per literal.
        let bindings = build_carrier_bindings(catalog)?;
        let binding = binding_for_tag(ENGLISH_TAG, &bindings)?.clone();
        for text in &texts {
            proses.push(build_prose(text, &binding)?);
        }
    }
    // Deterministic ordering by the content-addressed surface IRI (the texts already arrive
    // sorted, but sort the interned rows explicitly so the ledger + graph are reproducible).
    proses.sort_by(|a, b| a.surface_iri.cmp(&b.surface_iri));

    let ntriples = emit_ntriples(&proses);
    let mut loss = LossLedger::new();
    let ledger = vec![corpus_ledger_row(&proses, &mut loss)];
    Ok(LangFormCorpus {
        ntriples,
        ledger,
        loss,
    })
}

/// The bundle blob rows backing every document-scale surface's `lang:surfaceBlob`
/// reference: for each distinct `@x-gmeow-english` literal whose byte-length exceeds
/// [`DOCUMENT_SCALE_BYTES`], one [`BlobRow`] carrying the raw bytes, keyed (by the gts
/// writer's `digest_string`) under the SAME `blake3:<hex>` digest the corpus emits — so
/// adding the same bytes resolves the reference and no document-scale payload rides
/// inline in the graph. Recomputed from the SAME shared in-memory source [`SliceCatalog`]
/// the corpus draws its universe from (the guide-blob pattern: blobs are rebuilt in the
/// carrier, independent of the stage product), so the set is a pure function of the
/// composed sources — never a second disk walk. Deterministic (sorted by bytes) and —
/// until a source literal actually crosses the threshold — empty, exactly as the
/// total-prose corpus is all-inline today.
pub fn build_surface_blobs(
    catalog: Option<&SliceCatalog>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    let texts = collect_english_literals(catalog)?;
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

/// The totality of the prose lift: the size of the extraction universe (distinct
/// `@x-gmeow-english` literals) and how many of them the built corpus actually lifts to a
/// reachable `lang:SurfaceForm`. The lift is TOTAL — the count-equality the flagship
/// contract advertises — iff `covered == universe`.
pub struct ProseLiftCoverage {
    /// The distinct `@x-gmeow-english` literals the corpus must lift (the extraction
    /// universe, from [`collect_english_literals`]).
    pub universe: usize,
    /// How many of those universe literals are reachable as a `lang:SurfaceForm` in the
    /// built corpus — inline through `lang:surfaceText`, or by reference through a
    /// content-addressed `lang:surfaceBlob` digest for document-scale surfaces.
    pub covered: usize,
}

/// Compute the total prose-lift coverage over the shared in-memory source [`SliceCatalog`]:
/// the extraction universe (distinct `@x-gmeow-english` literals) and how many of them the
/// built corpus actually lifts to a reachable `lang:SurfaceForm`. This is the count-equality
/// the flagship contract advertises — in a total lift `covered == universe`.
///
/// A document-scale surface (byte-length exceeding [`DOCUMENT_SCALE_BYTES`]) holds its bytes
/// BY REFERENCE — a content-addressed `lang:surfaceBlob "blake3:<hex>"` handle — rather than
/// inline `lang:surfaceText`, so a universe literal counts as covered if it is reachable
/// through EITHER channel, keyed on the SAME threshold and [`surface_blob_digest`] the corpus
/// emits. `None` (no `slices/` tree) yields an empty universe.
pub fn prose_lift_coverage(
    catalog: Option<&SliceCatalog>,
) -> Result<ProseLiftCoverage, gmeow_errors::Diag> {
    let universe = collect_english_literals(catalog)?;
    let corpus = build_corpus(catalog)?;
    let nt = String::from_utf8(corpus.ntriples).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("lang-form: corpus N-Triples are not UTF-8: {e}"),
        })
    })?;

    // Index every emitted lang:surfaceText OBJECT literal and lang:surfaceBlob reference
    // ONCE (set membership, not a per-literal scan of the whole graph) so the coverage
    // computation is linear in the corpus size. Each line is `<surface> <predicate>
    // "escaped" .`; the object is the segment after the predicate marker, minus the trailing
    // ` .`.
    let text_marker = format!("<{}> ", iri(LANG_NS, "surfaceText"));
    let emitted: BTreeSet<&str> = nt
        .lines()
        .filter_map(|line| {
            line.find(&text_marker)
                .map(|idx| &line[idx + text_marker.len()..line.len() - 2])
        })
        .collect();
    let blob_marker = format!("<{}> ", iri(LANG_NS, "surfaceBlob"));
    let blobs: BTreeSet<&str> = nt
        .lines()
        .filter_map(|line| {
            line.find(&blob_marker)
                .map(|idx| &line[idx + blob_marker.len()..line.len() - 2])
        })
        .collect();

    // Resolve each universe literal through whichever channel its byte-length selects — a
    // document-scale literal through its content-addressed blob digest, a below-threshold
    // one through the inline surface text — and count it covered iff it is reachable there.
    let mut covered = 0usize;
    for text in &universe {
        let reachable = if text.len() > DOCUMENT_SCALE_BYTES {
            blobs.contains(nt_literal(&surface_blob_digest(text)).as_str())
        } else {
            emitted.contains(nt_literal(text).as_str())
        };
        if reachable {
            covered += 1;
        }
    }
    Ok(ProseLiftCoverage {
        universe: universe.len(),
        covered,
    })
}

/// Collect every DISTINCT `@x-gmeow-english` literal across the SHARED in-memory source
/// [`SliceCatalog`]'s Turtle artifacts. The catalog was discovered ONCE upstream (the
/// mappings stage holds it, and the correspondence lowerings consume the same instance), so
/// this parses only the already-resident artifact bytes — never a second `SliceCatalog::
/// discover` disk walk. `None` (no `slices/` tree) yields the empty set. Deterministic (a
/// `BTreeSet`), deduplicated by material identity.
fn collect_english_literals(
    catalog: Option<&SliceCatalog>,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let mut texts: BTreeSet<String> = BTreeSet::new();
    let Some(catalog) = catalog else {
        return Ok(texts);
    };
    for record in catalog.records() {
        for artifact in &record.artifacts {
            // The English canon lives in the Turtle sources; only Turtle carries an
            // `@x-gmeow-english` RDF literal (the `.po` catalogs are not Turtle).
            if artifact.media_type != "text/turtle" {
                continue;
            }
            let ds = parse_dataset(&artifact.content, "text/turtle", None).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("lang-form RDF parse of {}: {e}", artifact.logical_path),
                })
            })?;
            for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
                if let TermRef::Literal {
                    lexical,
                    language: Some(lang),
                    ..
                } = ds.resolve(q.o)
                    && lang == ENGLISH_TAG
                {
                    texts.insert(lexical.to_owned());
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
fn build_prose(text: &str, binding: &CarrierBinding) -> Result<Prose, gmeow_errors::Diag> {
    // Drive the shared plain-text bridge: verify the surface round-trip re-emits the bytes
    // verbatim before minting anything (never a silent lossy repair).
    let lifted = PlainTextBridge.lift(text.as_bytes()).map_err(|d| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-form: plain-text lift hard-failed on a source literal ({}): {}",
                d.failure_class.as_str(),
                d.construct
            ),
        })
    })?;
    if PlainTextBridge.emit(&lifted) != text.as_bytes() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: "lang-form: plain-text surface round-trip is not byte-exact".to_string(),
        }));
    }

    // The carrier binding (script individual + sign-system individual) is resolved ONCE
    // (data-driven, from the parsed ontology) by the caller and threaded in; frame the
    // surface with the material identity a stable hash needs.
    let surface = SurfaceForm {
        text: text.to_owned(),
        script: binding.script_local.clone(),
        encoding: "UTF-8".to_owned(),
        normalization: normalization_label(text).to_owned(),
        collation: "en".to_owned(),
    };
    let surface_key = surface.surface_key();
    let correspondence = exact_surface_correspondence(&surface).map_err(|d| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-form: exact surface correspondence hard-failed ({}): {}",
                d.failure_class.as_str(),
                d.construct
            ),
        })
    })?;

    Ok(Prose {
        text: text.to_owned(),
        surface_iri: example("lang-surface", &digest16("lang-surface", &surface_key)),
        corr_iri: example(
            "lang-form-correspondence",
            &digest16("lang-form-corr", &surface_key),
        ),
        script_iri: iri(LANG_NS, &binding.script_local),
        sign_system_iri: binding.language_iri.clone(),
        normalization: surface.normalization.clone(),
        // Hash the RAW literal text (no NFC transform) so the value coincides byte-for-byte
        // with the obligations gate's `candidate_source_hash`.
        source_hash: candidate_source_hash(text),
        correspondence,
    })
}

/// The data-driven resolution of one carrier tag: the `lang:Script` local name the surface
/// hash frames with, plus the sign-system individual IRI (the carrier variety, e.g.
/// `lang:gmeowEnglish` — a `lang:LanguageVariety ⊑ lang:SignSystem`) every lifted surface
/// is situated in through `lang:inSignSystem`.
#[derive(Clone, Debug)]
struct CarrierBinding {
    /// The `lang:Script` local name (the `lang:` IRI minus the namespace, e.g. `latinScript`).
    script_local: String,
    /// The full IRI of the carrier variety individual the tag is declared on.
    language_iri: String,
}

/// Build the carrier-tag → [`CarrierBinding`] resolution map from the PARSED source
/// ontology — never a hard-coded Rust match. A carrier tag is authored as machine-readable
/// data in `slices/grounding/lang/module.ttl`: a language individual (a `lang:SignSystem` or
/// `lang:LanguageVariety`) carries its tag through `lang:carrierTag`, and its script is bound
/// through an orthography (`lang:orthographyFor` the language, `lang:usesScript` the script).
/// This walks the SAME shared in-memory source [`SliceCatalog`] the corpus universe is drawn
/// from, joins those three edges, and yields `tag → (script-local-name, language IRI)` — the
/// language individual is kept because every lifted surface is a `lang:Form` and MUST be
/// situated in its sign system (`lang:inSignSystem`, the `lang:FormSituatedShape` bound).
/// Adding a language is therefore a pure DATA add — a new variety + orthography in
/// `module.ttl`, zero Rust change. `None` (no `slices/` tree) yields the empty map.
/// Deterministic (a `BTreeMap`).
fn build_carrier_bindings(
    catalog: Option<&SliceCatalog>,
) -> Result<BTreeMap<String, CarrierBinding>, gmeow_errors::Diag> {
    let mut bindings: BTreeMap<String, CarrierBinding> = BTreeMap::new();
    let Some(catalog) = catalog else {
        return Ok(bindings);
    };

    // The three data edges the join reads, all authored in the lang: module.
    let p_carrier_tag = iri(LANG_NS, "carrierTag");
    let p_orthography_for = iri(LANG_NS, "orthographyFor");
    let p_uses_script = iri(LANG_NS, "usesScript");

    // language-individual IRI → its carrier tag literal.
    let mut carrier_tag: BTreeMap<String, String> = BTreeMap::new();
    // orthography IRI → the language individual it serves.
    let mut orthography_for: BTreeMap<String, String> = BTreeMap::new();
    // orthography IRI → the script IRI it writes in.
    let mut uses_script: BTreeMap<String, String> = BTreeMap::new();

    for record in catalog.records() {
        for artifact in &record.artifacts {
            // The reference model lives in the Turtle sources.
            if artifact.media_type != "text/turtle" {
                continue;
            }
            let ds = parse_dataset(&artifact.content, "text/turtle", None).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!(
                        "lang-form script-binding RDF parse of {}: {e}",
                        artifact.logical_path
                    ),
                })
            })?;
            for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
                let TermRef::Iri(pred) = ds.resolve(q.p) else {
                    continue;
                };
                if pred == p_carrier_tag {
                    if let (
                        TermRef::Iri(subj),
                        TermRef::Literal {
                            lexical,
                            language: None,
                            ..
                        },
                    ) = (ds.resolve(q.s), ds.resolve(q.o))
                    {
                        carrier_tag.insert(subj.to_owned(), lexical.to_owned());
                    }
                } else if pred == p_orthography_for {
                    if let (TermRef::Iri(subj), TermRef::Iri(obj)) =
                        (ds.resolve(q.s), ds.resolve(q.o))
                    {
                        orthography_for.insert(subj.to_owned(), obj.to_owned());
                    }
                } else if pred == p_uses_script
                    && let (TermRef::Iri(subj), TermRef::Iri(obj)) =
                        (ds.resolve(q.s), ds.resolve(q.o))
                {
                    uses_script.insert(subj.to_owned(), obj.to_owned());
                }
            }
        }
    }

    // Join the edges: for each orthography that writes a script AND serves a language whose
    // carrier tag is declared, resolve tag → (script-local-name, language IRI). A script IRI
    // outside the lang: namespace is not a resolvable local name and is skipped (an
    // unresolvable tag then hard-fails at lookup, never a silent default).
    for (orthography, script_iri) in &uses_script {
        let Some(language) = orthography_for.get(orthography) else {
            continue;
        };
        let Some(tag) = carrier_tag.get(language) else {
            continue;
        };
        let Some(script_local) = script_iri.strip_prefix(LANG_NS) else {
            continue;
        };
        bindings.insert(
            tag.clone(),
            CarrierBinding {
                script_local: script_local.to_owned(),
                language_iri: language.clone(),
            },
        );
    }
    Ok(bindings)
}

/// Resolve the [`CarrierBinding`] (script local name + sign-system individual IRI) for a
/// source carrier tag from the data-driven [`build_carrier_bindings`] map. An unknown /
/// unresolvable tag is a HARD FAIL, never a silently-underspecified surface — adding a
/// language is a DATA add in `module.ttl`, never a Rust change.
fn binding_for_tag<'a>(
    tag: &str,
    bindings: &'a BTreeMap<String, CarrierBinding>,
) -> Result<&'a CarrierBinding, gmeow_errors::Diag> {
    bindings.get(tag).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "lang-form: no lang:Script binding for language tag '{tag}'; declare its \
                 carrier variety (lang:carrierTag) and an orthography (lang:orthographyFor + \
                 lang:usesScript naming its lang:Script) in slices/grounding/lang/module.ttl"
            ),
        })
    })
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
        // Every lang:Form is situated in exactly one sign system (lang:FormSituatedShape):
        // the lifted surface names the carrier variety its source tag resolves to.
        lines.push(triple(
            &prose.surface_iri,
            &iri(LANG_NS, "inSignSystem"),
            &prose.sign_system_iri,
        ));
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
fn corpus_ledger_row(proses: &[Prose], loss: &mut LossLedger) -> ProjectionResult {
    // The lift count is a descriptive summary, NOT a dropped item — a declared
    // ExactPreservation projection interns NO drops (a non-empty drop set under an exact
    // claim is the overclaim floor).
    loss.record_projection_drops("lang-form", PreservationKind::Exact, &[], &[]);
    ProjectionResult {
        target: "lang-form".to_string(),
        content: format!(
            "total prose lift: {n} distinct @x-gmeow-english literal(s) interned as raw \
             lang:SurfaceForm; surface round-trip exact, nothing dropped",
            n = proses.len(),
        ),
        is_rdf: false,
        preservation: PreservationKind::Exact,
        complexity: "n/a".to_string(),
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

    fn english_binding() -> CarrierBinding {
        CarrierBinding {
            script_local: "latinScript".to_string(),
            language_iri: iri(LANG_NS, "gmeowEnglish"),
        }
    }

    #[test]
    fn prose_hash_coincides_with_the_obligations_gate() {
        for text in ["A definition prose field.", "café", "", "   "] {
            let prose = build_prose(text, &english_binding()).expect("build prose");
            assert_eq!(prose.source_hash, candidate_source_hash(text));
        }
    }

    #[test]
    fn prose_hash_resolves_for_both_nfc_and_nfd() {
        let nfc = "caf\u{e9}"; // codespell:ignore caf
        let nfd = "cafe\u{301}";
        let p_nfc = build_prose(nfc, &english_binding()).expect("nfc");
        let p_nfd = build_prose(nfd, &english_binding()).expect("nfd");
        assert_ne!(p_nfc.surface_iri, p_nfd.surface_iri);
        assert_eq!(p_nfc.source_hash, candidate_source_hash(nfc));
        assert_eq!(p_nfd.source_hash, candidate_source_hash(nfd));
        assert_eq!(p_nfc.normalization, "NFC");
        assert_eq!(p_nfd.normalization, "NFD");
    }

    #[test]
    fn document_scale_surface_holds_bytes_by_reference() {
        let long = "x".repeat(DOCUMENT_SCALE_BYTES + 1);
        let nt = String::from_utf8(emit_ntriples(&[
            build_prose(&long, &english_binding()).expect("long prose")
        ]))
        .expect("utf8");
        assert!(nt.contains(&surface_blob_digest(&long)));
        assert!(!nt.contains(&iri(LANG_NS, "surfaceText")));
        assert!(!nt.contains(&long));
    }

    #[test]
    fn binding_lookup_hard_fails_unknown_without_discovery() {
        let bindings = BTreeMap::from([("x-gmeow-english".to_string(), english_binding())]);
        assert_eq!(
            binding_for_tag("x-gmeow-english", &bindings)
                .expect("known binding")
                .script_local,
            "latinScript"
        );
        let err = binding_for_tag("qtz", &bindings).expect_err("unknown tag must fail");
        assert!(format!("{err}").contains("no lang:Script binding"));
    }
}
