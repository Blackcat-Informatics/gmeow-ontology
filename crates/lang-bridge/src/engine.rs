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
use gmeow_ns::LANG_NS;

/// The `gmeow:` namespace base — the vantage and activity vocabulary the engine
/// handoff attributes readings against.
use gmeow_ns::GMEOW_NS;

/// The `logic:` namespace base — carries the engine's `logic:confidence` (a
/// confidence, never a `logic:probability`, absent a declared probability model).
use gmeow_ns::LOGIC_NS;

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

#[path = "engine.tests.rs"]
#[cfg(test)]
mod tests;
