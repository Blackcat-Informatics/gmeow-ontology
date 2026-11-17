// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Governed conjecture and candidate workflows and atomic runtime library persistence.

#[cfg(feature = "reasoning")]
use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(feature = "reasoning")]
use std::sync::Arc;

#[cfg(feature = "reasoning")]
use serde_json::{Value, json};

#[cfg(feature = "reasoning")]
use gmeow_errors::ResultExt;
#[cfg(feature = "reasoning")]
use gmeow_logic::conjecture::ConjectureLifecycleState;
#[cfg(feature = "reasoning")]
use gmeow_logic::result_rdf::project_conjecture_withdrawal;
#[cfg(feature = "reasoning")]
use gmeow_logic::transaction::execute::{CommitMode, TxReceipt, execute_transaction_dataset};
#[cfg(feature = "reasoning")]
use gmeow_logic_compile::action_policy::PreparedActionPolicy;
#[cfg(feature = "reasoning")]
use gmeow_logic_compile::ir::LOGIC_NAMESPACE;
#[cfg(feature = "reasoning")]
use purrdf::RdfTerm;
#[cfg(feature = "reasoning")]
use purrdf::gts::model::{Term as GtsTerm, TermKind as GtsTermKind};
#[cfg(feature = "reasoning")]
use sha2::{Digest, Sha256};

#[cfg(feature = "reasoning")]
use super::{RDF_TYPE, StoreMedium};
#[cfg(feature = "reasoning")]
use crate::storage::{LibraryLock, SegmentLibrary, storage};

// --------------------------------------------------------------------------- //
// The shared conjecture-test core (one evaluation implementation, three surfaces).
//
// [`evaluate_conjecture`] is the SINGLE parse → test → project path: it parses the candidate
// document and KB, re-homes the KB into the isolated scenario world, runs the native engine,
// and projects the verdict to deterministic N-Triples. Nothing calls the engine or the
// projector a second time.
//
// Two public entries share this evaluation:
//   - [`run_conjecture_test_pure`] — the "test" leg: evaluate only, NEVER TR-gate, NEVER
//     persist. Behind the MCP `conjecture_test` tool.
//   - [`run_conjecture_test`] — the "store" leg: evaluate, then TR-gate on the
//     `persistConjecture` schema and, on a committed precondition-met run, APPEND the verdict
//     to the append-only conjecture library. Behind the MCP `store_conjecture` tool AND the
//     public `gmeow conjecture test` CLI subcommand.
// Neither surface re-implements the evaluation; each renders its own outcome (JSON / text).
// --------------------------------------------------------------------------- //

/// The inputs shared by both evaluation entries: the candidate `logic:` Turtle document, the
/// KB Turtle it is tested against, the REQUIRED reified standpoint scope (Principle 9), an
/// optional `math:Conjecture` twin, and the optional derived-closure budget.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunPureInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// Optional post-hoc derived-closure-size ceiling on the isolated scenario evaluation: when
    /// the derived (non-EDB) closure exceeds this many steps the run is stamped `BudgetExhausted`
    /// → lifecycle Open → discharge Unknown. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling in answer bindings; see
    /// [`max_steps`](Self::max_steps). `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// The inputs to one PERSISTING conjecture test: [`ConjectureRunPureInput`]'s fields plus
/// whether the run is a sandbox (`dry_run`, writes nothing).
#[cfg(feature = "reasoning")]
pub struct ConjectureRunInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// When true, compute and return the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_steps`]. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_answers`]. `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// A refutation's contradiction witness, flattened for every response surface.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunWitness {
    /// The individual forced into a clash.
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples that witness the clash, each rendered `"s p o"`.
    pub premises: Vec<String>,
}

/// The outcome of a [`run_conjecture_test_pure`] call: the projected verdict facets, the
/// refutation witness (when refuted), the content-addressed node IRI, and the projected
/// N-Triples body. Nothing is ever committed or persisted on this path.
#[cfg(feature = "reasoning")]
pub struct ConjecturePureOutput {
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
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed conjecture node IRI of the complete projected verdict.
    pub node_iri: String,
    /// The deterministic N-Triples body `project_conjecture_verdict` emitted.
    pub verdict_nt: String,
    /// Complete authenticated GTS, including required binary execution evidence.
    pub artifact: Vec<u8>,
}

/// The outcome of a [`run_conjecture_test`] call: the projected verdict facets, the refutation
/// witness (when refuted), the content-addressed node IRI, the projected N-Triples body, and
/// the TR receipt gating the persist. `committed` is true exactly when the segment was appended.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunOutput {
    /// True exactly when the verdict segment + audit were appended to the library (a committed,
    /// precondition-met, non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET (the persist was refused, nothing
    /// written); `None` otherwise.
    pub precondition_unmet: Option<String>,
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
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed conjecture node IRI of the complete projected verdict.
    pub node_iri: String,
    /// The deterministic N-Triples body `project_conjecture_verdict` emitted.
    pub verdict_nt: String,
    /// Complete artifact, also present when persistence is refused or hypothetical.
    pub artifact: Vec<u8>,
    /// The TR receipt gating the persist (rendered as the transaction summary by callers).
    pub receipt: TxReceipt,
}

/// The shared evaluation core's result: everything computed by parsing, testing, and
/// projecting one conjecture verdict, before either public entry decides what to do with it.
#[cfg(feature = "reasoning")]
struct ConjectureEvaluation {
    lifecycle: String,
    information: String,
    evaluation: String,
    completeness: String,
    discharge: String,
    witness: Option<ConjectureRunWitness>,
    node_iri: String,
    verdict_nt: String,
    artifact: Vec<u8>,
    /// The candidate's content-addressing key, needed by the persisting tail's audit call id.
    content_key: String,
}

/// Parse the candidate document and KB, re-home the KB into the isolated scenario world, run
/// the native engine, and project the verdict to deterministic N-Triples. The SINGLE
/// evaluation path shared by [`run_conjecture_test_pure`] and [`run_conjecture_test`] — neither
/// TR-gates nor writes anything; that is each caller's own tail.
///
/// # Errors
///
/// Returns an error if the candidate document does not name exactly one candidate formula, if
/// the KB does not parse, if the native engine fails (see [`conjecture_test`]), or if a
/// refutation names a compound candidate with no soundly-derivable forbidden predicate.
#[cfg(feature = "reasoning")]
fn evaluate_conjecture(
    formula_ttl: &str,
    kb_ttl: &str,
    kb_format: &str,
    standpoint: &str,
    math_conjecture: Option<&str>,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> gmeow_errors::Result<ConjectureEvaluation> {
    // Delegate to the SINGLE conjecture-evaluation authority in gmeow-logic (shared with the
    // browser conjecture playground so both produce byte-identical verdict N-Triples), then
    // adapt its projection to the pipeline's response type at the boundary.
    let projection = gmeow_logic::conjecture_eval::evaluate_conjecture_eval(
        &gmeow_logic::conjecture_eval::ConjectureEvalInput {
            formula_ttl,
            kb_ttl,
            kb_format,
            standpoint,
            math_conjecture,
            max_steps,
            max_answers,
        },
    )
    .map_err(|error| error.with_context("conjecture evaluation failed"))?;

    let witness = projection.witness.map(|w| ConjectureRunWitness {
        individual: w.individual,
        world: w.world,
        premises: w.premises,
    });

    Ok(ConjectureEvaluation {
        lifecycle: projection.lifecycle,
        information: projection.information,
        evaluation: projection.evaluation,
        completeness: projection.completeness,
        discharge: projection.discharge,
        witness,
        node_iri: projection.node_iri,
        verdict_nt: projection.verdict_nt,
        artifact: projection.artifact,
        content_key: projection.content_key,
    })
}

/// Test one conjecture without persistence: evaluate the candidate against the
/// KB and project the verdict, but NEVER TR-gate and NEVER append to the conjecture library.
/// Behind the MCP `conjecture_test` tool. For the committing counterpart ("store"), see
/// [`run_conjecture_test`].
///
/// # Errors
///
/// See [`evaluate_conjecture`].
#[cfg(feature = "reasoning")]
pub fn run_conjecture_test_pure(
    input: &ConjectureRunPureInput,
) -> gmeow_errors::Result<ConjecturePureOutput> {
    let ConjectureRunPureInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    } = *input;
    let eval = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        "text/turtle",
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;
    Ok(ConjecturePureOutput {
        lifecycle: eval.lifecycle,
        information: eval.information,
        evaluation: eval.evaluation,
        completeness: eval.completeness,
        discharge: eval.discharge,
        witness: eval.witness,
        node_iri: eval.node_iri,
        verdict_nt: eval.verdict_nt,
        artifact: eval.artifact,
    })
}

/// Evaluate one conjecture and persist its verdict: test the candidate
/// against the KB (see [`evaluate_conjecture`]), TR-gate the write on the `persistConjecture`
/// schema, and — on a committed, precondition-met run — APPEND the verdict segment plus a
/// cold-auditable trajectory segment to the append-only conjecture library.
///
/// This is the shared persisting core behind both the MCP `store_conjecture` tool and the
/// `gmeow conjecture test` CLI subcommand. It never mutates the caller's KB (isolation is
/// inherent) and, on a `dry_run` or a precondition-unmet run, writes nothing.
///
/// # Errors
///
/// Returns an error if [`evaluate_conjecture`] fails, or if the TR transaction or the library
/// append fails. The caller supplies the policy from the same captured bundle
/// as the selected store medium.
#[cfg(feature = "reasoning")]
pub fn run_conjecture_test(
    input: &ConjectureRunInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    run_conjecture_test_with_library(input, medium, policy, conjecture_library)
}

/// Evaluate against the already selected library, preserving its identity from
/// standalone output validation through the existing locked persistence workflow.
///
/// # Errors
/// The same evaluation, policy or persistence failures as [`run_conjecture_test`].
#[cfg(feature = "reasoning")]
pub fn run_conjecture_test_in(
    input: &ConjectureRunInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
    library: Arc<dyn SegmentLibrary>,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    run_conjecture_test_with_library(input, medium, policy, || Ok(library))
}

#[cfg(feature = "reasoning")]
fn run_conjecture_test_with_library(
    input: &ConjectureRunInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
    resolve_library: impl FnOnce() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>>,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    let ConjectureRunInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    let ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        artifact,
        content_key,
    } = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        "text/turtle",
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;

    // (5) TR-gate the write on the persistConjecture schema. The precondition — a verdict has
    //     been presented — obtains once the engine returns a verdict (evaluation above).
    let obtains = [MCP_CONJECTURE_VERDICT_PRESENTED];
    let receipt = execute_memory_txn(policy, MCP_PERSIST_CONJECTURE_SCHEMA, &obtains, dry_run)?;

    let mut out = ConjectureRunOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        artifact,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run: the verdict is observed, nothing is appended to the library.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // (6) Committed: APPEND the verdict segment plus its cold-auditable trajectory segment
    //     (keyed to a content-addressed call id) to the append-only library — TOGETHER, as ONE
    //     atomic file replace under ONE held lock, so a failure building or committing either
    //     segment can never leave the library holding the verdict without its audit record.
    let library = resolve_library()?;
    with_library_lock(library.as_ref(), || {
        // The lock binds both continuations to the tail this transaction commits.
        let existing = library.read_bytes()?;
        let verdict_segment =
            build_verdict_segment(&existing, medium, &out.verdict_nt, &out.artifact)?;
        let mut body_image = existing.clone();
        body_image.extend_from_slice(&verdict_segment);
        let call_id = format!(
            "urn:gmeow:conjecture-call:{}",
            sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
        );
        let audit_segment = build_audit_segment(
            &body_image,
            medium,
            &call_id,
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &obtains,
            "1970-01-01T00:00:00Z",
        )?;
        append_library_segments(library.as_ref(), &[verdict_segment, audit_segment])
    })?;
    out.committed = true;
    Ok(out)
}

// ── Candidate submission: the neurosymbolic propose/verify seam ───────────────
//
// A candidate is a proposed authoring contribution — a candidate `logic:` formula tested
// against a KB — whose admissibility (a corroborated isolated-world verdict) gates its
// append to the separate, append-only candidate library. It reuses `evaluate_conjecture`
// for the verdict (the trusted symbolic half of propose/verify). The TR precondition
// `candidateAdmissible` obtains iff corroborated, so a refuted/open candidate stages nothing.
// The appended node carries its authoring role (`gmeow:AuthoringCandidate`) and target
// provenance, and lands in `candidates.gts`.

/// The inputs to one candidate submission: [`ConjectureRunInput`]'s test fields plus the
/// optional target provenance (`for_slice` / `for_packet`) recorded on the admitted node.
#[cfg(feature = "reasoning")]
pub struct CandidateSubmitInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against in its explicitly selected format.
    pub kb_ttl: &'a str,
    /// The media type of the selected KB (including graph-preserving formats).
    pub kb_format: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI (as `conjecture_test`).
    pub math_conjecture: Option<&'a str>,
    /// Optional provenance: the slice IRI this candidate is proposed FOR.
    pub for_slice: Option<&'a str>,
    /// Optional provenance: the `gmeow:AuthoringPacket` IRI this candidate answers.
    pub for_packet: Option<&'a str>,
    /// When true, compute the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling; `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling; `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// The outcome of a [`run_submit_candidate`] call: the projected verdict facets, the
/// admissibility decision, the refutation witness (when refuted), the content-addressed node
/// IRI, the projected N-Triples body, and the TR receipt gating the append. `committed` is true
/// exactly when the admissible candidate segment was appended.
#[cfg(feature = "reasoning")]
pub struct CandidateSubmitOutput {
    /// True exactly when the candidate segment + audit were appended (admissible, committed,
    /// non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET — the candidate was NOT admissible
    /// (refuted or open) or a dry sandbox on a non-admissible candidate — so nothing was written.
    pub precondition_unmet: Option<String>,
    /// The admissibility decision: true iff the isolated-world verdict CORROBORATED the candidate.
    pub admissible: bool,
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name.
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed candidate node IRI (shared with its `logic:Conjecture` verdict).
    pub node_iri: String,
    /// The deterministic N-Triples verdict body (before the candidate role/provenance lines).
    pub verdict_nt: String,
    /// Complete evaluation and candidate-provenance artifact on every outcome.
    pub artifact: Vec<u8>,
    /// The TR receipt gating the append.
    pub receipt: TxReceipt,
}

/// The authoring-role + provenance N-Triples a submitted candidate node carries IN ADDITION to
/// its `logic:Conjecture` verdict: `rdf:type gmeow:AuthoringCandidate` plus the optional
/// `gmeow:candidateForSlice` / `gmeow:candidateForPacket` target links. Every predicate/type is
/// a canonically-authored guides-slice term (no unauthored vocabulary).
#[cfg(feature = "reasoning")]
fn candidate_provenance_nt(
    node_iri: &str,
    for_slice: Option<&str>,
    for_packet: Option<&str>,
) -> String {
    let mut s = format!("<{node_iri}> <{RDF_TYPE}> <{GMEOW_AUTHORING_CANDIDATE}> .\n");
    if let Some(slice) = for_slice {
        s.push_str(&format!(
            "<{node_iri}> <{GMEOW_CANDIDATE_FOR_SLICE}> <{slice}> .\n"
        ));
    }
    if let Some(packet) = for_packet {
        s.push_str(&format!(
            "<{node_iri}> <{GMEOW_CANDIDATE_FOR_PACKET}> <{packet}> .\n"
        ));
    }
    s
}

#[cfg(feature = "reasoning")]
fn candidate_artifact(mut artifact: Vec<u8>, provenance: &str) -> gmeow_errors::Result<Vec<u8>> {
    let expected = gmeow_logic::result_rdf::parse_reasoning_artifact(&artifact)?;
    let dataset = purrdf::parse_dataset(provenance.as_bytes(), "application/n-triples", None)?;
    // A standalone response explicitly selects the unprimed runtime profile.
    // Library continuations below retain their supplied StoreMedium instead.
    let writer = gmeow_gts_profile::GmeowGtsWriter::new("runtime");
    let provenance = writer.finish_view_scoped(dataset.as_ref(), "candidate-provenance")?;
    artifact.extend_from_slice(&provenance.bytes);
    if gmeow_logic::result_rdf::parse_reasoning_artifact(&artifact)? != expected {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
            "candidate provenance changed its authenticated reasoning verdict".into(),
        )));
    }
    Ok(artifact)
}

#[cfg(feature = "reasoning")]
fn build_verdict_segment(
    existing: &[u8],
    medium: &StoreMedium,
    body: &str,
    artifact: &[u8],
) -> gmeow_errors::Result<Vec<u8>> {
    use gmeow_logic::result_rdf::{NativeReceiptRef, import_native_receipt};
    let incoming = import_native_receipt(artifact, None)?;
    let receipt = incoming
        .as_ref()
        .map(|(address, bytes)| NativeReceiptRef::admit(address, bytes))
        .transpose()?;
    let reused = if existing.is_empty() {
        false
    } else if let Some(receipt) = receipt {
        gmeow_logic::result_rdf::contains_admitted_native_receipt(existing, receipt)?
    } else {
        gmeow_logic::result_rdf::admit_reasoning_artifact(existing)?;
        false
    };
    build_nt_segment_with_receipt(existing, medium, body, receipt.filter(|_| !reused))
}

/// Submit one candidate end-to-end — the neurosymbolic propose/verify seam: evaluate the
/// candidate against the KB (reusing [`evaluate_conjecture`] verbatim), POLARITY-gate the write
/// on the `submitCandidate` schema (admissible iff corroborated), and — on a committed,
/// admissible, non-dry run — APPEND the candidate verdict segment (verdict + authoring role +
/// provenance) plus a cold-auditable trajectory segment to the append-only candidate library.
///
/// This is the shared core behind both the MCP `submit_candidate` tool and the `gmeow candidate
/// submit` CLI subcommand. It never mutates the caller's KB (isolation is inherent) and, on a
/// `dry_run` or a non-admissible run, writes nothing.
///
/// # Errors
///
/// Returns an error if [`evaluate_conjecture`] fails, or if the TR transaction or the library
/// append fails.
#[cfg(feature = "reasoning")]
pub fn run_submit_candidate(
    input: &CandidateSubmitInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
) -> gmeow_errors::Result<CandidateSubmitOutput> {
    run_submit_candidate_with_library(input, medium, policy, candidate_library)
}

/// Evaluate against the already selected candidate library, preserving its identity
/// from output validation through the existing locked persistence workflow.
///
/// # Errors
/// The same evaluation, policy or persistence failures as [`run_submit_candidate`].
#[cfg(feature = "reasoning")]
pub fn run_submit_candidate_in(
    input: &CandidateSubmitInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
    library: Arc<dyn SegmentLibrary>,
) -> gmeow_errors::Result<CandidateSubmitOutput> {
    run_submit_candidate_with_library(input, medium, policy, || Ok(library))
}

#[cfg(feature = "reasoning")]
fn run_submit_candidate_with_library(
    input: &CandidateSubmitInput,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
    resolve_library: impl FnOnce() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>>,
) -> gmeow_errors::Result<CandidateSubmitOutput> {
    let CandidateSubmitInput {
        formula_ttl,
        kb_ttl,
        kb_format,
        standpoint,
        math_conjecture,
        for_slice,
        for_packet,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    let ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        artifact,
        content_key,
    } = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        kb_format,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;

    // Polarity gate: the candidate is ADMISSIBLE iff the isolated-world verdict
    // CORROBORATED it. A refuted or open verdict is NOT admissible, so the `candidateAdmissible`
    // precondition does not obtain, the executional-entailment commit FAILS, and nothing is
    // appended — the score moves only via gate-passing content.
    let admissible = lifecycle == ConjectureLifecycleState::Corroborated.wire();
    let obtains: &[&str] = if admissible {
        &[MCP_CANDIDATE_ADMISSIBLE]
    } else {
        &[]
    };
    let receipt = execute_memory_txn(policy, MCP_SUBMIT_CANDIDATE_SCHEMA, obtains, dry_run)?;

    let provenance = candidate_provenance_nt(&node_iri, for_slice, for_packet);
    let artifact = candidate_artifact(artifact, &provenance)?;
    let mut out = CandidateSubmitOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        admissible,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        artifact,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run on an admissible candidate: the verdict is observed, nothing is appended.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // Committed + admissible: APPEND the candidate verdict segment (verdict N-Triples + the
    // authoring-role/provenance lines on the same node) plus its cold-auditable trajectory
    // segment to the append-only candidate library — TOGETHER, as ONE atomic file replace under
    // ONE held lock (reusing the same GTS-library primitives the conjecture library uses).
    let library = resolve_library()?;
    let body = format!("{}\n{}", out.verdict_nt.trim_end(), provenance,);
    with_library_lock(library.as_ref(), || {
        // Read and author against the same live tail under this library's lock.
        let existing = library.read_bytes()?;
        let verdict_segment = build_verdict_segment(&existing, medium, &body, &out.artifact)?;
        let mut body_image = existing.clone();
        body_image.extend_from_slice(&verdict_segment);
        let call_id = format!(
            "urn:gmeow:candidate-call:{}",
            sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
        );
        let audit_segment = build_audit_segment(
            &body_image,
            medium,
            &call_id,
            MCP_SUBMIT_CANDIDATE_SCHEMA,
            obtains,
            "1970-01-01T00:00:00Z",
        )?;
        append_library_segments(library.as_ref(), &[verdict_segment, audit_segment])
    })?;
    out.committed = true;
    Ok(out)
}

/// Withdraw a persisted candidate — the compensating author-WITHDRAWAL counterpart of
/// [`run_submit_candidate`] (P10, exactly as `refute_conjecture` compensates `store_conjecture`).
/// Appends one compensating "withdrawn" segment to the candidate library, flipping the target
/// node's EFFECTIVE lifecycle to `logic:ConjectureWithdrawn` — recorded, never deleted. The write
/// is a REAL TR gate on the `withdrawCandidate` schema, whose precondition — the candidate is
/// still in the library and not already withdrawn — is DERIVED from the live library state read
/// back by SEGMENT ORDER. An unknown or already-withdrawn id fails the commit and returns
/// `ok:false` before writing. Returns the JSON response body shared by the MCP tool and the CLI.
///
/// # Errors
///
/// Returns an error if the library read, the TR transaction, or the append fails.
#[cfg(feature = "reasoning")]
pub fn run_withdraw_candidate(
    candidate_id: &str,
    reason: &str,
    dry_run: bool,
    medium: &StoreMedium,
    policy: &PreparedActionPolicy,
) -> gmeow_errors::Result<String> {
    let library_handle = candidate_library()?;
    let library_ref = library_handle.as_ref();
    // The read → precondition-check → (on a real commit) append sequence runs entirely inside ONE
    // held exclusive lock, so two concurrent withdrawals cannot both observe "not yet withdrawn"
    // and both commit (lost-update).
    with_library_lock(library_ref, || {
        let library = read_library(library_ref)?;
        let effective = library.get(candidate_id).copied();
        let exists = effective.is_some();
        let in_library = matches!(
            effective,
            Some(state) if state != ConjectureLifecycleState::Withdrawn
        );

        let mut obtains: Vec<&str> = Vec::new();
        if in_library {
            obtains.push(MCP_CANDIDATE_IN_LIBRARY);
        }
        let receipt = execute_memory_txn(policy, MCP_WITHDRAW_CANDIDATE_SCHEMA, &obtains, dry_run)?;
        let nt_body = project_conjecture_withdrawal(candidate_id, reason);
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                let detail = if exists {
                    format!("candidate already withdrawn: {candidate_id}")
                } else {
                    format!("unknown candidate id: {candidate_id}")
                };
                return Ok(json!({
                    "ok": false,
                    "error": format!("withdrawCandidate precondition unmet: {detail}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "candidate": candidate_id,
                    "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                    "transaction": txn_json(&receipt),
                    "judgment_nquads": nt_body,
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let existing = library_ref.read_bytes()?;
        let withdrawal_segment = build_nt_segment(&existing, medium, &nt_body)?;
        // The audit continues the BODY, so it is authored against the bytes the body
        // leaves behind — not against the pre-commit library, which the body already moved past.
        let mut body_image = existing.clone();
        body_image.extend_from_slice(&withdrawal_segment);
        let call_id = format!(
            "urn:gmeow:candidate-call:{}",
            sha256_hex(format!("withdraw\u{1}{candidate_id}\u{1}{reason}").as_bytes())
        );
        let audit_segment = build_audit_segment(
            &body_image,
            medium,
            &call_id,
            MCP_WITHDRAW_CANDIDATE_SCHEMA,
            &[MCP_CANDIDATE_IN_LIBRARY],
            "1970-01-01T00:00:00Z",
        )?;
        append_library_segments(library_ref, &[withdrawal_segment, audit_segment])?;
        Ok(json!({
            "ok": true,
            "candidate": candidate_id,
            "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
            "transaction": txn_json(&receipt),
            "judgment_nquads": nt_body,
        })
        .to_string())
    })
}

/// List every admitted candidate in the library with its effective disposition
/// (`in-library` | `withdrawn`) and target provenance (`for_slice` / `for_packet`). The effective
/// lifecycle is resolved by SEGMENT ORDER (a later withdrawal supersedes the admission); the
/// immutable type/provenance is read from the unioned dataset. Optional `filter_slice` filters by
/// target provenance and `filter_disposition` by effective state. A missing library is an EMPTY
/// list, not an error. Returns the JSON response body shared by the MCP tool and the CLI.
///
/// # Errors
///
/// Returns an error if the library read fails.
#[cfg(feature = "reasoning")]
pub fn run_list_candidates(
    filter_slice: Option<&str>,
    filter_disposition: Option<&str>,
) -> gmeow_errors::Result<String> {
    run_list_candidates_in(
        candidate_library()?.as_ref(),
        filter_slice,
        filter_disposition,
    )
}

/// [`run_list_candidates`] against an EXPLICIT library rather than the process backend's.
///
/// The listing logic lives here, once, so it can be exercised against any
/// [`SegmentLibrary`] — in particular against the browser backend's in-process library,
/// which has no path to point [`run_list_candidates`] at.
///
/// # Errors
///
/// Returns an error if the library read fails.
#[cfg(feature = "reasoning")]
pub fn run_list_candidates_in(
    library: &dyn SegmentLibrary,
    filter_slice: Option<&str>,
    filter_disposition: Option<&str>,
) -> gmeow_errors::Result<String> {
    // Effective, segment-order-resolved lifecycle per stored node (last-writer-wins).
    let lifecycles = read_library(library)?;

    // Immutable type + provenance from the unioned dataset (set once at submit, never superseded,
    // so the union is sound for these fields).
    let mut for_slice: BTreeMap<String, String> = BTreeMap::new();
    let mut for_packet: BTreeMap<String, String> = BTreeMap::new();
    let mut is_candidate: BTreeSet<String> = BTreeSet::new();
    let bytes = library.read_bytes()?;
    if !bytes.is_empty() {
        {
            let bundle = purrdf::import_gts_events(&bytes)
                .with_ctx(|| "read candidate library".to_string())?;
            for quad in bundle.dataset.owned_quads() {
                let (RdfTerm::Iri(subj), RdfTerm::Iri(obj)) = (&quad.subject, &quad.object) else {
                    continue;
                };
                match quad.predicate.as_str() {
                    RDF_TYPE if obj == GMEOW_AUTHORING_CANDIDATE => {
                        is_candidate.insert(subj.clone());
                    }
                    GMEOW_CANDIDATE_FOR_SLICE => {
                        for_slice.insert(subj.clone(), obj.clone());
                    }
                    GMEOW_CANDIDATE_FOR_PACKET => {
                        for_packet.insert(subj.clone(), obj.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    let mut candidates: Vec<Value> = Vec::new();
    for (node, state) in &lifecycles {
        // Only nodes typed gmeow:AuthoringCandidate (belt-and-suspenders: every candidate is one;
        // a bare logic:Conjecture in this library would be foreign).
        if !is_candidate.contains(node) {
            continue;
        }
        let disposition = if *state == ConjectureLifecycleState::Withdrawn {
            "withdrawn"
        } else {
            "in-library"
        };
        if filter_disposition.is_some_and(|d| d != disposition) {
            continue;
        }
        let slice = for_slice.get(node);
        if let Some(want) = filter_slice
            && slice.map(String::as_str) != Some(want)
        {
            continue;
        }
        candidates.push(json!({
            "candidate": node,
            "disposition": disposition,
            "lifecycle": state.wire(),
            "for_slice": slice,
            "for_packet": for_packet.get(node),
        }));
    }

    Ok(json!({
        "ok": true,
        "candidate_count": candidates.len(),
        "candidates": candidates,
    })
    .to_string())
}

/// The transient world the TR run reasons in — a fresh in-memory store per call, NEVER persisted.
/// The executed verdict gates the write; the materialized outcome rides the tool response.
pub(super) const TXN_WORLD: &str = gmeow_logic_compile::action_policy::WORLD;
#[cfg(feature = "reasoning")]
const TXN_ROOT: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/txn";
#[cfg(feature = "reasoning")]
const TXN_START: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/start";

/// The canonical action-schema and situation IRIs defined by `mcp-action-policy.ttl`.
#[cfg(feature = "reasoning")]
pub(super) const MCP_STORE_CLAIM_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/storeClaim";
#[cfg(feature = "reasoning")]
pub(super) const MCP_REVISE_BELIEF_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/reviseBelief";
#[cfg(feature = "reasoning")]
pub(super) const MCP_WELL_FORMED_CLAIM: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/wellFormedClaim";
#[cfg(feature = "reasoning")]
pub(super) const MCP_TARGET_CLAIM_EXISTS: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/targetClaimExists";
#[cfg(feature = "reasoning")]
pub(super) const MCP_CLAIM_IN_MEMORY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/claimInMemory";

/// The `persistConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The `conjecture_test` write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the library.
#[cfg(feature = "reasoning")]
pub(super) const MCP_PERSIST_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/persistConjecture";
#[cfg(feature = "reasoning")]
pub(super) const MCP_CONJECTURE_VERDICT_PRESENTED: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureVerdictPresented";

/// The `withdrawConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The compensating author-withdrawal counterpart of
/// `persistConjecture` (P10, `logic:compensation`): the precondition — the conjecture is
/// still in the library (not already withdrawn) — gates the compensating append.
#[cfg(feature = "reasoning")]
pub(super) const MCP_WITHDRAW_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/withdrawConjecture";
#[cfg(feature = "reasoning")]
pub(super) const MCP_CONJECTURE_IN_LIBRARY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureInLibrary";

/// The `submitCandidate` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The candidate-submission write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the candidate
/// library. Unlike `persistConjecture`'s `conjectureVerdictPresented`, the precondition
/// `candidateAdmissible` is DERIVED FROM VERDICT POLARITY (corroborated, not merely present),
/// so a refuted or open candidate never obtains it and stages nothing.
#[cfg(feature = "reasoning")]
const MCP_SUBMIT_CANDIDATE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/submitCandidate";
#[cfg(feature = "reasoning")]
const MCP_CANDIDATE_ADMISSIBLE: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/candidateAdmissible";

/// The `withdrawCandidate` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The compensating author-withdrawal counterpart of
/// `submitCandidate` (P10, `logic:compensation`): the precondition — the candidate is still in
/// the library (not already withdrawn) — gates the compensating append.
#[cfg(feature = "reasoning")]
const MCP_WITHDRAW_CANDIDATE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/withdrawCandidate";
#[cfg(feature = "reasoning")]
const MCP_CANDIDATE_IN_LIBRARY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/candidateInLibrary";

/// The `gmeow:AuthoringCandidate` class and its provenance predicates (authored in the guides
/// slice `module.ttl`) — the authoring-role type and target links a submitted candidate node
/// carries IN ADDITION to its `logic:Conjecture` verdict, so the candidate library is queryable
/// by slice/packet and distinct from the conjecture library.
#[cfg(feature = "reasoning")]
pub(super) const GMEOW_AUTHORING_CANDIDATE: &str =
    "https://blackcatinformatics.ca/gmeow/AuthoringCandidate";
#[cfg(feature = "reasoning")]
pub(super) const GMEOW_CANDIDATE_FOR_SLICE: &str =
    "https://blackcatinformatics.ca/gmeow/candidateForSlice";
#[cfg(feature = "reasoning")]
const GMEOW_CANDIDATE_FOR_PACKET: &str = "https://blackcatinformatics.ca/gmeow/candidateForPacket";

#[cfg(feature = "reasoning")]
pub(super) const LOGIC_INSTANTIATES_SCHEMA: &str =
    "https://blackcatinformatics.ca/logic/instantiatesSchema";
#[cfg(feature = "reasoning")]
pub(super) const LOGIC_TRANSITION_FROM_STATE: &str =
    "https://blackcatinformatics.ca/logic/transitionFromState";
#[cfg(feature = "reasoning")]
pub(super) const LOGIC_SITUATION_OBTAINS: &str =
    "https://blackcatinformatics.ca/logic/situationObtains";
#[cfg(feature = "reasoning")]
pub(super) const LOGIC_PROPER_PART_OF: &str = "https://blackcatinformatics.ca/logic/properPartOf";
#[cfg(feature = "reasoning")]
pub(super) const GMEOW_AT_TIME: &str = "https://blackcatinformatics.ca/gmeow/atTime";
#[cfg(feature = "reasoning")]
pub(super) const GMEOW_EVENT_TEMPORAL_FRAME: &str =
    "https://blackcatinformatics.ca/gmeow/eventTemporalFrame";
#[cfg(feature = "reasoning")]
const GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN: &str =
    "https://blackcatinformatics.ca/gmeow/temporalFrameUTCGregorian";
#[cfg(feature = "reasoning")]
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// Add actual call state to the producer-prepared native action theory.
/// The immutable policy is hydrated once; no source RDF is reparsed or lowered.
#[cfg(feature = "reasoning")]
fn txn_world_dataset(
    policy: &PreparedActionPolicy,
    schema_iri: &str,
    obtains: &[&str],
) -> gmeow_errors::Result<Arc<purrdf::RdfDataset>> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_dataset(policy.dataset()?)?;
    let mut push = |subject: &str, predicate: &str, object: &str| -> gmeow_errors::Result<()> {
        builder.push_owned_quad(
            &purrdf::RdfQuad::new(
                RdfTerm::Iri(subject.to_owned()),
                predicate,
                RdfTerm::Iri(object.to_owned()),
            )
            .in_graph(RdfTerm::Iri(TXN_WORLD.to_owned())),
        )?;
        Ok(())
    };
    push(TXN_ROOT, LOGIC_INSTANTIATES_SCHEMA, schema_iri)?;
    push(TXN_ROOT, LOGIC_TRANSITION_FROM_STATE, TXN_START)?;
    for situation in obtains {
        push(TXN_START, LOGIC_SITUATION_OBTAINS, situation)?;
    }
    builder.freeze().map_err(gmeow_errors::Diag::from)
}

/// Execute the native policy with the real start state and explicit commit mode.
#[cfg(feature = "reasoning")]
pub(super) fn execute_memory_txn(
    policy: &PreparedActionPolicy,
    schema_iri: &str,
    obtains: &[&str],
    dry_run: bool,
) -> gmeow_errors::Result<TxReceipt> {
    let dataset = txn_world_dataset(policy, schema_iri, obtains)?;
    let mode = if dry_run {
        CommitMode::Hypothetical
    } else {
        CommitMode::Committed
    };
    execute_transaction_dataset(&dataset, TXN_WORLD, TXN_ROOT, mode).map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp::new(error.message().to_owned()))
    })
}

/// The TR outcome rendered for the tool response.
#[cfg(feature = "reasoning")]
pub(super) fn txn_json(receipt: &TxReceipt) -> Value {
    match receipt {
        TxReceipt::CommittedSuccess { path_len, .. } => {
            json!({"committed": true, "succeeded": true, "path_len": path_len})
        }
        TxReceipt::CommittedFailure { reason } => {
            json!({"committed": true, "succeeded": false, "reason": reason})
        }
        TxReceipt::HypotheticalSuccess { witness } => {
            json!({"committed": false, "succeeded": true, "witness": witness})
        }
        TxReceipt::HypotheticalFailure { reason } => {
            json!({"committed": false, "succeeded": false, "reason": reason})
        }
    }
}

#[cfg(feature = "reasoning")]
fn gts_iri(value: &str) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    }
}

#[cfg(feature = "reasoning")]
fn gts_literal_dt(value: &str, datatype: usize) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Literal,
        value: Some(value.to_string()),
        datatype: Some(datatype),
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    }
}

/// One quad as the GTS writers take it: subject, predicate and object as indices into the
/// segment's term table, plus the optional graph term.
#[cfg(feature = "reasoning")]
type GtsQuadRow = (usize, usize, usize, Option<usize>);

#[cfg(feature = "reasoning")]
fn push_gts_term(terms: &mut Vec<GtsTerm>, term: GtsTerm) -> usize {
    terms.push(term);
    terms.len() - 1
}

/// The medium a store's CURRENT tail was written through, or `None` for a store with no bytes yet.
///
/// A GTS segment declares exactly ONE codec catalog. A record appended to a store whose tail was
/// written through a DIFFERENT medium names a catalog that tail never declared, and the store stops
/// reconstructing: `Memory::graph()` yields `None` and every reifier-walking reader — `claims()`
/// among them — then reports an EMPTY store rather than an error. Reading the medium back off the
/// store's own bytes is what keeps the appended segment inside the tail's catalog.
#[cfg(feature = "reasoning")]
fn store_tail_medium(existing: &[u8]) -> gmeow_errors::Result<Option<StoreMedium>> {
    if existing.is_empty() {
        return Ok(None);
    }
    Ok(gmeow_gts_profile::segment_dictionaries(existing)?
        .into_iter()
        .next()
        .map(|(dictionary, bytes)| StoreMedium { dictionary, bytes }))
}

/// A writer that appends into `existing` under `medium`, opening a freshly-headed segment when
/// the tail does not already declare that medium's dictionary.
///
/// The append-only LIBRARIES are runtime stores in exactly the sense the claim store is: the
/// same `gmeow:gtsProducerRuntimeStores` production site covers agent memory, the conjecture
/// library and the candidate library, and all three are primed from the bundle's
/// `gmeow-memory-hot-v1`. A library written through an unprimed writer declares no dictionary
/// at all, so a consumer priming a decode from the bundle finds a store whose header promises
/// nothing.
#[cfg(feature = "reasoning")]
fn library_writer(
    existing: &[u8],
    medium: &StoreMedium,
) -> gmeow_errors::Result<(gmeow_gts_profile::GmeowGtsWriter, Vec<u8>)> {
    // `store_writer`'s appending branch emits ONLY the new frames — the segment header is not
    // repeated — so when the tail does not yet pin this medium the caller has to emit the
    // header itself, ahead of those frames. Returning it here keeps that pairing in one place.
    if existing.is_empty() || gmeow_gts_profile::store_tail_pins(existing, &medium.dictionary)? {
        let writer = gmeow_gts_profile::store_writer("ai-package", existing, medium)?;
        return Ok((writer, Vec::new()));
    }
    let header = gmeow_gts_profile::open_store_segment("ai-package", medium)?;
    let mut base = existing.to_vec();
    base.extend_from_slice(&header);
    let writer = gmeow_gts_profile::store_writer("ai-package", &base, medium)?;
    Ok((writer, header))
}

/// Build one trajectory-audit segment as a CONTINUATION of `existing` — the claim-store variant of
/// [`build_audit_segment`], which authors a standalone segment for the append-only libraries.
///
/// `store_writer` continues the tail when it already declares the medium's dictionary and opens a
/// freshly-headed segment when it does not; either way the appended bytes name a catalog the file
/// declares, which is the invariant [`store_tail_medium`] documents.
#[cfg(feature = "reasoning")]
pub(super) fn build_store_audit_segment(
    existing: &[u8],
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let (terms, quads) = audit_terms_and_quads(call_id, schema_iri, obtains, at_time);
    let mut writer = match store_tail_medium(existing)? {
        Some(medium) => gmeow_gts_profile::store_writer("ai-package", existing, &medium)?,
        None => gmeow_gts_profile::unprimed_store_writer("ai-package", existing)?,
    };
    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    Ok(writer.into_bytes())
}

/// Build one trajectory-audit segment before committing it with its verdict.
/// The caller builds both segments in memory and commits them together through
/// [`append_library_segments`], so one atomic replacement covers both records.
#[cfg(feature = "reasoning")]
pub(super) fn build_audit_segment(
    existing: &[u8],
    medium: &StoreMedium,
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let (terms, quads) = audit_terms_and_quads(call_id, schema_iri, obtains, at_time);
    let (mut writer, header) = library_writer(existing, medium)?;
    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    let mut out = header;
    out.extend_from_slice(&writer.into_bytes());
    Ok(out)
}

/// The terms and quads of ONE trajectory-audit context — the carrier-independent half, shared by
/// the standalone library segment ([`build_audit_segment`]) and the tail-continuing store segment
/// ([`build_store_audit_segment`]) so the two can never describe the turn differently.
#[cfg(feature = "reasoning")]
fn audit_terms_and_quads(
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> (Vec<GtsTerm>, Vec<GtsQuadRow>) {
    let anchor = format!("{call_id}#turn");
    let start = format!("{call_id}#start");

    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();

    let t_call = push_gts_term(&mut terms, gts_iri(call_id));
    let t_anchor = push_gts_term(&mut terms, gts_iri(&anchor));
    let t_start = push_gts_term(&mut terms, gts_iri(&start));

    let t_inst = push_gts_term(&mut terms, gts_iri(LOGIC_INSTANTIATES_SCHEMA));
    let t_schema = push_gts_term(&mut terms, gts_iri(schema_iri));
    quads.push((t_call, t_inst, t_schema, None));

    let t_ppo = push_gts_term(&mut terms, gts_iri(LOGIC_PROPER_PART_OF));
    quads.push((t_call, t_ppo, t_anchor, None));

    let t_dt = push_gts_term(&mut terms, gts_iri(XSD_DATETIME));
    let t_at_time_p = push_gts_term(&mut terms, gts_iri(GMEOW_AT_TIME));
    let t_at_time_o = push_gts_term(&mut terms, gts_literal_dt(at_time, t_dt));
    quads.push((t_call, t_at_time_p, t_at_time_o, None));

    let t_frame_p = push_gts_term(&mut terms, gts_iri(GMEOW_EVENT_TEMPORAL_FRAME));
    let t_frame_o = push_gts_term(&mut terms, gts_iri(GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN));
    quads.push((t_call, t_frame_p, t_frame_o, None));

    let t_tfs = push_gts_term(&mut terms, gts_iri(LOGIC_TRANSITION_FROM_STATE));
    quads.push((t_anchor, t_tfs, t_start, None));

    let t_so = push_gts_term(&mut terms, gts_iri(LOGIC_SITUATION_OBTAINS));
    for situation in obtains {
        let t_sit = push_gts_term(&mut terms, gts_iri(situation));
        quads.push((t_start, t_so, t_sit, None));
    }

    (terms, quads)
}

// ── Conjecture-library persistence (append-only GTS ai-package, TR-gated) ─────
//
// The conjecture library is a SEPARATE, append-only GTS collection — the read-only twin of
// `memory.gts`. Each `conjecture_test` commit appends one `ai-package` segment carrying the
// `project_conjecture_verdict` graph (a content-addressed, standpoint-scoped
// `logic:Conjecture` node with its embedded `logic:ReasoningResult` and refutation witness).
// It is NEVER folded into the base KB reasoning graph (R2): the reasoner reads
// `graph_dataset()` / the caller's KB, never `conjectures.gts`.

/// The conjecture library, resolved through the [`storage`] seam: natively the GTS
/// file at `GMEOW_CONJECTURE_PATH` (home-expanded) or `~/.gmeow/conjectures.gts`, in a
/// browser the in-process segment collection.
#[cfg(feature = "reasoning")]
pub(super) fn conjecture_library() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>> {
    storage().conjecture_library()
}

/// The candidate library, resolved through the [`storage`] seam: natively the GTS file
/// at `GMEOW_CANDIDATE_PATH` (home-expanded) or `~/.gmeow/candidates.gts`, in a browser
/// the in-process segment collection.
///
/// The candidate library is a SEPARATE, append-only GTS collection — the read-only twin
/// of the conjecture library — holding admissibility-gated authoring candidates. It is
/// NEVER folded into base-KB reasoning (R2). Mirrors [`conjecture_library`].
#[cfg(feature = "reasoning")]
fn candidate_library() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>> {
    storage().candidate_library()
}

/// A deterministic lowercase-hex SHA-256 of `bytes` (the KB-world content address seed).
#[cfg(feature = "reasoning")]
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    purrdf_hash::hex::encode(&Sha256::digest(bytes))
}

/// Intern one IRI into the GTS term table, deduplicated by value. Shared by the subject /
/// predicate / object paths and by a typed literal's datatype-IRI leg.
#[cfg(feature = "reasoning")]
fn intern_nt_iri(
    iri: &str,
    terms: &mut Vec<GtsTerm>,
    seen: &mut HashMap<purrdf::TermValue, usize>,
) -> usize {
    let sig = purrdf::TermValue::Iri(iri.to_owned());
    if let Some(&id) = seen.get(&sig) {
        return id;
    }
    let id = push_gts_term(terms, gts_iri(iri));
    seen.insert(sig, id);
    id
}

/// Intern one purrdf-parsed N-Triples node (IRI, blank node, or literal) into the GTS
/// term table using native term identity, including literal language and direction.
/// A literal first interns its effective datatype IRI so
/// the literal term references a live datatype id — reproducing the exact encounter order the
/// append-only, content-addressed GTS segment bytes are keyed on. A quoted-triple term is
/// rejected fail-closed via `reject`: the projection's closed subset never emits one.
#[cfg(feature = "reasoning")]
fn intern_nt_term(
    node: &RdfTerm,
    terms: &mut Vec<GtsTerm>,
    seen: &mut HashMap<purrdf::TermValue, usize>,
    reject: impl FnOnce() -> gmeow_errors::Diag,
) -> gmeow_errors::Result<usize> {
    match node {
        RdfTerm::Iri(iri) => Ok(intern_nt_iri(iri, terms, seen)),
        RdfTerm::BlankNode(label) => {
            let sig = purrdf::TermValue::from_rdf_term(node);
            if let Some(&id) = seen.get(&sig) {
                return Ok(id);
            }
            let term = GtsTerm {
                kind: GtsTermKind::Bnode,
                value: Some(label.clone()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
                triple: None,
            };
            let id = push_gts_term(terms, term);
            seen.insert(sig, id);
            Ok(id)
        }
        RdfTerm::Literal(lit) => {
            // Native owned terms omit datatypes implied by plain or tagged strings.
            let datatype = lit.datatype_iri();
            let sig = purrdf::TermValue::from_rdf_term(node);
            if let Some(&id) = seen.get(&sig) {
                return Ok(id);
            }
            let dt_id = intern_nt_iri(datatype, terms, seen);
            let mut term = gts_literal_dt(&lit.lexical_form, dt_id);
            term.lang = lit.language.clone();
            term.direction = lit.direction.map(|value| value.as_str().to_owned());
            let id = push_gts_term(terms, term);
            seen.insert(sig, id);
            Ok(id)
        }
        RdfTerm::Triple(_) => Err(reject()),
    }
}

/// Build one N-Triples body (e.g. the `project_conjecture_verdict` verdict, or a candidate
/// verdict carrying its authoring-role + provenance triples) into a GTS `ai-package` segment's
/// serialized bytes — the PURE, side-effect-free segment builder shared by the conjecture and
/// candidate libraries. The body is parsed into `(subject, predicate, object)` triples, interned
/// as RDF-1.2-native GTS terms (IRIs, blank nodes, and complete native literals), and
/// written via [`gmeow_gts_profile::GmeowGtsWriter`], so the append-only libraries stay
/// RDF-1.2-native. Building the bytes is separated from appending them so a caller can assemble
/// MULTIPLE segments (e.g. the verdict segment AND its audit segment) in memory and commit them
/// together as one atomic file replace — see [`append_library_segments`].
#[cfg(feature = "reasoning")]
pub(super) fn build_nt_segment(
    existing: &[u8],
    medium: &StoreMedium,
    nt_body: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    build_nt_segment_with_receipt(existing, medium, nt_body, None)
}

#[cfg(feature = "reasoning")]
fn build_nt_segment_with_receipt(
    existing: &[u8],
    medium: &StoreMedium,
    nt_body: &str,
    receipt: Option<gmeow_logic::result_rdf::NativeReceiptRef<'_>>,
) -> gmeow_errors::Result<Vec<u8>> {
    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
    let mut seen: HashMap<purrdf::TermValue, usize> = HashMap::new();

    for (lineno, raw) in nt_body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let malformed = |what: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "conjecture projection line {} bad {what}",
                lineno + 1
            )))
        };
        // Delegate N-Triples lexing of THIS single statement to purrdf, one line at a time,
        // in DOCUMENT order. A whole-body parse cannot be used here: purrdf's dataset freeze
        // sorts quads by term id, so `owned_quads()` would not preserve document order — and
        // that order (subject, predicate, [datatype,] object per line) is baked into the
        // append-only, content-addressed GTS segment bytes. Line-at-a-time parsing recovers
        // it exactly while still owning zero hand-rolled term lexing.
        let statement = purrdf::parse_dataset(line.as_bytes(), "application/n-triples", None)
            .map_err(|_| malformed("triple"))?;
        let mut quad_iter = statement.owned_quads();
        let quad = quad_iter.next().ok_or_else(|| malformed("triple"))?;
        if quad_iter.next().is_some() {
            return Err(malformed("triple (multiple statements on one line)"));
        }
        // A subject is an IRI or a blank node; a literal/quoted-triple subject is rejected
        // fail-closed (purrdf never yields a literal subject in N-Triples, but the guard keeps
        // the closed subset honest). The predicate is always an IRI in purrdf's `RdfQuad`.
        let s = match &quad.subject {
            RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => {
                intern_nt_term(&quad.subject, &mut terms, &mut seen, || {
                    malformed("subject")
                })?
            }
            RdfTerm::Literal(_) | RdfTerm::Triple(_) => return Err(malformed("subject")),
        };
        let p = intern_nt_iri(&quad.predicate, &mut terms, &mut seen);
        let o = intern_nt_term(&quad.object, &mut terms, &mut seen, || malformed("object"))?;
        quads.push((s, p, o, None));
    }

    let (mut writer, header) = library_writer(existing, medium)?;
    if let Some(receipt) = receipt {
        writer.add_blob(
            receipt.bytes().to_vec(),
            gmeow_logic::result_rdf::NATIVE_RECEIPT_MEDIA_TYPE,
            gmeow_logic::result_rdf::NATIVE_RECEIPT_REPRESENTATION,
        )?;
    }
    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    let mut out = header;
    out.extend_from_slice(&writer.into_bytes());
    Ok(out)
}

/// Hold the library's exclusive lock for the duration of `f`, serializing every
/// library operation — reads, precondition checks, and commits alike — against every
/// other holder. Callers that must read-then-decide-then-write (e.g.
/// `refute_conjecture`'s "is this id still in the library and not yet withdrawn?"
/// precondition) run the ENTIRE read → check → append sequence inside `f`, so two
/// concurrent callers can no longer both observe the pre-write state and both commit
/// (the lost-update / double-write race). The lock is released when the guard drops at
/// the end of this call, regardless of whether `f` succeeded.
///
/// What "exclusive" means is the backend's business ([`SegmentLibrary::lock`]): natively
/// a cross-process `flock` on a sidecar file, in a browser a mutex over the one
/// in-process library.
#[cfg(feature = "reasoning")]
pub(super) fn with_library_lock<T>(
    library: &dyn SegmentLibrary,
    f: impl FnOnce() -> gmeow_errors::Result<T>,
) -> gmeow_errors::Result<T> {
    let guard: Box<dyn LibraryLock + '_> = library.lock()?;
    let result = f();
    drop(guard);
    result
}

/// Atomically commit `segments` (each an already-serialized GTS `ai-package` segment, in order)
/// to `library`, ALL-OR-NOTHING: the new contents — the library's current bytes (if any)
/// followed by every segment in `segments`, in order — are assembled ENTIRELY in memory, then
/// handed to [`SegmentLibrary::replace_bytes`], which either lands the WHOLE new content or
/// leaves the PRIOR content completely untouched. So if anything fails partway (e.g. the audit
/// segment's bytes can't be built, or the commit fails), the library is never left holding some
/// but not all of `segments` (closing the "audit append fails, library append is left applied"
/// failure mode). The caller MUST already hold the library lock (see [`with_library_lock`]) —
/// this function does not lock by itself, so it can be called once per commit even when it
/// writes more than one segment.
#[cfg(feature = "reasoning")]
pub(super) fn append_library_segments(
    library: &dyn SegmentLibrary,
    segments: &[Vec<u8>],
) -> gmeow_errors::Result<()> {
    // Each segment is the FRAMES to append (plus its own header when it opens a segment), so
    // the commit concatenates them onto the library in order. They must have been authored in
    // that same order — each against the bytes the previous one produced — or the second one's
    // `prev` points past the first and the chain reads back broken.
    let mut bytes = library.read_bytes()?;
    for segment in segments {
        bytes.extend_from_slice(segment);
    }
    gmeow_logic::result_rdf::admit_reasoning_artifact(&bytes)?;
    library.replace_bytes(&bytes)
}

/// A per-segment collector for [`read_library`]: it captures each GTS segment's
/// term table and quads IN FILE (append) ORDER, so a `logic:conjectureLifecycleState`
/// supersession can be resolved as last-writer-wins by SEGMENT ORDER.
///
/// Order is the ONLY sound disambiguator here (R3). The unioned dataset a plain
/// `import_gts_events` yields carries EVERY state a node ever held at once — after a store
/// then a refute, one node holds both its engine verdict (`Open`/`Corroborated`/`Refuted…`)
/// AND `ConjectureWithdrawn` — and `gmeow:atTime` cannot break the tie either, because every
/// audit segment stamps the SAME fixed determinism epoch. The streaming reader
/// (`read_to_sink` with `allow_segments = true`) is the one path that preserves per-segment
/// identity: each appended `GtsWriter` blob reads back as its own segment, delivered in
/// append order, so folding the lifecycle assertions in that order makes the LAST one win.
#[cfg(feature = "reasoning")]
#[derive(Default)]
struct ConjectureSegments {
    /// One row per segment, indexed by segment order.
    segments: Vec<ConjectureSegmentRows>,
    /// The first reader diagnostic, if any — any diagnostic is a HARD read failure (no
    /// silent partial read of a corrupt library).
    diagnostic: Option<String>,
}

/// One segment's captured rows: its segment-local term table and its `(s, p, o)` quads.
#[cfg(feature = "reasoning")]
#[derive(Default)]
struct ConjectureSegmentRows {
    /// Segment-local term id → interned term (ids are dense from 0 within a segment).
    terms: Vec<Option<GtsTerm>>,
    /// `(subject, predicate, object)` segment-local term ids (the graph slot is dropped —
    /// the conjecture library writes only default-graph triples).
    quads: Vec<(usize, usize, usize)>,
}

#[cfg(feature = "reasoning")]
impl ConjectureSegments {
    /// The rows for `index`, growing the segment vector so an out-of-order or sparse
    /// segment index still lands in its own slot. A stream-global index this host cannot
    /// address is recorded as the read's diagnostic — a HARD failure — never wrapped.
    fn seg(&mut self, index: u64) -> Option<&mut ConjectureSegmentRows> {
        let Ok(index) = usize::try_from(index) else {
            self.diagnostic
                .get_or_insert_with(|| format!("segment index {index} exceeds addressable memory"));
            return None;
        };
        if index >= self.segments.len() {
            self.segments
                .resize_with(index + 1, ConjectureSegmentRows::default);
        }
        Some(&mut self.segments[index])
    }
}

#[cfg(feature = "reasoning")]
impl purrdf::gts::reader::StreamingSink for ConjectureSegments {
    fn term(&mut self, segment_index: u64, term_id: usize, term: Arc<GtsTerm>) {
        let Some(rows) = self.seg(segment_index) else {
            return;
        };
        if term_id >= rows.terms.len() {
            rows.terms.resize(term_id + 1, None);
        }
        rows.terms[term_id] = Some(term.as_ref().clone());
    }

    fn quad(&mut self, segment_index: u64, quad: purrdf::gts::model::Quad) {
        let (subject, predicate, object, _graph) = quad;
        if let Some(rows) = self.seg(segment_index) {
            rows.quads.push((subject, predicate, object));
        }
    }

    fn diagnostic(&mut self, diagnostic: &purrdf::gts::model::Diagnostic) {
        if self.diagnostic.is_none() {
            self.diagnostic = Some(format!("{}: {}", diagnostic.code, diagnostic.detail));
        }
    }
}

/// Read the append-only conjecture library at `path`, resolving each stored
/// `logic:Conjecture` node's **EFFECTIVE** `logic:conjectureLifecycleState` by SEGMENT
/// ORDER (last writer wins — see [`ConjectureSegments`] for why order, not the union or
/// `gmeow:atTime`, is the only sound key). A node typed `logic:Conjecture` in any segment is
/// in the library; its effective state is the object of the LAST lifecycle assertion for it
/// in append order. A missing library file is an EMPTY library (a first-ever refute of an
/// unknown id), not an error; any reader diagnostic or a torn trailing item is a HARD FAIL.
#[cfg(feature = "reasoning")]
pub(super) fn read_library(
    library: &dyn SegmentLibrary,
) -> gmeow_errors::Result<BTreeMap<String, ConjectureLifecycleState>> {
    let bytes = library.read_bytes()?;
    if bytes.is_empty() {
        // A library that has never been written is an EMPTY library (a first-ever
        // refute of an unknown id), not an error.
        return Ok(BTreeMap::new());
    }

    // Preserve operational file-order lifecycle folding below; separately admit
    // every exact aggregate and all required physical receipt occurrences first.
    gmeow_logic::result_rdf::admit_reasoning_artifact(&bytes)?;

    let mut sink = ConjectureSegments::default();
    let result = purrdf::gts::reader::read_to_sink(&bytes, true, None, &mut sink);
    if let Some(diag) = sink.diagnostic.take() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
            format!("conjecture library read diagnostic: {diag}"),
        )));
    }
    if let Some(first) = result.diagnostics.first() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
            format!(
                "conjecture library read diagnostic: {}: {}",
                first.code, first.detail
            ),
        )));
    }
    if let Some(offset) = result.torn {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
            format!("conjecture library has a torn trailing item at byte {offset}"),
        )));
    }

    let conjecture_iri = format!("{LOGIC_NAMESPACE}Conjecture");
    let lifecycle_iri = format!("{LOGIC_NAMESPACE}conjectureLifecycleState");

    let mut is_conjecture: BTreeSet<String> = BTreeSet::new();
    let mut effective: BTreeMap<String, ConjectureLifecycleState> = BTreeMap::new();

    // Fold the segments in FILE ORDER — a later segment's lifecycle assertion for a node
    // supersedes an earlier one, so `insert` (last-writer-wins) IS the supersession rule.
    for seg in &sink.segments {
        let resolve_iri = |id: usize| -> Option<&str> {
            seg.terms
                .get(id)?
                .as_ref()
                .filter(|term| term.kind == GtsTermKind::Iri)
                .and_then(|term| term.value.as_deref())
        };
        for &(subject, predicate, object) in &seg.quads {
            let (Some(subj), Some(pred)) = (resolve_iri(subject), resolve_iri(predicate)) else {
                continue;
            };
            if pred == RDF_TYPE {
                if resolve_iri(object) == Some(conjecture_iri.as_str()) {
                    is_conjecture.insert(subj.to_owned());
                }
            } else if pred == lifecycle_iri
                && let Some(obj) = resolve_iri(object)
                && let Some(local) = obj.strip_prefix(LOGIC_NAMESPACE)
                && let Some(state) = ConjectureLifecycleState::from_local(local)
            {
                effective.insert(subj.to_owned(), state);
            }
        }
    }

    // A node is in the library iff it was typed `logic:Conjecture`; every such node carries a
    // lifecycle state (REQUIRED by logic:ConjectureShape), so the intersection is exactly the
    // library's conjecture set paired with its effective, segment-order-resolved state.
    Ok(effective
        .into_iter()
        .filter(|(node, _)| is_conjecture.contains(node))
        .collect())
}

/// NATIVE ONLY: compaction rewrites a store IN PLACE through an atomic replace, and both
/// halves of that — the append-only segment library and the temp file the replace swaps
/// through — are filesystem doors this crate deliberately does not carry into the browser
/// image. A wasm consumer holds its store as live values, so there is nothing to compact.
#[cfg(all(not(target_arch = "wasm32"), feature = "reasoning"))]
/// lane.
///
/// A long-lived pack accumulates one segment boundary per medium change and one frame
/// per record; compaction rewrites that into a single streamable segment under a
/// single declared medium. The content claims are rewrite-invariant, so what changes
/// is the LAYOUT and the MEDIUM, never a statement.
///
/// # The dictionary is PINNED, not derived
///
/// `medium` is the resolved [`StoreMedium`] — the bytes [`crate::store_medium`] read out of
/// the loaded bundle's in-band `"dct"` map — and they are handed to upstream as
/// `purrdf::gts::compact::DictStrategy::Pinned`: used verbatim, no training, no corpus
/// derivation, no truncation. That is what makes one `gmeow:dictionaryId` resolve to
/// exactly one byte sequence. Deriving the dictionary from the pack's own content-blob
/// corpus instead would label PACK-LOCAL bytes with the shipped id, so two compacted
/// stores could pin different dictionaries under the same name — precisely what the
/// envelope contract exists to make impossible.
///
/// Pinning also removes the corpus precondition the derived strategies carry: a
/// wholly-pinned plan never touches the content-blob corpus, so an agent-memory store
/// — whose records are `terms`/`quads` frames and which has no content blobs at all —
/// compacts exactly like any other pack. The dictionary rides the new header in band,
/// so the compacted file stays self-decoding without the bundle.
///
/// The whole read → rewrite → replace runs under the store lock, and the replace is
/// atomic: a compaction that fails part-way leaves the PRIOR store completely intact
/// rather than a half-rewritten one.
///
/// # Errors
/// The store cannot be read, is not safely compactable (refuse-don't-trust), or the
/// atomic replace fails.
pub fn compact_store(
    path: &std::path::Path,
    timestamp: &str,
    medium: &StoreMedium,
    packaging_signer: (purrdf_ed25519::SigningKey, String),
) -> gmeow_errors::Result<()> {
    // The branch's lock is LIBRARY-scoped rather than path-scoped: a store is reached
    // through its `SegmentLibrary`, so the compaction lane takes the same lock every
    // other writer to that store takes.
    let library = crate::storage::fs_segment_library(path.to_path_buf());
    with_library_lock(library.as_ref(), move || {
        let bytes = std::fs::read(path)?;
        gmeow_logic::result_rdf::admit_reasoning_artifact(&bytes)?;
        let compacted = gmeow_gts_profile::compact_gmeow_gts(
            &bytes,
            timestamp,
            &medium.dictionary,
            purrdf::gts::compact::DictStrategy::Pinned(medium.bytes.clone()),
            packaging_signer,
        )?;
        gmeow_logic::result_rdf::admit_reasoning_artifact(&compacted)?;
        let dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from);
        let mut tmp = tempfile::Builder::new()
            .prefix(".compact-")
            .suffix(".tmp")
            .tempfile_in(&dir)?;
        std::io::Write::write_all(&mut tmp, &compacted)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        Ok(())
    })
}

#[cfg(all(test, feature = "reasoning", not(target_arch = "wasm32")))]
#[path = "libraries.tests.rs"]
mod tests;
