// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed execution views issued after checking the original compilation.

use std::convert::Infallible;

use purrdf::{
    DatasetView, GraphMatch, QuadIds, RdfDataset, RdfStoreCapabilities, TermId, TermRef, TermValue,
};

use crate::frontend::{AxiomSource, FormulaLowering, SourceCarrier, SourceStatementBinding};
use crate::ir::{Formula, LogicAxiom, LogicRule};
use crate::source_inventory::{SourceInventory, SourceObligation};

use super::{ResolvedTheorySelection, index::Membership};

/// Execution membership is distinct from the profile's semantic disposition.
#[derive(Debug, Clone, Copy)]
pub enum TheoryUnitSelection<'selection> {
    Included,
    OutsideRequestedTheory,
    Blocked {
        code: &'static str,
        reason: &'selection str,
    },
}

impl<'a> From<Membership<'a>> for TheoryUnitSelection<'a> {
    fn from(membership: Membership<'a>) -> Self {
        match membership {
            Membership::Included => Self::Included,
            Membership::Outside => Self::OutsideRequestedTheory,
            Membership::Blocked(boundary) => Self::Blocked {
                code: boundary.code,
                reason: &boundary.detail,
            },
        }
    }
}

impl ResolvedTheorySelection {
    /// Borrow the filtered physical streams of this exact source publication.
    ///
    /// # Errors
    /// Refuses a foreign compilation or document/root selection.
    pub fn source_view<'a, 'source>(
        &'a self,
        inventory: &'a SourceInventory<'source>,
    ) -> gmeow_errors::Result<SelectedSourceView<'a>> {
        self.authorize_inventory(inventory)?;
        Ok(SelectedSourceView {
            dataset: inventory.theory().source().dataset(),
            selection: self,
        })
    }

    /// Borrow original program coordinates without copying or reindexing IR.
    ///
    /// # Errors
    /// Refuses a foreign compilation or document/root selection.
    pub fn program_view<'a, 'source>(
        &'a self,
        inventory: &'a SourceInventory<'source>,
    ) -> gmeow_errors::Result<SelectedProgramView<'a, 'source>> {
        self.authorize_inventory(inventory)?;
        Ok(SelectedProgramView {
            inventory,
            selection: self,
        })
    }

    /// Account for one obligation from its actual immutable inventory.
    ///
    /// # Errors
    /// Refuses obligations borrowed from another compilation or selection.
    pub fn unit_selection(
        &self,
        unit: SourceObligation<'_>,
    ) -> gmeow_errors::Result<TheoryUnitSelection<'_>> {
        self.authorize_inventory(unit.inventory)?;
        let membership = self
            .0
            .index
            .obligation_membership(unit.inventory, unit.origin());
        // Top-level mode has no authority to exclude authored obligations.
        if !self.request().is_composed() && matches!(membership, Membership::Outside) {
            return Ok(TheoryUnitSelection::Blocked {
                code: "CONTEXT_ERASURE_FORBIDDEN",
                reason: "source scope remains untranslated in the top-level theory",
            });
        }
        Ok(membership.into())
    }

    /// Whether an actually selected successful correspondence accounts this row.
    /// A shared rule path does not select an otherwise excluded catalogue owner.
    pub fn correspondence_record_selected(
        &self,
        unit: SourceObligation<'_>,
    ) -> gmeow_errors::Result<bool> {
        self.authorize_inventory(unit.inventory)?;
        let crate::source_inventory::SourceObligationOrigin::Statement(binding) = unit.origin()
        else {
            return Ok(false);
        };
        Ok(self
            .0
            .index
            .correspondence_record_selected(unit.inventory, binding))
    }
}

/// Original-indexed, borrowed IR admitted by the compiler's source ownership.
#[derive(Clone, Copy)]
pub struct SelectedProgramView<'selection, 'source> {
    inventory: &'selection SourceInventory<'source>,
    selection: &'selection ResolvedTheorySelection,
}

impl SelectedProgramView<'_, '_> {
    /// Classify an original axiom index without manufacturing an obligation.
    ///
    /// # Errors
    /// Refuses an index outside this immutable compilation.
    pub fn axiom_selection(&self, index: usize) -> gmeow_errors::Result<TheoryUnitSelection<'_>> {
        let sources = self
            .inventory
            .theory()
            .axiom_sources()
            .get(index)
            .ok_or_else(|| super::selection_error("UNKNOWN_SELECTED_AXIOM", index))?;
        let mut outside = false;
        let mut blocked = None;
        for source in sources {
            match self
                .selection
                .0
                .index
                .axiom_source_membership(self.inventory, index, source)
            {
                Some(Membership::Included) => return Ok(TheoryUnitSelection::Included),
                Some(Membership::Outside) => outside = true,
                Some(Membership::Blocked(boundary)) => blocked = Some(boundary),
                None => {}
            }
        }
        if let Some(boundary) = blocked {
            return Ok(Membership::Blocked(boundary).into());
        }
        if outside && self.selection.request().is_composed() {
            return Ok(TheoryUnitSelection::OutsideRequestedTheory);
        }
        Ok(TheoryUnitSelection::Blocked {
            code: "NO_EXECUTABLE_OWNER_PATH",
            reason: "axiom has no admitted original source contributor",
        })
    }

    #[must_use]
    pub fn axioms(&self) -> impl Iterator<Item = (usize, &LogicAxiom)> {
        self.inventory
            .theory()
            .program()
            .axioms
            .iter()
            .enumerate()
            .filter(|(index, _)| self.selection.0.index.axioms[*index])
    }

    #[must_use]
    pub fn rules(&self) -> impl Iterator<Item = (usize, &LogicRule)> {
        self.inventory
            .theory()
            .program()
            .rules
            .iter()
            .enumerate()
            .filter(|(index, _)| self.selection.0.index.rules[*index])
    }

    #[must_use]
    pub fn formulas(&self) -> impl Iterator<Item = (usize, &Formula)> {
        self.inventory
            .theory()
            .program()
            .formulas
            .iter()
            .enumerate()
            .filter(|(index, _)| self.selection.0.index.formulas[*index])
    }

    /// Every admitted source occurrence survives shared-AST deduplication.
    #[must_use]
    pub fn formula_lowerings(&self) -> impl Iterator<Item = (usize, &FormulaLowering)> {
        self.inventory
            .theory()
            .formula_lowerings()
            .iter()
            .enumerate()
            .filter(|(ordinal, _)| self.selection.0.index.formula_occurrences[*ordinal])
    }

    /// Admitted contributors only, retaining their original axiom coordinate.
    #[must_use]
    pub fn axiom_sources(&self) -> impl Iterator<Item = (usize, &AxiomSource)> {
        self.inventory
            .theory()
            .axiom_sources()
            .iter()
            .enumerate()
            .flat_map(|(index, sources)| sources.iter().map(move |source| (index, source)))
            .filter(|(index, source)| {
                matches!(
                    self.selection
                        .0
                        .index
                        .axiom_source_membership(self.inventory, *index, source),
                    Some(Membership::Included)
                )
            })
    }

    /// Borrow the admitted contributors of one original axiom coordinate.
    ///
    /// # Errors
    /// Refuses an index outside this immutable compilation.
    pub fn axiom_sources_at(
        &self,
        index: usize,
    ) -> gmeow_errors::Result<impl Iterator<Item = &AxiomSource>> {
        let sources = self
            .inventory
            .theory()
            .axiom_sources()
            .get(index)
            .ok_or_else(|| super::selection_error("UNKNOWN_SELECTED_AXIOM", index))?;
        Ok(sources.iter().filter(move |source| {
            matches!(
                self.selection
                    .0
                    .index
                    .axiom_source_membership(self.inventory, index, source),
                Some(Membership::Included)
            )
        }))
    }

    /// Non-excluded observations retain blocked carriers for diagnostics only.
    pub(crate) fn axiom_source_observations_at(
        &self,
        index: usize,
    ) -> gmeow_errors::Result<impl Iterator<Item = &AxiomSource>> {
        let sources = self
            .inventory
            .theory()
            .axiom_sources()
            .get(index)
            .ok_or_else(|| super::selection_error("UNKNOWN_SELECTED_AXIOM", index))?;
        Ok(sources.iter().filter(move |source| {
            matches!(
                self.selection
                    .0
                    .index
                    .axiom_source_membership(self.inventory, index, source),
                Some(Membership::Included | Membership::Blocked(_))
            )
        }))
    }

    #[must_use]
    pub fn selection(&self) -> &ResolvedTheorySelection {
        self.selection
    }
}

/// A subsequence of each original physical stream, with unchanged native IDs.
/// Term lookup delegates to the complete dictionary; it never asserts a term.
pub struct SelectedSourceView<'selection> {
    dataset: &'selection RdfDataset,
    selection: &'selection ResolvedTheorySelection,
}

impl SelectedSourceView<'_> {
    fn allowed(&self, canonical: QuadIds, carrier: SourceCarrier) -> bool {
        !self
            .selection
            .0
            .index
            .excluded_rows
            .contains(&SourceStatementBinding { canonical, carrier })
    }
}

impl DatasetView for SelectedSourceView<'_> {
    type Id = TermId;
    type ReadError = Infallible;
    type TermGuard<'a>
        = TermRef<'a>
    where
        Self: 'a;
    type ProbePlan = <RdfDataset as DatasetView>::ProbePlan;

    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .quads()
            .zip(&self.selection.0.index.ordinary)
            .filter_map(|(quad, &admitted)| admitted.then_some(quad))
    }
    fn resolve(&self, id: TermId) -> Result<TermRef<'_>, Infallible> {
        Ok(self.dataset.resolve(id))
    }
    fn term_id_by_value(&self, value: &TermValue) -> Result<Option<TermId>, Infallible> {
        DatasetView::term_id_by_value(self.dataset, value)
    }
    fn capabilities(&self) -> RdfStoreCapabilities {
        self.dataset.capabilities()
    }
    fn probe_plan(&self, s: bool, p: bool, o: bool, g: GraphMatch) -> Self::ProbePlan {
        self.dataset.probe_plan(s, p, o, g)
    }
    fn quads_for_pattern(
        &self,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .quads_for_pattern(s, p, o, g)
            .filter(|quad| self.allowed(*quad, SourceCarrier::Statement))
    }
    fn quads_for_pattern_with_plan(
        &self,
        plan: &Self::ProbePlan,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .quads_for_pattern_with_plan(plan, s, p, o, g)
            .filter(|quad| self.allowed(*quad, SourceCarrier::Statement))
    }
    fn term_count(&self) -> u64 {
        DatasetView::term_count(self.dataset)
    }
    fn reifier_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .reifier_quads()
            .zip(&self.selection.0.index.reifiers)
            .filter_map(|(quad, &admitted)| admitted.then_some(quad))
    }
    fn reifier_quads_of(&self, reifier: TermId) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .reifier_quads_of(reifier)
            .filter(|quad| self.allowed(*quad, SourceCarrier::Reifier))
    }
    fn annotation_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.dataset
            .annotation_quads()
            .zip(&self.selection.0.index.annotations)
            .filter_map(|(quad, &admitted)| admitted.then_some(quad))
    }
    fn annotations_of_with_graph(
        &self,
        reifier: TermId,
    ) -> impl Iterator<Item = (TermId, TermId, Option<TermId>)> + '_ {
        self.dataset
            .annotations_of_with_graph(reifier)
            .filter(move |&(p, o, g)| {
                self.allowed(
                    QuadIds {
                        s: reifier,
                        p,
                        o,
                        g,
                    },
                    SourceCarrier::Annotation,
                )
            })
    }
}
