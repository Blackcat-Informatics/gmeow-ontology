// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Assess retained native worlds, never the verifier's flattened presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_errors::{
    Diag, DiagLedger, Finding, FindingCategory, Grade, Label, Location, ResultExt, Severity,
    StageId, Standpoint, register_code,
};
use purrdf::{DatasetMut, MutableDataset, QuadValues, RdfDataset, TermValue};

use crate::reason::InferredAxiom;
use crate::reason::refute::NativeFamilyCompletion;
use crate::reason::refute::native::{NativeClosureStatus, NativeFamilyLedger, NativeProofOrigin};
use crate::result::ReasoningResult;

use super::{
    DISCHARGE_FINITE, DISCHARGE_SYNTACTIC, LOGIC_NS, Obligation, ProgramReachability,
    ReachabilityEvidence, attribute_to_candidates, candidates_by_obligation,
    check_discharge_conditions, check_finite_closure, check_reachability, obligation_diag,
    obligation_err, parse_obligations,
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
    /// Sole diagnostic owner; semantic outcomes remain native assessment data.
    ledger: DiagLedger,
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

/// Retain only governance declaration/field statements in a native typed view.
/// The execution already normalized blank identities; no RDF text is emitted or parsed.
pub(crate) fn governance_statement(predicate: &str, object: &TermValue) -> bool {
    match predicate {
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        | "https://blackcatinformatics.ca/logic/instanceOf" => matches!(
            object.as_iri(),
            Some(
                "https://blackcatinformatics.ca/logic/NonEntailmentObligation"
                    | "https://blackcatinformatics.ca/logic/FormalizationCandidate"
            )
        ),
        predicate => matches!(
            predicate.strip_prefix(LOGIC_NS),
            Some(
                "obligationForbiddenPredicate"
                    | "obligationDischargeCondition"
                    | "candidateNonEntailment"
            )
        ),
    }
}

fn governance_view(ledger: &NativeFamilyLedger) -> gmeow_errors::Result<(Arc<RdfDataset>, usize)> {
    let mut store = MutableDataset::new(purrdf::RdfDatasetBuilder::new().freeze()?);
    let mut authored = BTreeSet::new();
    for proof in &ledger.proofs {
        let statement = &proof.statement;
        let selected = governance_statement(&statement.predicate, &statement.object);
        if selected {
            if matches!(
                statement.predicate.as_str(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    | "https://blackcatinformatics.ca/logic/instanceOf"
            ) && statement.object.as_iri()
                == Some("https://blackcatinformatics.ca/logic/NonEntailmentObligation")
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
    store
        .freeze()
        .map(|store| (store, authored.len()))
        .ctx("native governance view failed")
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
) -> gmeow_errors::Result<()> {
    let world = ledger.world.as_str();
    let (store, authored) = governance_view(ledger)?;
    let obligations = parse_obligations(&store)?;
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
    let mut discharged = 0;
    for obligation in &obligations {
        let conditions = check_discharge_conditions(world, obligation);
        let conditions_valid = conditions.is_empty();
        for diag in conditions {
            assessment.record(attribute_to_candidates(diag, &obligation.iri, &candidates));
        }
        let counterexample = derived
            .get(world)
            .and_then(|rows| rows.get(obligation.forbidden_predicate.as_str()))
            .copied();
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
                    check_finite_closure(world, obligation, &predicates, &candidates)
                {
                    assessment.record(diag);
                }
            }
            ObligationDisposition::Unknown => {
                let diag = check_reachability(world, obligation, may_write, &candidates).unwrap_or_else(|| {
                        attribute_to_candidates(obligation_diag(world, obligation, "absence-proof",
                            "verify.non-entailment.not-discharged",
                            format!("non-entailment obligation <{}> has no certified selected absence proof (logic:ObligationUnknown); source_complete={source_complete}, finite_complete={finite_complete}", obligation.iri),
                        ), &obligation.iri, &candidates)
                    });
                assessment.record(diag);
            }
            ObligationDisposition::SyntacticallyDischarged
            | ObligationDisposition::FiniteClosureDischarged => discharged += 1,
            ObligationDisposition::InvalidDischarge => {}
        }
        assessment.outcomes.push(ObligationOutcome {
            world: world.to_owned(),
            obligation: obligation.iri.clone(),
            forbidden_predicate: obligation.forbidden_predicate.clone(),
            disposition: status,
        });
    }
    // A descriptive governance note, neither transient chatter nor a proof of discharge.
    let inventory = Diag::new(register_code("verify.non-entailment.inventory"),
        Grade::new(Severity::Note, FindingCategory::PolicyWarning, Standpoint::Binding), format!(
            "discovered and checked {} distinct non-entailment obligation(s) in world <{world}>; authored={authored}, admitted={}, discharged={discharged}", obligations.len(), obligations.len(),
        ))
        .with_location(Location { logical: Some(world.to_owned()), ..Location::default() })
        .with_focus("obligation-inventory")
        .with_tag(format!("world:{world}"))
        .with_label(Label { location: Location { logical: Some(world.to_owned()), ..Location::default() }, text: "native world".to_owned() })
        .with_cited_iris(obligations
        .iter()
        .filter(|obligation| !obligation.iri.starts_with("_:"))
        .map(|obligation| obligation.iri.clone())
        )
        .with_context(analysis.inventory_detail(world)?);
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
        assess_world(analysis, execution, ledger, &derived, &mut assessment)?;
    }
    assessment
        .outcomes
        .sort_by(|a, b| (&a.world, &a.obligation).cmp(&(&b.world, &b.obligation)));
    Ok(assessment)
}
