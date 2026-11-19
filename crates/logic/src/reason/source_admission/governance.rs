// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bind native governance outcomes to the selected source occurrences.

use std::collections::{BTreeMap, HashMap, HashSet};

use gmeow_logic_compile::frontend::{SourceCarrier, SourceStatementBinding};
use gmeow_logic_compile::source_inventory::SelectedSourceView;
use purrdf::dataset_view::GraphMatch;
use purrdf::{DatasetView, QuadIds, RdfDataset, TermValue};
use purrdf_hash::fixed::FixedState;
use serde::Serialize;

use super::{
    Disposition, ENGINE_NATIVE, NativeSourceAdmission, NativeSourceDrop, ReasoningResult,
    SourceObligationId, SourceObligationOrigin, TheoryUnitSelection, reason_err,
};
use crate::obligations::{
    ObligationDisposition, ObligationField, assess_selected_obligations, obligation_field,
};
use crate::reason::InferredAxiom;
use crate::reason::refute::{RefutationPremise, native::NativeProofOrigin};

/// Governance observations from the live selected native execution.
#[derive(Debug, Serialize)]
pub struct SelectedNonEntailment {
    /// Content identity of the actual program and world writer analysis.
    pub analysis: String,
    /// Selected declarations and their exact outcome and source coordinates.
    pub obligations: Vec<SelectedNonEntailmentObligation>,
}

/// One obligation evaluated within the selected theory.
#[derive(Debug, Serialize)]
pub struct SelectedNonEntailmentObligation {
    /// Actual committed native world.
    pub world: String,
    /// Scoped identity used by that world.
    pub obligation: String,
    /// Predicate named by a readable forbidden-predicate field.
    pub forbidden_predicate: Option<String>,
    /// Exact selected authored source obligations used by the assessment.
    pub sources: Vec<SourceObligationId>,
    /// Outcome within the public proof contract.
    pub disposition: SelectedNonEntailmentDisposition,
    /// An invalid condition retained beside a supported violation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_error: Option<String>,
}

/// A selected discharge, a supported counterexample, or a named boundary.
#[derive(Debug, Serialize)]
pub enum SelectedNonEntailmentDisposition {
    /// A complete selected account establishes syntactic unreachability.
    SyntacticallyDischarged,
    /// The certified finite selected closure contains no forbidden inference.
    FiniteClosureDischarged,
    /// Exact committed native inference of the forbidden predicate.
    Violated(Box<InferredAxiom>),
    /// A required source account or selected absence proof is incomplete.
    Inconclusive { reason: &'static str },
    /// The selected declaration has malformed or unsupported fields.
    Invalid { detail: String },
}

impl SelectedNonEntailmentDisposition {
    /// Stable spelling for the text report.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SyntacticallyDischarged => "syntactically-discharged",
            Self::FiniteClosureDischarged => "finite-closure-discharged",
            Self::Violated(_) => "violated",
            Self::Inconclusive { .. } => "inconclusive",
            Self::Invalid { .. } => "invalid",
        }
    }

    fn boundary(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::SyntacticallyDischarged | Self::FiniteClosureDischarged => None,
            Self::Violated(_) => Some((
                "NON_ENTAILMENT_VIOLATED",
                "the selected closure derived the forbidden predicate",
            )),
            Self::Inconclusive { reason } => Some(("NON_ENTAILMENT_NOT_DISCHARGED", reason)),
            Self::Invalid { detail } => Some(("NON_ENTAILMENT_INVALID_DECLARATION", detail)),
        }
    }
}

fn premise_quad(dataset: &RdfDataset, premise: &RefutationPremise) -> Option<QuadIds> {
    if premise.graph.is_some() {
        return None;
    }
    Some(QuadIds {
        s: dataset.term_id_by_value(&premise.subject)?,
        p: dataset.term_id_by_iri(&premise.predicate)?,
        o: dataset.term_id_by_value(&premise.object)?,
        g: None,
    })
}

fn resource_identity(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        TermValue::Blank { label, scope } => Ok(format!("_:{}", scope.qualify_label(label))),
        _ => Err(reason_err(
            "native governance subject must be a resource".into(),
        )),
    }
}

struct SourceField {
    obligation: String,
    field: ObligationField,
}

fn source_fields(
    dataset: &RdfDataset,
    result: &ReasoningResult,
) -> gmeow_errors::Result<HashMap<QuadIds, SourceField, FixedState>> {
    let mut fields = HashMap::with_hasher(FixedState::new());
    for ledger in &result.native_execution()?.families {
        for proof in &ledger.proofs {
            let NativeProofOrigin::Asserted { sources, .. } = &proof.origin else {
                continue;
            };
            let Some(field) = obligation_field(&proof.statement.predicate, &proof.statement.object)
            else {
                continue;
            };
            for premise in sources {
                if obligation_field(&premise.predicate, &premise.object) != Some(field) {
                    continue;
                }
                let Some(quad) = premise_quad(dataset, premise) else {
                    continue;
                };
                let obligation = resource_identity(&premise.subject)?;
                let recorded = fields.entry(quad).or_insert_with(|| SourceField {
                    obligation: obligation.clone(),
                    field,
                });
                if recorded.obligation != obligation || recorded.field != field {
                    return Err(reason_err(
                        "a selected governance row has conflicting native source bindings".into(),
                    ));
                }
            }
        }
    }
    Ok(fields)
}

#[derive(Default)]
struct SourceRows {
    rows: Vec<SourceRow>,
}

struct SourceRow {
    index: usize,
    id: SourceObligationId,
    field: ObligationField,
    quad: QuadIds,
}

impl SourceRows {
    fn complete(&self) -> bool {
        [
            ObligationField::Declaration,
            ObligationField::ForbiddenPredicate,
            ObligationField::DischargeCondition,
        ]
        .iter()
        .all(|field| self.rows.iter().any(|row| row.field == *field))
    }

    fn account(self, dispositions: &mut [Disposition]) -> Vec<SourceObligationId> {
        self.rows
            .into_iter()
            .map(|row| {
                dispositions[row.index] = Disposition::NonEntailmentDeclaration;
                row.id
            })
            .collect()
    }
}

fn selected_binding(view: &SelectedSourceView<'_>, binding: SourceStatementBinding) -> bool {
    let quad = binding.canonical;
    match binding.carrier {
        SourceCarrier::Statement => view
            .quads_for_pattern(
                Some(quad.s),
                Some(quad.p),
                Some(quad.o),
                GraphMatch::Default,
            )
            .any(|selected| selected == quad),
        SourceCarrier::Annotation => view
            .annotations_of_with_graph(quad.s)
            .any(|selected| selected == (quad.p, quad.o, quad.g)),
        SourceCarrier::Reifier => view
            .reifier_quads_of(quad.s)
            .any(|selected| selected == quad),
    }
}

fn selected_rows(
    admission: &NativeSourceAdmission<'_>,
    result: &ReasoningResult,
) -> gmeow_errors::Result<BTreeMap<String, SourceRows>> {
    let dataset = admission.inventory.theory().source().dataset();
    let fields = source_fields(dataset, result)?;
    let view = admission.selection.source_view(admission.inventory)?;
    let mut rows = BTreeMap::<String, SourceRows>::new();
    for (index, (unit, disposition)) in admission.source_dispositions().enumerate() {
        let SourceObligationOrigin::Statement(binding) = unit.origin() else {
            continue;
        };
        if binding.canonical.g.is_some() {
            continue;
        }
        let Some(field) = fields.get(&binding.canonical) else {
            continue;
        };
        if !matches!(
            disposition,
            Disposition::Blocked {
                code: "UNIT_NOT_ACCOUNTED",
                ..
            }
        ) || !matches!(
            admission.selection.unit_selection(unit)?,
            TheoryUnitSelection::Included
        ) || !selected_binding(&view, binding)
        {
            continue;
        }
        rows.entry(field.obligation.clone())
            .or_default()
            .rows
            .push(SourceRow {
                index,
                id: unit.identity(),
                field: field.field,
                quad: binding.canonical,
            });
    }
    Ok(rows)
}

fn selected_disposition(
    disposition: ObligationDisposition,
    source_complete: bool,
    declaration_error: &mut Option<String>,
) -> gmeow_errors::Result<SelectedNonEntailmentDisposition> {
    Ok(match disposition {
        ObligationDisposition::Violated(witness) => {
            SelectedNonEntailmentDisposition::Violated(witness)
        }
        ObligationDisposition::InvalidDischarge => SelectedNonEntailmentDisposition::Invalid {
            detail: declaration_error.take().ok_or_else(|| {
                reason_err("invalid discharge omitted its declaration diagnostic".into())
            })?,
        },
        ObligationDisposition::SyntacticallyDischarged if source_complete => {
            SelectedNonEntailmentDisposition::SyntacticallyDischarged
        }
        ObligationDisposition::FiniteClosureDischarged if source_complete => {
            SelectedNonEntailmentDisposition::FiniteClosureDischarged
        }
        ObligationDisposition::SyntacticallyDischarged
        | ObligationDisposition::FiniteClosureDischarged => {
            SelectedNonEntailmentDisposition::Inconclusive {
                reason: "the selected source account retains unread premises or incomplete declaration bindings",
            }
        }
        ObligationDisposition::Unknown => SelectedNonEntailmentDisposition::Inconclusive {
            reason: "the selected native execution could not prove the declared non-entailment",
        },
    })
}

pub(super) fn account_non_entailment(
    admission: &mut NativeSourceAdmission<'_>,
    result: &ReasoningResult,
) -> gmeow_errors::Result<()> {
    let mut rows = selected_rows(admission, result)?;
    if rows.is_empty() {
        return Ok(());
    }
    let dataset = admission.inventory.theory().source().dataset();
    let selected_quads: HashSet<_, FixedState> = rows
        .values()
        .flat_map(|source| source.rows.iter().map(|row| row.quad))
        .collect();
    let (mut assessment, invalid) = assess_selected_obligations(result, &|premise| {
        premise_quad(dataset, premise).is_some_and(|quad| selected_quads.contains(&quad))
    })?;
    let mut obligations = Vec::new();
    let mut assessed = Vec::new();
    for outcome in assessment.outcomes {
        let source_rows = rows.remove(&outcome.obligation).unwrap_or_default();
        let bound = source_rows.complete();
        let sources = source_rows.account(&mut admission.dispositions);
        assessed.push((outcome, bound, sources));
    }
    for malformed in invalid {
        let sources = rows
            .remove(&malformed.obligation)
            .unwrap_or_default()
            .account(&mut admission.dispositions);
        let error = malformed.diagnostic.to_string();
        let (disposition, declaration_error) = match malformed.witness {
            Some(witness) => (
                SelectedNonEntailmentDisposition::Violated(witness),
                Some(error),
            ),
            None => (
                SelectedNonEntailmentDisposition::Invalid { detail: error },
                None,
            ),
        };
        obligations.push(SelectedNonEntailmentObligation {
            world: malformed.world,
            obligation: malformed.obligation,
            forbidden_predicate: malformed.forbidden_predicate,
            sources,
            disposition,
            declaration_error,
        });
    }
    let source_complete = obligations.is_empty()
        && assessment.condition_errors.is_empty()
        && assessed.iter().all(|(_, bound, _)| *bound)
        && admission.drops().next().is_none();
    for (outcome, _, sources) in assessed {
        let mut declaration_error = assessment
            .condition_errors
            .remove(&(outcome.world.clone(), outcome.obligation.clone()));
        obligations.push(SelectedNonEntailmentObligation {
            world: outcome.world,
            obligation: outcome.obligation,
            forbidden_predicate: Some(outcome.forbidden_predicate),
            sources,
            disposition: selected_disposition(
                outcome.disposition,
                source_complete,
                &mut declaration_error,
            )?,
            declaration_error,
        });
    }
    obligations.sort_by(|left, right| {
        (&left.world, &left.obligation).cmp(&(&right.world, &right.obligation))
    });
    for obligation in &obligations {
        let declaration_error = obligation
            .declaration_error
            .as_deref()
            .map(|detail| ("NON_ENTAILMENT_INVALID_DECLARATION", detail));
        for (code, detail) in obligation
            .disposition
            .boundary()
            .into_iter()
            .chain(declaration_error)
        {
            admission.execution_drops.insert(NativeSourceDrop::new(
                ENGINE_NATIVE,
                code,
                format!(
                    "{} in world {}: {detail}",
                    obligation.obligation, obligation.world
                ),
            ));
        }
    }
    admission.non_entailment = Some(SelectedNonEntailment {
        analysis: purrdf_hash::hex::encode(&assessment.analysis),
        obligations,
    });
    Ok(())
}
