// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable composition of admitted atomic property lenses.
//!
//! For `S -> V -> W`, get runs left to right, and put runs right to left
//! through each actual prior complement. The rich source complement belongs to
//! the first leg; a later leg receives a typed projected view and retains its
//! graph catalogue. Every original operation still executes its admission checks.
//! Checked complete-focus reuse removes reverse intermediate compactions. This
//! operational proof is not a law discharge or an unrestricted rewrite license.

use std::sync::Arc;

use purrdf::{DatasetView, RdfDataset};

use super::{
    AtomicInitialState, AtomicLensState, AtomicPropertyLens, AugmentedAtomicView, Complement,
    ComplementOrigin, DischargeOutcome, case, compare_graphs, exec_error, graph_catalogue,
    graph_catalogue_dataset,
};

#[cfg(test)]
mod tests;

pub mod optimizer;

/// A checked sequence of atomic source/view carrier types. Predicate equality
/// is a type boundary here, not evidence that arbitrary correspondences compose.
#[derive(Debug, Clone)]
pub struct AtomicComposition {
    stages: Arc<[AtomicPropertyLens]>,
}

/// The direction in which one original logical operation actually executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicOperation {
    /// Source acquisition and forward projection.
    Get,
    /// Edited-view admission and prior-state update.
    Put,
    /// Recovery under the explicit initial-state policy.
    Restore,
}

/// Operational trace of an executed stage, never a correspondence law claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicStageExecution {
    /// Position in the authored sequence; put/restore execute in reverse order.
    pub stage: usize,
    /// The operation actually executed at this position.
    pub operation: AtomicOperation,
    /// The complete-focus checker admitted reuse of this stage's restored source
    /// as its predecessor's view. No intermediate carrier was compacted.
    pub reused_intermediate: bool,
}

/// A complete composed source state. Each component retains its actual prior
/// complement, including graph declarations, for the next update.
#[derive(Debug, Clone)]
pub struct AtomicCompositionState {
    program: Arc<AtomicComposition>,
    stages: Vec<AtomicLensState>,
    execution: Vec<AtomicStageExecution>,
}

/// A terminal view with every required complement in order. Original states
/// and intermediate source payloads need not stay alive to recover the source.
#[derive(Debug, Clone)]
pub struct AugmentedCompositionView {
    program: Arc<AtomicComposition>,
    view: Arc<RdfDataset>,
    complements: Vec<Arc<Complement>>,
}

impl AtomicComposition {
    /// Admit a finite, nonempty atomic sequence with matching intermediate types.
    /// Every stage retains its own direction and native retention limits.
    ///
    /// # Errors
    /// Refuses an empty program or a source predicate different from the preceding
    /// view predicate. A lossful nonmatching bridge needs its own correspondence.
    pub fn new(stages: Vec<AtomicPropertyLens>) -> gmeow_errors::Result<Self> {
        if stages.is_empty() {
            return Err(exec_error(
                "atomic composition requires an executable stage",
            ));
        }
        for (index, pair) in stages.windows(2).enumerate() {
            if pair[0].view_predicate != pair[1].source_predicate {
                return Err(exec_error(format!(
                    "atomic composition boundary {} has incompatible predicates <{}> and <{}>",
                    index + 1,
                    pair[0].view_predicate,
                    pair[1].source_predicate,
                )));
            }
        }
        Ok(Self {
            stages: stages.into(),
        })
    }

    /// Compose two checked sequences without changing or erasing their stages.
    ///
    /// # Errors
    /// Refuses a mismatched boundary between the two programs.
    pub fn then(&self, next: &Self) -> gmeow_errors::Result<Self> {
        Self::new(
            self.stages
                .iter()
                .chain(next.stages.iter())
                .cloned()
                .collect(),
        )
    }

    /// Execute every forward stage over shared, native carriers.
    ///
    /// # Errors
    /// Returns the original stage's admission or RDF error with its exact index.
    pub fn acquire(&self, source: Arc<RdfDataset>) -> gmeow_errors::Result<AtomicCompositionState> {
        let first = self.stages[0]
            .acquire(source)
            .map_err(|error| stage_error(0, error))?;
        let mut states = vec![first];
        let graphs = (self.stages.len() > 1)
            .then(|| graph_catalogue_dataset(states[0].get().dataset()))
            .transpose()?;
        for (index, stage) in self.stages.iter().enumerate().skip(1) {
            let input = states.last().expect("nonempty checked sequence").get();
            states.push(
                stage
                    .acquire_projected(
                        &input,
                        Arc::clone(
                            graphs
                                .as_ref()
                                .expect("catalogue admitted for every noninitial stage"),
                        ),
                    )
                    .map_err(|error| stage_error(index, error))?,
            );
        }
        Ok(AtomicCompositionState {
            program: Arc::new(self.clone()),
            stages: states,
            execution: (0..self.stages.len())
                .map(|stage| AtomicStageExecution {
                    stage,
                    operation: AtomicOperation::Get,
                    reused_intermediate: false,
                })
                .collect(),
        })
    }
}

fn stage_error(index: usize, error: gmeow_errors::Diag) -> gmeow_errors::Diag {
    error.with_context(format!("atomic composition stage {index}"))
}

/// A borrowing witness checked against the complete, immutable intermediate.
/// There is no constructor accepting a verdict, sample result, or caller boolean.
struct CompleteFocus<'a> {
    dataset: &'a Arc<RdfDataset>,
}

impl<'a> CompleteFocus<'a> {
    fn check(state: &'a AtomicLensState) -> gmeow_errors::Result<Self> {
        let residual = &state.augmented.complement.residual;
        let Some(focus) = &state.source_focus else {
            return Err(exec_error("intermediate focus has no native publication"));
        };
        if state.augmented.complement.origin != ComplementOrigin::ProjectedCatalogue
            || residual.quads().next().is_some()
            || residual.reifier_quads().next().is_some()
            || residual.annotation_quads().next().is_some()
            || graph_catalogue(focus) != state.augmented.complement.graphs
        {
            return Err(exec_error(
                "intermediate focus does not cover its complete source carrier",
            ));
        }
        // Bind the borrowing witness to the actual immutable publication, not
        // merely to a focus-shaped dataset with a coincidentally equal catalogue.
        let mut published_focus = false;
        for source in state.carrier.sources() {
            if let Some(dataset) = source.dataset() {
                if Arc::ptr_eq(dataset, focus) {
                    published_focus = true;
                } else if dataset.rdf_row_count() != 0 {
                    return Err(exec_error(
                        "intermediate publication contains another RDF source",
                    ));
                }
            } else if !source
                .delta()
                .is_some_and(|delta| Arc::ptr_eq(delta, residual))
            {
                return Err(exec_error(
                    "intermediate publication has a different residual",
                ));
            }
        }
        let published_graphs = state
            .carrier
            .named_graphs()
            .map(|id| state.carrier.term_value(id))
            .collect();
        if !published_focus || state.augmented.complement.graphs != published_graphs {
            return Err(exec_error(
                "intermediate focus is not its bound complete publication",
            ));
        }
        Ok(Self { dataset: focus })
    }
}

impl AtomicCompositionState {
    /// Borrow the complete rich source; its complement remains in the first stage.
    pub fn carrier(&self) -> &purrdf::CompositeDatasetView {
        self.stages[0].carrier()
    }

    fn carrier_handle(&self) -> Arc<purrdf::CompositeDatasetView> {
        Arc::clone(&self.stages[0].carrier)
    }

    /// The actual execution order and checked intermediate reuse for this result.
    pub fn execution(&self) -> &[AtomicStageExecution] {
        &self.execution
    }

    /// Acquire the final view together with every ordered recovery complement.
    pub fn get(&self) -> AugmentedCompositionView {
        AugmentedCompositionView {
            program: Arc::clone(&self.program),
            view: Arc::clone(
                &self
                    .stages
                    .last()
                    .expect("nonempty checked sequence")
                    .augmented
                    .view,
            ),
            complements: self
                .stages
                .iter()
                .map(|stage| Arc::clone(&stage.augmented.complement))
                .collect(),
        }
    }

    /// Execute edited-view admission and put at every original stage in reverse.
    /// Reuse requires a fresh complete-focus witness for each actual intermediate.
    ///
    /// # Errors
    /// Refuses the first failed stage or incomplete intermediate; the immutable
    /// prior state is unaffected and no partial new state is published.
    pub fn put_shared_scopes(&self, view: Arc<RdfDataset>) -> gmeow_errors::Result<Self> {
        let mut input = view;
        let mut reversed = Vec::with_capacity(self.stages.len());
        let mut execution = Vec::with_capacity(self.stages.len());
        for (index, prior) in self.stages.iter().enumerate().rev() {
            let updated = prior
                .put_shared_scopes(input)
                .map_err(|error| stage_error(index, error))?;
            if index > 0 {
                input = Arc::clone(CompleteFocus::check(&updated)?.dataset);
            } else {
                // The loop has no next input; keep an existing immutable handle
                // instead of manufacturing an empty or materialized carrier.
                input = Arc::clone(&updated.augmented.view);
            }
            execution.push(AtomicStageExecution {
                stage: index,
                operation: AtomicOperation::Put,
                reused_intermediate: index > 0,
            });
            reversed.push(updated);
        }
        reversed.reverse();
        Ok(Self {
            program: Arc::clone(&self.program),
            stages: reversed,
            execution,
        })
    }

    /// Check acquisition stability against the complete original source.
    ///
    /// # Errors
    /// Returns stage or comparison errors; this is one bounded law case.
    pub fn check_get_put(&self, identity: &str) -> gmeow_errors::Result<DischargeOutcome> {
        let updated = self.put_shared_scopes(Arc::clone(self.get().dataset()))?;
        compare_states(identity, "composed put(get(s), s) = s", &updated, self)
    }

    /// Check an independently supplied edit by re-executing get on the actual
    /// updated source, rather than comparing against the cached input view.
    ///
    /// # Errors
    /// Returns stage or comparison errors; this is one bounded law case.
    pub fn check_put_get(
        &self,
        identity: &str,
        view: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        let updated = self.put_shared_scopes(Arc::clone(&view))?;
        let source = updated
            .carrier()
            .materialize()
            .map_err(|error| exec_error(error.to_string()))?;
        let actual = self.program.acquire(source)?.get();
        Ok(compare_graphs(
            &case(identity),
            "composed get(put(v, s)) = v",
            actual.dataset(),
            &view,
        ))
    }

    /// Check two independently selected successive edits against their direct put.
    ///
    /// # Errors
    /// Returns stage or comparison errors; this is one bounded law case.
    pub fn check_put_put(
        &self,
        identity: &str,
        first: Arc<RdfDataset>,
        second: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        let successive = self
            .put_shared_scopes(first)?
            .put_shared_scopes(Arc::clone(&second))?;
        let direct = self.put_shared_scopes(second)?;
        compare_states(
            identity,
            "composed put(v2, put(v1, s)) = put(v2, s)",
            &successive,
            &direct,
        )
    }

    /// Check source recovery with all complements and the declared initial state.
    ///
    /// # Errors
    /// Returns stage or comparison errors; this is one bounded law case.
    pub fn check_section(
        &self,
        identity: &str,
        initial: AtomicInitialState,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        compare_states(
            identity,
            "composed put(get(s), s0) = s",
            &self.get().restore(initial)?,
            self,
        )
    }
}

impl AugmentedCompositionView {
    /// The final projected RDF carrier; this alone is not a recovery artifact.
    pub fn dataset(&self) -> &Arc<RdfDataset> {
        &self.view
    }

    /// Recover each preceding source using its exact ordered complement.
    ///
    /// # Errors
    /// Refuses incomplete intermediate carriers and stage admission errors.
    pub fn restore(
        &self,
        initial: AtomicInitialState,
    ) -> gmeow_errors::Result<AtomicCompositionState> {
        if self.complements.len() != self.program.stages.len()
            || self
                .complements
                .iter()
                .zip(self.program.stages.iter())
                .any(|(complement, stage)| {
                    complement.lens.execution_identity() != stage.execution_identity()
                })
        {
            return Err(exec_error(
                "composed recovery has missing or mismatched stage complements",
            ));
        }
        let mut input = Arc::clone(&self.view);
        let mut reversed = Vec::with_capacity(self.complements.len());
        let mut execution = Vec::with_capacity(self.complements.len());
        for (index, complement) in self.complements.iter().enumerate().rev() {
            let augmented = AugmentedAtomicView::edited(input, Arc::clone(complement))
                .map_err(|error| stage_error(index, error))?;
            let restored = augmented
                .restore(initial)
                .map_err(|error| stage_error(index, error))?;
            input = if index > 0 {
                Arc::clone(CompleteFocus::check(&restored)?.dataset)
            } else {
                Arc::clone(&restored.augmented.view)
            };
            execution.push(AtomicStageExecution {
                stage: index,
                operation: AtomicOperation::Restore,
                reused_intermediate: index > 0,
            });
            reversed.push(restored);
        }
        reversed.reverse();
        Ok(AtomicCompositionState {
            program: Arc::clone(&self.program),
            stages: reversed,
            execution,
        })
    }
}

fn compare_states(
    identity: &str,
    law: &str,
    actual: &AtomicCompositionState,
    expected: &AtomicCompositionState,
) -> gmeow_errors::Result<DischargeOutcome> {
    let actual = actual
        .carrier()
        .materialize()
        .map_err(|error| exec_error(error.to_string()))?;
    let expected = expected
        .carrier()
        .materialize()
        .map_err(|error| exec_error(error.to_string()))?;
    Ok(compare_graphs(&case(identity), law, &actual, &expected))
}
