// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection back-ends: [`LogicProgram`] → each target format.
//!
//! The projection phase of the GMEOW Logic compiler; the Python duplicate
//! (`logic_projections.py`) has been retired.  Seven targets:
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

pub mod paths;
pub mod rdf;
pub mod report;
pub mod text;

use super::ir::{LogicAxiom, LogicModality, LogicProgram, PreservationKind};
use paths::PathProjection;

/// All artifacts produced from one [`LogicProgram`] — the unit the
/// `LogicGenerator` (and the PyO3 `compile_logic`) writes to disk.
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
    /// The `% === Rules ===` section of [`nemo`](Self::nemo) — the rule text the
    /// native reasoning engines (`materialize` / `certify` / `stable_models`)
    /// consume.  Surfaced so Python no longer re-extracts it from `nemo`.
    pub nemo_rules: String,
    /// The preservation ledger: `target -> (preservationKind, complexityClass,
    /// structural lossy-drop notes)`.  Surfaced as JSON by `compile_logic` so the
    /// conformance runner no longer rebuilds it from Python `ProjectionResult`s.
    pub preservation_ledger: Vec<LedgerEntry>,
    /// Per-shape property-path projections for every `logic:PathShape` declared in
    /// the program.  Each entry carries the SPARQL property-path expression, the
    /// depth-bounded Datalog rule scheme, and the `"property-path"` ledger row.
    /// Empty when the program declares no path shapes — never absent.
    pub path_projections: Vec<PathProjection>,
}

/// One preservation-ledger row (the per-target metadata the conformance runner
/// compares against `expected/projections/preservation-ledger.json`).
#[derive(Debug, Clone)]
pub struct LedgerEntry {
    /// Short target name (`"owl-dl"`, `"datalog"`, …).
    pub target: String,
    /// The declared preservation kind value string (e.g. `"ExactPreservation"`).
    pub preservation: String,
    /// The declared complexity class string.
    pub complexity: String,
    /// Structural lossy-drop notes (the `lossy_drops` field of the Python ledger).
    pub lossy_drops: Vec<String>,
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
    let mut owned: Vec<ProjectionResult> = results.iter().map(|r| (*r).clone()).collect();

    // Property-path projections: every logic:PathShape → (property_path, datalog,
    // ledger row).  Wired here so the target is genuinely exercised in the compile
    // funnel, not inert vocabulary.  Computed BEFORE the report so
    // the per-shape `property-path:<iri>` rows appear in BOTH exported summaries
    // (the report Turtle and the preservation ledger) — they must agree (maximal
    // information flow; the two summaries are surfaced together via compile_logic
    // and run_case).
    let path_projections = paths::project_path_shapes(program);

    // One ProjectionResult per path shape, keyed `property-path:<iri>`, fed into the
    // report alongside the seven whole-program projections so the report carries the
    // path targets too.  The kind is the declared `property-path` preservation; a
    // path projection records no `actual_drops` (the overclaim gate is a no-op for
    // its SoundUnder kind).
    let (pp_kind, _, _) = target_meta("property-path");
    let path_results: Vec<ProjectionResult> = path_projections
        .iter()
        .map(|pp| ProjectionResult {
            target: format!("property-path:{}", pp.shape_iri),
            content: pp.property_path.clone(),
            is_rdf: false,
            preservation: pp_kind,
            complexity: pp.ledger.complexity.clone(),
            lossy_drops: pp.ledger.lossy_drops.clone(),
            actual_drops: Vec::new(),
        })
        .collect();
    owned.extend(path_results);

    let report = report::build_projection_report(program, &owned).map_err(|e| e.to_string())?;

    // Preservation ledger: per-target (kind, complexity, structural drops).  The
    // runner compares only the structural `lossy_drops` (not `actual_drops`), so
    // this mirrors the Python `ledger_json` builder exactly.  `owned` already carries
    // the path `property-path:<iri>` rows (appended above), so the ledger and the
    // report are built from the SAME target list and cannot drift.
    let preservation_ledger: Vec<LedgerEntry> = owned
        .iter()
        .map(|p| LedgerEntry {
            target: p.target.clone(),
            preservation: p.preservation.as_str().to_owned(),
            complexity: p.complexity.clone(),
            lossy_drops: p.lossy_drops.clone(),
        })
        .collect();

    // The rule section of the nemo projection — the reasoning-engine surface.
    let nemo_rules = text::extract_nemo_rules_section(&nemo.content)?;

    Ok(CompiledArtifacts {
        owl_dl: owl_dl.content,
        owl_el: owl_el.content,
        datalog: datalog.content,
        n3: n3.content,
        gufo: gufo.content,
        canonical_rdf12: canonical_rdf12.content,
        nemo: nemo.content,
        report,
        nemo_rules,
        preservation_ledger,
        path_projections,
    })
}

// Namespaces (string constants — the byte-parity surface).
pub(crate) const LOGIC_NS: &str = super::ir::LOGIC_NAMESPACE;
// The GMEOW namespace. The runtime mirrors this as `gmeow_logic::provenance::NAMESPACE`;
// the wasm-able compiler keeps its own copy to stay free of the oxigraph-coupled
// runtime provenance module.
pub(crate) const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
pub(crate) const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(crate) const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub(crate) const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
pub(crate) const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The result of running a single projection back-end (the `ProjectionResult`
/// value; RDF content is re-parsed from `content` for isomorphism checks).
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
/// The per-target metadata table (the loss-ledger source of truth).
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
        "property-path" => (
            PreservationKind::SoundUnder,
            "terminating/PTIME-data (bounded); regular (unbounded)",
            vec![
                "bounded depth {n,m} and the predicate wildcard are GMEOW property-path \
                 extensions beyond SPARQL 1.1 §9; a consumer restricted to standard SPARQL \
                 receives the unrolled (bounded) or approximated (wildcard) form",
                "a predicate wildcard has no SPARQL §9 operator; its edge relation is \
                 materialized by a namespace-scoped pre-pass before the depth closure",
                "modal/world context and contextual scope are not carried by a path surface",
            ],
        ),
        other => panic!("unknown projection target: {other}"),
    }
}

/// The fixed set of whole-program projection targets, in the order
/// [`projection_ledger_rows`] emits them BEFORE sorting.  These are exactly the
/// standard targets [`compile_program`] runs (the per-shape `property-path:<iri>`
/// rows are program-dependent and so are NOT part of this static surface; the
/// generic `property-path` row IS).
const LEDGER_TARGETS: [&str; 8] = [
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "nemo",
    "property-path",
];

/// One row of the preservation loss ledger as a public, owned value: a projection
/// target with its declared preservation kind, complexity class, and the
/// structural lossy-drop notes for that target.  This is the documentation-facing
/// surface over the crate-private [`target_meta`] table — the loss-ledger source
/// of truth — so the docs renderers never reach into compiler internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionLedgerRow {
    /// Short target name (`"owl-dl"`, `"datalog"`, …).
    pub target: String,
    /// The declared preservation kind value string (e.g. `"ExactPreservation"`).
    pub preservation_kind: String,
    /// The declared complexity class string.
    pub complexity: String,
    /// Structural lossy-drop notes (empty for the exact-preservation targets).
    pub lossy_drops: Vec<String>,
}

/// The preservation loss ledger for the standard whole-program projection
/// targets, as owned rows sorted by target name (deterministic).  Each row is
/// built from the crate-private [`target_meta`] table, so this and the compile
/// surface cannot drift.  The per-shape `property-path:<iri>` rows a concrete
/// program contributes are program-dependent and not part of this static surface.
pub fn projection_ledger_rows() -> Vec<ProjectionLedgerRow> {
    let mut rows: Vec<ProjectionLedgerRow> = LEDGER_TARGETS
        .iter()
        .map(|target| {
            let (kind, complexity, lossy_drops) = target_meta(target);
            ProjectionLedgerRow {
                target: (*target).to_owned(),
                preservation_kind: kind.as_str().to_owned(),
                complexity: complexity.to_owned(),
                lossy_drops: lossy_drops.into_iter().map(str::to_owned).collect(),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.target.cmp(&b.target));
    rows
}

/// Raised when a projection's declared preservation is inconsistent with its flagged
/// residue — either an overclaim (too strong) or a silent under-disclosure (the floor
/// claimed but nothing flagged).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverclaimError(pub String);

impl std::fmt::Display for OverclaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OverclaimError {}

/// Enforce the legalization contract for one projection: a lowering is a total function
/// into `⟨ legal output ⊕ flagged residue ⟩` (LOGIC-IR.md § IR commitments).  Both
/// directions are machine-checked, so "never silently degrade" is a typed property, not
/// a promise:
///
/// * **Overclaim** — an [`PreservationKind::Exact`] target must drop nothing.  A
///   non-empty `residue` under an Exact claim is a build failure
///   (LOGIC-CONFORMANCE §overclaim→red).
/// * **Under-disclosure** — [`PreservationKind::Unsupported`] is the legalization floor:
///   a construct it cannot express must be *carried and flagged* (a non-empty residue),
///   never silently dropped (LOGIC-IR.md § Lowering — "carried and flagged, never
///   dropped").  An Unsupported claim with an empty residue is a build failure, because
///   the construct vanished in silence.
///
/// `residue` is the complete flagged residue for the target — the union of its structural
/// and concrete drops, exactly the set serialized as `gmeow:lossyDrop` in the report.
pub fn assert_no_overclaim(
    target: &str,
    declared: PreservationKind,
    residue: &[&str],
) -> Result<(), OverclaimError> {
    if declared == PreservationKind::Exact && !residue.is_empty() {
        let shown: Vec<&str> = residue.iter().take(10).copied().collect();
        return Err(OverclaimError(format!(
            "Overclaim in projection '{target}': declared logic:{} (ExactPreservation) \
             but {} item(s) were dropped:\n  {}",
            PreservationKind::Exact.as_str(),
            residue.len(),
            shown.join("\n  ")
        )));
    }
    if declared == PreservationKind::Unsupported && residue.is_empty() {
        return Err(OverclaimError(format!(
            "Silent under-disclosure in projection '{target}': declared logic:{} (the \
             legalization floor) but flagged no residue. An unsupported construct must be \
             carried and flagged in the loss ledger, never silently dropped.",
            PreservationKind::Unsupported.as_str(),
        )));
    }
    Ok(())
}

// --------------------------------------------------------------------------- //
// Shared helpers (used by both text and rdf back-ends)
// --------------------------------------------------------------------------- //

/// Whether an axiom carries non-trivial contextual scope.
pub(crate) fn is_modal_or_scoped(axiom: &LogicAxiom) -> bool {
    axiom.scope.modality != LogicModality::None
        || axiom.scope.standpoint.is_some()
        || axiom.scope.time.is_some()
        || axiom.scope.confidence.is_some()
        || axiom.scope.provenance.is_some()
}

/// Per-contract `actual_drops` notes for a LOSSY down-projection target.  A
/// reasoning contract is reasoning-configuration metadata; the lossy
/// rule/axiom surfaces (OWL-DL, OWL-EL, gUFO, Datalog, N3) carry no facet
/// vocabulary, so each contract a program declares is recorded as an explicit
/// drop rather than silently discarded.  The canonical RDF 1.2 target preserves
/// contracts losslessly and must NOT call this; the Nemo target consumes the
/// contract as the engine-selecting input (it is not encoded in the `.rls`).
pub(crate) fn contract_drop_notes(program: &LogicProgram, target_label: &str) -> Vec<String> {
    program
        .contracts
        .iter()
        .map(|contract| {
            let label = match contract.preset {
                Some(preset) => format!("preset logic:{}", preset.as_str()),
                None => "an anonymous faceted contract".to_owned(),
            };
            format!(
                "reasoning contract ({label}) is not representable in {target_label}; \
                 the full facet selection is dropped (preserved only in the canonical \
                 RDF 1.2 projection)"
            )
        })
        .collect()
}

/// The standard GENERATED header for a target.
pub(crate) fn generated_banner(target: &str) -> String {
    format!(
        "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
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
