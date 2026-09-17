// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source anchors carried by actual lowering, before canonical ordering and deduplication.

use purrdf::{QuadIds, TermId};

use super::{Diagnostic, LogicAxiom, LogicProgram, PreparedLogicSource, SourceNode};

/// Destination collection in the compiled program, distinct from source typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnerFamily {
    Rule,
    Contract,
    PathShape,
    Constraint,
    ReasoningProgram,
    Correspondence,
    CorrespondenceComposition,
    PresentationMerge,
    FinitePresentation,
    PresentationContext,
    PresentationSymbol,
    PresentationSentence,
    PresentationEvidence,
    PresentationMap,
    TransactionProgram,
}

/// An owner's extraction outcome. Emission alone does not imply native admission;
/// attached diagnostics may still reject a contract or another selected capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerDisposition {
    Emitted { index: usize },
    Rejected,
    OutsideDefaultGraph,
}

/// One actual owner extraction, with direct references to its original diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerLowering {
    pub source: SourceNode,
    pub family: OwnerFamily,
    pub disposition: OwnerDisposition,
    pub diagnostics: Vec<usize>,
}

/// An authored statement or structural root that actually emitted an axiom.
/// Native IDs belong only to the prepared source borrowed by the compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AxiomSource {
    Statement {
        subject: SourceNode,
        predicate: TermId,
        object: TermId,
    },
    ClassExpression(SourceNode),
    Reification(SourceNode),
    Formula(SourceNode),
}

impl AxiomSource {
    pub(super) fn statement(quad: QuadIds) -> Self {
        Self::Statement {
            subject: SourceNode {
                term: quad.s,
                graph: quad.g,
            },
            predicate: quad.p,
            object: quad.o,
        }
    }
}

/// What happened to one declared formula in this compiler invocation.
/// Reading an owned formula validates its syntax; it does not execute its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaDisposition {
    /// Index into this compilation's canonical `program().axioms`.
    Axiom(usize),
    /// Index into this compilation's canonical `program().formulas`.
    Formula(usize),
    /// Valid syntax retained under its source-graph owner, never asserted globally.
    ReadForOwner,
    /// Index into this compilation's diagnostics, including shared subtree failures.
    Malformed { diagnostic: usize },
    /// Graph placement excludes the node from the default-graph compilation.
    OutsideDefaultGraph,
}

/// Exact source formula and its lowering outcome, independent of content identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaLowering {
    pub source: SourceNode,
    pub disposition: FormulaDisposition,
}

/// Result of compiling one prepared source. Source anchors and IR positions remain
/// bound to this immutable result; they are not serialized as durable identities.
///
/// This is emission evidence, not a complete source-coverage or execution verdict.
/// In particular, an axiom's class-expression root is not an accounting of every
/// consumed restriction/list statement, and owned formulas still require their
/// owner's lowering and execution evidence.
#[derive(Debug)]
pub struct CompiledSource<S> {
    pub(super) source: S,
    pub(super) program: LogicProgram,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) trace: EmissionTrace,
}

/// Borrowed extraction from an already prepared native selection.
pub type SourceCompilation<'source> = CompiledSource<&'source PreparedLogicSource>;

/// An owned native selection, its program and the original extraction evidence.
/// Keeping them together prevents source-local IDs from outliving their dataset.
/// This value retains exclusions and errors; construction certifies neither
/// complete coverage nor admission to a reasoning operation.
pub type CompiledTheory = CompiledSource<PreparedLogicSource>;

impl<S: std::borrow::Borrow<PreparedLogicSource>> CompiledSource<S> {
    #[must_use]
    pub fn source(&self) -> &PreparedLogicSource {
        self.source.borrow()
    }

    #[must_use]
    pub fn program(&self) -> &LogicProgram {
        &self.program
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Source roots/statements aligned with `program().axioms`. Deduplicating an
    /// axiom unions its origins; it never picks a single winning source.
    #[must_use]
    pub fn axiom_sources(&self) -> &[Vec<AxiomSource>] {
        &self.trace.axiom_sources
    }

    /// Every declared formula, including malformed, owned and graph-excluded nodes.
    #[must_use]
    pub fn formula_lowerings(&self) -> &[FormulaLowering] {
        &self.trace.formulas
    }

    /// Closed presentation sentence roots read by the shared frontend. These
    /// remain source-owned syntax and are never asserted as global axioms.
    /// Keys belong to this exact immutable source publication.
    #[must_use]
    pub fn presentation_formulas(
        &self,
    ) -> &std::collections::BTreeMap<SourceNode, std::sync::Arc<crate::ir::Formula>> {
        &self.trace.presentation_formulas
    }

    /// Root bindings from the actual typed owner readers, before ordering.
    #[must_use]
    pub fn owner_lowerings(&self) -> &[OwnerLowering] {
        &self.trace.owners
    }
}

#[derive(Debug, Default)]
pub(super) struct EmissionTrace {
    pub axiom_sources: Vec<Vec<AxiomSource>>,
    pub formulas: Vec<FormulaLowering>,
    pub owners: Vec<OwnerLowering>,
    pub presentation_formulas:
        std::collections::BTreeMap<SourceNode, std::sync::Arc<crate::ir::Formula>>,
}

pub(super) struct OwnerEmission<T> {
    pub source: SourceNode,
    pub family: OwnerFamily,
    pub diagnostics: Vec<usize>,
    pub value: T,
}

/// Capture at the reader boundary. Shared diagnostics are supplied by the failed
/// sub-reader itself, never guessed by comparing the finished IR with source IRIs.
pub(super) fn capture_owner<T>(
    source: SourceNode,
    family: OwnerFamily,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
    read: impl FnOnce(&mut Vec<Diagnostic>) -> Result<T, Vec<usize>>,
) -> Option<OwnerEmission<T>> {
    let start = diagnostics.len();
    let result = read(diagnostics);
    let mut diagnostic_ids: Vec<_> = (start..diagnostics.len()).collect();
    match result {
        Ok(value) => Some(OwnerEmission {
            source,
            family,
            diagnostics: diagnostic_ids,
            value,
        }),
        Err(shared) => {
            diagnostic_ids.extend(shared);
            diagnostic_ids.sort_unstable();
            diagnostic_ids.dedup();
            lowerings.push(OwnerLowering {
                source,
                family,
                disposition: OwnerDisposition::Rejected,
                diagnostics: diagnostic_ids,
            });
            None
        }
    }
}

/// Detach already canonically ordered values while assigning their real positions.
pub(super) fn finish_owners<T>(
    emissions: Vec<OwnerEmission<T>>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<T> {
    emissions
        .into_iter()
        .enumerate()
        .map(|(index, emission)| {
            lowerings.push(OwnerLowering {
                source: emission.source,
                family: emission.family,
                disposition: OwnerDisposition::Emitted { index },
                diagnostics: emission.diagnostics,
            });
            emission.value
        })
        .collect()
}

pub(super) fn excluded_owners(graph: &super::StructuralSourceGraph) -> Vec<OwnerLowering> {
    use super::SourceUnitKind as K;
    graph
        .units()
        .filter(|unit| unit.node.graph.is_some())
        .flat_map(|unit| {
            let families: std::collections::BTreeSet<_> = unit
                .declared_kinds
                .iter()
                .filter_map(|kind| {
                    Some(match kind {
                        K::Rule => OwnerFamily::Rule,
                        K::ReasoningContract | K::ReasoningPreset => OwnerFamily::Contract,
                        K::PathShape => OwnerFamily::PathShape,
                        K::Constraint | K::ConstraintSugar => OwnerFamily::Constraint,
                        K::ReasoningProgram => OwnerFamily::ReasoningProgram,
                        K::Correspondence => OwnerFamily::Correspondence,
                        K::CorrespondenceComposition => OwnerFamily::CorrespondenceComposition,
                        K::TransactionProgram => OwnerFamily::TransactionProgram,
                        _ => return None,
                    })
                })
                .collect();
            families.into_iter().map(|family| OwnerLowering {
                source: unit.node,
                family,
                disposition: OwnerDisposition::OutsideDefaultGraph,
                diagnostics: Vec::new(),
            })
        })
        .collect()
}

pub(super) struct AxiomEmission {
    pub axiom: LogicAxiom,
    pub sources: Vec<AxiomSource>,
}

impl AxiomEmission {
    pub fn new(axiom: LogicAxiom, source: AxiomSource) -> Self {
        Self {
            axiom,
            sources: vec![source],
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod owner_tests;
