// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native producer observations for the independent oracle and TPTP consumers.
//! Serialized proof observations are diagnostic data, never proof certificates.

use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};
use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic::query_ir::{AnswerSet, Budget, parse_query_program};
use serde::{Deserialize, Serialize};

use crate::external::ExternalOutcome;
use crate::external::tptp::{
    AnnotatedFormula, DecisionError, TptpError, lower_and_decide, lower_to_fol_program, parse_tptp,
};

/// Native OntoUML discipline evidence, retaining capability gaps separately from errors.
pub type OntoumlOutcome = Result<BTreeSet<String>, crate::external::ontouml::OntoumlError>;

/// Observe one authored model through the native foundation evaluator. A source
/// parse and one native lowering supply the chase; no intermediate RDF text is built.
pub fn ontouml(bytes: &[u8], world: &str) -> gmeow_errors::Result<OntoumlOutcome> {
    use crate::external::ontouml::{evaluate_model, fired_disciplines, parse_ontouml_model};
    let text = std::str::from_utf8(bytes).map_err(gmeow_errors::Diag::from)?;
    Ok(parse_ontouml_model(text, None).and_then(|model| {
        evaluate_model(
            &model,
            world,
            gmeow_logic::foundation::AntiRigidityPolicy::WitnessObligation,
        )
        .map(|(quads, _dataset)| fired_disciplines(&quads))
    }))
}

/// Exact native projection consumed by the frozen independent DL oracle gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleVerdict {
    /// Native shared reasoning result's consistency, including its modal contract.
    pub consistent: bool,
    /// Unsatisfiable classes recovered from that same result's closure.
    pub unsatisfiable_classes: BTreeSet<String>,
}

/// Evaluate the selected Turtle fixture through the oracle lane's `reason_all`
/// operation. This is not interchangeable with the DL-only consistency operation.
pub fn dl_oracle(bytes: &[u8]) -> gmeow_errors::Result<OracleVerdict> {
    let dataset = purrdf::parse_dataset(bytes, purrdf::NativeRdfFormat::Turtle.media_type(), None)
        .map_err(gmeow_errors::Diag::from)?;
    let reasoning_input = prepare_reasoning_input(&dataset)?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow-conformance.dl-oracle.v1".to_owned(),
        *reasoning_input.ingress_contract(),
    )?])?;
    let result = gmeow_logic::reason::reason_all(reasoning_input, &domains)?;
    Ok(OracleVerdict {
        consistent: result.is_consistent(),
        unsatisfiable_classes: gmeow_logic::reason::dl::unsatisfiable_from_inferred(
            result.inferred(),
        )
        .into_iter()
        .map(|class| class.class)
        .collect(),
    })
}

/// Public dispatch outcome, preserving bindings, budget status, loss and frontier.
/// The outer producer result separately carries input/parse failures.
pub type NumericOutcome = Result<AnswerSet, gmeow_errors::RecordedDiag>;

#[derive(Deserialize)]
struct NumericInput {
    #[serde(default)]
    facts: Vec<NumericFact>,
    program: String,
}

#[derive(Deserialize)]
struct NumericFact {
    s: String,
    p: String,
    lex: String,
    datatype: String,
}

/// Load the selected facts once and invoke the public procedural dispatch surface.
/// A setup failure cannot satisfy an oracle expectation of a semantic refusal.
pub fn numeric_oracle(bytes: &[u8]) -> gmeow_errors::Result<NumericOutcome> {
    use gmeow_logic::profile_gate::PROCEDURAL_PROLOG_PROFILE;
    use gmeow_logic::seam::WorldFactSnapshot;
    use gmeow_logic::store::WorldStore;

    const WORLD: &str = "https://example.org/1428/world";
    let input: NumericInput = serde_json::from_slice(bytes).map_err(gmeow_errors::Diag::from)?;
    let program = parse_query_program(&input.program)?;
    let store = WorldStore::new();
    for fact in input.facts {
        store.insert_quad_terms(
            WORLD,
            purrdf::TermValue::iri(fact.s),
            purrdf::TermValue::iri(fact.p),
            purrdf::TermValue::typed_literal(fact.lex, fact.datatype),
        )?;
    }
    let foreign = WorldFactSnapshot::from_world(&store, WORLD, PROCEDURAL_PROLOG_PROFILE)?;
    Ok(gmeow_logic::dispatch::dispatch_query(
        &foreign,
        WORLD,
        &program,
        PROCEDURAL_PROLOG_PROFILE,
        &Budget::default(),
    )
    .map_err(|error| {
        gmeow_errors::DiagLedger::new()
            .record(error, gmeow_errors::StageId::new("native-numeric-oracle"))
    }))
}

/// One answer's complete exported proof surface. It confers no execution authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofAnswer {
    /// Ground answer atom as rendered by the native prover.
    pub atom: String,
    /// Exact goal-variable bindings.
    pub bindings: BTreeMap<String, String>,
    /// Root-first step observations with identities, rules and premise references.
    pub steps: Vec<gmeow_logic::proof_tree::ProofStepView>,
    /// Terminal TSTP export of the native checked proof.
    pub tstp: String,
    /// Parsed terminal export, used only to grade the required round-trip contract.
    pub parsed: Vec<AnnotatedFormula>,
}

/// Native proof result projected without exposing a deserializable checked proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofObservation {
    /// Content-addressed selected program identity.
    pub iri: String,
    /// Goal template retained by the native prover.
    pub goal: String,
    /// Native completion status, including partial results.
    pub status: String,
    /// Deterministically ordered proof-carrying answer observations.
    pub answers: Vec<ProofAnswer>,
}

/// A single parsed source supplies the independent DL and Horn selections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TptpObservation {
    /// Exact selected source syntax, including roles and source annotations.
    pub formulas: Vec<AnnotatedFormula>,
    /// The native DL reduction's outcome or typed refusal/failure.
    pub decision: Result<ExternalOutcome, DecisionError>,
    /// The native Horn prover's outcome or typed refusal/failure.
    pub proof: Result<ProofObservation, DecisionError>,
}

/// Observe both native selections over one parse. Syntax/unsupported-source errors
/// retain their distinction outside the two independent operation results.
pub fn tptp(bytes: &[u8], world: &str) -> gmeow_errors::Result<Result<TptpObservation, TptpError>> {
    let text = std::str::from_utf8(bytes).map_err(gmeow_errors::Diag::from)?;
    let formulas = match parse_tptp(text) {
        Ok(formulas) => formulas,
        Err(error) => return Ok(Err(error)),
    };
    let decision = lower_and_decide(&formulas, world).map(|(outcome, _)| outcome);
    let proof = observe_proof(&formulas);
    Ok(Ok(TptpObservation {
        formulas,
        decision,
        proof,
    }))
}

fn observe_proof(formulas: &[AnnotatedFormula]) -> Result<ProofObservation, DecisionError> {
    let failure = |error: String| DecisionError::Failure { detail: error };
    let program = lower_to_fol_program(formulas)?;
    let result = gmeow_logic::proof_tree::prove_reasoning_program(&program, &[])
        .map_err(|error| failure(error.to_string()))?;
    let answers = result
        .answers
        .into_iter()
        .map(|answer| {
            let tstp = answer
                .tree
                .to_tstp()
                .map_err(|error| failure(error.to_string()))?;
            let parsed = parse_tptp(&tstp).map_err(|error| failure(error.to_string()))?;
            Ok(ProofAnswer {
                atom: answer.atom,
                bindings: answer.bindings,
                steps: answer.tree.steps().to_vec(),
                tstp,
                parsed,
            })
        })
        .collect::<Result<_, DecisionError>>()?;
    Ok(ProofObservation {
        iri: result.iri,
        goal: result.goal,
        status: result.status,
        answers,
    })
}

#[path = "observations.tests.rs"]
#[cfg(test)]
mod tests;
