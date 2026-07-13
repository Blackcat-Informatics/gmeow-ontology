// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The external-NLP-engine handoff seam.
//!
//! Parsers, taggers, and MT systems are **oracles that produce claims, never
//! authorities that produce facts**: an external run is a `gmeow:Activity`
//! (`lang:InterpretationAct`) whose outputs enter the graph as vantage-held
//! [`Reading`]s carrying the *engine's* vantage and confidence, per the
//! process/result/claim separation. Two engines that disagree yield co-resident
//! readings; the disagreement is data, never collapsed.
//!
//! This module mirrors [`gmeow_logic_compile`]'s reasoning-oracle boundary (the
//! `oracle.rs` doctrine) on two points:
//!
//! * **Single naming site.** [`NlpEngine::name`] is the *only* place a concrete
//!   engine is named; every consumer depends on the trait, so swapping (or
//!   deleting) an engine adapter is a local change. Engines are reached through a
//!   declared handoff seam ([`EngineRegistry`]) rather than concretely-named call
//!   targets, and a missing engine is a **hard fail** of the lane that needs it
//!   ([`EngineError::UnregisteredEngine`]), never a silent skip.
//! * **Provenance as a queried capability.** [`NlpEngine::provides_provenance`]
//!   is a queried capability, never a mandatory method: an engine that cannot
//!   attribute its output reports `false`, and a consumer that would emit its
//!   readings as vantage-held claims must **hard-fail rather than fabricate
//!   attribution** ([`EngineError::UnattributableEngine`]).
//!
//! # R8 — engine output is corpus data, never a reasoned input
//!
//! A real engine is non-deterministic (model version, sampling, thread order),
//! so its readings are vantage-held CLAIMS, not asserted facts. They belong ONLY
//! in a non-EDB corpus graph and are NEVER wired into the reasoned/gated pipeline:
//! this crate deliberately exposes only the [`interpretation_act_to_ntriples`]
//! projection (corpus N-Triples) and no path that feeds a reasoned EDB. Every
//! emitted reading carries its `gmeow:vantage`, so no engine output is ever folded
//! as an unattributed assertion, and no `lang:resolvedReading` is emitted (the
//! seam picks no silent winner among co-resident readings). The
//! `r8_engine_output_is_vantage_held_corpus_data` test enforces this
//! shape on the emitter's output.

use std::collections::BTreeMap;
use std::fmt;

use gmeow_lang_form::{Form, SurfaceForm};

use crate::emit::{digest16, ntriples_sorted};

/// The `lang:` namespace base, byte-identical to the other `lang:` producers so every
/// `lang:` local name resolves to the same IRI across bridges.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// The `gmeow:` namespace base — the vantage and activity vocabulary the engine
/// handoff attributes readings against.
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `logic:` namespace base — carries the engine's `logic:confidence` (a
/// confidence, never a `logic:probability`, absent a declared probability model).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `rdfs:label` predicate IRI.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The `xsd:double` datatype IRI — the engine's confidence is a floating weight.
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// A candidate interpretation a [`NlpEngine`] produces for a surface — a reading
/// **held from a vantage**, never an unattributed fact.
///
/// Co-resident readings for one surface carry DISTINCT [`analysis`](Reading::analysis)
/// strings, which flow into the [`Form::Composed`] `analysis` field and therefore
/// distinguish the forms by [`Form::content_key`]: the interner and
/// [`dedup_by_content_key`](gmeow_lang_form::dedup_by_content_key) key on content
/// key, so two readings with distinct analyses never merge — the "no stage collapses
/// it" invariant at the identity layer.
#[derive(Clone, Debug)]
pub struct Reading {
    /// The analysis label distinguishing this reading from its co-residents. Two
    /// readings of one surface differ here (and only here at the analysis layer),
    /// which is exactly what keeps their [`Form::Composed`] keys distinct.
    pub analysis: String,
    /// The structured form the reading assigns to the surface.
    pub form: Form,
    /// The IRI of the vantage this reading is held from (the engine's vantage). A
    /// reading is a claim held from HERE, never a groundless assertion.
    pub vantage: String,
    /// The engine's confidence in this reading — a `logic:confidence`, never a
    /// probability absent a declared probability model.
    pub confidence: f64,
    /// The IRI of the denotation context the reading is anchored in.
    pub denotation_context: String,
}

/// A failure at the engine handoff seam. Every variant is a HARD FAIL naming the
/// engine or capability at fault — never a silent skip or a fabricated default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// A lane asked the registry for an engine name that was never registered.
    /// Mirrors the oracle seam's single-naming-site + hard-fail discipline: a
    /// missing engine is a hard fail of the lane, never a `None`-swallow.
    UnregisteredEngine(String),
    /// A consumer tried to emit an engine's readings as vantage-held claims, but the
    /// engine reports [`provides_provenance`](NlpEngine::provides_provenance) `false`
    /// — it cannot attribute its output, so emitting attributed readings would
    /// fabricate attribution. Named after the engine at fault.
    UnattributableEngine(String),
    /// The engine failed to interpret the surface, carrying the engine's own reason.
    InterpretFailed { engine: String, reason: String },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::UnregisteredEngine(name) => write!(
                f,
                "no NLP engine registered under name '{name}'; a missing engine is a hard fail \
                 of the lane that needs it, never a silent skip"
            ),
            EngineError::UnattributableEngine(name) => write!(
                f,
                "engine '{name}' reports it provides no provenance, so its output cannot be \
                 emitted as vantage-held readings; attributing it would fabricate attribution"
            ),
            EngineError::InterpretFailed { engine, reason } => {
                write!(
                    f,
                    "engine '{engine}' failed to interpret the surface: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for EngineError {}

/// An external NLP engine: a parser, tagger, lemmatizer, or MT system consulted as an
/// **oracle that produces claims**. This trait is the ONLY place a concrete engine is
/// named ([`name`](NlpEngine::name)); every consumer depends on the trait, so an
/// engine adapter is swappable and deletable without touching call sites.
pub trait NlpEngine {
    /// The engine's stable name — the single naming site. Ledgers and provenance
    /// attribute the run to this name.
    fn name(&self) -> &str;

    /// The engine's version, carried as provenance so a re-run under a different
    /// version is a distinguishable act (a real engine is non-deterministic across
    /// versions — R8).
    fn version(&self) -> &str;

    /// Whether the engine can attribute its output. A queried capability, never a
    /// mandatory method: an engine that cannot attribute reports `false`, and a
    /// consumer that would emit vantage-held readings must hard-fail rather than
    /// fabricate attribution (mirrors the reasoning oracle's `provides_provenance`).
    fn provides_provenance(&self) -> bool;

    /// Interpret a surface into zero or more candidate [`Reading`]s. An ambiguous
    /// surface returns MULTIPLE co-resident readings (distinct `analysis`); the
    /// engine never collapses them to a single winner — resolution is a separate,
    /// vantage-held editorial act.
    fn interpret(&self, surface: &SurfaceForm) -> Result<Vec<Reading>, EngineError>;
}

/// The declared engine handoff seam: engines are reached by name through this
/// registry rather than as concretely-named call targets. An UNREGISTERED name is a
/// HARD FAIL ([`EngineError::UnregisteredEngine`]), never a silent skip.
#[derive(Default)]
pub struct EngineRegistry {
    by_name: BTreeMap<String, Box<dyn NlpEngine>>,
}

impl EngineRegistry {
    /// A new, empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `engine` under `name`. A later registration under the same name
    /// replaces the earlier one (the seam names exactly one adapter per name).
    pub fn register(&mut self, name: impl Into<String>, engine: Box<dyn NlpEngine>) {
        self.by_name.insert(name.into(), engine);
    }

    /// Resolve the engine registered under `name`, or HARD FAIL naming it. A missing
    /// engine is never a `None`-swallow: the lane that needs it fails loudly.
    pub fn get(&self, name: &str) -> Result<&dyn NlpEngine, EngineError> {
        self.by_name
            .get(name)
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| EngineError::UnregisteredEngine(name.to_owned()))
    }

    /// The registered engine names, in sorted order (the store is a `BTreeMap`).
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.by_name.keys().map(String::as_str).collect()
    }
}

/// A test-double engine that returns its canned co-resident [`Reading`]s verbatim —
/// no live NLP dependency. It attributes its readings to a declared vantage, so it
/// [`provides_provenance`](NlpEngine::provides_provenance) `== true`.
pub struct FixtureEngine {
    name: String,
    version: String,
    readings: Vec<Reading>,
}

impl FixtureEngine {
    /// A fixture engine that will hand back `readings` for any surface.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        readings: Vec<Reading>,
    ) -> Self {
        FixtureEngine {
            name: name.into(),
            version: version.into(),
            readings,
        }
    }
}

impl NlpEngine for FixtureEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn provides_provenance(&self) -> bool {
        // The fixture attributes every reading to its declared vantage.
        true
    }

    fn interpret(&self, _surface: &SurfaceForm) -> Result<Vec<Reading>, EngineError> {
        Ok(self.readings.clone())
    }
}

/// Escape a string literal for an N-Triples object (`"..."`): backslash, double-quote,
/// and the line-ending controls, per the N-Triples grammar.
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Project an engine run over `surface_iri` into a deterministic (sorted, deduped)
/// N-Triples byte stream: a `lang:InterpretationAct` (a `gmeow:Activity`) attributed
/// to the engine and version, interpreting the surface through `lang:interpretedForm`,
/// with ONE `lang:producedReading` per reading. Each `lang:Reading` carries its
/// `gmeow:vantage`, its `lang:denotationContext`, its `logic:confidence`, and its
/// analysis label — a vantage-held claim, NEVER folded as an unattributed assertion,
/// and never resolved to a single winner (no `lang:resolvedReading`).
///
/// This is corpus data (R8): the emission belongs in a non-EDB corpus graph and is not
/// wired into the reasoned pipeline.
///
/// # Errors
/// [`EngineError::UnattributableEngine`] if `engine.provides_provenance()` is `false`:
/// a consumer must hard-fail rather than emit unattributed structure that fabricates
/// attribution. `vantage` and `denotation_context` are treated as IRIs, matching the
/// `gmeow:vantage` / `lang:denotationContext` object convention.
pub fn interpretation_act_to_ntriples(
    act_iri: &str,
    engine: &dyn NlpEngine,
    surface_iri: &str,
    readings: &[Reading],
) -> Result<Vec<u8>, EngineError> {
    // Provenance is a queried capability: an engine that cannot attribute its output
    // must not have its readings emitted as vantage-held claims (no fabricated
    // attribution). This is the emitter's half of the oracle seam's doctrine.
    if !engine.provides_provenance() {
        return Err(EngineError::UnattributableEngine(engine.name().to_owned()));
    }

    // The engine agent IRI, content-addressed on name+version so a re-run under a
    // different version is a distinguishable provenance node.
    let engine_iri = format!(
        "{act_iri}/engine/{}",
        digest16(
            "lang-engine",
            &format!("{}\u{1f}{}", engine.name(), engine.version())
        )
    );

    let mut lines = vec![
        format!("<{act_iri}> <{RDF_TYPE}> <{LANG_NS}InterpretationAct> ."),
        // lang:InterpretationAct rdfs:subClassOf gmeow:Activity — assert the activity
        // type too so the run is a first-class gmeow:Activity in the corpus graph.
        format!("<{act_iri}> <{RDF_TYPE}> <{GMEOW_NS}Activity> ."),
        // Mark the act as an ENGINE run and attribute it to the engine agent: this is
        // what obliges every produced reading to be vantage-held (the
        // lang:UnattributedEngineClaim gate keys on lang:interpretationEngine).
        format!("<{act_iri}> <{LANG_NS}interpretationEngine> <{engine_iri}> ."),
        // The engine and version as provenance: the run is attributed to this engine.
        format!(
            "<{engine_iri}> <{RDFS_LABEL}> \"{}\" .",
            escape_literal(&format!("{} {}", engine.name(), engine.version()))
        ),
        format!("<{act_iri}> <{LANG_NS}interpretedForm> <{surface_iri}> ."),
    ];

    for reading in readings {
        // Content-address the reading on its analysis label so co-resident readings
        // (distinct analyses) get distinct reading IRIs, exactly as their forms get
        // distinct content keys.
        let reading_iri = format!(
            "{act_iri}/reading/{}",
            digest16("lang-engine-reading", &reading.analysis)
        );
        lines.push(format!(
            "<{act_iri}> <{LANG_NS}producedReading> <{reading_iri}> ."
        ));
        lines.push(format!("<{reading_iri}> <{RDF_TYPE}> <{LANG_NS}Reading> ."));
        lines.push(format!(
            "<{reading_iri}> <{LANG_NS}readingOf> <{surface_iri}> ."
        ));
        // The vantage the reading is HELD FROM — never an unattributed assertion.
        lines.push(format!(
            "<{reading_iri}> <{GMEOW_NS}vantage> <{}> .",
            reading.vantage
        ));
        lines.push(format!(
            "<{reading_iri}> <{LANG_NS}denotationContext> <{}> .",
            reading.denotation_context
        ));
        lines.push(format!(
            "<{reading_iri}> <{LOGIC_NS}confidence> \"{}\"^^<{XSD_DOUBLE}> .",
            reading.confidence
        ));
        lines.push(format!(
            "<{reading_iri}> <{RDFS_LABEL}> \"{}\" .",
            escape_literal(&reading.analysis)
        ));
    }

    Ok(ntriples_sorted(lines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_lang_form::{Form, Slot, SurfaceForm, dedup_by_content_key};

    /// A leaf lexeme form for a slot.
    fn lexeme(lemma: &str) -> Form {
        Form::Lexeme {
            sign_system: "english".to_owned(),
            lemma: lemma.to_owned(),
            part_of_speech: None,
        }
    }

    /// One slot at `index` filling in `form`.
    fn slot(index: u32, form: Form) -> Slot {
        Slot {
            index,
            role: None,
            dep_relation: None,
            depends_on: None,
            form,
        }
    }

    /// The two co-resident readings of "saw her duck": reading A analyses `duck` as
    /// the bird (a nominal head), reading B as the crouch action (a verbal head).
    /// Same surface, DISTINCT analysis strings → distinct `Composed` forms.
    fn saw_her_duck_readings() -> (Reading, Reading) {
        let composed = |analysis: &str, head_lemma: &str| Form::Composed {
            sign_system: "english".to_owned(),
            level: "clause".to_owned(),
            analysis: Some(analysis.to_owned()),
            head: Some(2),
            slots: vec![
                slot(0, lexeme("see")),
                slot(1, lexeme("her")),
                slot(2, lexeme(head_lemma)),
            ],
        };
        let reading_a = Reading {
            analysis: "duck-as-bird".to_owned(),
            form: composed("duck-as-bird", "duck.n"),
            vantage: "http://example.org/lang/engineVantage".to_owned(),
            confidence: 0.55,
            denotation_context: "http://example.org/lang/ctx".to_owned(),
        };
        let reading_b = Reading {
            analysis: "duck-as-crouch".to_owned(),
            form: composed("duck-as-crouch", "duck.v"),
            vantage: "http://example.org/lang/engineVantage".to_owned(),
            confidence: 0.45,
            denotation_context: "http://example.org/lang/ctx".to_owned(),
        };
        (reading_a, reading_b)
    }

    /// A surface engine that never provides provenance — the counter-capability the
    /// emitter must refuse rather than fabricate attribution for.
    struct UnattributingEngine;
    impl NlpEngine for UnattributingEngine {
        fn name(&self) -> &str {
            "unattributing"
        }
        fn version(&self) -> &str {
            "0"
        }
        fn provides_provenance(&self) -> bool {
            false
        }
        fn interpret(&self, _surface: &SurfaceForm) -> Result<Vec<Reading>, EngineError> {
            Err(EngineError::InterpretFailed {
                engine: "unattributing".to_owned(),
                reason: "no provenance".to_owned(),
            })
        }
    }

    fn saw_her_duck_surface() -> SurfaceForm {
        SurfaceForm {
            text: "saw her duck".to_owned(),
            script: "Latn".to_owned(),
            encoding: "UTF-8".to_owned(),
            normalization: "NFC".to_owned(),
            collation: "en".to_owned(),
        }
    }

    /// The registry hard-fails naming an unregistered engine — never a silent skip.
    #[test]
    fn registry_hard_fails_on_absent_engine() {
        let registry = EngineRegistry::new();
        let err = match registry.get("absent") {
            Ok(_) => panic!("an unregistered engine name must be a hard fail, not Ok"),
            Err(e) => e,
        };
        assert_eq!(err, EngineError::UnregisteredEngine("absent".to_owned()));
        assert!(
            err.to_string()
                .contains("no NLP engine registered under name 'absent'")
        );
    }

    /// A registered engine resolves through the seam and reports its single-site name.
    #[test]
    fn registry_resolves_a_registered_engine() {
        let (a, b) = saw_her_duck_readings();
        let mut registry = EngineRegistry::new();
        registry.register(
            "fixture-ud",
            Box::new(FixtureEngine::new("fixture-ud", "1.0", vec![a, b])),
        );
        let engine = registry
            .get("fixture-ud")
            .expect("registered engine resolves");
        assert_eq!(engine.name(), "fixture-ud");
        assert!(engine.provides_provenance());
        assert_eq!(registry.names(), vec!["fixture-ud"]);
    }

    /// R6 / Gate 5 — the ambiguity-survival invariant at the identity layer: the two
    /// co-resident readings build DISTINCT `Composed` forms (distinct `content_key`),
    /// `dedup_by_content_key` keeps BOTH, and both vantages and denotation contexts
    /// are retained — no stage collapses the co-resident readings.
    #[test]
    fn co_resident_readings_survive_dedup_by_content_key() {
        let (a, b) = saw_her_duck_readings();

        // Distinct analyses → distinct content keys (the interner never merges them).
        assert_ne!(
            a.form.content_key(),
            b.form.content_key(),
            "co-resident readings with distinct analyses must key distinctly"
        );

        let mut forms = vec![a.form.clone(), b.form.clone()];
        dedup_by_content_key(&mut forms);
        assert_eq!(
            forms.len(),
            2,
            "dedup_by_content_key must keep BOTH co-resident readings, not collapse them"
        );

        // Both readings are held from a vantage and anchored in a denotation context.
        for reading in [&a, &b] {
            assert!(!reading.vantage.is_empty(), "every reading is vantage-held");
            assert!(
                !reading.denotation_context.is_empty(),
                "every reading names its denotation context"
            );
        }
    }

    /// The engine interprets the surface into two co-resident readings, and the
    /// projection emits BOTH — with both vantages intact — as a `lang:InterpretationAct`.
    #[test]
    fn interpret_and_emit_keeps_both_readings_attributed() {
        let (a, b) = saw_her_duck_readings();
        let engine = FixtureEngine::new("fixture-ud", "1.0", vec![a, b]);
        let readings = engine
            .interpret(&saw_her_duck_surface())
            .expect("fixture engine interprets");
        assert_eq!(readings.len(), 2, "both co-resident readings are returned");

        let act = "http://example.org/lang/act/saw-her-duck";
        let surface = "http://example.org/lang/surface/saw-her-duck";
        let bytes = interpretation_act_to_ntriples(act, &engine, surface, &readings)
            .expect("attributing engine emits");
        let ntriples = String::from_utf8(bytes).expect("N-Triples is UTF-8");

        // The act is a lang:InterpretationAct and a gmeow:Activity, attributed to the
        // engine + version, interpreting the surface.
        assert!(ntriples.contains(&format!(
            "<{act}> <{RDF_TYPE}> <{LANG_NS}InterpretationAct> ."
        )));
        assert!(ntriples.contains(&format!("<{act}> <{RDF_TYPE}> <{GMEOW_NS}Activity> .")));
        assert!(ntriples.contains("fixture-ud 1.0"));

        // BOTH readings survive to the projection, each producing exactly one
        // producedReading edge, and NO resolvedReading is emitted (no silent winner).
        assert_eq!(
            ntriples
                .matches(&format!("<{LANG_NS}producedReading>"))
                .count(),
            2,
            "both co-resident readings must survive to the projection"
        );
        assert!(
            !ntriples.contains(&format!("<{LANG_NS}resolvedReading>")),
            "the engine seam must never pick a silent winner"
        );
    }

    /// R8 — the emitter's output is vantage-held corpus data: every produced reading
    /// carries a `gmeow:vantage` (never an unattributed assertion) and no reading is
    /// resolved to a canonical winner. The two facts together are what let the emission
    /// live safely in a non-EDB corpus graph rather than a reasoned input.
    #[test]
    fn r8_engine_output_is_vantage_held_corpus_data() {
        let (a, b) = saw_her_duck_readings();
        let engine = FixtureEngine::new("fixture-ud", "1.0", vec![a.clone(), b.clone()]);
        let act = "http://example.org/lang/act/saw-her-duck";
        let surface = "http://example.org/lang/surface/saw-her-duck";
        let ntriples = String::from_utf8(
            interpretation_act_to_ntriples(act, &engine, surface, &[a, b]).expect("emits"),
        )
        .expect("UTF-8");

        // One gmeow:vantage per produced reading: no reading is an unattributed fact.
        let produced = ntriples
            .matches(&format!("<{LANG_NS}producedReading>"))
            .count();
        let vantages = ntriples.matches(&format!("<{GMEOW_NS}vantage>")).count();
        assert_eq!(
            produced, vantages,
            "every produced reading must carry a gmeow:vantage (no unattributed structure)"
        );
        assert!(
            !ntriples.contains(&format!("<{LANG_NS}resolvedReading>")),
            "corpus emission never asserts a resolved winner"
        );
    }

    /// The emitter HARD-FAILS on an engine that cannot attribute its output rather than
    /// fabricating attribution — provenance is a queried capability.
    #[test]
    fn emit_hard_fails_on_unattributing_engine() {
        let (a, b) = saw_her_duck_readings();
        let err = interpretation_act_to_ntriples(
            "http://example.org/lang/act/x",
            &UnattributingEngine,
            "http://example.org/lang/surface/x",
            &[a, b],
        )
        .expect_err("an engine with no provenance must not be emitted as vantage-held readings");
        assert_eq!(
            err,
            EngineError::UnattributableEngine("unattributing".to_owned())
        );
    }

    /// The projection is deterministic: two runs over the same readings are
    /// byte-identical (the project-wide determinism bar).
    #[test]
    fn emission_is_byte_deterministic() {
        let (a, b) = saw_her_duck_readings();
        let engine = FixtureEngine::new("fixture-ud", "1.0", vec![]);
        let act = "http://example.org/lang/act/saw-her-duck";
        let surface = "http://example.org/lang/surface/saw-her-duck";
        let first = interpretation_act_to_ntriples(act, &engine, surface, &[a.clone(), b.clone()])
            .expect("emits");
        let second = interpretation_act_to_ntriples(act, &engine, surface, &[a, b]).expect("emits");
        assert_eq!(first, second, "two runs must be byte-identical");
    }
}
