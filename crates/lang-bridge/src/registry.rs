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
use crate::gmn_metrics::{TokenMetrics, compute_token_metrics};
use crate::gmn_migrate::tag_schema_version;
use crate::gmn1_codec::{
    CurrentCodebook, Gmn0Model, GmnDictionary, gmn0_canonically_equal, gmn1_read, gmn1_write,
};
use crate::gmn1_digest::{EcosystemLeaves, codebook_digest, grammar_leaf, pack_root};
use crate::grammar::{
    EbnfBridge, Formalism, Grammar, RuleExpr, gbnf_blocking_constructs, grammar_correspondence,
    grammar_leg_pair, grammar_to_ntriples, lark_blocking_constructs, parse_grammar,
    serialize_grammar,
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
    /// The graph-resolved latest GMN dialect major (e.g. `"1"`), read from the version
    /// lineage (`gmeow:gmnDialectVersions` `gmeow:roleLatest` → `owl:versionInfo`) — NEVER a
    /// Rust constant. Every emitted GMN artifact path is keyed `gmn1/v<major>/…` under it, so
    /// a lineage major bump regenerates the whole versioned subtree atomically. `None` ⇒ no
    /// GMN emission (mirrors [`gmn_dictionary`](Self::gmn_dictionary)).
    pub gmn_dialect_major: Option<String>,
    /// The resolved verbalizable GMN operator forms — each executable operator glyph joined
    /// to its `(fixity, arity)` signature and its denotation target's `rdfs:label` (the
    /// controlled-NL nucleus). Resolved by the pipeline (`collect_input`) from the carrier
    /// glyph registry plus the cross-slice label index, so the bundle-level verbalizer
    /// emission consumes a graph-resolved inventory rather than re-deriving it. Empty ⇒ no
    /// verbalizer emission (mirrors an empty corpus for the pack/metrics products).
    pub gmn_operator_forms: Vec<crate::gmn_verbalize::GmnOperatorForm>,
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
        Box::new(GbnfTarget),
        Box::new(LarkTarget),
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
    ("Grammar", &["ebnf", "abnf", "gbnf", "lark"]),
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

// ── GBNF / Lark (GMN constrained-decode surfaces) ──────────────────────────────────

/// The GBNF grammar projection target (llama.cpp constrained-decode notation): renders the
/// GRAPH-DERIVED GMN glyph grammar — the SAME render-substituted `gmn` bytes the EBNF/ABNF
/// targets lift from `input.grammars` — into GBNF. GBNF is a GMN-ecosystem surface, so its
/// artifact lives under the version-keyed `gmn1/v<major>/gbnf/…` subtree, never the flat
/// `gbnf/…` path the EBNF/ABNF targets use. A grammar carrying a construct GBNF cannot round-trip
/// (set-difference, a bare hex / numeric-range terminal, a bounded repetition, or left-recursion —
/// see [`gbnf_blocking_constructs`]) is an honest [`PreservationKind::SoundUnder`] emission
/// enumerating those constructs and emitting NO artifact, mirroring [`AbnfTarget`]'s blocking
/// branch — never a fabricated best-effort GBNF that would not round-trip.
struct GbnfTarget;

impl LangProjectionTarget for GbnfTarget {
    fn name(&self) -> &'static str {
        "gbnf"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        gmn_glyph_grammar_emissions(
            input,
            Formalism::Gbnf,
            "gbnf",
            "gbnf",
            gbnf_blocking_constructs,
        )
    }
}

/// The Lark grammar projection target (Earley/LALR constrained-parse notation): renders the same
/// graph-derived GMN glyph grammar into Lark. Its artifact lives under `gmn1/v<major>/lark/…`.
/// Lark's blocking set is narrower than GBNF's (it expresses character classes natively as `/[…]/`
/// regex terminals and its Earley core handles left-recursion, so neither is a blocker — see
/// [`lark_blocking_constructs`]); the remaining blockers (set-difference, a bare hex /
/// numeric-range terminal, a bounded repetition) are handled with the SAME honest SoundUnder-no-
/// artifact discipline as [`GbnfTarget`] and [`AbnfTarget`].
struct LarkTarget;

impl LangProjectionTarget for LarkTarget {
    fn name(&self) -> &'static str {
        "lark"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        gmn_glyph_grammar_emissions(
            input,
            Formalism::Lark,
            "lark",
            "lark",
            lark_blocking_constructs,
        )
    }
}

/// Lower the GRAPH-DERIVED GMN glyph grammar (the `gmn` entry of `input.grammars`, whose bytes
/// `collect_input` already render-substituted from the executable glyph registry) into a
/// GMN-ecosystem grammar surface (`formalism`), keyed under the version subtree
/// `gmn1/v<major>/<target>/<name>.<ext>`. Parses the shared bytes ONCE to the single canonical
/// [`RuleExpr`] tree (never a second glyph→grammar render path), then serializes in `formalism`.
///
/// Scoped to the `gmn` grammar ALONE (GBNF/Lark are GMN constrained-decode surfaces, not general
/// grammar projections like EBNF/ABNF): an input with no `gmn` grammar yields an honest empty Vec
/// (the driver folds one no-source row). The version major is a mandatory capability for the
/// keyed path — its absence with a `gmn` grammar present is a HARD FAIL (no-optionality), mirroring
/// [`Gmn1Target`], never a constant default.
///
/// The `gmn` grammar's `lang:Grammar` RDF is emitted ONCE by the EBNF target; this emission points
/// at the SAME content-addressed source IRI (derived from the canonical EBNF serialization) and
/// never re-emits it (`source_rdf` empty), exactly like [`AbnfTarget`]. A grammar with a
/// `blocking`-set construct emits NO artifact and carries the SoundUnder judgment enumerating every
/// blocker — never a fabricated partial rendering.
fn gmn_glyph_grammar_emissions(
    input: &LangProjectionInput,
    formalism: Formalism,
    target: &str,
    ext: &str,
    blocking_constructs: impl Fn(&Grammar) -> Vec<String>,
) -> Result<Vec<LangEmission>, IngestDiagnostic> {
    let Some(source) = input.grammars.iter().find(|g| g.name == "gmn") else {
        // No GMN glyph grammar in the composed model: honest no-source (the driver folds the row).
        return Ok(Vec::new());
    };
    // The version major keys the artifact path; like the GMN-1 codec's dictionary it is a
    // mandatory capability — a gmn-grammar-driven emission with no resolved major hard-fails
    // rather than defaulting to a constant.
    let major = input
        .gmn_dialect_major
        .as_deref()
        .ok_or_else(|| IngestDiagnostic {
            failure_class: LangFailure::SilentIngestDrop,
            construct: format!(
                "resolved GMN dialect major is absent; the version-keyed {target} grammar-surface \
                 path cannot default"
            ),
        })?;

    let grammar = EbnfBridge.to_grammar(&source.bytes)?;
    let canon = grammar.canonicalize();
    // The gmn grammar's lang:Grammar RDF is emitted once by the EBNF target — this emission
    // points at the SAME source IRI (derived from the canonical EBNF serialization) and never
    // re-emits it.
    let ebnf_text = serialize_grammar(&canon);
    let source_iri = grammar_iri_for(&ebnf_text);

    let blocking = blocking_constructs(&canon);
    if blocking.is_empty() {
        // Representable: render the ONE canonical tree under `formalism` and hold it to the same
        // round-trip bar as EBNF.
        let view = Grammar {
            formalism,
            rules: canon.rules.clone(),
        };
        let text = serialize_grammar(&view);
        let round_trip_holds = grammar_round_trips(&view);
        let mut loss = LossLedger::new();
        Ok(vec![LangEmission {
            artifacts: vec![EmittedArtifact {
                path_suffix: format!("gmn1/v{major}/{target}/{}.{ext}", source.name),
                bytes: text.clone().into_bytes(),
                is_rdf: false,
            }],
            correspondence: grammar_correspondence(&text),
            ledger: vec![grammar_ledger_row(
                &mut loss,
                target,
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
        }])
    } else {
        // Not representable: emit no artifact (a partial rendering would be fabrication), carry
        // the SoundUnder judgment, and enumerate every blocking construct.
        let mut unsupported = blocking;
        unsupported.extend(NON_CF_SIDE_CONDITIONS.iter().map(|s| (*s).to_owned()));
        let mut loss = LossLedger::new();
        Ok(vec![LangEmission {
            artifacts: Vec::new(),
            correspondence: lossy_grammar_correspondence(&ebnf_text),
            ledger: vec![grammar_ledger_row(
                &mut loss,
                target,
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
        }])
    }
}

/// The EMITTED artifact bytes of a GMN ecosystem grammar view (gbnf/lark), recomputed from the
/// SAME [`gmn_glyph_grammar_emissions`] path the [`GbnfTarget`]/[`LarkTarget`] emit — so the
/// conformance pack folds a leaf over byte-identical content. Empty when no artifact is emitted
/// (no `gmn` grammar in the model, or a blocking-construct grammar): a stable empty leaf, never a
/// fabricated surface. Deterministic — a pure function of `input`.
fn grammar_view_bytes(
    input: &LangProjectionInput,
    formalism: Formalism,
    target: &str,
    ext: &str,
    blocking_constructs: impl Fn(&Grammar) -> Vec<String>,
) -> Result<Vec<u8>, IngestDiagnostic> {
    let emissions =
        gmn_glyph_grammar_emissions(input, formalism, target, ext, blocking_constructs)?;
    Ok(emissions
        .into_iter()
        .flat_map(|e| e.artifacts.into_iter())
        .map(|a| a.bytes)
        .next()
        .unwrap_or_default())
}

/// The first emitted artifact's bytes of an OPTIONAL bundle-level emission (token-metrics /
/// verbalizer), or empty when the emission is absent — the content the pack folds a leaf over.
fn emission_artifact_bytes(emission: Option<&LangEmission>) -> Vec<u8> {
    emission
        .and_then(|e| e.artifacts.first())
        .map(|a| a.bytes.clone())
        .unwrap_or_default()
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
            // The graph-resolved dialect major keys every emitted artifact path. It is a
            // mandatory codec capability like the dictionary — a source-driven emission with
            // no resolved major hard-fails rather than defaulting to a constant.
            let major = input
                .gmn_dialect_major
                .as_deref()
                .ok_or_else(|| IngestDiagnostic {
                    failure_class: LangFailure::SilentIngestDrop,
                    construct: "resolved GMN dialect major is absent; version-keyed artifact paths cannot default"
                        .to_owned(),
                })?;
            emissions.extend(self.per_source_emissions(input, dict, major));
        }
        // The two fallible bundle-level ecosystem products (token-metrics + verbalizer) are
        // computed BEFORE the pack so the pack folds a content-addressed leaf over each one's
        // EMITTED bytes (the whole-ecosystem pack root, Task 15). Both are Options: a missing
        // corpus / operator inventory yields None (no vacuous product), which folds as an empty
        // leaf. The token-metric gate is the flagship compression claim's teeth — a corpus where
        // GMN's byte-fallback worst case does NOT beat Turtle's best case HARD-FAILS. A
        // non-injective / non-round-tripping verbalization likewise HARD-FAILS.
        let token_metrics_emission = if let (Some(dict), Some(major)) = (
            input.gmn_dictionary.as_ref(),
            input.gmn_dialect_major.as_deref(),
        ) {
            gmn1_token_metrics_emission(&input.lang_models, dict, major)?
        } else {
            None
        };
        let verbalizer_emission = if let (Some(dict), Some(major)) = (
            input.gmn_dictionary.as_ref(),
            input.gmn_dialect_major.as_deref(),
        ) {
            gmn1_verbalizer_emission(&input.gmn_operator_forms, dict, major)?
        } else {
            None
        };

        // The bundle-level self-certifying conformance pack: the decoder identity an
        // independent implementation pins against, appended ONCE after the per-source loop
        // (so no new registered target is added and the registry-completeness gate is not
        // touched). Emitted iff the carrier supplied the codebook resolution AND the authored
        // grammar — the projection stage sets all pack inputs together with the dictionary, so
        // in the real pipeline the pack is always present. Its Merkle root now folds a
        // content-addressed leaf over EVERY ecosystem surface (the GBNF + Lark grammar
        // artifacts, the token-metrics measurement, and the verbalizations) beside the existing
        // codebook / grammar / sigil leaves, so the pack certifies the whole ecosystem from the
        // bundle alone and any perturbed surface changes the root (Task 15).
        if let (Some(dict), Some(codebook), Some(grammar), Some(major)) = (
            input.gmn_dictionary.as_ref(),
            input.gmn_codebook.as_ref(),
            input.gmn_grammar_source.as_ref(),
            input.gmn_dialect_major.as_deref(),
        ) {
            let gbnf_bytes = grammar_view_bytes(
                input,
                Formalism::Gbnf,
                "gbnf",
                "gbnf",
                gbnf_blocking_constructs,
            )?;
            let lark_bytes = grammar_view_bytes(
                input,
                Formalism::Lark,
                "lark",
                "lark",
                lark_blocking_constructs,
            )?;
            let ecosystem = EcosystemLeaves::from_view_bytes(
                &gbnf_bytes,
                &lark_bytes,
                &emission_artifact_bytes(token_metrics_emission.as_ref()),
                &emission_artifact_bytes(verbalizer_emission.as_ref()),
            );
            emissions.push(gmn1_conformance_pack_emission(
                dict, codebook, grammar, major, &ecosystem,
            ));
        }
        // The two ecosystem products ride into the bundle AFTER the pack (same emission order as
        // before), now that the pack has folded their bytes.
        if let Some(emission) = token_metrics_emission {
            emissions.push(emission);
        }
        if let Some(emission) = verbalizer_emission {
            emissions.push(emission);
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
        major: &str,
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
                        path_suffix: format!("gmn1/v{major}/{}.gmn", source.name),
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
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The pack subject: the current shipped GMN-1 conformance pack, mirroring how
/// `gmnCodebookCurrent` names the current codebook.
const GMN_PACK_IRI: &str = "https://blackcatinformatics.ca/gmeow/gmnPackCurrent";
/// The GMN-1 pack's own get-leg step (distinct from the write/read round-trip leg), so the
/// pack correspondence names its own projection step rather than conflating with the codec.
const GMN1_PACK_GET_LEG: &str = "https://blackcatinformatics.ca/lang/gmn1PackProjectStep";

/// The bundle-level self-certifying conformance-pack emission: ONE RDF artifact
/// (`gmn1/v<major>/conformance-pack.ttl`, `is_rdf: true`) carrying the DERIVED codebook Merkle digest
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
    major: &str,
    ecosystem: &EcosystemLeaves,
) -> LangEmission {
    let digest = codebook_digest(codebook, dict);
    let root = pack_root(&digest, dict, grammar_bytes, ecosystem);
    let grammar_digest = grammar_leaf(grammar_bytes);
    let nt = ntriples_sorted(pack_triples(&digest, &root, &grammar_digest, ecosystem));

    let mut loss = LossLedger::new();
    LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: format!("gmn1/v{major}/conformance-pack.ttl"),
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
/// Merkle leaf on `gmnGrammar`, the FOUR ecosystem-view Merkle leaves (gbnf / lark / token-metrics /
/// verbalizations, each on its own subject), the typed pack individual, its Merkle root, and its
/// seven part references. All digest literals are lowercase-hex (the view leaves) or algorithm-tagged
/// (`blake3:<hex>`) ASCII and need no escaping. Every leaf enters the bundle so the shipped
/// `gmeow gmn verify` recomputes `gmnPackRoot` from the bundle alone, with no source checkout —
/// certifying the WHOLE GMN ecosystem, tamper-evident (Task 15).
fn pack_triples(
    codebook_digest: &str,
    pack_root: &str,
    grammar_digest: &str,
    ecosystem: &EcosystemLeaves,
) -> Vec<String> {
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
        // The DERIVED ecosystem-view Merkle leaves (pack-root parts 4–7) enter the bundle here,
        // each on the subject that names its surface, so `gmeow gmn verify` reads each pinned leaf
        // by `<subject> <predicate>` (never a predicate-only scan that other envelopes could shadow).
        lit(&g("gmnGbnf"), &g("gmnGbnfDigest"), &ecosystem.gbnf),
        lit(&g("gmnLark"), &g("gmnLarkDigest"), &ecosystem.lark),
        lit(
            &g("gmnTokenMetricsCurrent"),
            &g("gmnTokenMetricsDigest"),
            &ecosystem.token_metrics,
        ),
        lit(
            &g("gmnVerbalizationsCurrent"),
            &g("gmnVerbalizationsDigest"),
            &ecosystem.verbalizations,
        ),
        triple(GMN_PACK_IRI, RDF_TYPE, &g("GmnConformancePack")),
        lit(GMN_PACK_IRI, &g("gmnPackRoot"), pack_root),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnCodebookCurrent")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnDictV3")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnGrammar")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnGbnf")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnLark")),
        triple(GMN_PACK_IRI, &g("references"), &g("gmnTokenMetricsCurrent")),
        triple(
            GMN_PACK_IRI,
            &g("references"),
            &g("gmnVerbalizationsCurrent"),
        ),
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

// ── GMN-1 token metrics (bundle-level math-grounded measurement) ────────────────────

/// The `math:` grounding vocabulary the measurement product reuses (never a competing
/// grounding — the metric magnitudes are dimensionless `math:Quantity` individuals, exactly
/// like the DECLARED rate `gmeow:gmnRateTokensPerStatement` they measure against).
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// The measurement subject: the current shipped GMN-1 token-metric vector, mirroring how
/// `gmnCodebookCurrent` / `gmnPackCurrent` name the current codebook / pack.
const GMN_METRICS_IRI: &str = "https://blackcatinformatics.ca/gmeow/gmnTokenMetricsCurrent";
/// The token-metrics projection get-leg step (distinct from the codec and pack legs).
const GMN1_METRICS_GET_LEG: &str = "https://blackcatinformatics.ca/lang/gmn1MetricsProjectStep";

/// The bundle-level token-metric measurement emission: ONE RDF artifact
/// (`gmn1/v<major>/token-metrics.ttl`, `is_rdf: true`) carrying the DERIVED 7-metric vector
/// over the grounding corpus as a `gmeow:Measurement` observation whose results are
/// dimensionless `math:Quantity` magnitudes — the MEASURED realization of the code the
/// codebook's `gmeow:gmnDeclaredRate` frames as its expectation. The same triples ride into
/// the reasoned `graph/lang-projection-corpus` via `source_rdf`, so a consumer resolves the
/// metrics by query, not only by reading the file. Carries an EXACT `logic:Correspondence`
/// (the metrics recompute deterministically from the corpus + codebook) with a measured-true
/// round-trip.
///
/// Fallible and gated: `None` when the corpus has no round-tripping source (nothing to
/// measure — no vacuous product), and a HARD FAIL when the SOUND compression gate does not
/// hold (`gmn_worst_case_tokens` not `< turtle_best_case_tokens`). The gate is the flagship
/// claim's teeth: a corpus where GMN's byte-fallback worst case fails to beat Turtle's best
/// case reds the projection rather than shipping a false "GMN < Turtle" claim.
fn gmn1_token_metrics_emission(
    sources: &[NamedSource],
    dict: &GmnDictionary,
    major: &str,
) -> Result<Option<LangEmission>, IngestDiagnostic> {
    let metrics = compute_token_metrics(sources, dict);
    if metrics.measured_sources == 0 {
        // No source round-trips ⇒ no GMN artifact ships ⇒ no corpus to measure. Emit nothing
        // rather than a vacuous all-zero product (mirrors the pack's no-input branch).
        return Ok(None);
    }
    if !metrics.compression_gate_holds() {
        return Err(IngestDiagnostic {
            failure_class: LangFailure::SilentIngestDrop,
            construct: format!(
                "GMN compression gate FAILED over the grounding corpus: the sound byte-fallback \
                 worst case gmn_worst_case_tokens={} (= ceil(ascii={}/4) + nonascii={}) is not \
                 strictly less than turtle_best_case_tokens={}; the flagship 'GMN costs fewer \
                 tokens than Turtle' claim must not ship false — scope the claim or revisit the \
                 encoding, never weaken the gate",
                metrics.gmn_worst_case_tokens,
                metrics.gmn_ascii_bytes,
                metrics.gmn_nonascii_bytes,
                metrics.turtle_best_case_tokens,
            ),
        });
    }
    let nt = ntriples_sorted(token_metrics_triples(&metrics, dict));
    let mut loss = LossLedger::new();
    Ok(Some(LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: format!("gmn1/v{major}/token-metrics.ttl"),
            bytes: nt.clone(),
            is_rdf: true,
        }],
        correspondence: gmn1_metrics_correspondence(),
        ledger: vec![emit_ledger_row(
            &mut loss,
            "gmn1:token-metrics".to_owned(),
            String::new(),
            true,
            PreservationKind::Exact,
            "n/a".to_owned(),
            Vec::new(),
            Vec::new(),
        )],
        loss,
        leg_pair: Some(gmn1_metrics_leg_pair()),
        emitted_reading_count: None,
        source_iri: GMN_METRICS_IRI.to_owned(),
        unsupported: Vec::new(),
        round_trip_holds: true,
        lossy_kind: PreservationKind::Exact,
        source_rdf: nt,
    }))
}

/// The token-metrics N-Triples: the `gmeow:Measurement` observation subject bound to the
/// GMN codebook (`gmeow:vantage`, the coding-theoretic frame the realized rate is measured
/// from) and the GMN-1 notation (`gmeow:observedFeature`, the surface whose encoding is
/// measured), each metric a dimensionless `math:Quantity` result reached through
/// `gmeow:observationResult`, plus the `tag_schema_version` provenance stamp. Every value is
/// a datatyped literal; the whole set is sorted + deduped by [`ntriples_sorted`], so two
/// runs over the same corpus + codebook serialize byte-identically.
fn token_metrics_triples(metrics: &TokenMetrics, dict: &GmnDictionary) -> Vec<String> {
    let g = |local: &str| format!("{GMEOW_NS}{local}");
    let m = |local: &str| format!("{MATH_NS}{local}");
    let triple = |s: &str, p: &str, o: &str| format!("<{s}> <{p}> <{o}> .");
    let typed = |s: &str, p: &str, lex: &str, dt: &str| format!("<{s}> <{p}> \"{lex}\"^^<{dt}> .");
    let label = |s: &str, l: &str| format!("<{s}> <{RDFS_LABEL}> \"{l}\" .");

    // The seven-metric vector plus the compression-gate witnesses, each an integer count or a
    // `[0,1]` ratio. The `is_int` flag picks integer vs fixed-6-place decimal formatting so the
    // literal is byte-stable (never a locale- or precision-varying float rendering).
    let vector: &[(&str, f64, bool)] = &[
        ("bytes_on_disk", metrics.bytes_on_disk as f64, true),
        ("tokens_in_context", metrics.tokens_in_context as f64, true),
        ("ast_validity_rate", metrics.ast_validity_rate, false),
        ("roundtrip_loss", metrics.roundtrip_loss, false),
        ("compression_ratio", metrics.compression_ratio, false),
        ("glyph_density", metrics.glyph_density, false),
        ("dictionary_hit_rate", metrics.dictionary_hit_rate, false),
        // Compression-gate witnesses (shipped beside the vector as auditable data).
        (
            "gmn_worst_case_tokens",
            metrics.gmn_worst_case_tokens as f64,
            true,
        ),
        (
            "gmn_realistic_tokens",
            metrics.gmn_realistic_tokens as f64,
            true,
        ),
        (
            "turtle_best_case_tokens",
            metrics.turtle_best_case_tokens as f64,
            true,
        ),
        ("gmn_ascii_bytes", metrics.gmn_ascii_bytes as f64, true),
        (
            "gmn_nonascii_bytes",
            metrics.gmn_nonascii_bytes as f64,
            true,
        ),
        (
            "turtle_bytes_on_disk",
            metrics.turtle_bytes_on_disk as f64,
            true,
        ),
        (
            "jsonld_bytes_on_disk",
            metrics.jsonld_bytes_on_disk as f64,
            true,
        ),
    ];

    let mut out = Vec::new();
    // The observation subject: a gmeow:Measurement bound to its vantage + observed feature.
    out.push(triple(GMN_METRICS_IRI, RDF_TYPE, &g("Measurement")));
    out.push(triple(
        GMN_METRICS_IRI,
        &g("vantage"),
        &g("gmnCodebookCurrent"),
    ));
    out.push(triple(
        GMN_METRICS_IRI,
        &g("observedFeature"),
        &g("gmnModelNotation"),
    ));
    for (name, value, is_int) in vector {
        let metric_iri = format!("{GMN_METRICS_IRI}/{name}");
        let lex = if *is_int {
            format!("{}", value.round() as u64)
        } else {
            format!("{value:.6}")
        };
        out.push(triple(
            GMN_METRICS_IRI,
            &g("observationResult"),
            &metric_iri,
        ));
        out.push(triple(&metric_iri, RDF_TYPE, &m("Quantity")));
        out.push(triple(&metric_iri, &m("hasDimension"), &m("dimensionless")));
        out.push(typed(&metric_iri, &m("quantityValue"), &lex, XSD_DECIMAL));
        out.push(label(&metric_iri, name));
    }
    // The version-provenance stamp, via the shared tag_schema_version quad (rendered here so
    // the metrics product carries the SAME `gmeow:gmnSchemaVersion` stamp every GMN record does).
    out.push(render_schema_version(&tag_schema_version(
        GMN_METRICS_IRI,
        dict,
    )));
    out
}

/// Render the single [`tag_schema_version`] quad as an N-Triples line. The quad is always an
/// IRI subject, the `gmeow:gmnSchemaVersion` predicate, and an `xsd:string`-typed literal
/// value (the graph-resolved schema major), so the rendering is total by construction.
fn render_schema_version(quad: &purrdf::RdfQuad) -> String {
    let subject = match &quad.subject {
        purrdf::RdfTerm::Iri(iri) => iri.as_str(),
        other => unreachable!("tag_schema_version always stamps an IRI subject, got {other:?}"),
    };
    let (lexical, datatype) = match &quad.object {
        purrdf::RdfTerm::Literal(lit) => (
            lit.lexical_form.as_str(),
            lit.datatype.as_deref().unwrap_or(XSD_STRING),
        ),
        other => unreachable!("tag_schema_version always stamps a literal object, got {other:?}"),
    };
    format!(
        "<{subject}> <{}> \"{lexical}\"^^<{datatype}> .",
        quad.predicate
    )
}

/// The EXACT `logic:Correspondence` the token-metrics emission carries: the measured 7-vector
/// is a faithful, deterministically-recomputable projection of the corpus + codebook, so the
/// rung is `logic:SectionRetraction`/`logic:ExactPreservation` with a discharged `GetPut`,
/// named on its own `gmn1-token-metrics` source key and metrics projection leg.
fn gmn1_metrics_correspondence() -> Correspondence {
    let iri = format!(
        "{GMN1_CORR_BASE}{}",
        digest16("gmn1-corr", "gmn1-token-metrics")
    );
    Correspondence::new(
        iri,
        CorrespondenceRelation::Subsumes,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        Some(Determinacy::Crisp),
        Some(GMN1_METRICS_GET_LEG.to_owned()),
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
    .expect("exact gmn1 token-metrics correspondence is well-formed by construction")
}

/// The token-metrics get/put leg pair — declarative projection-step metadata whose put leg is
/// the structural inverse of the get leg (the round-trip the driver cross-checks).
fn gmn1_metrics_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Step(GMN1_METRICS_GET_LEG.to_owned());
    let put = get.invert();
    (get, put)
}

// ── GMN-1 verbalizer (bundle-level GMN⇄controlled-NL training pairs) ─────────────────

use crate::gmn_verbalize::{VerbalizedPair, build_verbalization_pairs, round_trip_holds};

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// The verbalizer product subject: the current shipped GMN-1 verbalization corpus, mirroring
/// how `gmnCodebookCurrent` / `gmnPackCurrent` name the current codebook / pack.
const GMN_VERBALIZER_IRI: &str = "https://blackcatinformatics.ca/gmeow/gmnVerbalizationsCurrent";
/// The verbalizer projection get-leg step (distinct from the codec, pack, and metrics legs).
const GMN1_VERBALIZER_GET_LEG: &str =
    "https://blackcatinformatics.ca/lang/gmn1VerbalizeProjectStep";

/// The bundle-level GMN⇄controlled-NL verbalizer emission: ONE RDF artifact
/// (`gmn1/v<major>/verbalizations.ttl`, `is_rdf: true`) carrying every resolved operator
/// form's bidirectional training pair as a `lang:TranslationUnit` — the GMN operator surface
/// (`lang:translationSource`) crossing to its controlled-NL string (`lang:translationTarget`)
/// through a single `lang:translationCorrespondence` whose carried `logic:Correspondence`
/// records the MEASURED preservation. A neutral `gmeow:InformationObject` corpus subject
/// `gmeow:references` the units and carries the `tag_schema_version` provenance stamp. The
/// same triples ride into the reasoned `graph/lang-projection-corpus` via `source_rdf`, so a
/// consumer resolves the pairs by query, not only by reading the file.
///
/// Fallible and gated: `None` when no operator forms are resolved (nothing to verbalize — no
/// vacuous product); a HARD FAIL when the verbalization is not injective (two distinct forms
/// share a controlled-NL string that could not be disambiguated). Preservation is EXACT only
/// when the NL→GMN inverse template recovers the SAME form for every pair — MEASURED by
/// [`round_trip_holds`], never declared; a corpus that does not round-trip carries the honest
/// lossy correspondence instead of a fabricated `logic:ExactPreservation`.
fn gmn1_verbalizer_emission(
    forms: &[crate::gmn_verbalize::GmnOperatorForm],
    dict: &GmnDictionary,
    major: &str,
) -> Result<Option<LangEmission>, IngestDiagnostic> {
    if forms.is_empty() {
        return Ok(None);
    }
    let pairs = build_verbalization_pairs(forms).map_err(|e| IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: format!(
            "GMN⇄NL verbalization is not a sound bidirectional map over the operator inventory: \
             {e}; a training corpus that is not injective must never ship"
        ),
    })?;
    // MEASURE the bidirectional round-trip — the sole license for the EXACT preservation the
    // per-unit correspondences below claim. A false measurement downgrades every unit to the
    // honest lossy rung rather than shipping a false isomorphism.
    let exact = round_trip_holds(&pairs);

    let nt = ntriples_sorted(verbalization_triples(&pairs, dict, exact));
    let mut loss = LossLedger::new();
    let preservation = if exact {
        PreservationKind::Exact
    } else {
        PreservationKind::SoundUnder
    };
    Ok(Some(LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: format!("gmn1/v{major}/verbalizations.ttl"),
            bytes: nt.clone(),
            is_rdf: true,
        }],
        correspondence: gmn1_verbalizer_correspondence(exact),
        ledger: vec![emit_ledger_row(
            &mut loss,
            "gmn1:verbalizations".to_owned(),
            String::new(),
            true,
            preservation,
            "n/a".to_owned(),
            Vec::new(),
            if exact {
                Vec::new()
            } else {
                vec!["NL→GMN inverse does not recover every operator form".to_owned()]
            },
        )],
        loss,
        leg_pair: exact.then(gmn1_verbalizer_leg_pair),
        emitted_reading_count: None,
        source_iri: GMN_VERBALIZER_IRI.to_owned(),
        unsupported: Vec::new(),
        round_trip_holds: exact,
        lossy_kind: PreservationKind::SoundUnder,
        source_rdf: nt,
    }))
}

/// The verbalization N-Triples: for each bidirectional pair, a `lang:TranslationUnit` with
/// its two `lang:SurfaceForm`s (each carrying the material identity `lang:SurfaceMaterialShape`
/// requires — script, sign system, Unicode normalization, collation locale, and its surface
/// text) and its single `lang:translationCorrespondence` → a `logic:Correspondence` recording
/// the measured preservation. Surface text lives ONLY on the surface forms, never inline on a
/// crossing subject (so the corpus never trips `lang:SurfaceLeakInContentKey`). The corpus
/// subject `gmeow:references` its units and carries the version stamp; the whole set is sorted
/// + deduped by [`ntriples_sorted`], so two runs over the same forms serialize byte-identically.
fn verbalization_triples(
    pairs: &[VerbalizedPair],
    dict: &GmnDictionary,
    exact: bool,
) -> Vec<String> {
    let g = |local: &str| format!("{GMEOW_NS}{local}");
    let l = |local: &str| format!("{LANG_NS}{local}");
    let lo = |local: &str| format!("{LOGIC_NS}{local}");
    let triple = |s: &str, p: &str, o: &str| format!("<{s}> <{p}> <{o}> .");
    let lit = |s: &str, p: &str, v: &str| format!("<{s}> <{p}> {} .", nt_literal(v));
    let boolean = |s: &str, p: &str, b: bool| {
        format!(
            "<{s}> <{p}> \"{}\"^^<{XSD_BOOLEAN}> .",
            if b { "true" } else { "false" }
        )
    };

    // The English sign system the controlled-NL surfaces are written in — minted and typed
    // lang:SignSystem in the corpus (mirroring the live translation corpus), since the carrier's
    // gmeow:gmnEnglish is a lang:LanguageVariety, not a sign system.
    let english_sign_system = format!("{EXAMPLE_BASE}gmn-verbalization-sign-system/english");

    let mut out = Vec::new();
    // The corpus subject: a neutral gmeow:InformationObject that gmeow:references its crossing
    // units and carries the version stamp. It is deliberately NOT a lang:Translation — this is a
    // faithful producer of the crossings, not a form-view emission that flattens an epistemic
    // stratum, so its lang:ProjectionEmission never carries (and never needs to disclose) a
    // flattened Translation.
    out.push(triple(
        GMN_VERBALIZER_IRI,
        RDF_TYPE,
        &g("InformationObject"),
    ));
    out.push(triple(&english_sign_system, RDF_TYPE, &l("SignSystem")));

    for pair in pairs {
        // Content-addressed identities for the crossing, its correspondence, and both surfaces.
        let key = format!("{}\u{1f}{}", pair.form.term_iri, pair.form.fixity);
        let unit = format!(
            "{EXAMPLE_BASE}gmn-verbalization-unit/{}",
            digest16("gmn-verb-unit", &key)
        );
        let corr = format!(
            "{EXAMPLE_BASE}gmn-verbalization-correspondence/{}",
            digest16("gmn-verb-corr", &key)
        );
        let gmn_surface = format!(
            "{EXAMPLE_BASE}gmn-verbalization-surface/{}",
            digest16(
                "gmn-verb-surface-gmn",
                &format!("{key}\u{1f}{}", pair.gmn_surface)
            )
        );
        let nl_surface = format!(
            "{EXAMPLE_BASE}gmn-verbalization-surface/{}",
            digest16("gmn-verb-surface-nl", &format!("{key}\u{1f}{}", pair.nl))
        );

        // The crossing (clean of surface-stratum predicates).
        out.push(triple(&unit, RDF_TYPE, &l("TranslationUnit")));
        out.push(triple(&unit, &l("translationSource"), &gmn_surface));
        out.push(triple(&unit, &l("translationTarget"), &nl_surface));
        out.push(triple(&unit, &l("translationMethod"), &l("methodMachine")));
        out.push(triple(&unit, &l("translationCorrespondence"), &corr));
        out.push(triple(GMN_VERBALIZER_IRI, &g("references"), &unit));

        // The carried logic:Correspondence law-spine. An EXACT crossing is a section/retraction
        // isomorphism with a retained mnemomorphic witness (the measured inverse); a corpus that
        // does not round-trip carries the honest validation-only rung with no witness.
        out.push(triple(&corr, RDF_TYPE, &lo("Correspondence")));
        if exact {
            out.push(triple(
                &corr,
                &lo("preservationKind"),
                &lo("ExactPreservation"),
            ));
            out.push(triple(
                &corr,
                &lo("correspondenceRelation"),
                &lo("Subsumes"),
            ));
            out.push(triple(
                &corr,
                &lo("morphismClass"),
                &lo("SectionRetraction"),
            ));
            out.push(triple(&corr, &lo("hasDeterminacy"), &lo("Crisp")));
            out.push(boolean(&corr, &lo("mnemomorphic"), true));
        } else {
            out.push(triple(
                &corr,
                &lo("preservationKind"),
                &lo("ValidationOnly"),
            ));
            out.push(triple(
                &corr,
                &lo("correspondenceRelation"),
                &lo("RelatedMatch"),
            ));
            out.push(triple(&corr, &lo("morphismClass"), &lo("BridgeView")));
            out.push(triple(&corr, &lo("hasDeterminacy"), &lo("Vague")));
            out.push(boolean(&corr, &lo("mnemomorphic"), false));
        }

        // The two surface forms (surface text lives HERE, with full material identity).
        for (surface, sign_system, script, locale, text) in [
            (
                &gmn_surface,
                g("gmnModelNotation"),
                g("gmnScript"),
                "und",
                &pair.gmn_surface,
            ),
            (
                &nl_surface,
                english_sign_system.clone(),
                l("latinScript"),
                "en",
                &pair.nl,
            ),
        ] {
            out.push(triple(surface, RDF_TYPE, &l("SurfaceForm")));
            out.push(triple(surface, RDF_TYPE, &l("UnanalyzedProse")));
            out.push(triple(surface, &l("inSignSystem"), &sign_system));
            out.push(triple(surface, &l("inScript"), &script));
            out.push(lit(surface, &l("unicodeNormalization"), "NFC"));
            out.push(lit(surface, &l("collationLocale"), locale));
            out.push(lit(surface, &l("surfaceText"), text));
        }
    }

    // The version-provenance stamp, via the shared tag_schema_version quad.
    out.push(render_schema_version(&tag_schema_version(
        GMN_VERBALIZER_IRI,
        dict,
    )));
    out
}

/// The `logic:Correspondence` the verbalizer emission carries: an EXACT
/// `logic:SectionRetraction`/`logic:ExactPreservation` rung with a discharged `GetPut` when
/// the measured inverse round-trips (the GMN⇄NL map is a bijection over the operator
/// inventory), else the honest lossy lens with an unknown `GetPut`.
fn gmn1_verbalizer_correspondence(exact: bool) -> Correspondence {
    let iri = format!(
        "{GMN1_CORR_BASE}{}",
        digest16("gmn1-corr", "gmn1-verbalizations")
    );
    if exact {
        Correspondence::new(
            iri,
            CorrespondenceRelation::Subsumes,
            MorphismClass::SectionRetraction,
            MorphismKind::InstitutionMorphism,
            true,
            Some(Determinacy::Crisp),
            Some(GMN1_VERBALIZER_GET_LEG.to_owned()),
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
        .expect("exact gmn1 verbalizer correspondence is well-formed by construction")
    } else {
        Correspondence::new(
            iri,
            CorrespondenceRelation::RelatedMatch,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            false,
            Some(Determinacy::Crisp),
            Some(GMN1_VERBALIZER_GET_LEG.to_owned()),
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
        .expect("sound-under gmn1 verbalizer correspondence is well-formed by construction")
    }
}

/// The verbalizer get/put leg pair — declarative projection-step metadata whose put leg is
/// the structural inverse of the get leg (the round-trip the driver cross-checks).
fn gmn1_verbalizer_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Step(GMN1_VERBALIZER_GET_LEG.to_owned());
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

/// Escape a string as an N-Triples quoted literal (UTF-8 passes through verbatim). Used by
/// the verbalizer, whose surface text is arbitrary label/glyph content that may carry a quote
/// or backslash — unlike the digest/version literals the pack/metrics emitters format raw.
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

    /// The grounding-slice token-metric corpus: every `lang:`-bearing `examples/*.ttl` across
    /// the `lang`/`math`/`logic` grounding slices — the sources the GMN target lowers FROM,
    /// in deterministic (slice, filename) order. Mirrors the pipeline's `collect_input` scope
    /// filter (a source references the `lang:` namespace) so the unit-level corpus matches the
    /// bundle corpus the projection stage feeds.
    fn grounding_corpus() -> Vec<NamedSource> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices/grounding");
        let mut corpus = Vec::new();
        for slice in ["lang", "math", "logic"] {
            let dir = root.join(slice).join("examples");
            let mut paths: Vec<_> = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ttl"))
                .collect();
            paths.sort();
            for path in paths {
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                if String::from_utf8_lossy(&bytes).contains("blackcatinformatics.ca/lang/") {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("src");
                    corpus.push(NamedSource {
                        name: format!("{slice}-{stem}"),
                        bytes,
                    });
                }
            }
        }
        corpus
    }

    /// The carrier GMN dictionary + resolved dialect major, both read off the SAME lang module
    /// dataset (the pipeline's single source of both).
    fn carrier_dict_and_major() -> (GmnDictionary, String) {
        let module = lang_slice_file("module.ttl");
        let ds = purrdf::parse_dataset(&module, "text/turtle", None).expect("parse lang module");
        let dict = GmnDictionary::from_dataset(&ds).expect("load carrier GMN dictionary");
        let major = crate::gmn1_codec::resolve_dialect_acceptance(&ds)
            .expect("resolve GMN dialect acceptance")
            .expect("lang module carries the gmeow:gmnDialectVersions lineage")
            .latest_major_key();
        (dict, major)
    }

    /// The full projection input over the grounding corpus (dictionary + codebook + grammar +
    /// resolved major + the corpus), the shape `Gmn1Target::emit` consumes.
    fn grounding_input() -> LangProjectionInput {
        let module = lang_slice_file("module.ttl");
        let grammar = lang_slice_file("grammars/gmn.ebnf");
        let ds = purrdf::parse_dataset(&module, "text/turtle", None).expect("parse lang module");
        let dict = GmnDictionary::from_dataset(&ds).expect("dict");
        let codebook =
            crate::gmn1_codec::resolve_current_codebook(&ds).expect("resolve current codebook");
        let major = crate::gmn1_codec::resolve_dialect_acceptance(&ds)
            .expect("resolve acceptance")
            .expect("lineage present")
            .latest_major_key();
        LangProjectionInput {
            lang_models: grounding_corpus(),
            gmn_dictionary: Some(dict),
            gmn_codebook: Some(codebook),
            gmn_grammar_source: Some(grammar),
            gmn_dialect_major: Some(major),
            ..Default::default()
        }
    }

    /// The token-metric measurement product is DETERMINISTIC (two emissions byte-identical),
    /// keyed under the resolved-version subtree, carries all seven metrics + the version stamp,
    /// and is valid Turtle whose triples ride the corpus graph verbatim.
    #[test]
    fn gmn1_metrics_are_deterministic() {
        let input = grounding_input();
        let major = input.gmn_dialect_major.clone().expect("major");

        let find = |emissions: &[LangEmission]| -> LangEmission {
            emissions
                .iter()
                .find(|e| e.source_iri == GMN_METRICS_IRI)
                .expect("token-metrics emission present")
                .clone()
        };
        let first = find(&Gmn1Target.emit(&input).expect("gmn1 emit"));
        let second = find(&Gmn1Target.emit(&input).expect("gmn1 emit (second run)"));

        let versioned_path = format!("gmn1/v{major}/token-metrics.ttl");
        let artifact = first
            .artifacts
            .iter()
            .find(|a| a.path_suffix == versioned_path)
            .unwrap_or_else(|| panic!("metrics artifact keyed at {versioned_path}"));
        assert!(
            artifact.is_rdf,
            "the metrics artifact is an RDF serialization"
        );

        // Byte-deterministic across runs.
        assert_eq!(
            artifact.bytes, second.artifacts[0].bytes,
            "the token-metrics product is byte-deterministic"
        );
        // The triples ride the reasoned corpus graph verbatim.
        assert_eq!(
            first.source_rdf, artifact.bytes,
            "the metrics triples ride the corpus graph verbatim"
        );
        // Valid Turtle (N-Triples is a Turtle subset).
        purrdf::parse_dataset(artifact.bytes.as_slice(), "text/turtle", None)
            .expect("the metrics artifact is valid Turtle");
        // Carries an EXACT correspondence with a measured-true round-trip.
        assert!(
            crate::is_exact_correspondence(&first.correspondence),
            "the metrics carry an exact correspondence"
        );
        assert!(
            first.round_trip_holds,
            "the metrics round-trip is measured true"
        );

        let ttl = String::from_utf8(artifact.bytes.clone()).expect("utf8");
        // The measurement subject is a gmeow:Measurement bound to codebook + notation.
        assert!(
            ttl.contains(&format!(
                "<{GMN_METRICS_IRI}> <{RDF_TYPE}> \
                 <https://blackcatinformatics.ca/gmeow/Measurement>"
            )),
            "metrics subject typed gmeow:Measurement:\n{ttl}"
        );
        // All SEVEN core metrics are present as dimensionless math:Quantity results.
        for metric in [
            "bytes_on_disk",
            "tokens_in_context",
            "ast_validity_rate",
            "roundtrip_loss",
            "compression_ratio",
            "glyph_density",
            "dictionary_hit_rate",
        ] {
            let metric_iri = format!("{GMN_METRICS_IRI}/{metric}");
            assert!(
                ttl.contains(&format!(
                    "<{metric_iri}> <{RDF_TYPE}> \
                     <https://blackcatinformatics.ca/math/Quantity>"
                )),
                "metric {metric} typed math:Quantity:\n{ttl}"
            );
            assert!(
                ttl.contains(&format!(
                    "<{metric_iri}> <https://blackcatinformatics.ca/math/quantityValue>"
                )),
                "metric {metric} carries a math:quantityValue:\n{ttl}"
            );
        }
        // Version-tagged via tag_schema_version.
        assert!(
            ttl.contains(&format!(
                "<{GMN_METRICS_IRI}> <{}> ",
                crate::gmn_migrate::PRED_GMN_SCHEMA_VERSION
            )),
            "metrics carry the gmnSchemaVersion provenance stamp:\n{ttl}"
        );
    }

    /// The SOUND compression gate: GMN's CONSISTENT byte-fallback worst case (ASCII merged 4:1,
    /// non-ASCII glyph bytes each a fallback token) is strictly cheaper than Turtle's optimistic
    /// best case over the REAL grounding corpus. Asserts the exact formula so a regression that
    /// charged ALL bytes (the internally-inconsistent bound) would change `gmn_worst_case_tokens`
    /// and RED this test.
    #[test]
    fn gmn_beats_turtle_under_byte_fallback_worst_case() {
        let (dict, _major) = carrier_dict_and_major();
        let metrics = compute_token_metrics(&grounding_corpus(), &dict);

        // The corpus is non-vacuous and every source round-trips (the grounding scope).
        assert!(
            metrics.measured_sources > 0,
            "the grounding corpus has round-tripping sources"
        );

        // The gate's left side is EXACTLY the non-ASCII-byte-fallback formula — not total bytes.
        let expected_worst = metrics.gmn_ascii_bytes.div_ceil(4) + metrics.gmn_nonascii_bytes;
        assert_eq!(
            metrics.gmn_worst_case_tokens, expected_worst,
            "gmn_worst_case_tokens must be ceil(ascii/4)+nonascii, not total bytes \
             (ascii={}, nonascii={}, total_bytes={})",
            metrics.gmn_ascii_bytes, metrics.gmn_nonascii_bytes, metrics.bytes_on_disk
        );
        // Falsifiable teeth: the inconsistent all-bytes bound (bytes_on_disk) must be visibly
        // LARGER than the sound bound here — so if a regression swapped it in, the gate below
        // would flip. (On this corpus all-bytes LOSES: bytes_on_disk > turtle_best.)
        assert!(
            metrics.bytes_on_disk > metrics.gmn_worst_case_tokens,
            "the all-bytes bound is strictly more pessimistic than the sound bound"
        );

        // THE GATE: GMN worst case < Turtle best case, with a real, non-trivial margin.
        assert!(
            metrics.compression_gate_holds(),
            "GMN must beat Turtle under the sound byte-fallback worst case: \
             gmn_worst={} !< turtle_best={} (ascii={}, nonascii={})",
            metrics.gmn_worst_case_tokens,
            metrics.turtle_best_case_tokens,
            metrics.gmn_ascii_bytes,
            metrics.gmn_nonascii_bytes
        );
        assert!(
            metrics.gmn_worst_case_tokens < metrics.turtle_best_case_tokens,
            "explicit gate inequality on the real numbers: {} < {}",
            metrics.gmn_worst_case_tokens,
            metrics.turtle_best_case_tokens
        );
        // The realistic reading (both at chars/4) wins by an even wider margin, and GMN is
        // smaller on disk than both Turtle and JSON-LD.
        assert!(metrics.gmn_realistic_tokens < metrics.turtle_best_case_tokens);
        assert!(metrics.bytes_on_disk < metrics.turtle_bytes_on_disk);
        assert!(metrics.bytes_on_disk < metrics.jsonld_bytes_on_disk);
    }

    /// Every emitted GMN artifact (per-source `.gmn` AND the bundle conformance pack) is keyed
    /// under `gmn1/v<major>/…`, where `<major>` is RESOLVED FROM THE GRAPH (the dialect
    /// lineage's roleLatest member `owl:versionInfo`), never a hardcoded literal.
    #[test]
    fn artifacts_are_keyed_by_resolved_dialect_version() {
        let module = lang_slice_file("module.ttl");
        let grammar = lang_slice_file("grammars/gmn.ebnf");
        // A real lang example that round-trips exactly, so a per-source artifact is emitted.
        let example = lang_slice_file("examples/gmn-grounding-glyphs.ttl");
        let ds = purrdf::parse_dataset(&module, "text/turtle", None).expect("parse lang module");
        let dict = GmnDictionary::from_dataset(&ds).expect("load carrier GMN dictionary");
        let codebook =
            crate::gmn1_codec::resolve_current_codebook(&ds).expect("resolve current codebook");

        // The version-key major is READ OFF THE GRAPH, never a literal.
        let major = crate::gmn1_codec::resolve_dialect_acceptance(&ds)
            .expect("resolve GMN dialect acceptance")
            .expect("lang module carries the gmeow:gmnDialectVersions lineage")
            .latest_major_key();

        let build = |keyed_major: &str| {
            let input = LangProjectionInput {
                lang_models: vec![NamedSource {
                    name: "gmn-grounding-glyphs".to_owned(),
                    bytes: example.clone(),
                }],
                gmn_dictionary: Some(dict.clone()),
                gmn_codebook: Some(codebook.clone()),
                gmn_grammar_source: Some(grammar.clone()),
                gmn_dialect_major: Some(keyed_major.to_owned()),
                ..Default::default()
            };
            Gmn1Target
                .emit(&input)
                .expect("gmn1 emit")
                .iter()
                .flat_map(|e| e.artifacts.iter().map(|a| a.path_suffix.clone()))
                .collect::<Vec<_>>()
        };

        // Under the graph-resolved major, EVERY emitted artifact is keyed gmn1/v<major>/…,
        // and both the per-source .gmn and the conformance pack are present.
        let paths = build(&major);
        let prefix = format!("gmn1/v{major}/");
        assert!(
            !paths.is_empty(),
            "at least the conformance pack is emitted"
        );
        for path in &paths {
            assert!(
                path.starts_with(&prefix),
                "artifact {path:?} must be keyed under the resolved-version subtree {prefix:?}"
            );
        }
        assert!(
            paths.iter().any(|p| p.ends_with("/conformance-pack.ttl")),
            "the bundle conformance pack is emitted under the versioned subtree: {paths:?}"
        );
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("gmn-grounding-glyphs.gmn")),
            "the per-source GMN artifact is emitted under the versioned subtree: {paths:?}"
        );

        // Falsifiability: the key comes from the THREADED (graph-resolved) major, not a
        // hardcoded string. Re-key under a synthetic major and every path moves — a literal
        // in the emitter would leave them under v1 and fail here.
        assert_ne!(
            major, "7",
            "the resolved major differs from the synthetic bump"
        );
        for path in build("7") {
            assert!(
                path.starts_with("gmn1/v7/"),
                "re-keyed artifact {path:?} must follow the threaded major into gmn1/v7/"
            );
        }
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

        // The version-key major is RESOLVED FROM THE GRAPH (the same lineage the codec reads),
        // never a literal — so a change to the lineage's roleLatest membership flows through.
        let major = crate::gmn1_codec::resolve_dialect_acceptance(&ds)
            .expect("resolve GMN dialect acceptance")
            .expect("lang module carries the gmeow:gmnDialectVersions lineage")
            .latest_major_key();
        let input = LangProjectionInput {
            gmn_dictionary: Some(dict.clone()),
            gmn_codebook: Some(codebook.clone()),
            gmn_grammar_source: Some(grammar.clone()),
            gmn_dialect_major: Some(major.clone()),
            ..Default::default()
        };

        // With no lang_models, the sole emission is the bundle-level conformance pack.
        let emissions = Gmn1Target.emit(&input).expect("gmn1 emit");
        let pack = emissions
            .iter()
            .find(|e| e.source_iri == GMN_PACK_IRI)
            .expect("bundle-level conformance-pack emission present");
        let versioned_pack_path = format!("gmn1/v{major}/conformance-pack.ttl");
        let artifact = pack
            .artifacts
            .iter()
            .find(|a| a.path_suffix == versioned_pack_path)
            .unwrap_or_else(|| panic!("pack artifact keyed at {versioned_pack_path}"));
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
        // Every ecosystem-view Merkle leaf is pinned into the bundle on its own subject, so
        // `gmeow gmn verify` recomputes the whole-ecosystem pack root from the bundle alone.
        for (subject, predicate) in [
            ("gmnGbnf", "gmnGbnfDigest"),
            ("gmnLark", "gmnLarkDigest"),
            ("gmnTokenMetricsCurrent", "gmnTokenMetricsDigest"),
            ("gmnVerbalizationsCurrent", "gmnVerbalizationsDigest"),
        ] {
            assert!(
                ttl.contains(&format!(
                    "<https://blackcatinformatics.ca/gmeow/{subject}> \
                     <https://blackcatinformatics.ca/gmeow/{predicate}> \""
                )),
                "pack artifact pins the {predicate} ecosystem-view Merkle leaf:\n{ttl}"
            );
            assert!(
                ttl.contains(&format!(
                    "<https://blackcatinformatics.ca/gmeow/gmnPackCurrent> \
                     <https://blackcatinformatics.ca/gmeow/references> \
                     <https://blackcatinformatics.ca/gmeow/{subject}> ."
                )),
                "pack references its {subject} ecosystem view:\n{ttl}"
            );
        }

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

        // Independent recomputation of the Merkle construction matches the emitted root. This
        // input carries no `gmn` grammar / corpus / operator inventory, so every ecosystem view
        // is absent and folds as the stable empty leaf — the SAME leaves the emission computed.
        let expected_digest = codebook_digest(&codebook, &dict);
        let expected_ecosystem = EcosystemLeaves::from_view_bytes(&[], &[], &[], &[]);
        let expected_root = pack_root(&expected_digest, &dict, &grammar, &expected_ecosystem);
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

    use crate::gmn_verbalize::{
        FIXITY_INFIX, FIXITY_POSTFIX, FIXITY_PREFIX, GmnOperatorForm, VerbalizedPair,
        build_verbalization_pairs, forward_index, invert_nl, round_trip_holds as pairs_round_trip,
    };

    fn op(term: &str, label: &str, glyph: &str, fixity: &str, arity: u32) -> GmnOperatorForm {
        GmnOperatorForm {
            term_iri: term.to_owned(),
            term_label: label.to_owned(),
            gmn_glyph: glyph.to_owned(),
            fixity: fixity.to_owned(),
            arity,
        }
    }

    /// Build the bundle-level verbalizer emission over a set of operator forms, using the real
    /// carrier dictionary (for the version stamp) and resolved major (for the keyed path).
    fn verbalizer_emission(forms: Vec<GmnOperatorForm>) -> (LangEmission, String) {
        let (dict, major) = carrier_dict_and_major();
        let input = LangProjectionInput {
            gmn_dictionary: Some(dict),
            gmn_dialect_major: Some(major),
            gmn_operator_forms: forms,
            ..Default::default()
        };
        let emissions = Gmn1Target.emit(&input).expect("gmn1 emit");
        let emission = emissions
            .into_iter()
            .find(|e| e.source_iri == GMN_VERBALIZER_IRI)
            .expect("verbalizer emission present");
        let major = input.gmn_dialect_major.clone().expect("major");
        (emission, major)
    }

    /// The verbalizer emission's bidirectional pairs are injective, deterministic, version-
    /// tagged, carry `lang:translationCorrespondence`, MEASURE their NL→GMN inverse as exact,
    /// and are genuinely fixity-driven (perturbing a fixity changes the verbalization).
    #[test]
    fn verbalizer_pairs_are_bidirectional_and_injective_and_deterministic() {
        // One of each fixity plus a HOMOGRAPH: two distinct terms sharing the label "contains".
        let forms = vec![
            op("logic:not", "not", "¬", FIXITY_PREFIX, 1),
            op("logic:subClassOf", "subsumes", "⊑", FIXITY_INFIX, 2),
            op("math:factorial", "factorial", "!", FIXITY_POSTFIX, 1),
            op("math:supersetRel", "contains", "⊃", FIXITY_INFIX, 2),
            op("math:hasElement", "contains", "∋", FIXITY_INFIX, 2),
        ];

        let (emission, major) = verbalizer_emission(forms.clone());

        // Keyed under the resolved-version subtree, an RDF artifact.
        let path = format!("gmn1/v{major}/verbalizations.ttl");
        let artifact = emission
            .artifacts
            .iter()
            .find(|a| a.path_suffix == path)
            .unwrap_or_else(|| panic!("verbalizations artifact keyed at {path}"));
        assert!(artifact.is_rdf, "the verbalizations artifact is RDF");

        // MEASURED-exact: the carried correspondence derives Exact and the round-trip holds.
        assert!(
            crate::is_exact_correspondence(&emission.correspondence),
            "the verbalizer carries an exact correspondence"
        );
        assert!(
            emission.round_trip_holds,
            "the NL→GMN inverse round-trip is measured true"
        );

        let ttl = String::from_utf8(artifact.bytes.clone()).expect("utf8");
        // Valid Turtle (N-Triples is a Turtle subset), and the triples ride the corpus verbatim.
        purrdf::parse_dataset(artifact.bytes.as_slice(), "text/turtle", None)
            .expect("the verbalizations artifact is valid Turtle");
        assert_eq!(
            emission.source_rdf, artifact.bytes,
            "the verbalization triples ride the corpus graph verbatim"
        );

        // It types translation crossings carried by lang:translationCorrespondence.
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/lang/TranslationUnit>"),
            "verbalizations type lang:TranslationUnit:\n{ttl}"
        );
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/lang/translationCorrespondence>"),
            "verbalizations carry lang:translationCorrespondence:\n{ttl}"
        );
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/logic/ExactPreservation>"),
            "an exact verbalization crossing records logic:ExactPreservation:\n{ttl}"
        );
        // Version-tagged via tag_schema_version.
        assert!(
            ttl.contains(&format!(
                "<{GMN_VERBALIZER_IRI}> <{}> ",
                crate::gmn_migrate::PRED_GMN_SCHEMA_VERSION
            )),
            "verbalizations carry the gmnSchemaVersion provenance stamp:\n{ttl}"
        );

        // Byte-deterministic across runs.
        let (again, _) = verbalizer_emission(forms.clone());
        assert_eq!(
            artifact.bytes, again.artifacts[0].bytes,
            "the verbalizer product is byte-deterministic"
        );

        // ── Injectivity + bidirectionality over the SAME forms, at the pair level ──
        let pairs = build_verbalization_pairs(&forms).expect("pairs build");
        // Every controlled-NL string is distinct (injective).
        let mut nls: Vec<&str> = pairs.iter().map(|p| p.nl.as_str()).collect();
        let count = nls.len();
        nls.sort_unstable();
        nls.dedup();
        assert_eq!(nls.len(), count, "controlled-NL strings must be injective");
        // The homograph "contains" collided → both got disambiguated with a CURIE tag.
        let contains: Vec<&VerbalizedPair> = pairs
            .iter()
            .filter(|p| p.form.term_label == "contains")
            .collect();
        assert_eq!(contains.len(), 2);
        assert!(
            contains.iter().all(|p| p.nl.contains('⟪')),
            "a homograph label must be disambiguated by CURIE: {:?}",
            contains.iter().map(|p| &p.nl).collect::<Vec<_>>()
        );
        // The NL→GMN inverse template recovers the SAME operator form for every pair.
        assert!(pairs_round_trip(&pairs), "every pair round-trips");
        let index = forward_index(&pairs);
        for pair in &pairs {
            assert_eq!(
                invert_nl(&pair.nl, &index),
                Some(&pair.form),
                "inverse of {:?} must recover its own form",
                pair.nl
            );
        }

        // ── Falsifiability: perturbing ONE form's fixity changes the emitted product ──
        let mut perturbed = forms;
        perturbed[1].fixity = FIXITY_PREFIX.to_owned(); // subsumes: infix → prefix
        let (perturbed_emission, _) = verbalizer_emission(perturbed);
        assert_ne!(
            artifact.bytes, perturbed_emission.artifacts[0].bytes,
            "perturbing a form's fixity must change its verbalization"
        );
    }
}
