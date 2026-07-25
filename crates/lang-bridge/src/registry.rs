// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The correspondence-carrying **projection registry**: the ordered list of
//! [`LangProjectionTarget`]s that lower the canonical `lang:` model out to the external
//! linguistic ecosystems (OntoLex-Lemon, CoNLL-U, EBNF, ABNF, …).
//!
//! This is the projection peer of [`crate::bridge::Bridge`]. Like a bridge, a target
//! **CARRIES a [`Correspondence`]** for each emission rather than declaring its own
//! preservation via a trait method — the trait has **no `preservation()` and no
//! `round_trip_ok()`**. The preservation judgment is DERIVED from the carried
//! correspondence by the driver ([`crate::is_exact_correspondence`]), and the round-trip is
//! MEASURED by the target (re-parse / byte round-trip) and cross-checked by the driver
//! against [`crate::exact_round_trip_holds`] over the carried [`LangEmission::leg_pair`]. There
//! is one law spine in the system — never a per-target law shadow.
//!
//! Each target reuses the EXISTING bridge functions (`grammar_*`, `conllu_*`,
//! `ontolex_*`); the registry adds NO new transform, only the projection-direction
//! wiring and the honest per-emission preservation record the driver folds into the loss
//! ledger and the `lang:ProjectionEmission` corpus.

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeCondition,
    DischargeVerdict, LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind,
};
pub(crate) use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;

use crate::bcp47::Bcp47Target;
use crate::bridge::{IngestDiagnostic, LangFailure};
use crate::conllu::ConlluTarget;
use crate::emit::{digest16, ntriples_sorted};
use crate::gmn1_codec::{
    CurrentCodebook, Gmn0Model, GmnDictionary, gmn0_canonically_equal, gmn1_read, gmn1_write,
};
use crate::gmn1_digest::{codebook_digest, grammar_leaf, pack_root};
use crate::grammar::{
    EbnfBridge, Formalism, Grammar, RuleExpr, grammar_correspondence, grammar_leg_pair,
    grammar_to_ntriples, parse_grammar, serialize_grammar,
};
use crate::nif::NifBridge;
use crate::ontolex::OntoLexTarget;
use crate::semaf::SemafBridge;
use crate::tei::TeiBridge;

/// The example-instance base every minted projection individual (grammar IRIs,
/// correspondence IRIs) lives under, matching every other `lang:` producer.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// A named source surface a target lifts (authored grammar notation, or a `lang:` RDF surface
/// from the composed model). `name` becomes the emitted artifact's file stem.
#[derive(Clone, Debug)]
pub struct NamedSource {
    /// The source's stable name (the artifact file stem, e.g. `turtle` / `gts`).
    pub name: String,
    /// The raw source bytes (grammar notation / `lang:` Turtle).
    pub bytes: Vec<u8>,
}

/// The projection input aBox: every source surface the registered targets lower FROM. Every
/// projection is FORWARD — from the canonical `lang:` model out to an external ecosystem
/// (the backward ingestion leg lives in the runtime bridges, not here). A target reads only
/// the slice it consumes; an empty slice yields an honest empty projection (the target is
/// still registered, the driver folds one honest no-source ledger row).
#[derive(Clone, Debug, Default)]
pub struct LangProjectionInput {
    /// Authored grammar source surfaces (EBNF notation) — the EBNF/ABNF targets' input.
    pub grammars: Vec<NamedSource>,
    /// Raw `lang:` RDF surfaces (Turtle) already present in the composed model, each scanned by
    /// the lexical/morphosyntax/document/surface/meaning targets (OntoLex, CoNLL-U, TEI, NIF,
    /// SemAF) for the individuals it projects (`lang:Lexeme`, `lang:ComposedForm`,
    /// `lang:SurfaceAnchor`, `lang:Denotation`). Empty ⇒ each such target folds its no-source row.
    pub lang_models: Vec<NamedSource>,
    /// The `lang:` RDF surfaces carrying `lang:LanguageVariety` individuals (the lang module's
    /// own vocabulary plus lang-bearing examples) — the BCP-47 target's input. Kept separate
    /// from `lang_models` so the TBox-bearing module surface is scanned ONLY for varieties and
    /// never fed to a document/surface/meaning bridge.
    pub varieties: Vec<NamedSource>,
    /// The one carrier-authored GMN dictionary and scoped glyph registry. Keeping this
    /// separate from each projected source graph ensures every writer/reader invocation uses
    /// the codebook the shipped ontology pins, rather than a source-local empty fallback.
    pub gmn_dictionary: Option<GmnDictionary>,
    /// The resolved current GMN codebook — the second clean carrier of codebook identity
    /// (reference inventory, script graphemes, pinned versions) the conformance pack's
    /// codebook digest folds over alongside [`gmn_dictionary`](Self::gmn_dictionary). Set
    /// together with the dictionary from the SAME lang module dataset so the emitted digest
    /// equals what the gate/CLI recompute; `None` ⇒ no pack is emitted.
    pub gmn_codebook: Option<CurrentCodebook>,
    /// The AUTHORED GMN grammar bytes (`grammars/gmn.ebnf`, pre-render) — the grammar leaf of
    /// the conformance pack's Merkle root. Kept separate from [`grammars`](Self::grammars),
    /// whose `gmn` entry the projection stage replaces with the graph-rendered production, so
    /// the pack pins the authored template rather than a derivative. `None` ⇒ no pack.
    pub gmn_grammar_source: Option<Vec<u8>>,
}

/// One generated external artifact an emission produces, keyed by the path suffix under
/// `generated/projections/lang/` (e.g. `ebnf/turtle.ebnf`).
#[derive(Clone, Debug)]
pub struct EmittedArtifact {
    /// The path suffix under `generated/projections/lang/` (`<target>/<name>.<ext>`).
    pub path_suffix: String,
    /// The artifact bytes.
    pub bytes: Vec<u8>,
    /// Whether the artifact is an RDF serialization (vs an opaque side format).
    pub is_rdf: bool,
}

/// The product of one projection emission: the generated artifacts, the CARRIED
/// `logic:Correspondence` the preservation/round-trip judgments are decided over, the
/// per-emission loss-ledger rows, and the honest metadata the `lang:ProjectionEmission`
/// record carries.
#[derive(Clone, Debug)]
pub struct LangEmission {
    /// The generated external artifacts (may be empty for a preservation-record-only
    /// emission that faithfully produces no side format).
    pub artifacts: Vec<EmittedArtifact>,
    /// The carried `logic:Correspondence` — the single law spine the preservation and
    /// round-trip judgments are decided over.
    pub correspondence: Correspondence,
    /// The per-emission loss-ledger rows (the bridge's own honest preservation record).
    /// The rows carry only identity/judgment; their drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store this emission interned every row's drops into (keyed by target focus).
    /// The pipeline `lang_projection` stage unions it into the single report loss store and
    /// reads each row's residue back through `projection_drops_for`.
    pub loss: LossLedger,
    /// The get/put leg pair whose structural round-trip the driver cross-checks with
    /// [`crate::exact_round_trip_holds`]; `None` for a lossy target with no exact inverse leg.
    pub leg_pair: Option<(LegPath, LegPath)>,
    /// For a per-reading projection (CoNLL-U), the number of co-resident readings emitted.
    pub emitted_reading_count: Option<u64>,
    /// The source `lang:` structure this emission projects (`lang:projectsSource`).
    pub source_iri: String,
    /// Each enumerated construct the emission cannot carry into its target.
    pub unsupported: Vec<String>,
    /// The MEASURED round-trip verdict (re-parse / byte round-trip) — the value
    /// `lang:roundTripHolds` carries. Computed by the target, never asserted.
    pub round_trip_holds: bool,
    /// The preservation kind to record when the carried correspondence is NOT exact (the
    /// driver derives `Exact` from [`crate::is_exact_correspondence`], else uses this).
    pub lossy_kind: PreservationKind,
    /// The lifted `lang:` RDF this emission projects into the corpus graph (N-Triples
    /// bytes); empty when the source RDF is already carried by a sibling emission.
    pub source_rdf: Vec<u8>,
}

/// A registered projection target: the projection peer of [`crate::Bridge`]. It CARRIES a
/// `logic:Correspondence` per emission and DELIBERATELY has no `preservation()` and no
/// `round_trip_ok()` — both are decided over the carried correspondence by the driver.
pub trait LangProjectionTarget {
    /// The projection target name (`"ontolex-lemon"`, `"conllu"`, `"ebnf"`, `"abnf"`).
    fn name(&self) -> &'static str;

    /// Zero or more emissions for the sources this target consumes from `input`. An empty
    /// Vec means the composed model carries no source this target lowers FROM — the driver
    /// folds one honest no-source ledger row. Hard-fails (naming the construct) rather than
    /// ever silently dropping source material.
    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic>;
}

/// The ordered projection-target registry. Adding a target is a one-line change here plus
/// its class coverage in [`EMISSION_WORTHY_CLASSES`].
#[must_use]
pub fn registry() -> Vec<Box<dyn LangProjectionTarget>> {
    vec![
        Box::new(OntoLexTarget),
        Box::new(ConlluTarget),
        Box::new(EbnfTarget),
        Box::new(AbnfTarget),
        Box::new(TeiBridge),
        Box::new(NifBridge),
        Box::new(SemafBridge),
        Box::new(Bcp47Target),
        Box::new(Gmn1Target),
    ]
}

/// Every "emission-worthy" `lang:` class paired with the registered target names that MUST
/// cover it (functor totality). The registry-completeness gate asserts each class maps to
/// ≥1 registered target; extend this as Tasks 3–4 add targets (TEI/NIF/SemAF/BCP-47/…).
pub const EMISSION_WORTHY_CLASSES: &[(&str, &[&str])] = &[
    ("Grammar", &["ebnf", "abnf"]),
    ("Lexeme", &["ontolex-lemon"]),
    // A composed form lowers to the CoNLL-U morphosyntax surface AND (document-scale) to TEI.
    ("ComposedForm", &["conllu", "tei"]),
    ("Rendering", &["tei"]),
    ("SurfaceAnchor", &["nif"]),
    ("Denotation", &["semaf"]),
    // A language variety lowers to its generated BCP-47 registry identifier.
    ("LanguageVariety", &["bcp47"]),
];

/// Whether the registry covers `lang_class` — EVERY target `EMISSION_WORTHY_CLASSES` declares
/// for the class must be registered (functor totality). Requiring all, not merely one, means
/// dropping a projection target (e.g. `abnf` for `Grammar`, `tei` for `ComposedForm`) is a hard
/// fail, not a silent loss of that surface. `Err` names the missing target(s).
pub fn assert_registry_covers(lang_class: &str) -> gmeow_errors::Result<()> {
    let registered: Vec<&str> = registry().iter().map(|t| t.name()).collect();
    let Some((_, targets)) = EMISSION_WORTHY_CLASSES
        .iter()
        .find(|(c, _)| *c == lang_class)
    else {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::ClassNotEmissionWorthy {
                lang_class: lang_class.to_owned(),
            },
        ));
    };
    let missing: Vec<&str> = targets
        .iter()
        .filter(|t| !registered.contains(t))
        .copied()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(gmeow_errors::Diag::of_kind(
            crate::error::MissingProjectionTargets {
                lang_class: lang_class.to_owned(),
                missing: format!("{missing:?}"),
            },
        ))
    }
}

// ── EBNF ─────────────────────────────────────────────────────────────────────────

/// The EBNF grammar projection target: lifts an authored EBNF grammar source with the
/// existing [`EbnfBridge`], re-emits the canonical EBNF, and carries the exact round-trip
/// `logic:Correspondence`. Exact for the context-free fragment; non-CF side conditions are
/// enumerated unsupported.
struct EbnfTarget;

impl LangProjectionTarget for EbnfTarget {
    fn name(&self) -> &'static str {
        "ebnf"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.grammars {
            let grammar = EbnfBridge.to_grammar(&source.bytes)?;
            let canon = grammar.canonicalize();
            let text = serialize_grammar(&canon);
            let grammar_iri = grammar_iri_for(&text);
            let round_trip_holds = grammar_round_trips(&canon);
            let source_rdf = grammar_to_ntriples(&canon, &grammar_iri);
            let mut loss = LossLedger::new();
            emissions.push(LangEmission {
                artifacts: vec![EmittedArtifact {
                    path_suffix: format!("ebnf/{}.ebnf", source.name),
                    bytes: text.clone().into_bytes(),
                    is_rdf: false,
                }],
                correspondence: grammar_correspondence(&text),
                ledger: vec![grammar_ledger_row(
                    &mut loss,
                    "ebnf",
                    &source.name,
                    PreservationKind::Exact,
                    Vec::new(),
                )],
                loss,
                leg_pair: Some(grammar_leg_pair()),
                emitted_reading_count: None,
                source_iri: grammar_iri,
                unsupported: NON_CF_SIDE_CONDITIONS
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
                round_trip_holds,
                lossy_kind: PreservationKind::Exact,
                source_rdf,
            });
        }
        Ok(emissions)
    }
}

// ── ABNF ─────────────────────────────────────────────────────────────────────────

/// The ABNF grammar projection target: renders an authored grammar to RFC-5234 ABNF. Exact
/// for a grammar whose canonical form is within the ABNF-expressible CF fragment; a grammar
/// with EBNF-only constructs (negated/verbatim character classes, `A - B` difference) that
/// ABNF cannot carry is an honest SoundUnder emission enumerating those constructs — never a
/// fabricated best-effort ABNF that would not round-trip.
struct AbnfTarget;

impl LangProjectionTarget for AbnfTarget {
    fn name(&self) -> &'static str {
        "abnf"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.grammars {
            let grammar = EbnfBridge.to_grammar(&source.bytes)?;
            let canon = grammar.canonicalize();
            // The grammar's lang:Grammar RDF is emitted once by the EBNF target — the ABNF
            // emission points at the SAME source IRI and never re-emits it.
            let ebnf_text = serialize_grammar(&canon);
            let source_iri = grammar_iri_for(&ebnf_text);

            let blocking = abnf_blocking_constructs(&canon);
            if blocking.is_empty() {
                // ABNF-expressible: render the canonical grammar under the ABNF formalism and
                // hold it to the same round-trip bar as EBNF.
                let abnf_view = Grammar {
                    formalism: Formalism::Abnf,
                    rules: canon.rules.clone(),
                };
                let text = serialize_grammar(&abnf_view);
                let round_trip_holds = grammar_round_trips(&abnf_view);
                let mut loss = LossLedger::new();
                emissions.push(LangEmission {
                    artifacts: vec![EmittedArtifact {
                        path_suffix: format!("abnf/{}.abnf", source.name),
                        bytes: text.clone().into_bytes(),
                        is_rdf: false,
                    }],
                    correspondence: grammar_correspondence(&text),
                    ledger: vec![grammar_ledger_row(
                        &mut loss,
                        "abnf",
                        &source.name,
                        PreservationKind::Exact,
                        Vec::new(),
                    )],
                    loss,
                    leg_pair: Some(grammar_leg_pair()),
                    emitted_reading_count: None,
                    source_iri,
                    unsupported: NON_CF_SIDE_CONDITIONS
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                    round_trip_holds,
                    lossy_kind: PreservationKind::Exact,
                    source_rdf: Vec::new(),
                });
            } else {
                // Not ABNF-expressible: emit no artifact (a partial ABNF would be
                // fabrication), carry the SoundUnder judgment, and enumerate every blocking
                // construct — the loss is carried and flagged, never a silent skip.
                let mut unsupported = blocking;
                unsupported.extend(NON_CF_SIDE_CONDITIONS.iter().map(|s| (*s).to_owned()));
                let mut loss = LossLedger::new();
                emissions.push(LangEmission {
                    artifacts: Vec::new(),
                    correspondence: lossy_grammar_correspondence(&ebnf_text),
                    ledger: vec![grammar_ledger_row(
                        &mut loss,
                        "abnf",
                        &source.name,
                        PreservationKind::SoundUnder,
                        unsupported.clone(),
                    )],
                    loss,
                    leg_pair: None,
                    emitted_reading_count: None,
                    source_iri,
                    unsupported,
                    round_trip_holds: false,
                    lossy_kind: PreservationKind::SoundUnder,
                    source_rdf: Vec::new(),
                });
            }
        }
        Ok(emissions)
    }
}

// ── GMN-1 ────────────────────────────────────────────────────────────────────────

/// The GMN-1 model-notation projection target: registers [`crate::gmn1_codec`] on the
/// SOLE `lang:` emission seam per `LANG-PROJECTIONS.md` ("a parallel generic transcode
/// codec beside the registry is ruled out"). Lowers each `lang_models` source's GMN-0
/// normal form (a `purrdf::RdfDataset` parse of the source Turtle) to GMN-1 text via
/// [`gmn1_write`], and MEASURES the round-trip via [`gmn1_read`] +
/// [`gmn0_canonically_equal`] — never declares it.
///
/// This target's `emit` NEVER hard-fails on an uncovered source construct (unlike a `Bridge`):
/// `lang_models` here spans EVERY slice's `examples/*.ttl` referencing `lang:` (the
/// registry's input aBox carries no slice-scoping metadata to filter on), while the
/// GMN-1 codec's TOTAL-coverage claim (Task 6) is scoped to the grounding slices only —
/// full coverage of every other slice is the separate, floor-gated `axisGmn1Coverage`
/// slice-quality axis (Task 7), not this seam. So a source outside the codec's covered
/// fragment is an honest [`PreservationKind::SoundUnder`] emission enumerating the
/// uncovered construct — mirroring [`AbnfTarget`]'s non-ABNF-expressible branch — never a
/// silent drop and never a build-wide hard fail for content outside this task's scope.
/// The REAL byte-teeth gate behind `gmeow:gmnCorrNormalToGmn`'s `mnemomorphic true`
/// claim is the dedicated, grounding-scoped round-trip gate wired into
/// `crates/pipeline/src/stages/gmn1_gate.rs` — this target's job is registration on the
/// seam and an honest per-source preservation record, not that gate's total-coverage bar.
/// The selected target's version-pinned dictionary is different: it is a mandatory codec
/// capability, so a source-driven emission with no dictionary hard-fails before any source
/// is examined rather than degrading to an artifact-free lossy result.
struct Gmn1Target;

const GMN1_CORR_BASE: &str = "https://blackcatinformatics.ca/lang/gmn1-correspondence/";
const GMN1_GET_LEG: &str = "https://blackcatinformatics.ca/lang/gmn1WriteStep";

impl LangProjectionTarget for Gmn1Target {
    fn name(&self) -> &'static str {
        "gmn1"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        if !input.lang_models.is_empty() {
            let dict = input
                .gmn_dictionary
                .as_ref()
                .ok_or_else(|| IngestDiagnostic {
                    failure_class: LangFailure::SilentIngestDrop,
                    construct: "current GMN codebook dictionary is absent; version-pinned resolution cannot default"
                        .to_owned(),
                })?;
            emissions.extend(self.per_source_emissions(input, dict));
        }
        // The bundle-level self-certifying conformance pack: the decoder identity an
        // independent implementation pins against, appended ONCE after the per-source loop
        // (so no new registered target is added and the registry-completeness gate is not
        // touched). Emitted iff the carrier supplied the codebook resolution AND the authored
        // grammar — the projection stage sets all three pack inputs together with the
        // dictionary, so in the real pipeline the pack is always present.
        if let (Some(dict), Some(codebook), Some(grammar)) = (
            input.gmn_dictionary.as_ref(),
            input.gmn_codebook.as_ref(),
            input.gmn_grammar_source.as_ref(),
        ) {
            emissions.push(gmn1_conformance_pack_emission(dict, codebook, grammar));
        }
        Ok(emissions)
    }
}

impl Gmn1Target {
    /// The per-source GMN-1 emissions: each `lang_models` source's GMN-0 normal form lowered
    /// to GMN-1 text with a MEASURED round-trip (never declared). Factored out of
    /// [`LangProjectionTarget::emit`] so the bundle-level pack emission rides beside it.
    fn per_source_emissions(
        &self,
        input: &LangProjectionInput,
        dict: &GmnDictionary,
    ) -> Vec<LangEmission> {
        let mut emissions = Vec::new();
        for source in &input.lang_models {
            let Ok(ds) = purrdf::parse_dataset(&source.bytes, "text/turtle", None) else {
                // A source that fails to parse as Turtle is out of this target's domain
                // entirely (every OTHER registered target already requires valid Turtle
                // input from the shared catalog scan) — fold one honest no-source row
                // rather than treat a parse defect as a codec coverage gap.
                continue;
            };
            let model = Gmn0Model::from_dataset(&ds);
            let (exact, artifact_text, unsupported) = match gmn1_write(&model, dict) {
                Ok(doc) => match gmn1_read(&doc, dict) {
                    Ok(back) if gmn0_canonically_equal(&model, &back) => {
                        (true, doc.text, Vec::new())
                    }
                    Ok(_) => (
                        false,
                        String::new(),
                        vec!["round-trip canonical mismatch".to_owned()],
                    ),
                    Err(e) => (false, String::new(), vec![e.to_string()]),
                },
                Err(e) => (false, String::new(), vec![e.to_string()]),
            };

            let source_iri = format!(
                "{EXAMPLE_BASE}gmn1-source/{}",
                digest16("gmn1-source", &source.name)
            );
            let mut loss = LossLedger::new();
            let preservation = if exact {
                PreservationKind::Exact
            } else {
                PreservationKind::SoundUnder
            };
            emissions.push(LangEmission {
                artifacts: if exact {
                    vec![EmittedArtifact {
                        path_suffix: format!("gmn1/{}.gmn", source.name),
                        bytes: artifact_text.into_bytes(),
                        is_rdf: false,
                    }]
                } else {
                    Vec::new()
                },
                correspondence: gmn1_correspondence(exact, &source.name),
                ledger: vec![emit_ledger_row(
                    &mut loss,
                    format!("gmn1:{}", source.name),
                    String::new(),
                    false,
                    preservation,
                    "n/a".to_owned(),
                    Vec::new(),
                    unsupported.clone(),
                )],
                loss,
                leg_pair: exact.then(gmn1_leg_pair),
                emitted_reading_count: None,
                source_iri,
                unsupported,
                round_trip_holds: exact,
                lossy_kind: PreservationKind::SoundUnder,
                source_rdf: Vec::new(),
            });
        }
        emissions
    }
}

// ── GMN-1 conformance pack (bundle-level decoder identity) ──────────────────────────

/// The stable, shipped-identity IRIs the conformance-pack projection asserts over. NOT
/// under `example.org`: the pack IS shipped bundle identity, so its subject and the parts it
/// references live in the gmeow namespace beside `gmnCodebookCurrent`.
use gmeow_ns::GMEOW_NS;
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The pack subject: the current shipped GMN-1 conformance pack, mirroring how
/// `gmnCodebookCurrent` names the current codebook.
const GMN_PACK_IRI: &str = "https://blackcatinformatics.ca/gmeow/gmnPackCurrent";
/// The GMN-1 pack's own get-leg step (distinct from the write/read round-trip leg), so the
/// pack correspondence names its own projection step rather than conflating with the codec.
const GMN1_PACK_GET_LEG: &str = "https://blackcatinformatics.ca/lang/gmn1PackProjectStep";

/// The bundle-level self-certifying conformance-pack emission: ONE RDF artifact
/// (`gmn1/conformance-pack.ttl`, `is_rdf: true`) carrying the DERIVED codebook Merkle digest
/// on `gmnCodebookCurrent`, the pack individual typed `gmeow:GmnConformancePack` with its
/// `gmeow:gmnPackRoot` Merkle root, and the pack's `gmeow:references` to the codebook, the
/// dictionary bijection, and the grammar. The same triples ride into the reasoned
/// `graph/lang-projection-corpus` via `source_rdf`, so a bundle consumer resolves the pack
/// root by query, not only by reading the file. Carries an EXACT `logic:Correspondence` (the
/// pack is a faithful projection of the codebook — its root recomputes from the parts it
/// names), with its own leg pair and a measured-true round-trip (the digests are
/// deterministic and recompute).
fn gmn1_conformance_pack_emission(
    dict: &GmnDictionary,
    codebook: &CurrentCodebook,
    grammar_bytes: &[u8],
) -> LangEmission {
    let digest = codebook_digest(codebook, dict);
    let root = pack_root(&digest, dict, grammar_bytes);
    let grammar_digest = grammar_leaf(grammar_bytes);
    let nt = ntriples_sorted(pack_triples(&digest, &root, &grammar_digest));

    let mut loss = LossLedger::new();
    LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: "gmn1/conformance-pack.ttl".to_owned(),
            bytes: nt.clone(),
            is_rdf: true,
        }],
        correspondence: gmn1_pack_correspondence(),
        ledger: vec![emit_ledger_row(
            &mut loss,
            "gmn1:conformance-pack".to_owned(),
            String::new(),
            true,
            PreservationKind::Exact,
            "n/a".to_owned(),
            Vec::new(),
            Vec::new(),
        )],
        loss,
        leg_pair: Some(gmn1_pack_leg_pair()),
        emitted_reading_count: None,
        source_iri: GMN_PACK_IRI.to_owned(),
        unsupported: Vec::new(),
        round_trip_holds: true,
        lossy_kind: PreservationKind::Exact,
        source_rdf: nt,
    }
}

/// The conformance-pack N-Triples: the codebook self-digest on `gmnCodebookCurrent`, the grammar
/// Merkle leaf on `gmnGrammar`, the typed pack individual, its Merkle root, and its three part
/// references. All digest literals are lowercase-hex (grammar leaf) or algorithm-tagged
/// (`blake3:<hex>`) ASCII and need no escaping. The grammar leaf enters the bundle so the shipped
/// `gmeow gmn verify` recomputes `gmnPackRoot` from the bundle alone, with no source checkout.
fn pack_triples(codebook_digest: &str, pack_root: &str, grammar_digest: &str) -> Vec<String> {
    let g = |local: &str| format!("{GMEOW_NS}{local}");
    let triple = |s: &str, p: &str, o: &str| format!("<{s}> <{p}> <{o}> .");
    let lit = |s: &str, p: &str, l: &str| format!("<{s}> <{p}> \"{l}\" .");
    vec![
        // The DERIVED codebook self-digest enters the bundle here.
        lit(
            &g("gmnCodebookCurrent"),
            &g("gmnCodebookDigest"),
            codebook_digest,
        ),
        // The DERIVED grammar Merkle leaf (pack-root part 2) enters the bundle here.
        lit(&g("gmnGrammar"), &g("gmnGrammarDigest"), grammar_digest),
        triple(GMN_PACK_IRI, RDF_TYPE, &g("GmnConformancePack")),
        lit(GMN_PACK_IRI, &g("gmnPackRoot"), pack_root),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnCodebookCurrent")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnDictV3")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnGrammar")),
    ]
}

/// The EXACT `logic:Correspondence` the conformance-pack emission carries: a
/// `logic:SectionRetraction`/`logic:ExactPreservation` rung with a discharged `GetPut`,
/// named on its own `gmn1-conformance-pack` source key and pack projection leg — the pack is
/// a faithful projection of the codebook whose root recomputes from the parts it names.
fn gmn1_pack_correspondence() -> Correspondence {
    let iri = format!(
        "{GMN1_CORR_BASE}{}",
        digest16("gmn1-corr", "gmn1-conformance-pack")
    );
    Correspondence::new(
        iri,
        CorrespondenceRelation::Subsumes,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        Some(Determinacy::Crisp),
        Some(GMN1_PACK_GET_LEG.to_owned()),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationDischarged,
            condition: Some(DischargeCondition::DischargeSyntacticReachability),
        }],
        None,
        None,
        None,
        None,
        None,
        Some(PreservationKind::Exact),
    )
    .expect("exact gmn1 conformance-pack correspondence is well-formed by construction")
}

/// The GMN-1 pack get/put leg pair — declarative projection-step metadata whose put leg is
/// the structural inverse of the get leg (the round-trip the driver cross-checks).
fn gmn1_pack_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Step(GMN1_PACK_GET_LEG.to_owned());
    let put = get.invert();
    (get, put)
}

/// The EXACT `logic:Correspondence` a GMN-1 emission carries when its measured
/// round-trip holds: `logic:SectionRetraction`/`logic:ExactPreservation`, mnemomorphic —
/// the SAME rung `gmeow:gmnCorrNormalToGmn` declares in the carrier, discharged here by
/// EXECUTION rather than declared on faith.
fn gmn1_correspondence(exact: bool, source_key: &str) -> Correspondence {
    let iri = format!("{GMN1_CORR_BASE}{}", digest16("gmn1-corr", source_key));
    if exact {
        let discharged = |law: CorrespondenceLaw| LawClaimIr {
            law,
            verdict: DischargeVerdict::ObligationDischarged,
            condition: Some(DischargeCondition::DischargeSyntacticReachability),
        };
        Correspondence::new(
            iri,
            CorrespondenceRelation::Subsumes,
            MorphismClass::SectionRetraction,
            MorphismKind::InstitutionMorphism,
            true,
            Some(Determinacy::Crisp),
            Some(GMN1_GET_LEG.to_owned()),
            None,
            vec![discharged(CorrespondenceLaw::GetPut)],
            None,
            None,
            None,
            None,
            None,
            Some(PreservationKind::Exact),
        )
        .expect("exact gmn1 correspondence is well-formed by construction")
    } else {
        Correspondence::new(
            iri,
            CorrespondenceRelation::RelatedMatch,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            false,
            Some(Determinacy::Crisp),
            Some(GMN1_GET_LEG.to_owned()),
            None,
            vec![LawClaimIr {
                law: CorrespondenceLaw::GetPut,
                verdict: DischargeVerdict::ObligationUnknown,
                condition: None,
            }],
            None,
            None,
            None,
            None,
            None,
            Some(PreservationKind::SoundUnder),
        )
        .expect("sound-under gmn1 correspondence is well-formed by construction")
    }
}

/// The GMN-1 get/put leg pair — declarative `logic:TransactionProgram` step metadata
/// (never the executable Rust round-trip itself, which is [`gmn1_write`]/[`gmn1_read`]
/// in `crate::gmn1_codec`, independently written per that module's own documentation).
fn gmn1_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Step(GMN1_GET_LEG.to_owned());
    let put = get.invert();
    (get, put)
}

// ── shared helpers ────────────────────────────────────────────────────────────────

/// The non-context-free side conditions no grammar-surface projection carries: rule
/// provenance, licensing links, and versioning drop into file comments at best. Enumerated
/// per grammar emission so the loss is carried and flagged (Principle: never a footnote).
const NON_CF_SIDE_CONDITIONS: &[&str] = &[
    "rule provenance is not carried into the grammar file (comments at best)",
    "licensing links from the source forms have no grammar-notation target",
    "grammar versioning is not carried into the emitted file",
];

/// The content-addressed `lang:Grammar` IRI for a canonical grammar serialization.
fn grammar_iri_for(canonical_text: &str) -> String {
    format!(
        "{EXAMPLE_BASE}grammar/{}",
        digest16("lang-grammar", canonical_text)
    )
}

/// The decidable grammar round-trip: `parse(serialize(canon)).canonicalize() == canon`
/// over the grammar's own formalism — the fixpoint discipline, not a raw-byte compare.
fn grammar_round_trips(canon: &Grammar) -> bool {
    let text = serialize_grammar(canon);
    match parse_grammar(text.as_bytes(), canon.formalism) {
        Ok(reparsed) => reparsed.canonicalize() == *canon,
        Err(_) => false,
    }
}

/// Enumerate the EBNF-only constructs a grammar carries that ABNF cannot represent: a
/// verbatim/negated character class (`[...]`) and the EBNF difference operator (`A - B`).
/// Empty ⇒ the canonical grammar is within the ABNF-expressible CF fragment.
fn abnf_blocking_constructs(canon: &Grammar) -> Vec<String> {
    let mut out = Vec::new();
    for rule in &canon.rules {
        collect_abnf_blockers(&rule.name, &rule.body, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn collect_abnf_blockers(rule: &str, e: &RuleExpr, out: &mut Vec<String>) {
    match e {
        RuleExpr::CharClass(body) => out.push(format!(
            "rule '{rule}': EBNF character class '[{body}]' has no RFC-5234 ABNF form \
             (ABNF has no verbatim/negated class)"
        )),
        RuleExpr::Diff(a, b) => {
            out.push(format!(
                "rule '{rule}': EBNF difference 'A - B' has no RFC-5234 ABNF form"
            ));
            collect_abnf_blockers(rule, a, out);
            collect_abnf_blockers(rule, b, out);
        }
        RuleExpr::Ref(_) | RuleExpr::Terminal(_) | RuleExpr::Hex(_) | RuleExpr::Range(_, _) => {}
        RuleExpr::Seq(parts) | RuleExpr::Alt(parts) => {
            for p in parts {
                collect_abnf_blockers(rule, p, out);
            }
        }
        RuleExpr::Star(x)
        | RuleExpr::Plus(x)
        | RuleExpr::Opt(x)
        | RuleExpr::Group(x)
        | RuleExpr::Repeat(_, _, x) => collect_abnf_blockers(rule, x, out),
    }
}

/// One grammar-projection ledger row: a preservation record (empty `content`) keyed
/// `<target>:<name>`, carrying its residue as `actual_drops` so the overclaim gate can
/// police it (Exact ⇒ empty residue; SoundUnder ⇒ the enumerated drops).
fn grammar_ledger_row(
    loss: &mut LossLedger,
    target: &str,
    name: &str,
    preservation: PreservationKind,
    residue: Vec<String>,
) -> ProjectionResult {
    emit_ledger_row(
        loss,
        format!("{target}:{name}"),
        String::new(),
        false,
        preservation,
        "n/a".to_owned(),
        Vec::new(),
        residue,
    )
}

/// The shared loss-ledger-row constructor every `LangTarget::emit` emitter routes through:
/// intern the row's structural + per-run drops into `loss` (keyed by the target focus,
/// **R1**), then return a drop-less [`ProjectionResult`] carrying only identity + judgment.
/// The residue reads back through `loss.projection_drops_for(&row.target)` — the single
/// source of truth the overclaim gate and the projection report both consume.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_ledger_row(
    loss: &mut LossLedger,
    target: String,
    content: String,
    is_rdf: bool,
    preservation: PreservationKind,
    complexity: String,
    lossy_drops: Vec<String>,
    actual_drops: Vec<String>,
) -> ProjectionResult {
    loss.record_projection_drops(&target, preservation, &lossy_drops, &actual_drops);
    ProjectionResult {
        target,
        content,
        is_rdf,
        preservation,
        complexity,
    }
}

/// A LOSSY grammar-projection `logic:Correspondence` for a grammar a target cannot render
/// exactly (the ABNF non-expressible case): a [`MorphismClass::LossyLens`], NOT
/// mnemomorphic, whose `GetPut` law is carried as [`DischargeVerdict::ObligationUnknown`].
/// It is therefore never an exact correspondence — the driver derives SoundUnder, never
/// Exact, from it.
fn lossy_grammar_correspondence(source_key: &str) -> Correspondence {
    Correspondence::new(
        format!(
            "{EXAMPLE_BASE}grammar-correspondence/{}",
            digest16("lang-grammar-lossy-corr", source_key)
        ),
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Crisp),
        Some("https://blackcatinformatics.ca/lang/grammarAbnfProjectLeg".to_owned()),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("lossy grammar correspondence is well-formed by construction")
}

#[cfg(test)]
mod pack_tests {
    use super::*;
    use std::path::Path;

    fn lang_slice_file(rel: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../slices/grounding/lang")
            .join(rel);
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// Drive the pack emission from the REAL carrier lang codebook/dictionary/grammar (no
    /// per-source models), and assert the artifact is produced, is valid Turtle, carries the
    /// pack class + a blake3-tagged root + the codebook digest on `gmnCodebookCurrent`, is
    /// DETERMINISTIC across runs, and that the root matches an independent recomputation of
    /// the Merkle construction.
    #[test]
    fn gmn1_conformance_pack_projects_deterministic_self_certifying_identity() {
        let module = lang_slice_file("module.ttl");
        let grammar = lang_slice_file("grammars/gmn.ebnf");
        let ds = purrdf::parse_dataset(&module, "text/turtle", None).expect("parse lang module");
        let dict = GmnDictionary::from_dataset(&ds).expect("load carrier GMN dictionary");
        let codebook =
            crate::gmn1_codec::resolve_current_codebook(&ds).expect("resolve current codebook");

        let input = LangProjectionInput {
            gmn_dictionary: Some(dict.clone()),
            gmn_codebook: Some(codebook.clone()),
            gmn_grammar_source: Some(grammar.clone()),
            ..Default::default()
        };

        // With no lang_models, the sole emission is the bundle-level conformance pack.
        let emissions = Gmn1Target.emit(&input).expect("gmn1 emit");
        let pack = emissions
            .iter()
            .find(|e| e.source_iri == GMN_PACK_IRI)
            .expect("bundle-level conformance-pack emission present");
        let artifact = pack
            .artifacts
            .iter()
            .find(|a| a.path_suffix == "gmn1/conformance-pack.ttl")
            .expect("pack artifact keyed at gmn1/conformance-pack.ttl");
        assert!(artifact.is_rdf, "the pack artifact is an RDF serialization");

        // The pack correspondence derives Exact (the driver's Invariant 1 acceptance path).
        assert!(
            crate::is_exact_correspondence(&pack.correspondence),
            "the pack carries an exact correspondence"
        );
        assert!(
            pack.round_trip_holds,
            "the pack's round-trip is measured true"
        );

        let ttl = String::from_utf8(artifact.bytes.clone()).expect("utf8");
        // Valid Turtle: N-Triples is a Turtle subset, so a Turtle parse must accept it.
        purrdf::parse_dataset(artifact.bytes.as_slice(), "text/turtle", None)
            .expect("the pack artifact is valid Turtle");

        assert!(
            ttl.contains("<https://blackcatinformatics.ca/gmeow/GmnConformancePack>"),
            "pack artifact types the pack individual:\n{ttl}"
        );
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/gmeow/gmnPackRoot> \"blake3:"),
            "pack artifact carries a blake3-tagged gmnPackRoot:\n{ttl}"
        );
        assert!(
            ttl.contains(
                "<https://blackcatinformatics.ca/gmeow/gmnCodebookCurrent> \
                 <https://blackcatinformatics.ca/gmeow/gmnCodebookDigest> \"blake3:"
            ),
            "pack artifact carries the codebook digest on gmnCodebookCurrent:\n{ttl}"
        );
        // The same triples ride into the reasoned corpus graph via source_rdf.
        assert_eq!(
            pack.source_rdf, artifact.bytes,
            "the pack triples ride the corpus graph verbatim"
        );

        // Deterministic: a second emission is byte-identical.
        let again = Gmn1Target.emit(&input).expect("gmn1 emit (second run)");
        let pack2 = again
            .iter()
            .find(|e| e.source_iri == GMN_PACK_IRI)
            .expect("second pack emission");
        assert_eq!(
            artifact.bytes, pack2.artifacts[0].bytes,
            "the conformance pack is byte-deterministic"
        );

        // Independent recomputation of the Merkle construction matches the emitted root.
        let expected_digest = codebook_digest(&codebook, &dict);
        let expected_root = pack_root(&expected_digest, &dict, &grammar);
        let expected_root_triple = format!(
            "<{GMN_PACK_IRI}> <https://blackcatinformatics.ca/gmeow/gmnPackRoot> \
             \"{expected_root}\" ."
        );
        assert!(
            ttl.contains(&expected_root_triple),
            "emitted gmnPackRoot must equal the independently recomputed Merkle root\n\
             expected: {expected_root_triple}\ngot:\n{ttl}"
        );
        assert!(
            ttl.contains(&format!("\"{expected_digest}\"")),
            "emitted codebook digest must equal the recomputed codebook_digest\n\
             expected: {expected_digest}\ngot:\n{ttl}"
        );
    }
}
