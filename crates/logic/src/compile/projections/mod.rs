// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection back-ends: [`LogicProgram`] → each target format.
//!
//! A faithful Rust port of `src/gmeow_tools/logic_projections.py` — the
//! projection phase of the #664 compiler.  Seven targets:
//!
//! * [`text::project_datalog`], [`text::project_n3`], [`text::project_nemo`] —
//!   **byte-identical** text targets (the conformance goldens compare bytes).
//! * [`rdf::project_owl_dl`], [`rdf::project_owl_el`], [`rdf::project_gufo`],
//!   [`rdf::project_canonical_rdf12`] — **RDF-isomorphic** targets (serialized via
//!   oxigraph; the goldens compare by graph isomorphism).
//!
//! Each projection declares its [`PreservationKind`] + complexity class; the
//! overclaim gate ([`assert_no_overclaim`]) turns the build red when a target
//! claims `ExactPreservation` but dropped content.  [`report::build_projection_report`]
//! aggregates the loss ledger.

pub mod rdf;
pub mod report;
pub mod text;

use super::ir::{LogicAxiom, LogicModality, LogicProgram, PreservationKind};

/// All eight committed artifacts produced from one [`LogicProgram`] — the unit
/// the `LogicGenerator` (and the PyO3 `compile_logic`) writes to disk.
#[derive(Debug, Clone)]
pub struct CompiledArtifacts {
    /// `generated/owl/gmeow-dl.ttl`.
    pub owl_dl: String,
    /// `generated/owl/gmeow-el.ttl`.
    pub owl_el: String,
    /// `generated/datalog/gmeow.dl`.
    pub datalog: String,
    /// `generated/n3/gmeow.n3`.
    pub n3: String,
    /// `generated/foundation/gufo.ttl`.
    pub gufo: String,
    /// `generated/logic/gmeow.logic.rdf12.ttl`.
    pub canonical_rdf12: String,
    /// `generated/logic/gmeow.rls`.
    pub nemo: String,
    /// `generated/logic/projection-report.ttl`.
    pub report: String,
}

/// Run every projection back-end over `program` and build the report — the full
/// compile surface.  Returns `Err` on a Nemo safety violation or an overclaim.
pub fn compile_program(program: &LogicProgram) -> Result<CompiledArtifacts, String> {
    let owl_dl = rdf::project_owl_dl(program).map_err(|e| e.to_string())?;
    let owl_el = rdf::project_owl_el(program).map_err(|e| e.to_string())?;
    let datalog = text::project_datalog(program);
    let n3 = text::project_n3(program);
    let gufo = rdf::project_gufo(program).map_err(|e| e.to_string())?;
    let canonical_rdf12 = rdf::project_canonical_rdf12(program).map_err(|e| e.to_string())?;
    let nemo = text::project_nemo(program)?;

    let results = [
        &owl_dl,
        &owl_el,
        &datalog,
        &n3,
        &gufo,
        &canonical_rdf12,
        &nemo,
    ];
    let owned: Vec<ProjectionResult> = results.iter().map(|r| (*r).clone()).collect();
    let report = report::build_projection_report(program, &owned).map_err(|e| e.to_string())?;

    Ok(CompiledArtifacts {
        owl_dl: owl_dl.content,
        owl_el: owl_el.content,
        datalog: datalog.content,
        n3: n3.content,
        gufo: gufo.content,
        canonical_rdf12: canonical_rdf12.content,
        nemo: nemo.content,
        report,
    })
}

// Namespaces (string constants — the byte-parity surface).
pub(crate) const LOGIC_NS: &str = super::ir::LOGIC_NAMESPACE;
pub(crate) const GMEOW_NS: &str = crate::provenance::NAMESPACE;
pub(crate) const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(crate) const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub(crate) const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
pub(crate) const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The result of running a single projection back-end (port of the Python
/// `ProjectionResult` dataclass, minus the rdflib `graph` field — RDF content is
/// re-parsed from `content` for isomorphism checks).
#[derive(Debug, Clone)]
pub struct ProjectionResult {
    /// Short target name (`"owl-dl"`, `"datalog"`, …).
    pub target: String,
    /// The serialized output (Turtle string, Datalog text, or N3/Nemo).
    pub content: String,
    /// Whether `content` is an RDF (Turtle) serialization (vs. plain text).
    pub is_rdf: bool,
    /// The declared preservation kind.
    pub preservation: PreservationKind,
    /// The declared complexity class string.
    pub complexity: String,
    /// Structural lossy-drop notes (from the target metadata).
    pub lossy_drops: Vec<String>,
    /// Concrete items skipped during this run.
    pub actual_drops: Vec<String>,
}

/// Per-target metadata: `(preservationKind, complexityClass, structural drops)`.
/// A verbatim port of `_TARGET_META`.
pub(crate) fn target_meta(target: &str) -> (PreservationKind, &'static str, Vec<&'static str>) {
    match target {
        "owl-dl" => (
            PreservationKind::SoundUnder,
            "decidable/N2EXPTIME",
            vec![
                "modal/world context is erased",
                "contextual scope (standpoint, time, confidence) is dropped",
                "rule bodies mapped to OWL axioms where OWL is expressive enough; \
                 existential rules beyond OWL DL expressivity are dropped",
                "probabilistic profile not representable in OWL DL",
            ],
        ),
        "owl-el" => (
            PreservationKind::SoundUnder,
            "PTIME",
            vec![
                "modal/world context is erased",
                "contextual scope (standpoint, time, confidence) is dropped",
                "only EL-safe axioms emitted (no disjointness, no inverseOf, \
                 no cardinality restrictions, no nominals)",
                "rules beyond EL expressivity are dropped",
            ],
        ),
        "datalog" => (
            PreservationKind::SoundUnder,
            "terminating/PTIME-data",
            vec![
                "modal/world context flattened to predicate reification",
                "no existential rule heads (skolemisation not emitted)",
                "OWL class expressions not representable as Datalog atoms are dropped",
            ],
        ),
        "n3" => (
            PreservationKind::CompleteOver,
            "semi-decidable",
            vec![
                "modal context encoded as quoted graph arguments (may overgenerate)",
                "N3 builtins used for arithmetic/string predicates where available",
            ],
        ),
        "gufo" => (
            PreservationKind::ValidationOnly,
            "PTIME",
            vec![
                "only gUFO-mapped sorts and structural predicates emitted",
                "logic: world-modal/contextual structure has no gUFO equivalent",
                "rules not representable in gUFO; only type/subtype declarations kept",
                "fluents / 4D temporal-part structure (logic:Fluent, \
                 logic:temporalPartOf, logic:Process, logic:Perdurant beyond \
                 gufo:Event) have no gUFO class and are dropped",
                "multi-level / HiLog instantiation (logic:instanceOf, \
                 logic:orderedType — arbitrary type-of-type levels) has no gUFO \
                 punning equivalent and is dropped",
                "scoped open/closed worlds (logic:WorldBoundary, logic:closedUnder, \
                 scoped-CWA) have no gUFO equivalent and are dropped",
                "native builtins (logic:Builtin and the builtin individuals) have no \
                 gUFO class and are dropped",
                "strict-partial-order parthood characteristics (asymmetric + \
                 irreflexive on logic:properPartOf) cannot be declared in OWL 2 DL \
                 gUFO (it only states transitivity) and are dropped",
                "native edge-property metadata (RDF-1.2 statement-level scope on \
                 logic edges) has no gUFO equivalent and is dropped",
                "the five gUFO temporary-situation reifiers \
                 (QualityValueAttributionSituation, TemporaryConstitution/\
                 Instantiation/Parthood/RelationshipSituation) are SUPERSEDED by \
                 logic:Fluent + RDF-1.2 edge properties — not re-emitted by the \
                 down-projection",
                "logic:Mode (and Intrinsic/Extrinsic mode/aspect refinements) has no \
                 single gUFO base class as a projection target and is dropped \
                 (gufo:Aspect is emitted; gufo:Mode does not exist)",
                "preservation kind is ValidationOnly: gUFO is an anti-pattern check, \
                 not an entailment surface",
            ],
        ),
        "canonical-rdf12" => (
            PreservationKind::Exact,
            "N/A (identity serialization)",
            vec![],
        ),
        "nemo" => (PreservationKind::Exact, "PTIME/datalog", vec![]),
        other => panic!("unknown projection target: {other}"),
    }
}

/// Raised when a projection's declared preservation is stronger than achieved
/// (port of `OverclaimError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverclaimError(pub String);

impl std::fmt::Display for OverclaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OverclaimError {}

/// Assert that `declared` is not stronger than what `drops` implies — i.e. an
/// `ExactPreservation` target must drop nothing (LOGIC-CONFORMANCE §overclaim→red).
pub fn assert_no_overclaim(
    target: &str,
    declared: PreservationKind,
    drops: &[String],
) -> Result<(), OverclaimError> {
    if declared == PreservationKind::Exact && !drops.is_empty() {
        let shown: Vec<&str> = drops.iter().take(10).map(String::as_str).collect();
        return Err(OverclaimError(format!(
            "Overclaim in projection '{target}': declared logic:{} (ExactPreservation) \
             but {} item(s) were dropped:\n  {}",
            PreservationKind::Exact.as_str(),
            drops.len(),
            shown.join("\n  ")
        )));
    }
    Ok(())
}

// --------------------------------------------------------------------------- //
// Shared helpers (used by both text and rdf back-ends)
// --------------------------------------------------------------------------- //

/// Whether an axiom carries non-trivial contextual scope (port of
/// `_is_modal_or_scoped`).
pub(crate) fn is_modal_or_scoped(axiom: &LogicAxiom) -> bool {
    axiom.scope.modality != LogicModality::None
        || axiom.scope.standpoint.is_some()
        || axiom.scope.time.is_some()
        || axiom.scope.confidence.is_some()
        || axiom.scope.provenance.is_some()
}

/// The standard GENERATED header for a target (port of `_generated_banner`).
pub(crate) fn generated_banner(target: &str) -> String {
    format!(
        "# GENERATED by `gmeow logic compile` (logic_projections.py) — DO NOT EDIT.\n\
         # {target} projection of the canonical logic: program.\n"
    )
}

/// CPython `repr(s)` for a string value — the byte-parity hinge for literal
/// values in the Datalog and N3 text targets.  Matches CPython: single-quote by
/// default, switch to double-quote only when the string contains `'` but not `"`;
/// escape `\`, the active quote, and the C0 control characters.
pub(crate) fn python_repr(s: &str) -> String {
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests;
