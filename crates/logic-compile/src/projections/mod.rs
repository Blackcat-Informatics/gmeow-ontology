// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection back-ends: [`LogicProgram`] → each target format.
//!
//! The projection phase of the GMEOW Logic compiler; the Python duplicate
//! (`logic_projections.py`) has been retired. Whole-program targets include:
//!
//! * [`text::project_datalog`] and [`text::project_n3`] —
//!   **byte-identical** text targets (the conformance goldens compare bytes).
//! * [`rdf::project_owl_dl`], [`rdf::project_owl_el`], [`rdf::project_gufo`],
//!   [`rdf::project_canonical_rdf12`] — **RDF-isomorphic** targets (serialized via
//!   oxigraph; the goldens compare by graph isomorphism).
//! * [`crate::clif::project_clif`] — the bidirectional **CLIF** s-expression FOL
//!   dialect, `PreservationKind::Exact` in both directions.
//! * [`crate::cgif::project_cgif`] — the bidirectional **CGIF** conceptual-graph FOL
//!   dialect, `PreservationKind::Exact` in both directions.
//! * [`crate::xcl::project_xcl`] — the bidirectional **XCL** XML FOL dialect,
//!   `PreservationKind::Exact` in both directions.
//! * [`shacl_af::project_shacl_af`] — the SHACL-AF `sh:SPARQLRule` **computation**
//!   surface (a byte-stable text target; the canon's derivation rules projected to a
//!   SHACL rule dialect, never bolted onto SHACL — `design/LOGIC-SHACL-AF.md`).
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
// The seven correspondence-stack soundness checks: the five alignment
// graph-reasoning checks (inverse-direction / domain-range / property-character /
// equivalence-collapse=Principle 5 / dc-refinement) + the two FnO back-end checks
// (fno-type / fno-ref), oxigraph-free over DslView.
pub mod correspondence_soundness;
// The EDOAL correspondence lowering (get leg + relation lattice → EDOAL alignment).
pub mod edoal;
// The EmotionML lowering (affect category + dimension vocabularies → EmotionML XML, a
// many-to-one lossy emitter — needs no external RDF namespace).
pub mod emotionml;
// The FnO correspondence lowering (get-leg transform functions → FnO catalog).
pub mod fno;
// The shared get leg both EDOAL and SPARQL lower from (spec-drift gone by construction).
pub mod get_leg;
pub mod paths;
pub mod rdf;
pub mod reified_claim;
pub mod report;
// The SHACL-AF rule projection (logic: derivation rules → sh:SPARQLRule) — the
// computation surface (design/LOGIC-SHACL-AF.md): computation added to the canon and
// emitted, never bolted onto SHACL (Principle 17).
pub mod shacl_af;
pub mod shapes;
// Shared low-level SPARQL token / escaping primitives, reused by the SHACL-AF rule
// projection (shacl_af) and the procedural-constraint projection (shapes) so the two
// SPARQL surfaces render terms/predicates/literals identically and cannot drift.
pub(crate) mod sparql_lower;
// The shape-component subsumption engine: enforcement-key equivalence (`≡`) and a sound
// under-approximation of the enforcement pre-order (`⊑`) over closed-world validation shapes.
pub mod subsumption;
// The lift (Galois adjoint of `derive_validation_shapes`): a validation shape → the OWL/RDFS
// + `logic:ClosureEntry` axiom-text PROPOSAL that re-derives it, with a machine-checkable
// equivalence-before-deletion certificate over the real forward derive.
pub mod lift;
// The SPARQL-CONSTRUCT correspondence lowering (get leg → executable CONSTRUCT).
pub mod sparql;
// The inverse-ingest ("put") SPARQL-CONSTRUCT lowering: the role-swap of `sparql` (external
// template atoms → gmeow source atoms + mint-with-claim envelope), derived from the same
// get-leg model so the two legs cannot drift.
pub mod sparql_put;
// The SSSOM correspondence lowering (1:1 lattice band → SSSOM TSV).
pub mod sssom;
pub mod text;

use gmeow_errors::Diag;

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
    /// `generated/cl/gmeow.clif`.
    pub clif: String,
    /// `generated/cl/gmeow.cgif`.
    pub cgif: String,
    /// `generated/cl/gmeow.xcl`.
    pub xcl: String,
    /// `generated/shacl-af/gmeow.shacl-af.ttl` — the SHACL-AF rule (computation) surface.
    pub shacl_af: String,
    /// `generated/logic/projection-report.ttl`.
    pub report: String,
    /// The preservation ledger: `target -> (preservationKind, complexityClass,
    /// structural lossy-drop notes)`.  Surfaced as JSON by `compile_logic` so the
    /// conformance runner no longer rebuilds it from Python `ProjectionResult`s.
    pub preservation_ledger: Vec<LedgerEntry>,
    /// Per-shape property-path projections for every `logic:PathShape` declared in
    /// the program.  Each entry carries the SPARQL property-path expression, the
    /// depth-bounded Datalog rule scheme, and the `"property-path"` ledger row.
    /// Empty when the program declares no path shapes — never absent.
    pub path_projections: Vec<PathProjection>,
    /// The whole-program + path-shape projection rows (the nine standard targets plus
    /// the per-shape `property-path:<iri>` rows) that fed [`report`](Self::report).
    /// Surfaced so a downstream assembler (the pipeline) can union them with the
    /// correspondence-calculus loss ledger and serialize the FINAL projection report
    /// over the union through the one routine — the committed report's logic rows stay
    /// byte-identical.
    pub logic_projections: Vec<ProjectionResult>,
    /// The three header counts of [`report`](Self::report) — surfaced for the same
    /// reason as [`logic_projections`](Self::logic_projections).
    pub report_header: report::ReportHeader,
    /// The single runtime loss store this compile interned every projection drop into —
    /// the sole origin of the `gmeow:lossyDrop` report rows and the preservation ledger's
    /// per-target drops. Handed to the pipeline mappings stage (as serialized nodes over
    /// the projection channel) so the FINAL union report reads back from the SAME store
    /// rather than a re-derived one.
    pub loss: crate::loss_ledger::LossLedger,
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
/// compile surface. Returns `Err` on a projection failure or an overclaim.
///
/// `verdicts` is the per-correspondence EXECUTED lens-law verdict map the five correspondence
/// gates read (keyed by correspondence IRI). A correspondence-free program supplies an empty
/// map (the gates never run); a program that authors `logic:Correspondence` cells MUST supply
/// a verdict for every one (an engine-adjacent caller computes them via
/// `gmeow_logic::correspondence_exec::program_verdicts` over the derived program) — a present
/// correspondence with no verdict HARD-fails in the gates, never a silent pass.
pub fn compile_program(
    program: &LogicProgram,
    verdicts: &correspondence_gates::CorrespondenceVerdicts,
) -> gmeow_errors::Result<CompiledArtifacts> {
    // The ONE runtime loss store for this compile. Every lossy producer interns its
    // structural + per-run drops here (keyed by target focus); the report and the
    // preservation ledger below both READ this same instance, so the two loss surfaces
    // cannot drift. The four Exact targets (canonical-rdf12/clif/cgif/xcl) drop nothing,
    // so they never touch the store and keep their pure signatures.
    let mut loss = crate::loss_ledger::LossLedger::new();

    let owl_dl = rdf::project_owl_dl(program, &mut loss).map_err(|e| {
        Diag::of_kind(crate::error::Projection {
            detail: e.to_string(),
        })
    })?;
    let owl_el = rdf::project_owl_el(program, &mut loss).map_err(|e| {
        Diag::of_kind(crate::error::Projection {
            detail: e.to_string(),
        })
    })?;
    let datalog = text::project_datalog(program, &mut loss);
    let n3 = text::project_n3(program, &mut loss);
    let gufo = rdf::project_gufo(program, &mut loss).map_err(|e| {
        Diag::of_kind(crate::error::Projection {
            detail: e.to_string(),
        })
    })?;
    let canonical_rdf12 = rdf::project_canonical_rdf12(program).map_err(|e| {
        Diag::of_kind(crate::error::Projection {
            detail: e.to_string(),
        })
    })?;
    let clif = crate::clif::project_clif(program)?;
    let cgif = crate::cgif::project_cgif(program)?;
    let xcl = crate::xcl::project_xcl(program)?;
    let shacl_af = shacl_af::project_shacl_af(program, &mut loss);

    let results = [
        &owl_dl,
        &owl_el,
        &datalog,
        &n3,
        &gufo,
        &canonical_rdf12,
        &clif,
        &cgif,
        &xcl,
        &shacl_af,
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
    // report alongside the nine whole-program projections so the report carries the
    // path targets too.  The kind is the declared `property-path` preservation; a
    // path projection records no `actual_drops` (the overclaim gate is a no-op for
    // its SoundUnder kind).
    let (pp_kind, _, _) = target_meta("property-path");
    let path_results: Vec<ProjectionResult> = path_projections
        .iter()
        .map(|pp| {
            let target = format!("property-path:{}", pp.shape_iri);
            loss.record_projection_drops(&target, pp_kind, &pp.ledger.lossy_drops, &[]);
            ProjectionResult {
                target,
                content: pp.property_path.clone(),
                is_rdf: false,
                preservation: pp_kind,
                complexity: pp.ledger.complexity.clone(),
            }
        })
        .collect();
    owned.extend(path_results);

    // Validation-shape surfaces: the closed-world SHACL Core + ShEx projections of every
    // logic:ValidationShape, each a ledgered target (shacl-core / shex). Emitted as
    // whole-program documents so the pipeline can write generated/shapes/validation-shapes.
    // {ttl,shex}; a shape-free program yields empty documents and only the structural ledger
    // rows (no per-shape residue), so the corpus is byte-stable until shapes are attached.
    let shacl_shape_residue: Vec<String> = program
        .validation_shapes
        .iter()
        .flat_map(shapes::shacl_residue)
        .collect();
    let (sc_kind, sc_compl, sc_struct) = target_meta("shacl-core");
    let sc_struct: Vec<String> = sc_struct.into_iter().map(str::to_owned).collect();
    loss.record_projection_drops("shacl-core", sc_kind, &sc_struct, &shacl_shape_residue);
    owned.push(ProjectionResult {
        target: "shacl-core".to_owned(),
        content: shapes::project_validation_shapes_shacl(program),
        is_rdf: false,
        preservation: sc_kind,
        complexity: sc_compl.to_owned(),
    });
    let shex_shape_residue: Vec<String> = program
        .validation_shapes
        .iter()
        .flat_map(shapes::shex_residue)
        .collect();
    let (sx_kind, sx_compl, sx_struct) = target_meta("shex");
    let sx_struct: Vec<String> = sx_struct.into_iter().map(str::to_owned).collect();
    loss.record_projection_drops("shex", sx_kind, &sx_struct, &shex_shape_residue);
    owned.push(ProjectionResult {
        target: "shex".to_owned(),
        content: shapes::project_validation_shapes_shex(program),
        is_rdf: false,
        preservation: sx_kind,
        complexity: sx_compl.to_owned(),
    });

    // The procedural-constraint surface: the closed-world SPARQL-constraint projection of every
    // logic:Constraint (the validation twin of the SHACL-AF rule surface — those DERIVE, these
    // VALIDATE). Emitted as a whole-program document so the pipeline writes
    // generated/shapes/procedural-constraints.ttl; a constraint-free program yields the
    // header-only document and only the structural ledger row (no per-constraint residue), so
    // the corpus is byte-stable until constraints are authored. The concrete per-constraint
    // residue is the union of the SPARQL-fragment residue (constraints exceeding the
    // range-restricted guarded fragment, carried-and-flagged) and the blanket ShEx-unsupported
    // note (a sh:SPARQLConstraint has no ShEx form at all).
    let mut pc_residue = shapes::procedural_constraint_residue(program);
    pc_residue.extend(shapes::procedural_constraint_shex_residue(program));
    let (pc_kind, pc_compl, pc_struct) = target_meta("procedural-constraint");
    let pc_struct: Vec<String> = pc_struct.into_iter().map(str::to_owned).collect();
    loss.record_projection_drops("procedural-constraint", pc_kind, &pc_struct, &pc_residue);
    owned.push(ProjectionResult {
        target: "procedural-constraint".to_owned(),
        content: shapes::project_procedural_constraints(program),
        is_rdf: false,
        preservation: pc_kind,
        complexity: pc_compl.to_owned(),
    });

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
        let collapse = [GOAL_EVAL_COLLAPSE_DROP.to_owned()];
        for result in &owned {
            if GOAL_EVAL_COLLAPSE_TARGETS.contains(&result.target.as_str()) {
                // An additional STRUCTURAL drop on the lossy target, interned into the same
                // store the producer used (structural notes read back sorted, so the extra
                // note lands in the same place regardless of interning order).
                loss.record_projection_drops(&result.target, result.preservation, &collapse, &[]);
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
            let report = correspondence_gates::evaluate_gates(&derived, &[], verdicts);
            (Some(report), outcomes, Some(derived))
        };

    // Per-correspondence preservation residue (Principle-17 loss row): every
    // carrier-extracted `program.correspondences` member that authors a lossy
    // `logic:preservationKind` (Some, non-`Exact`) drops a real distinction its coarse view
    // cannot carry. Fold ONE loss-ledger row per such correspondence HERE — the canonical
    // doc's "one preservation row per correspondence" (LOGIC-CORRESPONDENCE.md ~:78) — so the
    // dropped construct is a structured ledger row, never DARK. Mirrors the per-shape
    // `property-path:<iri>` rows above (append to `owned`/`loss`; NOT part of the fixed
    // LEDGER_TARGETS surface, which is program-independent).
    {
        use sha2::{Digest, Sha256};
        let (_c_kind, c_compl, c_struct) = target_meta("correspondence");
        let c_struct: Vec<String> = c_struct.into_iter().map(str::to_owned).collect();
        for c in &program.correspondences {
            let Some(pres) = c.preservation else { continue };
            if pres == PreservationKind::Exact {
                continue;
            }
            let digest = Sha256::digest(c.iri.as_bytes());
            let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
            let target = format!("correspondence:{short}");
            // The concrete dropped constructs (`actual` notes), attributed to this cell. The
            // SZS-status → verdict collapse's residue is the CAX≠UNS / CSA≠SAT distinctions:
            // the finer contradictory-vs-unsatisfiable and countersatisfiable-vs-satisfiable
            // tokens collapse into the coarse verdict and survive only via logic:rawStatusToken.
            let actual = vec![
                format!("correspondence: {}", c.iri),
                "logic:SzsContradictoryAxioms and logic:SzsUnsatisfiable collapse to \
                 logic:ConfInconsistent; the CAX≠UNS distinction survives only via \
                 logic:rawStatusToken"
                    .to_owned(),
                "logic:SzsCounterSatisfiable and logic:SzsSatisfiable collapse to \
                 logic:ConfConsistent; the CSA≠SAT distinction survives only via \
                 logic:rawStatusToken"
                    .to_owned(),
            ];
            loss.record_projection_drops(&target, pres, &c_struct, &actual);
            owned.push(ProjectionResult {
                target,
                // The legal output is the correspondence's dialect artifacts (written
                // elsewhere); the row is a preservation/residue record, not a serialization.
                content: String::new(),
                is_rdf: false,
                preservation: pres,
                complexity: c_compl.to_owned(),
            });
        }
    }

    let report_header = {
        let base = report::ReportHeader::of_program(program);
        match &correspondence_gates {
            Some(gates) => base.with_lawful_uplift(correspondence_gates::liftability(gates).lawful),
            None => base,
        }
    };
    let report =
        report::build_projection_report_from(report_header, &owned, &loss).map_err(|e| {
            Diag::of_kind(crate::error::Projection {
                detail: e.to_string(),
            })
        })?;

    // Preservation ledger: per-target (kind, complexity, combined lossy drops), each row's
    // `lossy_drops` READ BACK from the ONE loss store `loss` — the same instance the Turtle
    // report projects from just above, so the JSON preservation ledger and the
    // `gmeow:lossyDrop` report demonstrably cannot drift. `owned` already carries the path
    // `property-path:<iri>` rows (appended above), so both summaries span the SAME targets.
    let preservation_ledger: Vec<LedgerEntry> = owned
        .iter()
        .map(|p| LedgerEntry {
            target: p.target.clone(),
            preservation: p.preservation.as_str().to_owned(),
            complexity: p.complexity.clone(),
            lossy_drops: loss.projection_drops_for(&p.target),
        })
        .collect();

    Ok(CompiledArtifacts {
        owl_dl: owl_dl.content,
        owl_el: owl_el.content,
        datalog: datalog.content,
        n3: n3.content,
        gufo: gufo.content,
        canonical_rdf12: canonical_rdf12.content,
        clif: clif.content,
        cgif: cgif.content,
        xcl: xcl.content,
        shacl_af: shacl_af.content,
        report,
        preservation_ledger,
        path_projections,
        logic_projections: owned,
        report_header,
        loss,
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
///
/// The per-target loss drops no longer live here: every producer interns its structural
/// and per-run drops directly into the single [`crate::loss_ledger::LossLedger`] (keyed by
/// the target focus), and every serializer/consumer reads them back from that one store.
/// This value now carries only the identity + serialized output + declared judgment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectionResult {
    /// Short target name (`"owl-dl"`, `"datalog"`, …).
    pub target: String,
    /// The serialized output (Turtle string, Datalog text, or N3).
    pub content: String,
    /// Whether `content` is an RDF (Turtle) serialization (vs. plain text).
    pub is_rdf: bool,
    /// The declared preservation kind.
    pub preservation: PreservationKind,
    /// The declared complexity class string.
    pub complexity: String,
}

/// Build the **one preservation row per correspondence** the canonical doc requires
/// (`LOGIC-CORRESPONDENCE.md` line ~78): the loss ledger attributes a dropped
/// construct to the leg that dropped it.  `dialect` selects the pinned preservation +
/// the dialect-level structural drops (the get/put-leg/caveat/standpoint losses
/// from [`target_meta`]); `key` is the stable per-correspondence target name
/// (`<dialect>:<correspondence-iri-or-cell::profile>`); `residue` is the concrete,
/// per-correspondence flagged set (profile losses + A1's rejected constructs),
/// each note already attributed to its leg.  The static (dialect-level) drops and the
/// concrete per-correspondence drops are interned into `ledger` under the row's target
/// focus; the report reads them back as `gmeow:lossyDrop`. The returned row carries only
/// the identity/judgment — the drops live in the single loss store.
/// The GMEOW endpoint of an alignment cell — the documented `gmeow:` term the
/// correspondence's projection loss attributes to. GMEOW is always the source `S` of a
/// `logic:Correspondence` lens, but an authored cell may carry the gmeow term as either the
/// subject or the object, so prefer a gmeow subject, then a gmeow object; a cell with no
/// gmeow endpoint (a purely external↔external crossing) yields `None` — never fabricated onto
/// a term.
pub(crate) fn gmeow_endpoint(subject: &str, object: &str) -> Option<String> {
    if subject.starts_with(GMEOW_NS) {
        Some(subject.to_owned())
    } else if object.starts_with(GMEOW_NS) {
        Some(object.to_owned())
    } else {
        None
    }
}

/// A single IRI as a DOCUMENTED gmeow: source term — `Some(iri)` when GMEOW-namespaced (so a
/// funnel drop naming it, e.g. a rule head whose predicate is a gmeow: property with no OWL-DL
/// / SHACL-AF derivation form, attributes to that term's page), else `None` (a `logic:`-NS
/// construct — no term page yet — or an external IRI stays whole-program; honest computed
/// absence, never fabricated onto a term).
pub(crate) fn gmeow_term(iri: &str) -> Option<String> {
    iri.starts_with(GMEOW_NS).then(|| iri.to_owned())
}

pub(crate) fn correspondence_result(
    ledger: &mut crate::loss_ledger::LossLedger,
    dialect: &str,
    key: &str,
    residue: Vec<String>,
    source_term: Option<String>,
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
    let target = format!("{dialect}:{short}");
    let structural: Vec<String> = structural.into_iter().map(str::to_owned).collect();
    let mut actual_drops = Vec::with_capacity(residue.len() + 1);
    actual_drops.push(format!("correspondence: {key}"));
    actual_drops.extend(residue);
    // Attribute this correspondence cell's residue to its DOCUMENTED source term (the GMEOW
    // endpoint of the alignment) when one is supplied: the whole cell's drops concern that
    // term projected DOWN to the external vocabulary (Principle 17), so its projection-loss
    // row lands on that term's page. `None` (a non-gmeow / undocumented endpoint) leaves the
    // drops whole-program — never fabricated onto a term.
    let attributed: Vec<(String, Option<String>)> = actual_drops
        .into_iter()
        .map(|note| (note, source_term.clone()))
        .collect();
    ledger.record_projection_drops_attributed(&target, kind, &structural, &attributed);
    ProjectionResult {
        target,
        // The legal output is the dialect artifact itself (written elsewhere); the row
        // is a preservation/residue record, not a serialization, so content is empty.
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: complexity.to_owned(),
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
pub(crate) fn formula_residue_notes(
    program: &LogicProgram,
    target_label: &str,
    representable: &dyn Fn(&crate::ir::Formula) -> bool,
) -> Vec<String> {
    program
        .formulas
        .iter()
        .enumerate()
        .filter(|(_, f)| !representable(f))
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
        // CLIF: a bidirectional s-expression FOL dialect. ExactPreservation —
        // the idiomatic FOL channel (rules + formulas) round-trips verbatim and the
        // RDF/predication channel rides the lossless canonical-RDF-1.2 leg, so nothing
        // is dropped (the production round-trip test pins this).
        "clif" => (
            PreservationKind::Exact,
            "full first-order (semi-decidable)",
            vec![],
        ),
        // CGIF: a bidirectional conceptual-graph FOL dialect. ExactPreservation —
        // the idiomatic conceptual-graph channel (rules + formulas) round-trips verbatim and
        // the RDF/predication channel rides the lossless canonical-RDF-1.2 leg, so nothing is
        // dropped (the production round-trip test pins this, as for its CLIF sibling).
        "cgif" => (
            PreservationKind::Exact,
            "full first-order (semi-decidable)",
            vec![],
        ),
        // XCL: a bidirectional XML (eXtended Common Logic Markup Language) FOL dialect.
        // ExactPreservation — the idiomatic XCL2 sentence channel (rules + formulas) is a
        // human-readable view and the RDF/predication channel rides the lossless
        // canonical-RDF-1.2 leg carried as N-Triples in <gmeow-rdf-meta>, so nothing is
        // dropped (the production round-trip test pins this, as for its CLIF/CGIF siblings).
        "xcl" => (
            PreservationKind::Exact,
            "full first-order (semi-decidable)",
            vec![],
        ),
        "shacl-af" => (
            PreservationKind::SoundUnder,
            "terminating/PTIME-data",
            vec![
                "the stratified Horn-with-stratified-negation-and-aggregation rule fragment is \
                 projected (positive body → graph patterns, negation-as-failure → \
                 FILTER NOT EXISTS, inequality guards → FILTER, and a reduce rule → an \
                 aggregating sh:SPARQLRule with a GROUP-BY sub-SELECT): full first-order formula \
                 bodies, existential (value-inventing) rule heads, and ground-subject rules have \
                 no faithful SHACL-AF sh:SPARQLRule form and remain in the canonical logic: layer \
                 (carried by canonical-rdf12)",
                "modal / world / standpoint context of a contextualized rule has no SHACL-AF \
                 form; a context-scoped rule is not projected (it would be unsound over the \
                 default graph) and is recorded as a drop",
                "the SHACL-AF surface is emit-only: there is no parse-back from sh:SPARQLRule \
                 into a logic: rule (the logic: canon is the authoring ground, Principle 4)",
                "ground class/property subsumption axioms are projected as cax-sco / prp-spo1 \
                 sh:SPARQLRule shapes; every other ground axiom (type / metamodel assertions, \
                 asserted relations, domain/range, modal or scoped axioms, literal-valued \
                 assertions) is not a derivation rule, has no SHACL-AF form, and is carried in \
                 the canonical RDF-1.2 layer (recorded as a drop)",
            ],
        ),
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
        "sparql-put" => (
            PreservationKind::ValidationOnly,
            "terminating/PTIME-data",
            vec![
                "the inverse-ingest up-lift is validation-only mint-with-claim: minted \
                 nodes/edges are marked import-derived (gmeow:wasGeneratedBy / \
                 gmeow:mappedFrom), not asserted as extracted fact",
                "durable subject, gmeow:DigitalSubjectTenure, distribution/versioning \
                 framing, and attributed provenance absent from the external source are \
                 disclosed as residue and NOT synthesized",
            ],
        ),
        "shacl-core" => (
            PreservationKind::ValidationOnly,
            "closed-world shape validation (SHACL Core)",
            vec![
                "a shape surface validates but does not entail (ValidationOnly)",
                "full-FOL integrity conditions, standpoint/world/time-indexed constraints, and \
                 cross-node conditions have no SHACL Core form and are carried in the canonical \
                 logic: layer",
                "sh:pattern carries regex-dialect residue (SHACL uses the XPath flavour) and \
                 external terminology bindings have no faithful SHACL Core form; both are carried \
                 and flagged per shape",
                "an intentionally-open existential range (owl:someValuesFrom owl:Thing / \
                 rdfs:Literal) is read closed-world as a bare sh:nodeKind (sh:BlankNodeOrIRI / \
                 sh:Literal): the existential's at-least-one force and the vacuous universal-top \
                 class membership are dropped, and the open range is carried in the canonical \
                 logic: layer",
            ],
        ),
        "procedural-constraint" => (
            PreservationKind::ValidationOnly,
            "closed-world SPARQL constraint (SHACL Core)",
            vec![
                "a sh:SPARQLConstraint surface validates but does not entail (ValidationOnly)",
                "the projectable fragment is a range-restricted, ∀-guarded integrity condition \
                 (guard(this) → φ(this)) whose per-focus condition φ is a Horn / NNF tree of \
                 binary atoms, existentials, disjunctions and (path-)universals: it lowers to a \
                 SELECT $this WHERE { guard ∧ ¬φ } via BGP + FILTER NOT EXISTS + UNION",
                "full-FOL, standpoint/world/time-indexed, and aggregate-comparison (COUNT/SUM \
                 threshold) integrity conditions exceed that fragment; there is no formula-level \
                 aggregate term, so an aggregate comparison cannot be expressed as a genuine \
                 GROUP BY sub-SELECT and is carried-and-flagged, not projected",
                "the surface is emit-only: there is no parse-back from a sh:SPARQLConstraint into \
                 a logic:Constraint (the logic: canon is the authoring ground, Principle 4)",
                "a sh:SPARQLConstraint has no ShEx form at all (logic:unsupported); every \
                 projected constraint is disclosed as a ShEx drop, carried in the canonical \
                 logic: layer",
                "a hand-authored CLOSED-WORLD lint whose finding is SUPERSEDED by RDFS/OWL \
                 entailment is not projected as a logic:Constraint: e.g. gmeow:CoreObservationMethodShape \
                 flags a gmeow:TemporalMeasurement carrying only gmeow:measurementMethod for a \
                 missing gmeow:observationMethod, but gmeow:measurementMethod rdfs:subPropertyOf \
                 gmeow:observationMethod entails the method, so the canonical logic: reasoning layer \
                 proves the datum well-formed — a faithful closed-world projection cannot reproduce \
                 that finding without contradicting the entailment (it would over-claim), so the \
                 finding is a reasoning artifact carried in the canonical logic: layer and its \
                 hand-authored shape is retained as a closed-world lint (dating1 residue)",
            ],
        ),
        "shex" => (
            PreservationKind::ValidationOnly,
            "closed-world shape validation (ShEx, strictly narrower than SHACL Core)",
            vec![
                "a shape surface validates but does not entail (ValidationOnly)",
                "ShEx has no SPARQL target, no RDF-1.2 statement layer, no languageIn, and no \
                 datetime-range facet; those conditions are carried in the canonical logic: layer",
                "everything SHACL Core drops (regex dialect, external terminology) is also dropped \
                 by ShEx, plus the ShEx-only drops above (a strictly larger residue set)",
            ],
        ),
        // EmotionML: a many-to-one W3C EmotionML XML projection of the affect surface. The
        // category vocabulary is built from gmeow:EmotionType individuals and the dimension
        // vocabulary from gmeow:AppraisalDimension / gmeow:CoreAffectDimension; Emotion,
        // AffectiveExperience, Appraisal, and AffectClassifierOutput ALL collapse into one
        // <emotion> envelope, so the projection is lossy by construction and MUST name the
        // collapsed source families (the affect design's hard-fail rule 9).
        "emotionml" => (
            PreservationKind::SoundUnder,
            "XML vocabulary + <emotion> envelope (no entailment)",
            vec![
                "gmeow:Emotion, gmeow:AffectiveExperience, gmeow:Appraisal, and \
                 gmeow:AffectClassifierOutput all project into a single EmotionML <emotion> \
                 envelope: the mode / experience / expression / classifier-output distinction is \
                 collapsed and survives only in the canonical logic:/gmeow: layer",
                "the evidence/claim boundary, self-report authority, appraiser vantage/standpoint, \
                 and scale-profile framing of a dimensional reading have no EmotionML form and are \
                 dropped",
                "category and dimension names are emitted as a closed EmotionML vocabulary set; the \
                 open, contested axis basis (Principle 9) is flattened to a fixed enumeration",
            ],
        ),
        // The per-correspondence preservation residue (Principle-17 loss row): a
        // `logic:Correspondence` on a lossy rung authoring a non-`Exact`
        // `logic:preservationKind` drops a real distinction its coarse view cannot carry.
        // The concrete dropped constructs are the per-correspondence `actual` notes (e.g. the
        // SZS-status collapse's CAX≠UNS / CSA≠SAT distinctions); this is the shared structural
        // limitation every such row inherits.
        "correspondence" => (
            PreservationKind::SoundUnder,
            "decidable/graph-iso lens-law check",
            vec![
                "a lossy correspondence's get leg is many-to-one (non-injective): the coarse \
                 view cannot recover the source distinctions its finer domain drew, so the \
                 lowering is a sound under-approximation, never exact — the dropped source \
                 distinctions survive only in the canonical logic: layer",
            ],
        ),
        // The Pydantic v2 model package (gmeow_models): a closed-record instance
        // VALIDATION surface co-derived from the SAME shape compilation as the
        // JSON-Schema projection (so a model's model_json_schema() agrees with the
        // packed JSON Schema), carrying full docstrings + traceability. It validates
        // instance shape, it does not entail — ValidationOnly, exactly like its
        // JSON-Schema / SHACL-Core sibling.
        "pydantic" => (
            PreservationKind::ValidationOnly,
            "closed-record instance validation (Pydantic v2, draft-2020-12 core)",
            vec![
                "a model validates instance shape but does not entail (ValidationOnly): no \
                 reasoning, no subsumption, no entailment survives — the canonical logic: layer \
                 carries them",
                "the open-world default is read as a closed RECORD: rdfs:domain/range are open-world \
                 inferences that a Pydantic field cannot express, so a field is validation-only and \
                 the open-world force is carried in the canonical logic: layer",
                "rdfs:subClassOf is flattened to at most one same-module Python base (single \
                 inheritance); multiple / cross-module superclasses are dropped from the class \
                 hierarchy and survive only in the canonical logic: layer",
                "SHACL constructs with no JSON-Schema/Pydantic analogue (sh:sparql full-FOL integrity \
                 conditions, standpoint/world/time-indexed constraints) are dropped exactly as the \
                 JSON-Schema projection drops them, and are carried in the canonical logic: layer",
                "an xsd datatype with no Pydantic scalar mapping is either hard-failed (never \
                 silently widened) or, for an explicitly allowlisted lexical form, carried as str \
                 and disclosed as a declared datatype loss",
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
const LEDGER_TARGETS: [&str; 20] = [
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "clif",
    "cgif",
    "xcl",
    "shacl-af",
    "property-path",
    // The correspondence-calculus alignment lowerings: each carries its own
    // preservation judgment in the same loss ledger as OWL/Datalog/gUFO.
    "sssom",
    "fno",
    "edoal",
    "sparql-construct",
    // The EmotionML XML lowering: a many-to-one, lossy-by-construction emitter of the
    // affect category + dimension vocabularies (its residue names the collapsed families).
    "emotionml",
    // The closed-world validation-shape surfaces (SHACL Core + ShEx + the SPARQL-constraint
    // projection of every logic:Constraint), each carrying its own per-target preservation
    // judgment in the same loss ledger.
    "shacl-core",
    "shex",
    "procedural-constraint",
    // The Pydantic v2 model package (gmeow_models): a closed-record instance
    // validation surface co-derived from the JSON-Schema shape compilation, carrying
    // its own ValidationOnly preservation judgment in this same loss ledger.
    "pydantic",
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
/// The exact-preservation target (`canonical-rdf12`) carries the full
/// `logic:GoalEvaluation` structure in its materialized output and is excluded.
pub const GOAL_EVAL_COLLAPSE_DROP: &str = concat!(
    "logic:GoalEvaluation factored axes (satisfaction/feasibility/lifecycle status, ",
    "satisfaction degree, criterion, evaluator/standpoint vantage multiplicity) ",
    "collapsed to flat binary gmeow:satisfiedBy edge"
);

/// Targets that lose the `logic:GoalEvaluation` structure when a `satisfiedBy`
/// collapse is present. `canonical-rdf12` is exact-preservation and carries the
/// full evaluation in its materialized output — it is NOT augmented.
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
        || axiom.scope.module.is_some()
}

/// Per-contract `actual_drops` notes for a LOSSY down-projection target.  A
/// reasoning contract is reasoning-configuration metadata; the lossy
/// rule/axiom surfaces (OWL-DL, OWL-EL, gUFO, Datalog, N3) carry no facet
/// vocabulary, so each contract a program declares is recorded as an explicit
/// drop rather than silently discarded.  The canonical RDF 1.2 target preserves
/// contracts losslessly and must NOT call this.
pub(crate) fn contract_drop_notes(
    program: &LogicProgram,
    target_label: &str,
    representable: &dyn Fn(&crate::ir::Formula) -> bool,
) -> Vec<String> {
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
    notes.extend(formula_residue_notes(program, target_label, representable));
    notes
}

/// One drop note per stratified-aggregation (reduce) rule, for a target that cannot represent
/// aggregation (Datalog / N3). Carried-and-flagged, never silent; an aggregation-free
/// program adds nothing, so its ledger is byte-unchanged.
/// Each note paired with the DOCUMENTED gmeow: source term it concerns — the aggregation
/// rule's head predicate when it is a gmeow: property (so a `gmeow:` aggregation whose reduce
/// form Datalog/N3 cannot carry lands on that term's page), else `None` (whole-program).
pub(crate) fn aggregation_drop_notes(
    program: &LogicProgram,
    target_label: &str,
) -> Vec<(String, Option<String>)> {
    program
        .rules
        .iter()
        .filter(|r| r.aggregation.is_some())
        .map(|r| {
            let note = format!(
                "rule deriving <{}> uses stratified aggregation (reduce/GROUP BY), which \
                 {target_label} does not represent; it is carried in the canonical logic: layer \
                 and projected to the SHACL-AF reduce surface",
                r.head.predicate
            );
            (note, gmeow_term(&r.head.predicate))
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
