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
//! correspondence by the driver ([`is_exact_correspondence`]), and the round-trip is
//! MEASURED by the target (re-parse / byte round-trip) and cross-checked by the driver
//! against [`exact_round_trip_holds`] over the carried [`LangEmission::leg_pair`]. There
//! is one law spine in the system — never a per-target law shadow.
//!
//! Each target reuses the EXISTING bridge functions (`grammar_*`, `conllu_*`,
//! `ontolex_*`); the registry adds NO new transform, only the projection-direction
//! wiring and the honest per-emission preservation record the driver folds into the loss
//! ledger and the `lang:ProjectionEmission` corpus.

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict,
    LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind,
};
use gmeow_logic_compile::projections::ProjectionResult;

use crate::bridge::{Bridge, IngestDiagnostic, LangFailure};
use crate::conllu::{conllu_correspondence, conllu_leg_pair, ConlluBridge};
use crate::emit::digest16;
use crate::grammar::{
    grammar_correspondence, grammar_leg_pair, grammar_to_ntriples, parse_grammar,
    serialize_grammar, EbnfBridge, Formalism, Grammar, RuleExpr,
};
use crate::ontolex::OntoLexBridge;

/// The example-instance base every minted projection individual (grammar IRIs,
/// correspondence IRIs) lives under, matching every other `lang:` producer.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// A named external source surface a target lifts (grammar notation, an OntoLex-Lemon
/// Turtle lexicon, …). `name` becomes the emitted artifact's file stem.
#[derive(Clone, Debug)]
pub struct NamedSource {
    /// The source's stable name (the artifact file stem, e.g. `turtle` / `gts`).
    pub name: String,
    /// The raw source bytes (grammar notation / OntoLex Turtle).
    pub bytes: Vec<u8>,
}

/// A CoNLL-U treebank source carrying its co-resident readings — one `.conllu` byte
/// stream PER reading. A per-reading projection emits one artifact per reading and never
/// a single silently-chosen winner (`lang:ProjectionSilentDisambiguation`), so the source
/// carries every reading explicitly rather than a pre-collapsed single tree.
#[derive(Clone, Debug)]
pub struct ConlluSource {
    /// The source's stable name (the artifact file stem).
    pub name: String,
    /// One `.conllu` byte stream per co-resident reading, in reading order.
    pub readings: Vec<Vec<u8>>,
}

/// The projection input aBox: every external source surface the registered targets may
/// lower FROM. A target reads only the slice it consumes; an empty slice yields an honest
/// empty projection (the target is still registered, the driver folds one honest
/// no-source ledger row).
#[derive(Clone, Debug, Default)]
pub struct LangProjectionInput {
    /// Authored grammar source surfaces (EBNF notation) — the EBNF/ABNF targets' input.
    pub grammars: Vec<NamedSource>,
    /// OntoLex-Lemon Turtle lexicons — the OntoLex target's input.
    pub lexicons: Vec<NamedSource>,
    /// CoNLL-U treebanks (one file per co-resident reading) — the CoNLL-U target's input.
    pub treebanks: Vec<ConlluSource>,
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
    pub ledger: Vec<ProjectionResult>,
    /// The get/put leg pair whose structural round-trip the driver cross-checks with
    /// [`exact_round_trip_holds`]; `None` for a lossy target with no exact inverse leg.
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
    /// driver derives `Exact` from [`is_exact_correspondence`], else uses this).
    pub lossy_kind: PreservationKind,
    /// The lifted `lang:` RDF this emission projects into the corpus graph (N-Triples
    /// bytes); empty when the source RDF is already carried by a sibling emission.
    pub source_rdf: Vec<u8>,
}

/// A registered projection target: the projection peer of [`Bridge`]. It CARRIES a
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
    ]
}

/// Every "emission-worthy" `lang:` class paired with the registered target names that MUST
/// cover it (functor totality). The registry-completeness gate asserts each class maps to
/// ≥1 registered target; extend this as Tasks 3–4 add targets (TEI/NIF/SemAF/BCP-47/…).
pub const EMISSION_WORTHY_CLASSES: &[(&str, &[&str])] = &[
    ("Grammar", &["ebnf", "abnf"]),
    ("Lexeme", &["ontolex-lemon"]),
    ("ComposedForm", &["conllu"]),
];

/// Whether the registry covers `lang_class` — every emission-worthy class maps to at least
/// one REGISTERED target. `Err` names the gap (functor totality failure).
pub fn assert_registry_covers(lang_class: &str) -> Result<(), String> {
    let registered: Vec<&str> = registry().iter().map(|t| t.name()).collect();
    let Some((_, targets)) = EMISSION_WORTHY_CLASSES
        .iter()
        .find(|(c, _)| *c == lang_class)
    else {
        return Err(format!(
            "lang:{lang_class} is not listed in EMISSION_WORTHY_CLASSES; add it with the \
             target(s) that project it"
        ));
    };
    if targets.iter().any(|t| registered.contains(t)) {
        Ok(())
    } else {
        Err(format!(
            "no registered projection target covers emission-worthy class lang:{lang_class} \
             (expected one of {targets:?})"
        ))
    }
}

// ── OntoLex-Lemon ────────────────────────────────────────────────────────────────

/// The OntoLex-Lemon lexical projection target. Lowers a `lang:Lexeme`/`lang:Sense`
/// inventory to OntoLex through the existing [`OntoLexBridge`] — SoundUnder, whose residue
/// is the sense glosses plus the flattened epistemic strata (vantage / interpretation /
/// denotation-beyond-reference).
struct OntoLexTarget;

impl LangProjectionTarget for OntoLexTarget {
    fn name(&self) -> &'static str {
        "ontolex-lemon"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.lexicons {
            let bridge = OntoLexBridge {
                source_vantage: format!("{EXAMPLE_BASE}ontolex-source/{}", source.name),
            };
            let lifted = bridge.lift(&source.bytes)?;
            let rdf = bridge.emit(&lifted);
            let source_iri = format!(
                "{EXAMPLE_BASE}ontolex-lift/lexicon/{}",
                digest16("lang-ontolex-lexicon", &source.name)
            );
            // The flattened epistemic strata Lemon has no slot for — enumerated so the
            // form-view flattening is carried and flagged, never hidden.
            let mut unsupported = vec![
                "denotesLogicFormula and non-reference denotation kinds have no OntoLex-Lemon \
                 target"
                    .to_owned(),
                "lang:vantage / gmeow:vantage epistemic stratum flattens: senses fold to flat \
                 lexical structure"
                    .to_owned(),
                "lang:InterpretationAct and co-resident readings have no Lemon form".to_owned(),
            ];
            // The concrete gloss residue the bridge recorded, carried verbatim.
            for row in &lifted.ledger {
                unsupported.extend(row.actual_drops.iter().cloned());
            }
            emissions.push(LangEmission {
                artifacts: vec![EmittedArtifact {
                    path_suffix: format!("ontolex-lemon/{}.ttl", source.name),
                    bytes: rdf,
                    is_rdf: true,
                }],
                correspondence: lifted.correspondence.clone(),
                ledger: lifted.ledger.clone(),
                leg_pair: None,
                emitted_reading_count: None,
                source_iri,
                unsupported,
                round_trip_holds: false,
                lossy_kind: PreservationKind::SoundUnder,
                source_rdf: Vec::new(),
            });
        }
        Ok(emissions)
    }
}

// ── CoNLL-U ──────────────────────────────────────────────────────────────────────

/// The Universal-Dependencies / CoNLL-U morphosyntax projection target. Exact byte
/// round-trip via the existing [`ConlluBridge`]; emits ONE artifact per co-resident
/// reading — never a silently chosen winner — and records the reading count.
struct ConlluTarget;

impl LangProjectionTarget for ConlluTarget {
    fn name(&self) -> &'static str {
        "conllu"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.treebanks {
            if source.readings.is_empty() {
                return Err(IngestDiagnostic {
                    failure_class: LangFailure::SilentIngestDrop,
                    construct: format!(
                        "CoNLL-U source '{}' carries no readings; a co-resident-reading source \
                         must carry every reading explicitly",
                        source.name
                    ),
                });
            }
            let bridge = ConlluBridge;
            let mut artifacts = Vec::with_capacity(source.readings.len());
            let mut round_trip_holds = true;
            // One artifact per reading (never a single winner). Content-address the carried
            // correspondence on the concatenated byte round-trip of every reading.
            let mut corr_key = String::new();
            for (i, reading) in source.readings.iter().enumerate() {
                let round = bridge.round_trip(reading)?;
                if round != *reading {
                    round_trip_holds = false;
                }
                corr_key.push_str(&String::from_utf8_lossy(&round));
                corr_key.push('\u{1f}');
                artifacts.push(EmittedArtifact {
                    path_suffix: format!("conllu/{}.reading-{}.conllu", source.name, i),
                    bytes: round,
                    is_rdf: false,
                });
            }
            let source_iri = format!(
                "{EXAMPLE_BASE}conllu-lift/form/{}",
                digest16("lang-conllu-form", &source.name)
            );
            emissions.push(LangEmission {
                artifacts,
                correspondence: conllu_correspondence(&corr_key),
                ledger: vec![ProjectionResult {
                    target: format!("conllu:{}", source.name),
                    content: String::new(),
                    is_rdf: false,
                    preservation: PreservationKind::Exact,
                    complexity: "n/a".to_owned(),
                    lossy_drops: Vec::new(),
                    actual_drops: Vec::new(),
                }],
                leg_pair: Some(conllu_leg_pair()),
                emitted_reading_count: Some(source.readings.len() as u64),
                source_iri,
                unsupported: Vec::new(),
                round_trip_holds,
                lossy_kind: PreservationKind::Exact,
                source_rdf: Vec::new(),
            });
        }
        Ok(emissions)
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
            emissions.push(LangEmission {
                artifacts: vec![EmittedArtifact {
                    path_suffix: format!("ebnf/{}.ebnf", source.name),
                    bytes: text.clone().into_bytes(),
                    is_rdf: false,
                }],
                correspondence: grammar_correspondence(&text),
                ledger: vec![grammar_ledger_row(
                    "ebnf",
                    &source.name,
                    PreservationKind::Exact,
                    Vec::new(),
                )],
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
                emissions.push(LangEmission {
                    artifacts: vec![EmittedArtifact {
                        path_suffix: format!("abnf/{}.abnf", source.name),
                        bytes: text.clone().into_bytes(),
                        is_rdf: false,
                    }],
                    correspondence: grammar_correspondence(&text),
                    ledger: vec![grammar_ledger_row(
                        "abnf",
                        &source.name,
                        PreservationKind::Exact,
                        Vec::new(),
                    )],
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
                emissions.push(LangEmission {
                    artifacts: Vec::new(),
                    correspondence: lossy_grammar_correspondence(&ebnf_text),
                    ledger: vec![grammar_ledger_row(
                        "abnf",
                        &source.name,
                        PreservationKind::SoundUnder,
                        unsupported.clone(),
                    )],
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
    target: &str,
    name: &str,
    preservation: PreservationKind,
    residue: Vec<String>,
) -> ProjectionResult {
    ProjectionResult {
        target: format!("{target}:{name}"),
        content: String::new(),
        is_rdf: false,
        preservation,
        complexity: "n/a".to_owned(),
        lossy_drops: Vec::new(),
        actual_drops: residue,
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
    )
    .expect("lossy grammar correspondence is well-formed by construction")
}
