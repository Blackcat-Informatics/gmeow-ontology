// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Assess retained native worlds, never the verifier's flattened presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_errors::{
    Diag, DiagLedger, Finding, FindingCategory, Grade, Location, ResultExt, Severity, StageId,
    Standpoint, register_code,
};
use purrdf::{DatasetMut, MutableDataset, QuadValues, RdfDataset, TermValue};

use crate::reason::InferredAxiom;
use crate::reason::refute::NativeFamilyCompletion;
use crate::reason::refute::RefutationPremise;
use crate::reason::refute::native::{NativeClosureStatus, NativeFamilyLedger, NativeProofOrigin};
use crate::result::ReasoningResult;

use super::{
    DISCHARGE_FINITE, DISCHARGE_SYNTACTIC, LOGIC_NS, Obligation, ProgramReachability,
    ReachabilityEvidence, attribute_to_candidates, candidates_by_obligation,
    check_discharge_conditions, check_finite_closure, check_reachability, governance_node,
    obligation_diag, obligation_err, parse_obligations,
};

/// The actual sufficient proof, uncertainty, or positive counterexample.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ObligationDisposition {
    /// No admitted producer can write the forbidden predicate.
    SyntacticallyDischarged,
    /// The certified completed closure contains no derived forbidden edge.
    FiniteClosureDischarged,
    /// A committed derived edge, irrespective of the declared negative-proof arm.
    Violated(Box<InferredAxiom>),
    /// No selected sufficient negative proof succeeded.
    Unknown,
    /// Missing or unwired discharge conditions are capability errors.
    InvalidDischarge,
}

/// One obligation under its original native world and program binding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObligationOutcome {
    /// Exact execution world, never a flattened union of source contexts.
    pub world: String,
    /// Native resource identity, including deterministic document-scoped blank lowering.
    pub obligation: String,
    /// The declaration has asserted native support in this world, independently
    /// of any other declaration derived for the same or a different identity.
    pub authored: bool,
    /// Exact forbidden predicate named by the admitted declaration.
    pub forbidden_predicate: String,
    /// Actual selected proof or unresolved/error disposition.
    pub disposition: ObligationDisposition,
}

/// A deterministic assessment bound to the authenticated native execution's analysis.
#[derive(Debug, Clone)]
pub struct ObligationAssessment {
    /// Content address of the exact program/source/world/engine writer analysis.
    pub analysis: [u8; 32],
    /// Every discovered and admitted obligation, including unknowns and hard errors.
    pub outcomes: Vec<ObligationOutcome>,
    pub(crate) condition_errors: BTreeMap<(String, String), String>,
    pub(crate) absence_reasons: BTreeMap<(String, String), String>,
    /// Sole diagnostic owner; semantic outcomes remain native assessment data.
    ledger: DiagLedger,
}

/// A selected declaration whose fields could not be read as an obligation.
pub(crate) struct MalformedObligation {
    pub(crate) world: String,
    pub(crate) obligation: String,
    pub(crate) forbidden_predicate: Option<String>,
    pub(crate) witness: Option<Box<InferredAxiom>>,
    pub(crate) diagnostic: Diag,
}

impl ObligationAssessment {
    /// Borrow the native diagnostic owner without changing semantic outcomes.
    pub fn ledger(&self) -> &DiagLedger {
        &self.ledger
    }

    /// Project current diagnostics at the verifier's wire boundary.
    pub fn findings(&self) -> impl Iterator<Item = Finding> + '_ {
        self.ledger.iter_findings("verify")
    }

    fn record(&mut self, diag: Diag) {
        self.ledger.attach(diag, StageId::new("verify.obligations"));
    }
}

/// The authored rows required for a complete non-entailment declaration.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ObligationField {
    Declaration,
    ForbiddenPredicate,
    DischargeCondition,
}

impl ObligationField {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Declaration => "NonEntailmentObligation",
            Self::ForbiddenPredicate => "obligationForbiddenPredicate",
            Self::DischargeCondition => "obligationDischargeCondition",
        }
    }
}

pub(crate) fn obligation_field(
    predicate: &str,
    object_iri: Option<&str>,
) -> Option<ObligationField> {
    match predicate {
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        | "https://blackcatinformatics.ca/logic/instanceOf"
            if object_iri
                == Some("https://blackcatinformatics.ca/logic/NonEntailmentObligation") =>
        {
            Some(ObligationField::Declaration)
        }
        "https://blackcatinformatics.ca/logic/obligationForbiddenPredicate" => {
            Some(ObligationField::ForbiddenPredicate)
        }
        "https://blackcatinformatics.ca/logic/obligationDischargeCondition" => {
            Some(ObligationField::DischargeCondition)
        }
        _ => None,
    }
}

/// Retain only governance declaration/field statements in a native typed view.
/// The execution already normalized blank identities; no RDF text is emitted or parsed.
pub(crate) fn governance_statement(predicate: &str, object: &TermValue) -> bool {
    obligation_field(predicate, object.as_iri()).is_some()
        || predicate == "https://blackcatinformatics.ca/logic/candidateNonEntailment"
        || (matches!(
            predicate,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                | "https://blackcatinformatics.ca/logic/instanceOf"
        ) && object.as_iri()
            == Some("https://blackcatinformatics.ca/logic/FormalizationCandidate"))
}

fn governance_view(
    ledger: &NativeFamilyLedger,
    selected: Option<&dyn Fn(&RefutationPremise) -> bool>,
) -> gmeow_errors::Result<(Arc<RdfDataset>, BTreeSet<String>)> {
    let mut store = MutableDataset::new(purrdf::RdfDatasetBuilder::new().freeze()?);
    let mut authored = BTreeSet::new();
    for proof in &ledger.proofs {
        let statement = &proof.statement;
        if let Some(admits) = selected {
            let Some(field) = obligation_field(&statement.predicate, statement.object.as_iri())
            else {
                continue;
            };
            let NativeProofOrigin::Asserted { sources, .. } = &proof.origin else {
                continue;
            };
            for source in sources.iter().filter(|source| admits(source)) {
                if obligation_field(&source.predicate, source.object.as_iri()) != Some(field) {
                    continue;
                }
                if field == ObligationField::Declaration {
                    authored.insert(&source.subject);
                }
                store
                    .insert(QuadValues {
                        s: source.subject.clone(),
                        p: TermValue::iri(&source.predicate),
                        o: source.object.clone(),
                        g: None,
                    })
                    .ctx("selected native governance projection failed")?;
            }
            continue;
        }
        let selected = governance_statement(&statement.predicate, &statement.object);
        if selected {
            if obligation_field(&statement.predicate, statement.object.as_iri())
                == Some(ObligationField::Declaration)
                && matches!(proof.origin, NativeProofOrigin::Asserted { .. })
            {
                authored.insert(&statement.subject);
            }
            // Scalar declaration fields must retain their original RDF kind:
            // native blank lowering cannot turn an invalid predicate/condition
            // blank into an admissible IRI. Reference links keep native identity.
            let originals = match &proof.origin {
                NativeProofOrigin::Asserted { sources, .. }
                    if matches!(
                        statement.predicate.strip_prefix(LOGIC_NS),
                        Some("obligationForbiddenPredicate" | "obligationDischargeCondition")
                    ) =>
                {
                    Some(sources.as_slice())
                }
                _ => None,
            };
            let objects = originals
                .into_iter()
                .flatten()
                .map(|source| &source.object)
                .chain(originals.is_none().then_some(&statement.object));
            for object in objects {
                store
                    .insert(QuadValues {
                        s: statement.subject.clone(),
                        p: TermValue::iri(&statement.predicate),
                        o: object.clone(),
                        g: None,
                    })
                    .ctx("native governance projection failed")?;
            }
        }
    }
    let store = store.freeze().ctx("native governance view failed")?;
    let authored = authored
        .into_iter()
        .map(|subject| {
            let node = store.term_id_by_value(subject).ok_or_else(|| {
                obligation_err(
                    "authored obligation is missing from its native governance view".to_owned(),
                )
            })?;
            governance_node(&store, node)
        })
        .collect::<gmeow_errors::Result<BTreeSet<_>>>()?;
    Ok((store, authored))
}

fn source_complete(
    analysis: &ProgramReachability,
    execution: &crate::result::NativeExecutionEvidence,
    world: &str,
) -> bool {
    let admissions = &execution.source_coverage.worlds[world].admissions;
    analysis.source_complete(world, !admissions.is_empty())
        && execution
            .class_admission
            .selected_worlds
            .get(world)
            .is_none_or(|source| source.refusal.is_none())
        && admissions.iter().all(|admission| {
            admission.completion == NativeFamilyCompletion::Complete
                && admission.obstructions.is_empty()
        })
}

fn disposition(
    obligation: &Obligation,
    counterexample: Option<&InferredAxiom>,
    may_write: bool,
    source_complete: bool,
    finite_complete: bool,
    conditions_valid: bool,
) -> ObligationDisposition {
    // A declaration chooses a sufficient absence proof; it cannot suppress a
    // supported positive counterexample, even under an unrelated incomplete lane.
    if let Some(counterexample) = counterexample {
        return ObligationDisposition::Violated(Box::new(counterexample.clone()));
    }
    if !conditions_valid {
        return ObligationDisposition::InvalidDischarge;
    }
    if source_complete
        && !may_write
        && obligation
            .discharge_conditions
            .contains(DISCHARGE_SYNTACTIC)
    {
        return ObligationDisposition::SyntacticallyDischarged;
    }
    if finite_complete && obligation.discharge_conditions.contains(DISCHARGE_FINITE) {
        return ObligationDisposition::FiniteClosureDischarged;
    }
    ObligationDisposition::Unknown
}

fn assess_world(
    analysis: &ProgramReachability,
    execution: &crate::result::NativeExecutionEvidence,
    ledger: &NativeFamilyLedger,
    derived: &BTreeMap<&str, BTreeMap<&str, &InferredAxiom>>,
    assessment: &mut ObligationAssessment,
    selected: Option<&dyn Fn(&RefutationPremise) -> bool>,
    invalid: &mut impl FnMut(MalformedObligation) -> gmeow_errors::Result<()>,
) -> gmeow_errors::Result<()> {
    let world = ledger.world.as_str();
    let source_graph = purrdf::iri::BaseIri::parse(world)?;
    let (store, authored) = governance_view(ledger, selected)?;
    let counterexample = |predicate: &str| {
        derived
            .get(world)
            .and_then(|rows| rows.get(predicate))
            .copied()
    };
    let obligations = parse_obligations(&store, |obligation, forbidden_predicate, diagnostic| {
        let witness = forbidden_predicate
            .as_deref()
            .and_then(counterexample)
            .cloned()
            .map(Box::new);
        invalid(MalformedObligation {
            world: world.to_owned(),
            obligation,
            forbidden_predicate,
            witness,
            diagnostic,
        })
    })?;
    let candidates = candidates_by_obligation(&store)?;
    let source_complete = source_complete(analysis, execution, world);
    let finite_complete = source_complete
        && execution.status == NativeClosureStatus::Completed
        && ledger.complete()
        && execution.classes.iter().any(|class| {
            class.world == world
                && matches!(
                    class.completion,
                    NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
                )
        });
    let mut discharged = BTreeSet::new();
    for obligation in &obligations {
        let conditions = check_discharge_conditions(&source_graph, obligation);
        let conditions_valid = conditions.is_empty();
        if selected.is_some() && !conditions_valid {
            assessment.condition_errors.insert(
                (world.to_owned(), obligation.iri.clone()),
                conditions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        for diag in conditions {
            assessment.record(attribute_to_candidates(diag, &obligation.iri, &candidates));
        }
        let counterexample = counterexample(&obligation.forbidden_predicate);
        let may_write = analysis.may_write(world, &obligation.forbidden_predicate)?;
        let status = disposition(
            obligation,
            counterexample,
            may_write,
            source_complete,
            finite_complete,
            conditions_valid,
        );
        match &status {
            ObligationDisposition::Violated(_) => {
                let predicates = BTreeSet::from([obligation.forbidden_predicate.clone()]);
                if let Some(diag) =
                    check_finite_closure(&source_graph, obligation, &predicates, &candidates)
                {
                    assessment.record(diag);
                }
            }
            ObligationDisposition::Unknown => {
                let diag = check_reachability(&source_graph, obligation, may_write, &candidates).unwrap_or_else(|| {
                        attribute_to_candidates(obligation_diag(&source_graph, obligation, "absence-proof",
                            "verify.non-entailment.not-discharged",
                            format!("non-entailment obligation <{}> has no certified selected absence proof (logic:ObligationUnknown); source_complete={source_complete}, finite_complete={finite_complete}", obligation.iri),
                        ), &obligation.iri, &candidates)
                    });
                if selected.is_some() {
                    assessment.absence_reasons.insert(
                        (world.to_owned(), obligation.iri.clone()),
                        diag.message().to_owned(),
                    );
                }
                assessment.record(diag);
            }
            ObligationDisposition::SyntacticallyDischarged
            | ObligationDisposition::FiniteClosureDischarged => {
                discharged.insert(obligation.iri.clone());
            }
            ObligationDisposition::InvalidDischarge => {}
        }
        assessment.outcomes.push(ObligationOutcome {
            world: world.to_owned(),
            obligation: obligation.iri.clone(),
            authored: authored.contains(&obligation.iri),
            forbidden_predicate: obligation.forbidden_predicate.clone(),
            disposition: status,
        });
    }
    // A descriptive governance note, neither transient chatter nor a proof of discharge.
    let inventory = Diag::new(register_code("verify.non-entailment.inventory"),
        Grade::new(Severity::Note, FindingCategory::PolicyWarning, Standpoint::Binding), format!(
            "discovered and checked {} distinct non-entailment obligation(s) in world <{world}>; authored={}, admitted={}, discharged={}", obligations.len(), authored.len(), obligations.len(), discharged.len(),
        ))
        .with_location(Location { logical: Some(world.to_owned()), ..Location::default() })
        .with_focus("obligation-inventory")
        .with_graph(source_graph)
        .with_cited_iris(obligations
        .iter()
        .filter(|obligation| !obligation.iri.starts_with("_:"))
        .map(|obligation| obligation.iri.clone())
        )
        .with_context(format!(
            "{}; authored obligations: {authored:?}; discovered/admitted obligations: {:?}; discharged obligations: {discharged:?}",
            analysis.inventory_detail(world)?,
            obligations.iter().map(|obligation| obligation.iri.as_str()).collect::<Vec<_>>(),
        ));
    assessment.record(inventory);
    Ok(())
}

/// Assess all obligations against the selected program and each retained native world.
/// Syntactic possibility never fabricates violation; a genuine counterexample always
/// survives, while absence needs a selected sufficient proof with complete source scope.
///
/// # Errors
/// Rejects missing, stale, foreign or class-only evidence, malformed declarations and
/// invalid candidate references. No source program, corpus or closure is regenerated.
pub fn assess_non_entailment_obligations(
    result: &ReasoningResult,
) -> gmeow_errors::Result<ObligationAssessment> {
    result.validate_native_closure()?;
    assess_obligations(result, None, |record| Err(record.diagnostic))
}

/// Keep invalid declarations as named boundaries beside supported witnesses.
/// Execution and candidate-reference failures remain errors.
pub(crate) fn assess_selected_obligations(
    result: &ReasoningResult,
    selected: &dyn Fn(&RefutationPremise) -> bool,
) -> gmeow_errors::Result<(ObligationAssessment, Vec<MalformedObligation>)> {
    let mut malformed = Vec::new();
    let assessment = assess_obligations(result, Some(selected), |record| {
        malformed.push(record);
        Ok(())
    })?;
    Ok((assessment, malformed))
}

fn assess_obligations(
    result: &ReasoningResult,
    selected: Option<&dyn Fn(&RefutationPremise) -> bool>,
    mut invalid: impl FnMut(MalformedObligation) -> gmeow_errors::Result<()>,
) -> gmeow_errors::Result<ObligationAssessment> {
    let execution = result.native_execution()?;
    let ReachabilityEvidence::Program(analysis) = &execution.reachability else {
        return Err(obligation_err(
            "class diagnostics do not authorize program obligation discharge".to_owned(),
        ));
    };
    if result.provenance.engine != crate::result::EngineId::native()
        || result.provenance.contract_hash != crate::reason::native_contract_hash()
    {
        return Err(obligation_err(
            "obligation assessment has a foreign native engine".to_owned(),
        ));
    }
    let mut assessment = ObligationAssessment {
        analysis: analysis.identity(),
        outcomes: Vec::new(),
        condition_errors: BTreeMap::new(),
        absence_reasons: BTreeMap::new(),
        ledger: DiagLedger::new(),
    };
    let mut derived = BTreeMap::<&str, BTreeMap<&str, &InferredAxiom>>::new();
    for row in result.inferred().iter().filter(|row| !row.is_edb) {
        derived
            .entry(&row.world)
            .or_default()
            .entry(&row.predicate)
            .or_insert(row);
    }
    for ledger in &execution.families {
        assess_world(
            analysis,
            execution,
            ledger,
            &derived,
            &mut assessment,
            selected,
            &mut invalid,
        )?;
    }
    assessment
        .outcomes
        .sort_by(|a, b| (&a.world, &a.obligation).cmp(&(&b.world, &b.obligation)));
    Ok(assessment)
}
