// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source accounting and live completeness for the selected native executor.
//!
//! An inventory row is not execution evidence. Only the executor in this module
//! can attach a completion to these immutable source coordinates; serialized rows
//! are observations and have no constructor that can restore that authority.

mod accounting;

use std::collections::BTreeSet;

use gmeow_logic_compile::frontend::{
    AxiomSource, FormulaDisposition, OwnerDisposition, OwnerFamily, SemanticUnitDisposition,
    Severity, SourceNode,
};
use gmeow_logic_compile::source_inventory::{
    ResolvedTheorySelection, SourceInventory, SourceObligation, SourceObligationOrigin,
    TheoryUnitSelection,
};
use serde::Serialize;
use serde::ser::{SerializeSeq, SerializeStruct};

use super::{ClaimIngress, PreparedReasoningInput, SelectedProgram, reason_err};
use crate::physical::{LogicalGraph, SelectedDomains};
use crate::query_ir::Budget;
use crate::result::{EvaluationStatus, InformationState, ReasoningResult};
use accounting::{AtomPolarity, UnitTerm, account_axiom, account_formula, account_predication};

const ENGINE_NATIVE: &str = "native";
const PROFILE: &str = "gmeow-native-source-accounting-v1";
const NAF_BOUNDARY: &str = "NEGATION_AS_FAILURE_IS_NOT_CLASSICAL_NEGATION";
const SOURCE_ACCOUNTING_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-native-source-accounting-v1\0");
const SOURCE_EXECUTION_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-live-native-source-execution-v1\0");

/// One named source or execution boundary. It never grants proof authority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NativeSourceDrop {
    pub engine: &'static str,
    pub code: String,
    pub detail: String,
}

impl NativeSourceDrop {
    /// Construct an observation of a boundary without granting proof authority.
    #[must_use]
    pub fn new(engine: &'static str, code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            engine,
            code: code.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Serialize)]
/// Borrowable report disposition; observations grant no execution authority.
pub enum NativeSourceDisposition {
    Accounted { route: &'static str },
    StructuralSupport { route: &'static str },
    ConfigurationDeclaration { route: &'static str },
    MetatheoryDeclaration { route: &'static str },
    OutsideSelection { reason: &'static str },
    OutsideRequestedTheory { reason: &'static str },
    Blocked { code: &'static str, reason: String },
    Malformed { code: &'static str, reason: String },
}

use NativeSourceDisposition as Disposition;

impl Disposition {
    fn blocked(code: &'static str, reason: impl Into<String>) -> Self {
        Self::Blocked {
            code,
            reason: reason.into(),
        }
    }
    fn malformed(code: &'static str, reason: impl Into<String>) -> Self {
        Self::Malformed {
            code,
            reason: reason.into(),
        }
    }
    fn accounted(route: &'static str) -> Self {
        Self::Accounted { route }
    }
    fn support(route: &'static str) -> Self {
        Self::StructuralSupport { route }
    }
    fn boundary(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::Blocked { code, reason } | Self::Malformed { code, reason } => {
                Some((code, reason))
            }
            Self::OutsideSelection { reason } => Some(("OUTSIDE_SELECTION", reason)),
            _ => None,
        }
    }
}

/// Compact source observation sharing the compiler's single occurrence inventory.
/// The live completion is private, has no decoder and is issued only while the
/// selected executor holds its actual input, preparation, domains and result.
pub struct NativeSourceAdmission<'inventory> {
    inventory: &'inventory SourceInventory<'inventory>,
    selection: ResolvedTheorySelection,
    dispositions: Vec<Disposition>,
    execution_drops: BTreeSet<NativeSourceDrop>,
    completion: Option<LiveCompletion>,
}

struct LiveCompletion {
    result: std::sync::Weak<ReasoningResult>,
    execution: [u8; 32],
    ingress_contract: [u8; 32],
    input_contract: [u8; 32],
    preparation: [u8; 32],
    accounting: [u8; 32],
    complete: bool,
}

impl<'inventory> NativeSourceAdmission<'inventory> {
    /// Inspect the complete supplied source without granting execution authority.
    /// This operation supports the entire supplied compilation and top-level theory;
    /// narrower document/root projections must use a separately implemented executor.
    ///
    /// # Errors
    /// Refuses a document/root selection the actual native executor does not filter.
    pub fn inspect(
        inventory: &'inventory SourceInventory<'inventory>,
    ) -> gmeow_errors::Result<Self> {
        let selection = inventory
            .resolve_theory_selection(&gmeow_logic_compile::ir::TheorySelection::top_level_only())?;
        Self::inspect_selected(inventory, &selection)
    }

    /// Inspect one compiler-resolved composition against its original inventory.
    ///
    /// # Errors
    /// Refuses a foreign selection or an unimplemented document/root projection.
    pub fn inspect_selected(
        inventory: &'inventory SourceInventory<'inventory>,
        selection: &ResolvedTheorySelection,
    ) -> gmeow_errors::Result<Self> {
        selection.authorize_inventory(inventory)?;
        if *inventory.selection()
            != gmeow_logic_compile::frontend::SourceSelection::all_classical_roots(
                inventory.theory(),
            )
        {
            return Err(reason_err("native source accounting requires all supplied documents and roots; the selected executor does not implement source filtering".into()));
        }
        let dispositions = inventory
            .obligations()
            .map(|unit| classify_selected(inventory, unit, None, selection))
            .collect::<gmeow_errors::Result<_>>()?;
        Ok(Self {
            inventory,
            selection: selection.clone(),
            dispositions,
            execution_drops: BTreeSet::new(),
            completion: None,
        })
    }

    /// Validate completeness against the exact live result issued by this executor.
    /// A decoded result or an edited clone cannot inherit another run's authority.
    ///
    /// # Errors
    /// Rejects absent live completion, a foreign result, invalid native evidence or
    /// any changed outcome field before exposing the completeness decision.
    pub fn complete_for(&self, result: &ReasoningResult) -> gmeow_errors::Result<bool> {
        let completion = self.completion.as_ref().ok_or_else(|| {
            reason_err("native source account has no live executor-issued completion".into())
        })?;
        let actual = completion.result.upgrade().ok_or_else(|| {
            reason_err("the actual native execution result is no longer live".into())
        })?;
        if !std::ptr::eq(actual.as_ref(), result) {
            return Err(reason_err("native source completion belongs to a different live result; decoded observations cannot supply execution authority".into()));
        }
        if completion.accounting != self.accounting_digest() {
            return Err(reason_err(
                "native completion does not bind the current source-obligation account".into(),
            ));
        }
        let verdict = result.native_verdict()?;
        if completion.execution != execution_digest(result, &verdict) {
            return Err(reason_err(
                "native source completion does not bind the current result and execution evidence"
                    .into(),
            ));
        }
        Ok(completion.complete)
    }

    fn observed_complete(&self) -> bool {
        self.completion
            .as_ref()
            .is_some_and(|completion| completion.complete)
    }

    #[must_use]
    pub fn malformed(&self) -> bool {
        self.dispositions
            .iter()
            .any(|row| matches!(row, Disposition::Malformed { .. }))
    }

    /// Named source and execution boundaries, rendered from borrowed coordinates.
    pub fn drops(&self) -> impl Iterator<Item = NativeSourceDrop> + '_ {
        self.inventory
            .obligations()
            .zip(&self.dispositions)
            .filter_map(|(unit, disposition)| {
                disposition.boundary().map(|(code, reason)| {
                    NativeSourceDrop::new(
                        ENGINE_NATIVE,
                        code,
                        format!("{}: {reason}", unit.anchor()),
                    )
                })
            })
            .chain(self.execution_drops.iter().cloned())
    }

    #[must_use]
    pub fn units(&self) -> usize {
        self.dispositions.len()
    }

    /// Stream the original source obligations and their current report judgments.
    pub fn source_dispositions(
        &self,
    ) -> impl ExactSizeIterator<Item = (SourceObligation<'_>, &NativeSourceDisposition)> {
        self.inventory.obligations().zip(&self.dispositions)
    }

    #[must_use]
    pub fn resolved_selection(&self) -> &ResolvedTheorySelection {
        &self.selection
    }

    fn accounting_digest(&self) -> [u8; 32] {
        let mut hasher = purrdf_hash::blake3::Hasher::new();
        hasher.update(SOURCE_ACCOUNTING_DOMAIN.as_bytes());
        serde_json::to_writer(&mut hasher, &Rows(self))
            .expect("native source rows serialize to an infallible digest sink");
        *hasher.finalize().as_bytes()
    }
}

struct Rows<'a, 'source>(&'a NativeSourceAdmission<'source>);

impl Serialize for Rows<'_, '_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.dispositions.len()))?;
        for (unit, disposition) in self.0.inventory.obligations().zip(&self.0.dispositions) {
            #[derive(Serialize)]
            struct Row<'a> {
                id: gmeow_logic_compile::source_inventory::SourceObligationId,
                kind: gmeow_logic_compile::frontend::SemanticUnitKind,
                anchor: String,
                documents: BTreeSet<String>,
                disposition: &'a Disposition,
                #[serde(skip_serializing_if = "Option::is_none")]
                configuration: Option<&'a SemanticUnitDisposition>,
            }
            sequence.serialize_element(&Row {
                id: unit.identity(),
                kind: unit.kind(),
                anchor: unit.anchor(),
                documents: unit.documents(),
                disposition,
                configuration: if matches!(
                    disposition,
                    Disposition::ConfigurationDeclaration { .. }
                ) {
                    unit.configuration_declaration()
                } else {
                    None
                },
            })?;
        }
        sequence.end()
    }
}

impl Serialize for NativeSourceAdmission<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("NativeSourceAdmission", 15)?;
        record.serialize_field("profile", PROFILE)?;
        record.serialize_field("basis", "compiler source obligations and exact lowering routes; authored occurrences are retained by their document receipts")?;
        record.serialize_field("source_digest", self.inventory.source_digest())?;
        record.serialize_field("program_digest", self.inventory.program_digest())?;
        record.serialize_field("selection_digest", self.inventory.selection_digest())?;
        record.serialize_field("selection", self.inventory.selection())?;
        record.serialize_field("theory_selection", self.selection.observation())?;
        record.serialize_field("complete", &self.observed_complete())?;
        record.serialize_field("units", &Rows(self))?;
        record.serialize_field("execution_drops", &self.execution_drops)?;
        record.serialize_field(
            "ingress_contract",
            &self
                .completion
                .as_ref()
                .map(|completion| purrdf_hash::hex::encode(&completion.ingress_contract)),
        )?;
        record.serialize_field(
            "input_contract",
            &self
                .completion
                .as_ref()
                .map(|completion| purrdf_hash::hex::encode(&completion.input_contract)),
        )?;
        record.serialize_field(
            "preparation",
            &self
                .completion
                .as_ref()
                .map(|completion| purrdf_hash::hex::encode(&completion.preparation)),
        )?;
        record.serialize_field(
            "accounting_digest",
            &self
                .completion
                .as_ref()
                .map(|completion| purrdf_hash::hex::encode(&completion.accounting)),
        )?;
        record.serialize_field(
            "execution_digest",
            &self
                .completion
                .as_ref()
                .map(|completion| purrdf_hash::hex::encode(&completion.execution)),
        )?;
        record.end()
    }
}

fn execution_digest(result: &ReasoningResult, verdict: &super::DlVerdict) -> [u8; 32] {
    #[derive(Serialize)]
    struct ExecutionV1<'a> {
        schema_version: u32,
        result: &'a ReasoningResult,
        validated_native_verdict: &'a super::DlVerdict,
    }
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    hasher.update(SOURCE_EXECUTION_DOMAIN.as_bytes());
    serde_json::to_writer(
        &mut hasher,
        &ExecutionV1 {
            schema_version: 1,
            result,
            validated_native_verdict: verdict,
        },
    )
    .expect("typed native execution serializes to an infallible digest sink");
    *hasher.finalize().as_bytes()
}

fn account_refusals(
    refusals: impl IntoIterator<Item = String>,
    route: &'static str,
) -> Disposition {
    let refusals: Vec<_> = refusals.into_iter().collect();
    if refusals.is_empty() {
        Disposition::accounted(route)
    } else {
        Disposition::blocked("UNIT_NOT_ACCOUNTED", refusals.join("; "))
    }
}

fn classify(
    inventory: &SourceInventory<'_>,
    unit: SourceObligation<'_>,
    ingress: Option<&ClaimIngress>,
    contexts_admitted: bool,
    axiom_sources: &[AxiomSource],
    correspondence_record_selected: bool,
) -> Disposition {
    let theory = inventory.theory();
    let outside = || Disposition::OutsideSelection {
        reason: "source placement is outside the selected default graph",
    };
    match unit.origin() {
        SourceObligationOrigin::Statement(binding) => {
            let quad = binding.canonical;
            if quad.g.is_some() {
                return outside();
            }
            if correspondence_record_selected {
                return Disposition::MetatheoryDeclaration {
                    route: "validated correspondence catalogue record; no object premise or law execution",
                };
            }
            if unit.requires_class_law() {
                return Disposition::blocked(
                    "CLASS_DEFINITION_NOT_ACCOUNTED",
                    "class-constructor syntax requires a complete native source-law account",
                );
            }
            if unit.collection_roles().next().is_some() {
                return Disposition::blocked(
                    "COLLECTION_LAW_NOT_ACCOUNTED",
                    "collection constructors require a complete native source-law account",
                );
            }
            if unit.is_structural_statement() {
                return Disposition::support("source grammar accounted by its inventoried owner");
            }
            match unit
                .assertion()
                .expect("statement obligation has native statement terms")
            {
                Ok(assertion) => account_refusals(
                    account_predication(
                        UnitTerm::atomic(&assertion.subject),
                        UnitTerm::Iri(&assertion.predicate),
                        UnitTerm::atomic(&assertion.object),
                        AtomPolarity::Positive,
                    ),
                    "native asserted fact and interpreted-relation account",
                ),
                Err(error) => {
                    Disposition::blocked("UNSUPPORTED_SELECTED_ASSERTION", error.to_string())
                }
            }
        }
        SourceObligationOrigin::Quotation { reifier, graph, .. } => {
            if graph.is_some() {
                outside()
            } else if let Some(claim) = ingress.and_then(|ingress| {
                ingress.unread_carrier(SourceNode {
                    term: reifier,
                    graph,
                })
            }) {
                Disposition::blocked(
                    "SCOPED_CLAIM_NOT_READ",
                    format!("{}: {}", claim.source_detail(), claim.reason),
                )
            } else {
                Disposition::support(
                    "quotation does not assert its payload; any claim is separately bound to ingress",
                )
            }
        }
        SourceObligationOrigin::ClassDefinition(node) => {
            if node.graph.is_some() {
                outside()
            } else {
                Disposition::blocked(
                    "CLASS_DEFINITION_NOT_ACCOUNTED",
                    "a class-expression law has no complete model account in this native profile",
                )
            }
        }
        SourceObligationOrigin::Owner(ordinal) => {
            let owner = &theory.owner_lowerings()[ordinal];
            if owner.source.graph.is_some() {
                return outside();
            }
            if owner
                .diagnostics
                .iter()
                .any(|&index| theory.diagnostics()[index].severity == Severity::Error)
            {
                return Disposition::malformed(
                    "MALFORMED_SELECTED_OWNER",
                    format!(
                        "owner compiler errors at diagnostics {:?}",
                        owner.diagnostics
                    ),
                );
            }
            match &owner.disposition {
                OwnerDisposition::OutsideDefaultGraph => outside(),
                OwnerDisposition::Rejected => {
                    if owner.diagnostics.is_empty() {
                        Disposition::malformed(
                            "MALFORMED_SELECTED_OWNER",
                            "rejected owner has no actionable compiler diagnostic",
                        )
                    } else {
                        Disposition::blocked(
                            "SKIPPED_SELECTED_OWNER",
                            format!(
                                "compiler skipped owner at diagnostics {:?}",
                                owner.diagnostics
                            ),
                        )
                    }
                }
                OwnerDisposition::Emitted { index } if owner.family == OwnerFamily::Rule => {
                    match theory.program().rules.get(*index) {
                        Some(rule) if rule.body.iter().any(|atom| atom.negated) => {
                            Disposition::blocked(
                                NAF_BOUNDARY,
                                format!(
                                    "program:rule:{index} uses negation-as-failure body atoms; absence is not classical negation"
                                ),
                            )
                        }
                        Some(rule) => account_refusals(
                            std::iter::once((&rule.head, AtomPolarity::Positive))
                                .chain(rule.body.iter().map(|atom| (atom, AtomPolarity::Negative)))
                                .filter_map(|(atom, polarity)| account_axiom(atom, polarity)),
                            "selected native rule; exact preparation and lowering residue checked by executor",
                        ),
                        None => Disposition::malformed(
                            "MISSING_RULE_IR",
                            "owner route is outside the canonical rule program",
                        ),
                    }
                }
                OwnerDisposition::Emitted { .. } if owner.family == OwnerFamily::Contract => {
                    match unit.configuration_declaration() {
                        Some(SemanticUnitDisposition::ConfigurationDeclaration { .. }) => {
                            Disposition::ConfigurationDeclaration {
                                route: "metatheory catalogue; no implicit contract execution",
                            }
                        }
                        Some(SemanticUnitDisposition::Malformed { code, reason }) => {
                            Disposition::malformed(
                                "MALFORMED_CONTRACT_DECLARATION",
                                format!("{code}: {reason}"),
                            )
                        }
                        Some(SemanticUnitDisposition::Blocked { code, reason }) => {
                            Disposition::blocked(
                                "UNSUPPORTED_CONTRACT_DECLARATION",
                                format!("{code}: {reason}"),
                            )
                        }
                        _ => Disposition::malformed(
                            "MISSING_CONTRACT_IR",
                            "contract route has no shared declaration account",
                        ),
                    }
                }
                OwnerDisposition::Emitted { index }
                    if owner.family == OwnerFamily::Correspondence =>
                {
                    if theory.program().correspondences.get(*index).is_none()
                        || owner.record_support.is_empty()
                    {
                        Disposition::malformed(
                            "MISSING_CORRESPONDENCE_RECORD",
                            "correspondence route has no compiled record and exact reader support",
                        )
                    } else {
                        Disposition::MetatheoryDeclaration {
                            route: "validated correspondence catalogue record; no object premise or law execution",
                        }
                    }
                }
                OwnerDisposition::Emitted { .. } => Disposition::blocked(
                    "UNSUPPORTED_SELECTED_OWNER",
                    format!(
                        "no selected native execution contract for {:?}",
                        owner.family
                    ),
                ),
            }
        }
        SourceObligationOrigin::MissingRule(_) => Disposition::malformed(
            "RULE_WITHOUT_SOURCE_TRACE",
            "compiled rule has no exact source-owner route",
        ),
        SourceObligationOrigin::Formula(ordinal) => {
            let lowering = &theory.formula_lowerings()[ordinal];
            if lowering.source.graph.is_some() {
                return outside();
            }
            if inventory
                .untranslated_formula_contexts()
                .contains(&lowering.source)
                && !contexts_admitted
                && !matches!(lowering.disposition, FormulaDisposition::ReadForOwner)
            {
                return Disposition::blocked(
                    "CONTEXT_ERASURE_FORBIDDEN",
                    "formula or descendant scope is not represented by the selected top-level execution",
                );
            }
            match lowering.disposition {
                FormulaDisposition::OutsideDefaultGraph => outside(),
                FormulaDisposition::ReadForOwner => Disposition::support(
                    "formula syntax retained by its separately inventoried owner",
                ),
                FormulaDisposition::Malformed { diagnostic } => Disposition::malformed(
                    "MALFORMED_SELECTED_FORMULA",
                    format!("compiler rejected formula at diagnostic {diagnostic}"),
                ),
                FormulaDisposition::Axiom(index) => {
                    classify_axiom(theory, index, ingress, axiom_sources)
                }
                FormulaDisposition::Formula(index) => {
                    let Some(formula) = theory.program().formulas.get(index) else {
                        return Disposition::malformed(
                            "MISSING_FORMULA_IR",
                            "formula route is outside the canonical program",
                        );
                    };
                    let mut refusals = Vec::new();
                    account_formula(formula, AtomPolarity::Positive, &mut refusals);
                    account_refusals(
                        refusals,
                        "selected native formula; exact preparation and lowering residue checked by executor",
                    )
                }
            }
        }
        SourceObligationOrigin::Axiom(index) => {
            classify_axiom(theory, index, ingress, axiom_sources)
        }
        SourceObligationOrigin::Import(index) => {
            let edge = &theory.source().source_graph().edges()[index];
            if edge.source.graph.is_some() {
                return outside();
            }
            let target = SourceNode {
                term: edge.target,
                graph: edge.source.graph,
            };
            if inventory.node_documents(target).is_empty() {
                Disposition::blocked(
                    "UNADMITTED_IMPORT",
                    "selected import has no target in the supplied document receipts",
                )
            } else {
                Disposition::support(
                    "import target present in exact supplied source; no implicit module composition",
                )
            }
        }
        SourceObligationOrigin::PropertyCharacteristic(node) => {
            if node.graph.is_some() {
                return outside();
            }
            match unit
                .property_characteristic()
                .expect("characteristic obligation has a compiler record reader")
            {
                Ok(record)
                    if accounting::ACCOUNTED_CHARACTERISTIC_RECORDS
                        .iter()
                        .any(|(sort, _)| *sort == record.characteristic) =>
                {
                    Disposition::accounted("native functional characteristic record")
                }
                Ok(record) => Disposition::blocked(
                    "CHARACTERISTIC_RECORD_NOT_READ",
                    format!(
                        "native engine does not read a {} record for {}",
                        record.characteristic, record.property
                    ),
                ),
                Err(error) => {
                    Disposition::malformed("MALFORMED_PROPERTY_CHARACTERISTIC", error.to_string())
                }
            }
        }
        SourceObligationOrigin::Diagnostic(index) => {
            let diagnostic = &theory.diagnostics()[index];
            let detail = format!(
                "{} [{}] {}{}",
                diagnostic.severity.as_str(),
                diagnostic.code,
                diagnostic.message,
                diagnostic
                    .subject
                    .as_ref()
                    .map_or_else(String::new, |subject| format!(" (subject {subject})"))
            );
            if diagnostic.severity == Severity::Error {
                Disposition::malformed("COMPILER_ERROR_IN_SELECTED_THEORY", detail)
            } else {
                Disposition::blocked("COMPILER_OBSERVATION", detail)
            }
        }
        SourceObligationOrigin::SelectionIssue(index) => {
            let issue = &inventory.selection_issues()[index];
            Disposition::malformed(issue.code, issue.reason.clone())
        }
    }
}

fn classify_selected(
    inventory: &SourceInventory<'_>,
    unit: SourceObligation<'_>,
    ingress: Option<&ClaimIngress>,
    selection: &ResolvedTheorySelection,
) -> gmeow_errors::Result<Disposition> {
    let membership = selection.unit_selection(unit)?;
    if matches!(membership, TheoryUnitSelection::OutsideRequestedTheory) {
        return Ok(Disposition::OutsideRequestedTheory {
            reason: "original source obligation belongs exclusively to modules outside the requested theory",
        });
    }
    let axiom_index = match unit.origin() {
        SourceObligationOrigin::Axiom(index) => Some(index),
        SourceObligationOrigin::Formula(ordinal) => {
            match inventory.theory().formula_lowerings()[ordinal].disposition {
                FormulaDisposition::Axiom(index) => Some(index),
                _ => None,
            }
        }
        _ => None,
    };
    let view = selection.program_view(inventory)?;
    let axiom_sources = axiom_index
        .map(|index| {
            view.axiom_sources_at(index)
                .map(|sources| sources.copied().collect::<Vec<_>>())
        })
        .transpose()?
        .unwrap_or_default();
    let disposition = classify(
        inventory,
        unit,
        ingress,
        matches!(membership, TheoryUnitSelection::Included),
        &axiom_sources,
        selection.correspondence_record_selected(unit)?,
    );
    Ok(match membership {
        TheoryUnitSelection::Blocked { code, reason } if disposition.boundary().is_none() => {
            Disposition::blocked(code, reason)
        }
        _ => disposition,
    })
}

fn classify_axiom(
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
    index: usize,
    ingress: Option<&ClaimIngress>,
    sources: &[AxiomSource],
) -> Disposition {
    let Some(axiom) = theory.program().axioms.get(index) else {
        return Disposition::malformed(
            "MISSING_AXIOM_IR",
            "source route is outside the canonical axiom program",
        );
    };
    if sources.iter().any(|source| {
        matches!(
            source,
            AxiomSource::Collection { .. } | AxiomSource::RewrittenField { .. }
        )
    }) {
        let class_definition = sources.iter().any(|source| match source {
            AxiomSource::Collection { source, .. } => {
                source.role.owner == gmeow_logic_compile::frontend::SourceOwnerRole::ClassDefinition
            }
            AxiomSource::RewrittenField { owner, .. } => {
                *owner == gmeow_logic_compile::frontend::SourceOwnerRole::ClassDefinition
            }
            _ => false,
        });
        return Disposition::blocked(
            if class_definition {
                "CLASS_DEFINITION_NOT_ACCOUNTED"
            } else {
                "SCHEMA_LOWERING_NOT_ACCOUNTED"
            },
            "rewritten schema and collection syntax requires an explicit native law account",
        );
    }
    if sources
        .iter()
        .any(|source| matches!(source, AxiomSource::Formula(_)))
        && !sources.iter().any(|source| {
            matches!(
                source,
                AxiomSource::Statement { .. } | AxiomSource::ClassExpression(_)
            )
        })
    {
        return Disposition::blocked(
            "FORMULA_AXIOM_NOT_READ",
            "atomic formula is not an asserted source statement the native input reads",
        );
    }
    if sources
        .iter()
        .any(|source| matches!(source, AxiomSource::Reification(_)))
    {
        if !ingress.is_some_and(|ingress| {
            ingress
                .read()
                .binary_search_by_key(&index, |claim| claim.axiom)
                .is_ok()
        }) {
            return Disposition::blocked(
                "SCOPED_CLAIM_NOT_READ",
                ingress
                    .and_then(|ingress| {
                        ingress
                            .unread()
                            .binary_search_by_key(&index, |claim| claim.axiom)
                            .ok()
                            .map(|position| &ingress.unread()[position])
                    })
                    .map_or_else(
                        || {
                            "claim has not been authorized by the actual selected input ingress"
                                .into()
                        },
                        |claim| {
                            format!(
                                "{} {} {}: {}",
                                axiom.subject,
                                axiom.predicate,
                                axiom.obj.key(),
                                claim.reason
                            )
                        },
                    ),
            );
        }
    } else if sources
        .iter()
        .any(|source| matches!(source, AxiomSource::ClassExpression(_)))
    {
        return Disposition::blocked(
            "CLASS_DEFINITION_NOT_ACCOUNTED",
            "class-expression lowering has no complete native source-law account",
        );
    }
    if axiom.negated {
        return Disposition::blocked(
            NAF_BOUNDARY,
            "compact negation is procedural and has no model account",
        );
    }
    account_refusals(
        account_axiom(axiom, AtomPolarity::Positive),
        "exact source axiom; scoped claim authorized by actual selected ingress",
    )
}

/// Execute the selected theory and issue its source-bound completion in this scope.
/// Decoded results and matching digest strings are never accepted as authority.
///
/// # Errors
/// Refuses foreign source/input/preparation/domain bindings and procedural NAF
/// rules before execution, and propagates the native engine's typed failures.
pub fn reason_admitted_theory_within(
    admission: &mut NativeSourceAdmission<'_>,
    selected: &SelectedProgram,
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
    ingress: &ClaimIngress,
    budget: &Budget,
) -> gmeow_errors::Result<std::sync::Arc<ReasoningResult>> {
    admission.completion = None;
    admission.execution_drops.clear();
    let theory = admission.inventory.theory();
    ingress.authorize_source(theory)?;
    ingress.authorize(theory.program(), &input)?;
    input.require_default_graph_profile()?;
    admission
        .selection
        .authorize_inventory(admission.inventory)?;
    ingress
        .resolved_selection()
        .authorize_inventory(admission.inventory)?;
    selected
        .selection
        .authorize_inventory(admission.inventory)?;
    if admission.selection.observation().identity
        != ingress.resolved_selection().observation().identity
    {
        return Err(reason_err(
            "native source accounting and ingress require the same resolved theory composition"
                .into(),
        ));
    }
    let expected = crate::program_analysis::preparation_identity(
        theory.program(),
        &crate::program_analysis::selected_profile(ingress.resolved_selection()),
    )?;
    if selected.prepared.identity != expected {
        return Err(reason_err(
            "source admission does not bind this actual selected preparation".into(),
        ));
    }
    domains.validate()?;
    if domains.worlds().len() != 1
        || domains.worlds()[0].graph() != &LogicalGraph::Default
        || *domains.worlds()[0].selection() != *input.ingress_contract()
    {
        return Err(reason_err("native source profile requires exactly the caller-admitted default world bound to the actual input".into()));
    }
    for (unit, disposition) in admission
        .inventory
        .obligations()
        .zip(&mut admission.dispositions)
    {
        let reads_ingress = match unit.origin() {
            SourceObligationOrigin::Axiom(_) | SourceObligationOrigin::Quotation { .. } => true,
            SourceObligationOrigin::Formula(ordinal) => matches!(
                theory.formula_lowerings()[ordinal].disposition,
                FormulaDisposition::Axiom(_)
            ),
            _ => false,
        };
        if reads_ingress {
            *disposition = classify_selected(
                admission.inventory,
                unit,
                Some(ingress),
                &admission.selection,
            )?;
        }
    }
    if admission.malformed() {
        let detail = admission
            .inventory
            .obligations()
            .zip(&admission.dispositions)
            .filter_map(|(unit, disposition)| match disposition {
                Disposition::Malformed { code, reason } => {
                    Some(format!("{} {code}: {reason}", unit.anchor()))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(reason_err(format!(
            "selected source contains malformed obligations: {detail}"
        )));
    }
    for claim in ingress.unread() {
        admission.execution_drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "SCOPED_CLAIM_NOT_READ",
            format!(
                "{}: {} {} {}: {}",
                claim.source_detail(),
                theory.program().axioms[claim.axiom].subject,
                theory.program().axioms[claim.axiom].predicate,
                theory.program().axioms[claim.axiom].obj.key(),
                claim.reason
            ),
        ));
    }
    super::admit_selected_source_rules(
        admission.inventory,
        &admission.selection,
        super::SelectedRuleProfile::SourceProof,
    )?;
    let contract = *input.ingress_contract();
    let closure = super::program::execute(&selected.prepared, input, domains, budget.max_steps)?;
    let execution_contract = closure.input_contract;
    let result = super::result_from_closure(&selected.prepared, closure, budget.max_steps)?.0;
    let evidence = result.native_execution()?;
    if evidence.input_contract != execution_contract || &evidence.selected_domains != domains {
        return Err(reason_err(
            "native execution changed its actual input or admitted domains".into(),
        ));
    }
    let crate::obligations::ReachabilityEvidence::Program(reachability) = &evidence.reachability
    else {
        return Err(reason_err(
            "selected source execution omitted its prepared-program reachability".into(),
        ));
    };
    reachability.validate_preparation(&selected.prepared)?;
    let verdict = result.native_verdict()?;
    let drops = &mut admission.execution_drops;
    for gap in &verdict.gaps {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            gap.code.clone(),
            gap.message.clone(),
        ));
    }
    for finding in &verdict.boundary_findings {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            finding.code.clone(),
            finding.message.clone(),
        ));
    }
    for family in &verdict.coverage.unsupported {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "UNDECIDED_CONSTRUCT_FAMILY",
            family.clone(),
        ));
    }
    for construct in &result.preservation.unsupported_constructs {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "UNSUPPORTED_CONSTRUCT",
            construct.clone(),
        ));
    }
    if result.evaluation != EvaluationStatus::Completed {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "NATIVE_EVALUATION_INCOMPLETE",
            format!(
                "native closure ended {} after {} steps",
                result.evaluation.wire(),
                result.provenance.consumed_budget.consumed
            ),
        ));
    }
    for certificate in &evidence.chase_certificates {
        if let crate::materialize::ChaseAdmission::Uncertified { violations, bounds } =
            &certificate.admission
        {
            for bound in bounds {
                drops.insert(NativeSourceDrop::new(
                    ENGINE_NATIVE,
                    "NATIVE_ANALYSIS_BOUND",
                    bound.clone(),
                ));
            }
            drops.insert(NativeSourceDrop::new(
                ENGINE_NATIVE,
                "CHASE_UNCERTIFIED",
                format!("world {}: {}", certificate.world, violations.join("; ")),
            ));
        }
    }
    for assumption in &result.provenance.assumptions {
        if !matches!(
            assumption,
            crate::result::Assumption::OpenWorld | crate::result::Assumption::SkolemWitness
        ) {
            drops.insert(NativeSourceDrop::new(
                ENGINE_NATIVE,
                "NONCLASSICAL_ASSUMPTION",
                format!("native result rests on {assumption}"),
            ));
        }
    }
    drops.extend(accounting::equality_drops(result.inferred()));
    if verdict.consistent && result.information != InformationState::Supported {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "NATIVE_NOT_SUPPORTED",
            format!("native closure reports {}", result.information.wire()),
        ));
    }
    let has_theory = admission
        .dispositions
        .iter()
        .any(|disposition| matches!(disposition, Disposition::Accounted { .. }));
    if !has_theory {
        drops.insert(NativeSourceDrop::new(
            ENGINE_NATIVE,
            "EMPTY_SELECTION",
            "the supplied source contains no theory unit",
        ));
    }
    let complete = admission.drops().next().is_none();
    let execution = execution_digest(&result, &verdict);
    let result = std::sync::Arc::new(result);
    admission.completion = Some(LiveCompletion {
        result: std::sync::Arc::downgrade(&result),
        execution,
        ingress_contract: contract,
        input_contract: execution_contract,
        preparation: selected.prepared.identity,
        accounting: admission.accounting_digest(),
        complete,
    });
    Ok(result)
}
