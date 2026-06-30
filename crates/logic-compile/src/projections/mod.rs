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

pub mod correspondence;
// The dsl/mappings/ frontend: authored alignment cells → typed logic:Correspondence set.
pub mod correspondence_frontend;
// The correspondence overclaim gate (relation/morphism vs emitted predicate; P5).
pub mod correspondence_gate;
// The lawful put leg derived from the same node as get (F4 mnemomorphism up-lift).
pub mod put_derivation;
// The five correspondence conformance gates (Law/Overclaim/Round-trip/Mnemomorphism/Composition).
pub mod correspondence_gates;
// The EDOAL correspondence lowering (get leg + relation lattice → EDOAL alignment).
pub mod edoal;
// The FnO correspondence lowering (get-leg transform functions → FnO catalog).
pub mod fno;
// The shared get leg both EDOAL and SPARQL lower from (spec-drift gone by construction).
pub mod get_leg;
pub mod paths;
pub mod rdf;
pub mod report;
// The SPARQL-CONSTRUCT correspondence lowering (get leg → executable CONSTRUCT).
pub mod sparql;
// The SSSOM correspondence lowering (1:1 lattice band → SSSOM TSV).
pub mod sssom;
pub mod text;

use super::ir::{LogicAxiom, LogicModality, LogicProgram, PreservationKind};
use correspondence::CorrespondenceProgram;
use correspondence_gates::CorrespondenceGateReport;
use paths::PathProjection;
use put_derivation::DerivedPutOutcome;

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
    /// The whole-program + path-shape projection rows (the seven standard targets plus
    /// the per-shape `property-path:<iri>` rows) that fed [`report`](Self::report).
    /// Surfaced so a downstream assembler (the pipeline) can union them with the
    /// correspondence-calculus loss ledger and serialize the FINAL projection report
    /// over the union through the one routine — the committed report's logic rows stay
    /// byte-identical.
    pub logic_projections: Vec<ProjectionResult>,
    /// The three header counts of [`report`](Self::report) — surfaced for the same
    /// reason as [`logic_projections`](Self::logic_projections).
    pub report_header: report::ReportHeader,
    /// The five-gate verdict report over the program's correspondences (F4) — `None` when
    /// the program declares no correspondences (so a correspondence-free compile is
    /// byte-unchanged). The per-correspondence gates are evaluated with no compositions;
    /// the conformance runner re-evaluates with the case's declared compositions.
    pub correspondence_gates: Option<CorrespondenceGateReport>,
    /// The per-correspondence put-derivation outcomes (the derived legs / mint-with-claim
    /// / unsupported residue) — the source of the up-lift loss-ledger rows and the derived
    /// liftability statistic. Empty when no put leg was derived.
    pub correspondence_outcomes: Vec<DerivedPutOutcome>,
    /// The derived correspondence program (every put-less cell's `put` minted) — surfaced
    /// so the conformance runner re-runs the gates with the case's compositions without
    /// rebuilding. `None` when the program declares no correspondences.
    pub correspondence_program: Option<CorrespondenceProgram>,
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

    // Teleology-specific lossy disclosure.  When the program carries the flat
    // gmeow:satisfiedBy edge generated from a factored logic:GoalEvaluation, the
    // OWL/flat surfaces cannot represent the factored axes (satisfaction /
    // feasibility / lifecycle status, satisfaction degree, criterion,
    // evaluator/standpoint vantage multiplicity).  Record the collapse as a
    // structural drop on each lossy target HERE, in the production compile funnel,
    // so the projection report AND the preservation ledger both disclose it on the
    // real `gmeow logic compile` surface — not just under the conformance harness
    // (maximal information flow; the two summaries are built from `owned` below and
    // therefore agree).
    if program
        .axioms
        .iter()
        .any(|a| a.predicate == SATISFIED_BY_IRI)
    {
        for result in &mut owned {
            if GOAL_EVAL_COLLAPSE_TARGETS.contains(&result.target.as_str()) {
                result.lossy_drops.push(GOAL_EVAL_COLLAPSE_DROP.to_owned());
            }
        }
    }

    // Correspondence calculus (F4): derive the lawful put leg for every put-less cell and
    // evaluate the five gates BEFORE the report, so the derived liftability statistic
    // (`lawfulUpliftCount / correspondenceCount` over the gate verdicts) folds into the
    // report header. Gated on a non-empty correspondence set so a correspondence-free
    // compile is byte-identical (no gates, no header counts). The gate report is RECORDED
    // here (compile_program stays total); the hard-fail `assert_gates` is thrown only by
    // the pipeline stage. The per-correspondence gates run with no compositions — the
    // conformance runner re-evaluates with the case's declared compositions over
    // `correspondence_program`.
    let (correspondence_gates, correspondence_outcomes, correspondence_program) =
        if program.correspondences.is_empty() {
            (None, Vec::new(), None)
        } else {
            let assembled = CorrespondenceProgram::new(
                program.correspondences.clone(),
                Vec::new(),
                PreservationKind::SoundUnder,
            )
            .with_leg_programs(program.transaction_programs.clone());
            let (derived, outcomes) = assembled.with_derived_puts()?;
            let report = correspondence_gates::evaluate_gates(&derived, &[]);
            (Some(report), outcomes, Some(derived))
        };

    let report_header = {
        let base = report::ReportHeader::of_program(program);
        match &correspondence_gates {
            Some(gates) => base.with_lawful_uplift(correspondence_gates::liftability(gates).lawful),
            None => base,
        }
    };
    let report =
        report::build_projection_report_from(report_header, &owned).map_err(|e| e.to_string())?;

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
        logic_projections: owned,
        report_header,
        correspondence_gates,
        correspondence_outcomes,
        correspondence_program,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Build the **one preservation row per correspondence** the canonical doc requires
/// (`LOGIC-CORRESPONDENCE.md` line ~78): the loss ledger attributes a dropped
/// construct to the leg that dropped it.  `dialect` selects the pinned preservation +
/// the dialect-level structural drops (the get/put-leg/caveat/standpoint losses
/// from [`target_meta`]); `key` is the stable per-correspondence target name
/// (`<dialect>:<correspondence-iri-or-cell::profile>`); `residue` is the concrete,
/// per-correspondence flagged set (profile losses + A1's rejected constructs),
/// each note already attributed to its leg.  The static drops live in `lossy_drops`,
/// the concrete per-correspondence drops in `actual_drops` — the report serializes
/// both as `gmeow:lossyDrop`.
pub(crate) fn correspondence_result(
    dialect: &str,
    key: &str,
    residue: Vec<String>,
) -> ProjectionResult {
    use sha2::{Digest, Sha256};

    let (kind, complexity, structural) = target_meta(dialect_target(dialect));
    // The per-correspondence key embeds full IRIs + separators (`|`, `::`, spaces) that
    // are illegal in an IRI, so the target NAME (which the report uses as the IRI's local
    // segment) is `<dialect>:<sha256(key)[:16]>` — a stable, collision-free, IRI-legal
    // identity. The human-readable key is preserved as the first residue note so nothing
    // is lost.
    let digest = Sha256::digest(format!("{dialect}\u{1f}{key}").as_bytes());
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let mut actual_drops = Vec::with_capacity(residue.len() + 1);
    actual_drops.push(format!("correspondence: {key}"));
    actual_drops.extend(residue);
    ProjectionResult {
        target: format!("{dialect}:{short}"),
        // The legal output is the dialect artifact itself (written elsewhere); the row
        // is a preservation/residue record, not a serialization, so content is empty.
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: complexity.to_owned(),
        lossy_drops: structural.into_iter().map(str::to_owned).collect(),
        actual_drops,
    }
}

/// Map a correspondence dialect name to its [`target_meta`] key (SPARQL's metadata
/// lives under `"sparql-construct"`; the others are 1:1).
fn dialect_target(dialect: &str) -> &str {
    match dialect {
        "sparql" | "sparql-construct" => "sparql-construct",
        other => other,
    }
}

/// One loss-ledger note per `logic:Formula` a Horn-fragment target cannot carry, each
/// tagged with the closed [`FormulaShape`](crate::ir::FormulaShape) set naming *which*
/// first-order constructs exceed the Horn+NAF fragment — carried-and-flagged in the
/// canonical `logic:` layer (lossless under canonical-rdf12, reached for execution through
/// the relational-core lowering), never silently dropped (take1 §10.1 legalization). The
/// per-instance, closed-tag form keeps the ledger informative and the goldens stable (no
/// free text). Emitted only when the program carries formulas, so a formula-free program's
/// ledger is byte-unchanged.
fn formula_residue_notes(program: &LogicProgram, target_label: &str) -> Vec<String> {
    program
        .formulas
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let tags = f
                .shape_tags()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("+");
            format!(
                "logic:Formula #{i} [{tags}] (full first-order quantifier / connective tree) \
                 is not representable in {target_label}; it remains in the canonical logic: \
                 layer (carried by canonical-rdf12) as flagged unsupported residue"
            )
        })
        .collect()
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
                "transaction-path evolution has no faithful OWL DL projection: \
                 serial conjunction, executional entailment over a path of states, \
                 elementary update as supersession, the hypothetical-execution \
                 sandbox and its discarded-run witness, and concurrent composition \
                 with conflict-serializability have no OWL axiom form and are \
                 dropped — OWL describes a single static state, not a transaction path",
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
                "transaction-path evolution has no faithful OWL EL projection: \
                 serial conjunction, executional entailment over a path of states, \
                 elementary update as supersession, the hypothetical-execution \
                 sandbox and its discarded-run witness, and concurrent composition \
                 with conflict-serializability have no OWL axiom form and are \
                 dropped — OWL describes a single static state, not a transaction path",
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
                "negation-as-failure guards are dropped; monotone log:implies cannot \
                 express a defeater, so a defeasible rule is over-approximated \
                 (CompleteOver) rather than emitting the guard as a positive antecedent",
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
        // ── Correspondence-calculus alignment lowerings ──────────────────────────
        "sssom" => (
            PreservationKind::SoundUnder,
            "N/A (1:1 lattice band)",
            vec![
                "the caveat/law/leg structure of the correspondence is dropped; only \
                 subject/predicate/object, confidence, and justification survive",
                "world/standpoint scope and the put leg are not carried",
            ],
        ),
        "fno" => (
            PreservationKind::ValidationOnly,
            "N/A (transform signatures)",
            vec![
                "FnO is not an entailment surface: parameter/output signatures are exact, \
                 but the transform's semantics are validation-only",
                "the correspondence relation, caveats, and standpoint scope are dropped",
            ],
        ),
        "edoal" => (
            PreservationKind::SoundUnder,
            "N/A (alignment)",
            vec![
                "the SOL caveats, the put leg, and world/standpoint scope are dropped",
                "EDOAL carries the get leg + relation + measure only",
            ],
        ),
        "sparql-construct" => (
            PreservationKind::SoundUnder,
            "terminating/PTIME-data",
            vec![
                "the faithful executable down-projection; per-profile losses are made \
                 explicit in the query header (`# Lossy and directional by design; drops:`)",
                "world/standpoint scope and the put leg are not carried",
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
const LEDGER_TARGETS: [&str; 12] = [
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "nemo",
    "property-path",
    // The correspondence-calculus alignment lowerings: each carries its own
    // preservation judgment in the same loss ledger as OWL/Datalog/gUFO.
    "sssom",
    "fno",
    "edoal",
    "sparql-construct",
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
// Teleology-specific preservation disclosure (production pipeline)
// --------------------------------------------------------------------------- //

/// The full IRI of `gmeow:satisfiedBy` — the flat binary projection of a satisfied
/// + completed `logic:GoalEvaluation`.
pub const SATISFIED_BY_IRI: &str = "https://blackcatinformatics.ca/gmeow/satisfiedBy";

/// The drop note appended to every LOSSY projection target when the teleology
/// materialization emitted a `gmeow:satisfiedBy` edge.
///
/// Exact-preservation targets (`canonical-rdf12`, `nemo`) carry the full
/// `logic:GoalEvaluation` structure in their materialized output and are excluded.
pub const GOAL_EVAL_COLLAPSE_DROP: &str = concat!(
    "logic:GoalEvaluation factored axes (satisfaction/feasibility/lifecycle status, ",
    "satisfaction degree, criterion, evaluator/standpoint vantage multiplicity) ",
    "collapsed to flat binary gmeow:satisfiedBy edge"
);

/// Targets that lose the `logic:GoalEvaluation` structure when a `satisfiedBy`
/// collapse is present.  `canonical-rdf12` and `nemo` are exact-preservation
/// targets and carry the full evaluation in their materialized output — they are NOT
/// augmented.
pub const GOAL_EVAL_COLLAPSE_TARGETS: &[&str] = &["owl-dl", "owl-el", "gufo", "datalog", "n3"];

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
    let mut notes: Vec<String> = program
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
        .collect();
    // The full-FOL formula layer is beyond every Horn-fragment target; disclose each formula
    // as its own shape-tagged drop (take1 §10.1 legalization — carried+flagged, never
    // silent). A formula-free program adds nothing, so its ledger is byte-unchanged.
    notes.extend(formula_residue_notes(program, target_label));
    notes
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
