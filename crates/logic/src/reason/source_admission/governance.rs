// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bind native governance outcomes to the selected source occurrences.

use std::collections::{BTreeMap, HashMap, HashSet};

use gmeow_logic_compile::frontend::{SourceCarrier, SourceStatementBinding};
use gmeow_logic_compile::source_inventory::SelectedSourceView;
use purrdf::dataset_view::GraphMatch;
use purrdf::{DatasetView, QuadIds, RdfDataset, TermRef, TermValue};
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
    /// Exact selected authored source obligations for this declaration.
    pub sources: Vec<SourceObligationId>,
    /// Selected declaration rows whose original assertion remains unread.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_boundaries: Vec<NonEntailmentSourceBoundary>,
    /// Outcome within the public proof contract.
    pub disposition: SelectedNonEntailmentDisposition,
    /// An invalid condition retained beside a supported violation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_error: Option<String>,
}

/// The original source and diagnostic for an unsupported governance field.
#[derive(Debug, Serialize)]
pub struct NonEntailmentSourceBoundary {
    /// Identity of the unread selected source row.
    pub source: SourceObligationId,
    /// Authored declaration field governed by the unread row.
    pub field: &'static str,
    /// Original native source-admission code.
    pub code: &'static str,
    /// Original native source-admission diagnostic.
    pub detail: String,
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
    Inconclusive { reason: String },
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

#[derive(Clone)]
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
            let Some(field) =
                obligation_field(&proof.statement.predicate, proof.statement.object.as_iri())
            else {
                continue;
            };
            for premise in sources {
                if obligation_field(&premise.predicate, premise.object.as_iri()) != Some(field) {
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
    boundary: Option<NonEntailmentSourceBoundary>,
}

impl SourceRows {
    fn complete(&self) -> bool {
        [
            ObligationField::Declaration,
            ObligationField::ForbiddenPredicate,
            ObligationField::DischargeCondition,
        ]
        .iter()
        .all(|field| {
            self.rows
                .iter()
                .any(|row| row.field == *field && row.boundary.is_none())
        })
    }

    fn account(
        self,
        dispositions: &mut [Disposition],
    ) -> (Vec<SourceObligationId>, Vec<NonEntailmentSourceBoundary>) {
        let mut sources = Vec::new();
        let mut boundaries = Vec::new();
        for row in self.rows {
            if let Some(boundary) = row.boundary {
                boundaries.push(boundary);
            } else {
                dispositions[row.index] = Disposition::NonEntailmentDeclaration;
            }
            sources.push(row.id);
        }
        (sources, boundaries)
    }
}

fn authored_field(dataset: &RdfDataset, quad: QuadIds) -> Option<ObligationField> {
    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
        return None;
    };
    let object_iri = match dataset.resolve(quad.o) {
        TermRef::Iri(iri) => Some(iri),
        _ => None,
    };
    obligation_field(predicate, object_iri)
}

pub(super) fn unread_field_name(dataset: &RdfDataset, quad: QuadIds) -> Option<&'static str> {
    authored_field(dataset, quad).map(ObligationField::as_str)
}

fn unread_field(dataset: &RdfDataset, quad: QuadIds) -> Option<SourceField> {
    let field = authored_field(dataset, quad)?;
    let obligation = match dataset.resolve(quad.s) {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, scope } => format!("_:{}", scope.qualify_label(label)),
        _ => return None,
    };
    Some(SourceField { obligation, field })
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
        let (field, unread) = match disposition {
            Disposition::Blocked {
                code: "UNIT_NOT_ACCOUNTED",
                ..
            } => {
                let Some(field) = fields.get(&binding.canonical) else {
                    continue;
                };
                (field.clone(), None)
            }
            Disposition::Blocked {
                code: code @ "UNSUPPORTED_SELECTED_ASSERTION",
                reason,
                non_entailment_field: Some(_),
            } => {
                let Some(field) = unread_field(dataset, binding.canonical) else {
                    continue;
                };
                (field, Some((*code, reason)))
            }
            _ => continue,
        };
        if !matches!(
            admission.selection.unit_selection(unit)?,
            TheoryUnitSelection::Included
        ) || !selected_binding(&view, binding)
        {
            continue;
        }
        let id = unit.identity();
        let boundary = unread.map(|(code, detail)| NonEntailmentSourceBoundary {
            source: id.clone(),
            field: field.field.as_str(),
            code,
            detail: detail.clone(),
        });
        rows.entry(field.obligation)
            .or_default()
            .rows
            .push(SourceRow {
                index,
                id,
                field: field.field,
                quad: binding.canonical,
                boundary,
            });
    }
    Ok(rows)
}

fn selected_disposition(
    disposition: ObligationDisposition,
    admission_gap: Option<&str>,
    declaration_error: &mut Option<String>,
    assessment_reason: Option<String>,
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
        ObligationDisposition::SyntacticallyDischarged if admission_gap.is_none() => {
            SelectedNonEntailmentDisposition::SyntacticallyDischarged
        }
        ObligationDisposition::FiniteClosureDischarged if admission_gap.is_none() => {
            SelectedNonEntailmentDisposition::FiniteClosureDischarged
        }
        ObligationDisposition::SyntacticallyDischarged
        | ObligationDisposition::FiniteClosureDischarged => {
            SelectedNonEntailmentDisposition::Inconclusive {
                reason: admission_gap
                    .ok_or_else(|| {
                        reason_err("withheld discharge omitted its source boundary".into())
                    })?
                    .to_owned(),
            }
        }
        ObligationDisposition::Unknown => SelectedNonEntailmentDisposition::Inconclusive {
            reason: assessment_reason.ok_or_else(|| {
                reason_err("unresolved discharge omitted its assessment diagnostic".into())
            })?,
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
        .flat_map(|source| {
            source
                .rows
                .iter()
                .filter(|row| row.boundary.is_none())
                .map(|row| row.quad)
        })
        .collect();
    let (mut assessment, invalid) = assess_selected_obligations(result, &|premise| {
        premise_quad(dataset, premise).is_some_and(|quad| selected_quads.contains(&quad))
    })?;
    let mut obligations = Vec::new();
    let mut assessed = Vec::new();
    for outcome in assessment.outcomes {
        let source_rows = rows.remove(&outcome.obligation).unwrap_or_default();
        let bound = source_rows.complete();
        let (sources, boundaries) = source_rows.account(&mut admission.dispositions);
        assessed.push((outcome, bound, sources, boundaries));
    }
    for malformed in invalid {
        let (sources, source_boundaries) = rows
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
            source_boundaries,
            disposition,
            declaration_error,
        });
    }
    let admission_gap = if let Some(drop) = admission.drops().next() {
        Some(format!(
            "the selected theory has incomplete coverage ({}): {}",
            drop.code, drop.detail
        ))
    } else if assessed.iter().any(|(_, bound, _, _)| !bound) {
        Some("the selected theory has incomplete non-entailment declaration bindings".to_owned())
    } else if !obligations.is_empty() || !assessment.condition_errors.is_empty() {
        Some(
            "the selected theory has malformed or unsupported non-entailment declarations"
                .to_owned(),
        )
    } else {
        None
    };
    for (outcome, _, sources, source_boundaries) in assessed {
        let key = (outcome.world.clone(), outcome.obligation.clone());
        let mut declaration_error = assessment.condition_errors.remove(&key);
        let assessment_reason = assessment.absence_reasons.remove(&key);
        obligations.push(SelectedNonEntailmentObligation {
            world: outcome.world,
            obligation: outcome.obligation,
            forbidden_predicate: Some(outcome.forbidden_predicate),
            sources,
            source_boundaries,
            disposition: selected_disposition(
                outcome.disposition,
                admission_gap.as_deref(),
                &mut declaration_error,
                assessment_reason,
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
