// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pure, wasm-clean conjecture-evaluation orchestration — the SINGLE authority
//! shared by the native MCP/CLI surface (`gmeow-pipeline`'s `evaluate_conjecture`)
//! and the browser conjecture playground (`gmeow-reason-wasm`'s `conjecture` export).
//!
//! # The symmetric test, orchestrated from Turtle
//!
//! [`evaluate_conjecture_eval`] parses the candidate `logic:` document into exactly one
//! [`Formula`] ([`parse_candidate_formula`]), re-homes the caller's KB Turtle into the
//! isolated [`CONJECTURE_SCENARIO_WORLD`] ([`rehome_kb_into_scenario`]), runs the native
//! symmetric [`conjecture_test`], and projects the verdict to deterministic, sorted
//! N-Triples and its node IRI through one validated [`ResultProjection`]. The result is
//! byte-for-byte identical whether it is produced on the native gate or in the browser —
//! the native≡wasm witness pins that identity.
//!
//! [`evaluate_conjecture_ttl`] and [`evaluate_conjecture_kb`] are the Turtle-in wrappers
//! each surface actually calls; both funnel into the same [`evaluate_conjecture_eval`]
//! core, so neither can drift from the other's verdict.
//!
//! [`evaluate_conjecture_eval`]: crate::conjecture_eval::evaluate_conjecture_eval
//! [`Formula`]: gmeow_logic_compile::ir::Formula
//! [`parse_candidate_formula`]: crate::conjecture_eval::parse_candidate_formula
//! [`CONJECTURE_SCENARIO_WORLD`]: crate::conjecture_eval::CONJECTURE_SCENARIO_WORLD
//! [`rehome_kb_into_scenario`]: crate::conjecture_eval::rehome_kb_into_scenario
//! [`conjecture_test`]: crate::conjecture::conjecture_test
//! [`ResultProjection`]: crate::result_rdf::ResultProjection
//!
//! Nothing here TR-gates, persists, or mutates the caller's KB (isolation is inherent):
//! it is the pure evaluation core each surface wraps with its own tail.

use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, Term as IrTerm};
use purrdf::{CompositeDatasetView, CompositeSource, GraphPlacement, TermValue, ViewLimits};
use sha2::{Digest, Sha256};

use crate::conjecture::{ConjectureLifecycleState, conjecture_test};
use crate::query_ir::Budget;
use crate::result::InformationState;
use crate::result_rdf::{ConjectureVerdictInput, ResultProjection};

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The single, fixed ISOLATED scenario world every conjecture test reasons in. The KB the
/// caller supplies is re-homed into this world (so the world-scoped DL calculus joins the
/// KB facts with the asserted / evaluated candidate), and the run is inherently isolated —
/// [`conjecture_test`] borrows the scenario view and never mutates the input.
pub const CONJECTURE_SCENARIO_WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/agentic/conjecture/scenario";

/// A refutation's contradiction witness, flattened for every response surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjectureVerdictWitness {
    /// The individual forced into a clash.
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples that witness the clash, each rendered `"s p o"`.
    pub premises: Vec<String>,
}

/// The inputs to [`evaluate_conjecture_eval`]: the candidate `logic:` document, the KB
/// Turtle, the reified standpoint, and the optional math-twin / budget knobs.
pub struct ConjectureEvalInput<'a> {
    /// The candidate `logic:` document (names exactly one `logic:Formula` / ground axiom).
    pub formula_ttl: &'a str,
    /// The KB serialization, re-homed into [`CONJECTURE_SCENARIO_WORLD`].
    pub kb_ttl: &'a str,
    /// The KB media type / short id purrdf understands (`text/turtle`, `application/n-quads`,
    /// …). The native surface passes Turtle; the browser playground passes N-Quads (the core
    /// bundle serialization). Placement retains every logical assertion layer of the KB;
    /// each selected serialization must faithfully represent those layers.
    pub kb_format: &'a str,
    /// The reified standpoint IRI the verdict is scoped to (REQUIRED — Principle 9).
    pub standpoint: &'a str,
    /// When the conjecture is the runtime twin of a `math:Conjecture`, that statement's IRI.
    pub math_conjecture: Option<&'a str>,
    /// Optional post-hoc derived-closure-size ceiling (`None` ⟹ unbounded).
    pub max_steps: Option<u64>,
    /// Optional post-hoc answer-count ceiling (`None` ⟹ unbounded).
    pub max_answers: Option<usize>,
}

/// The projected verdict of one conjecture evaluation: the deterministic N-Triples body,
/// the content-addressed node IRI, the flattened verdict facets, the symmetric two-leg
/// booleans, and the refutation witness (present exactly when refuted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjectureVerdictProjection {
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The **proof leg**: `true` iff `KB ⊨ φ` (Belnap `supported` or `both`).
    pub has_proof: bool,
    /// The **counterproof leg**: `true` iff `KB ∪ {φ} ⊨ ⊥` (Belnap `opposed` or `both`).
    pub has_counterproof: bool,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureVerdictWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body from the validated [`ResultProjection`].
    pub verdict_nt: String,
    /// The candidate's content-addressing key (formula identity across standpoints).
    pub content_key: String,
}

/// Lower-case hex of `sha256(bytes)`.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Parse the candidate `logic:` document and extract exactly ONE candidate [`Formula`]: the
/// single top-level `logic:Formula` when present, else the single ground `logic:` axiom
/// lifted to a binary [`Formula::Atom`]. Any other shape (zero / multiple candidates) is a
/// hard, fail-closed error — a conjecture test names one formula, never a program.
///
/// # Errors
///
/// Returns an error if the document fails to parse or does not name exactly one candidate.
pub fn parse_candidate_formula(formula_src: &str) -> gmeow_errors::Result<Formula> {
    let (program, _diags) = parse_logic_str(formula_src, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("candidate logic: document failed to parse: {e}"),
        })
    })?;
    let bad = |detail: String| gmeow_errors::Diag::of_kind(crate::error::Reason { detail });
    // A reified `logic:Formula` is the primary surface: prefer the single top-level formula
    // even when the frontend leaks the formula's own structural triples as axioms.
    let formula_count = program.formulas.len();
    if formula_count == 1 {
        return Ok(program.formulas.into_iter().next().expect("len == 1"));
    }

    // No single top-level formula. A REIFIED trivially-Horn `logic:Formula` (a ground binary
    // atom — e.g. `relation=rdf:type, arg0=ex:a, arg1=ex:B`, the fact `ex:a rdf:type ex:B`) is
    // routed by the front-end to `LogicProgram.axioms` (its Horn home — `with_formulas`
    // hard-fails on a trivially-Horn leaf), so `formulas` is EMPTY and the candidate lives in
    // the axiom set. That set also carries the formula node's own `rdf:type logic:Formula`
    // self-typing, which leaks as a structural axiom — drop that noise, then a lone remaining
    // axiom IS the candidate fact, lifted here to a binary `Formula::Atom`. `conjecture_test`'s
    // `as_ground_fact` then asserts a ground lift as an EDB fact and decides it like any
    // candidate, so a reified ground-atom conjecture is a first-class, evaluated candidate —
    // never a panic (the previously dead `(0,1)` lift is now genuinely reachable).
    let logic_formula_iri = format!("{LOGIC_NAMESPACE}Formula");
    let mut candidate_axioms: Vec<_> = program
        .axioms
        .into_iter()
        .filter(|ax| {
            !(ax.predicate == RDF_TYPE && ax.obj.as_iri() == Some(logic_formula_iri.as_str()))
        })
        .collect();
    if formula_count == 0 && candidate_axioms.len() == 1 {
        let ax = candidate_axioms.pop().expect("len == 1");
        let object = match ax.obj {
            gmeow_logic_compile::ir::AtomicTerm::Var(value) => {
                IrTerm::Var(value.trim_start_matches('?').to_owned())
            }
            gmeow_logic_compile::ir::AtomicTerm::Iri(value) => IrTerm::Iri(value),
            gmeow_logic_compile::ir::AtomicTerm::Literal(value) => IrTerm::Literal(value),
            gmeow_logic_compile::ir::AtomicTerm::Blank(_) => {
                return Err(bad(
                    "a blank candidate object needs explicit existential binding".to_owned(),
                ));
            }
        };
        return Formula::atom(
            IrTerm::Iri(ax.predicate),
            vec![IrTerm::Iri(ax.subject), object],
        )
        .map_err(|e| bad(e.message().to_owned()));
    }
    Err(bad(format!(
        "candidate must be exactly one formula/atom, got {formula_count} formula(s) and \
         {} candidate axiom(s)",
        candidate_axioms.len()
    )))
}

/// Place the caller's complete KB in [`CONJECTURE_SCENARIO_WORLD`] through a shared
/// native view. The world-scoped calculus evaluates the KB and the candidate in
/// that same world. Ordinary assertions, reifier bindings, annotations and empty
/// declarations all acquire the scenario graph; former graph IRIs remain intact
/// when they occur as provenance or other RDF values.
///
/// This is an explicit collapse of the input's graph roles. The view retains the
/// original parsed source and its blank identities without copying or freezing
/// its rows. It passes directly to the native conjecture evaluator.
/// `kb_format` is a purrdf media type / short id (`text/turtle`, `application/n-quads`, …).
///
/// # Errors
///
/// Returns an error if the KB fails to parse or native placement exceeds its
/// retention limits. No partial scenario is returned.
pub fn rehome_kb_into_scenario(
    kb_src: &str,
    kb_format: &str,
) -> gmeow_errors::Result<CompositeDatasetView> {
    let parsed = purrdf::parse_dataset(kb_src.as_bytes(), kb_format, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("KB ({kb_format}) failed to parse: {e}"),
        })
    })?;
    let source = CompositeSource::new(parsed).with_graph_placement(GraphPlacement::Named(
        TermValue::iri(CONJECTURE_SCENARIO_WORLD),
    ));
    CompositeDatasetView::from_shared_sources(vec![source], ViewLimits::default()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("place KB in the conjecture scenario: {e}"),
        })
    })
}

/// Evaluate one conjecture end-to-end from Turtle: parse the candidate, re-home the KB into
/// the isolated scenario world, run the native symmetric [`conjecture_test`], and project the
/// verdict to deterministic N-Triples. This is the SINGLE evaluation authority shared by the
/// native MCP/CLI surface and the browser conjecture playground; it never TR-gates, persists,
/// or mutates the caller's KB.
///
/// # Errors
///
/// Returns an error if the candidate document does not name exactly one candidate formula, if
/// the KB does not parse, if the native engine fails (see [`conjecture_test`]), or if a
/// refutation names a compound candidate with no soundly-derivable forbidden predicate,
/// or if the result fails terminal projection admission.
pub fn evaluate_conjecture_eval(
    input: &ConjectureEvalInput<'_>,
) -> gmeow_errors::Result<ConjectureVerdictProjection> {
    // (1) Parse the candidate document and extract exactly one candidate formula.
    let candidate = parse_candidate_formula(input.formula_ttl)?;

    // (2) Parse the KB and place every RDF layer into the isolated scenario world, so the
    //     world-scoped DL calculus joins the KB with the asserted / evaluated candidate.
    let kb = rehome_kb_into_scenario(input.kb_ttl, input.kb_format)?;

    // (3) Run the engine. The KB is borrowed and never mutated (isolation is inherent).
    let kb_world = format!(
        "{CONJECTURE_SCENARIO_WORLD}#kb-{}",
        sha256_hex(input.kb_ttl.as_bytes())
    );
    let budget = Budget {
        max_answers: input.max_answers,
        max_steps: input.max_steps,
    };
    let answer = conjecture_test(
        &kb,
        CONJECTURE_SCENARIO_WORLD,
        &candidate,
        input.standpoint,
        &[],
        &budget,
    )?;

    // (4) Project the verdict → deterministic N-Triples, and mint the content-addressed
    //     (formula × standpoint × KB-world) conjecture node IRI.
    let content_key = candidate.content_key();
    // The anti-conjecture leg's forbidden predicate: the refuted formula's PRINCIPAL
    // predicate (the predicate the closure must never draw). A refuted formula that names no
    // single predicate (a compound conjunction / disjunction / implication / biconditional)
    // has no soundly-derivable forbidden predicate — its `logic:NonEntailmentObligation`
    // forbidden predicate is a reviewer decision — so we HARD-FAIL rather than fabricate one
    // or emit a shape-invalid obligation node (Constitution: no fabrication, no optionality).
    let forbidden_predicate = candidate.principal_predicate();
    if answer.lifecycle == ConjectureLifecycleState::RefutedInStandpoint
        && forbidden_predicate.is_none()
    {
        let standpoint = input.standpoint;
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!(
                "conjecture refuted in standpoint <{standpoint}>, but its candidate formula \
                 is compound and names no single predicate: the anti-conjecture \
                 logic:NonEntailmentObligation's forbidden predicate cannot be soundly \
                 derived and must be a reviewer decision. Refute an atomic or universally \
                 quantified single-predicate claim, or author the obligation directly."
            ),
        }));
    }
    let verdict_input = ConjectureVerdictInput {
        content_key: content_key.as_str(),
        standpoint: input.standpoint,
        kb_world: &kb_world,
        answer: &answer,
        math_conjecture: input.math_conjecture,
        forbidden_predicate: forbidden_predicate.as_deref(),
    };
    let projection = ResultProjection::conjecture(&verdict_input)?;
    let verdict_nt = projection.to_ntriples();
    let node_iri = projection.node_iri().to_owned();

    let verdict = &answer.verdict;
    let witness = answer.witness.as_ref().map(|w| ConjectureVerdictWitness {
        individual: w.individual.clone(),
        world: w.world.clone(),
        premises: w
            .premises
            .iter()
            .map(|(s, p, o)| format!("{s} {p} {o}"))
            .collect(),
    });
    // The symmetric two legs, read off the Belnap information state (module doc): a proof
    // exists iff the candidate is supported (`supported` or the `both` glut); a counterproof
    // exists iff the candidate is opposed (`opposed` or the `both` glut).
    let has_proof = matches!(
        verdict.information,
        InformationState::Supported | InformationState::Both
    );
    let has_counterproof = matches!(
        verdict.information,
        InformationState::Opposed | InformationState::Both
    );

    Ok(ConjectureVerdictProjection {
        lifecycle: answer.lifecycle.wire().to_string(),
        information: verdict.information.wire().to_string(),
        evaluation: verdict.evaluation.wire().to_string(),
        completeness: verdict.completeness.wire().to_string(),
        discharge: answer.discharge.local_name().to_string(),
        has_proof,
        has_counterproof,
        witness,
        node_iri,
        verdict_nt,
        content_key: content_key.into_string(),
    })
}

/// Evaluate one conjecture from Turtle with the default (unbounded, no-math-twin) profile —
/// the browser playground's single entry. A thin convenience over [`evaluate_conjecture_eval`].
///
/// # Errors
///
/// See [`evaluate_conjecture_eval`].
pub fn evaluate_conjecture_ttl(
    formula_ttl: &str,
    kb_ttl: &str,
    standpoint: &str,
) -> gmeow_errors::Result<ConjectureVerdictProjection> {
    evaluate_conjecture_eval(&ConjectureEvalInput {
        formula_ttl,
        kb_ttl,
        kb_format: "text/turtle",
        standpoint,
        math_conjecture: None,
        max_steps: None,
        max_answers: None,
    })
}

/// Evaluate one conjecture from a KB in an explicit serialization (`kb_format`) with the
/// default (unbounded, no-math-twin) profile — the browser conjecture playground's entry,
/// which passes the core bundle as N-Quads. A thin convenience over
/// [`evaluate_conjecture_eval`].
///
/// # Errors
///
/// See [`evaluate_conjecture_eval`].
pub fn evaluate_conjecture_kb(
    formula_ttl: &str,
    kb: &str,
    kb_format: &str,
    standpoint: &str,
) -> gmeow_errors::Result<ConjectureVerdictProjection> {
    evaluate_conjecture_eval(&ConjectureEvalInput {
        formula_ttl,
        kb_ttl: kb,
        kb_format,
        standpoint,
        math_conjecture: None,
        max_steps: None,
        max_answers: None,
    })
}

#[path = "conjecture_eval.scenario_tests.rs"]
#[cfg(test)]
mod scenario_tests;
