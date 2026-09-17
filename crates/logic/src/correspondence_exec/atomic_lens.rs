// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Stateful atomic property lenses over one explicitly shared RDF identity space.
//!
//! The focus is the ordinary assertions of one predicate. Graph declarations,
//! reifier bindings, annotations and all other assertions form the complement.
//! Updating the focus never rewrites a historical quoted statement. The graph
//! catalogue is fixed for this operation; editing it requires a different lens.

use std::collections::BTreeSet;
use std::sync::Arc;

use purrdf::ir::import::DatasetImporter;
use purrdf::ir::{DeltaDatasetView, MutableDataset, QuadValues};
use purrdf::{
    CompositeDatasetView, CompositeSource, DatasetMut, DatasetView, GraphMatch, QuadIds,
    RdfDataset, RdfDatasetBuilder, TermRef, TermValue, ViewLimits,
};

use super::{DischargeOutcome, SeedGraph, compare_graphs, exec_error};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

#[cfg(test)]
mod tests;

pub mod composition;

/// Compiled, graph-preserving predicate rename, optionally reversing endpoints.
/// This is an executable primitive, not a certificate for an authored leg pair.
#[derive(Debug, Clone)]
pub struct AtomicPropertyLens {
    source_predicate: String,
    view_predicate: String,
    inverse: bool,
    limits: ViewLimits,
}

#[derive(Debug)]
struct Complement {
    lens: AtomicPropertyLens,
    residual: Arc<DeltaDatasetView>,
    graphs: BTreeSet<TermValue>,
    origin: ComplementOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComplementOrigin {
    Source,
    ProjectedCatalogue,
}

/// A native augmented view. Its required complement is inseparable from the
/// view's operation and identity space; callers cannot substitute a boolean or
/// construct an incomplete recovery token.
#[derive(Debug, Clone)]
pub struct AugmentedAtomicView {
    view: Arc<RdfDataset>,
    complement: Arc<Complement>,
}

/// A source after acquisition or update. Successive puts share one residual
/// source and retain only the current focus, rather than a chain of prior states.
#[derive(Debug, Clone)]
pub struct AtomicLensState {
    augmented: AugmentedAtomicView,
    carrier: Arc<CompositeDatasetView>,
    // A restored focus is already native. Composition may reuse it only after
    // checking that the complete intermediate source has no other RDF content.
    source_focus: Option<Arc<RdfDataset>>,
}

/// The initial-state policy under which an augmented view restores a source.
#[derive(Debug, Clone, Copy)]
pub enum AtomicInitialState {
    /// Start without source assertions; recover the residual from the required
    /// complement and the focus from the view.
    EmptyWithComplement,
}

impl AtomicPropertyLens {
    /// Compile the selected property focus once. Metadata stays in the residual.
    ///
    /// # Errors
    /// Refuses invalid predicate IRIs and `rdf:reifies`, whose binding semantics
    /// require a statement lens rather than an ordinary assertion lens.
    pub fn new(
        source_predicate: &str,
        view_predicate: &str,
        inverse: bool,
        limits: ViewLimits,
    ) -> gmeow_errors::Result<Self> {
        for predicate in [source_predicate, view_predicate] {
            super::algebra_iri(predicate)?;
            if predicate == RDF_REIFIES {
                return Err(exec_error("atomic property focus cannot own rdf:reifies"));
            }
        }
        Ok(Self {
            source_predicate: source_predicate.to_owned(),
            view_predicate: view_predicate.to_owned(),
            inverse,
            limits,
        })
    }

    /// Acquire an immutable source and retain the part outside this lens's focus.
    /// Source locations and other native sidecars remain on the shared base.
    ///
    /// # Errors
    /// Refuses unfolded statement bindings, invalid transformed RDF and native
    /// retention-limit overflow. A flat wire adapter must establish native
    /// statement classification before selecting this operation.
    pub fn acquire(&self, source: Arc<RdfDataset>) -> gmeow_errors::Result<AtomicLensState> {
        if let Some(predicate) = source.term_id_by_iri(RDF_REIFIES)
            && source
                .quads_for_pattern(None, Some(predicate), None, GraphMatch::Any)
                .any(|quad| matches!(source.resolve(quad.o), TermRef::Triple { .. }))
        {
            return Err(exec_error(
                "atomic acquisition requires native statement bindings, not an unfolded wire carrier",
            ));
        }
        let mut residual = MutableDataset::new(Arc::clone(&source));
        if let Some(predicate) = source.term_id_by_iri(&self.source_predicate) {
            for quad in source.quads_for_pattern(None, Some(predicate), None, GraphMatch::Any) {
                residual.remove(&QuadValues {
                    s: source.term_value(quad.s),
                    p: source.term_value(quad.p),
                    o: source.term_value(quad.o),
                    g: quad.g.map(|graph| source.term_value(graph)),
                });
            }
        }
        let residual = Arc::new(
            residual
                .snapshot_view_with_limits(self.limits)
                .map_err(|error| exec_error(format!("retain atomic complement: {error}")))?,
        );
        self.finish_acquisition(source, residual, None, ComplementOrigin::Source)
    }

    // Only a typed view produced by an admitted atomic state reaches this seam.
    // Its source locations and rich complement still belong to the outer stage;
    // this stage's own residual consists exactly of the fixed graph catalogue.
    fn acquire_projected(
        &self,
        input: &AugmentedAtomicView,
        graphs: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<AtomicLensState> {
        if self.source_predicate != input.complement.lens.view_predicate {
            return Err(exec_error(
                "atomic composition has mismatched intermediate predicates",
            ));
        }
        if graphs.rdf_row_count() != 0 || graph_catalogue(&graphs) != input.complement.graphs {
            return Err(exec_error(
                "atomic composition has an incompatible graph catalogue",
            ));
        }
        let source = Arc::clone(&input.view);
        let residual = MutableDataset::new(graphs)
            .snapshot_view_with_limits(self.limits)
            .map_err(|error| {
                exec_error(format!("retain intermediate graph complement: {error}"))
            })?;
        self.finish_acquisition(
            Arc::clone(&source),
            Arc::new(residual),
            Some(source),
            ComplementOrigin::ProjectedCatalogue,
        )
    }

    fn finish_acquisition(
        &self,
        source: Arc<RdfDataset>,
        residual: Arc<DeltaDatasetView>,
        source_focus: Option<Arc<RdfDataset>>,
        origin: ComplementOrigin,
    ) -> gmeow_errors::Result<AtomicLensState> {
        let view = rename_focus(
            &source,
            &self.source_predicate,
            &self.view_predicate,
            self.inverse,
        )?;
        let complement = Arc::new(Complement {
            lens: self.clone(),
            residual,
            graphs: graph_catalogue(&source),
            origin,
        });
        // Acquisition already owns this exact source. Do not reconstruct it from
        // the just-derived view merely to create the first state.
        let carrier = CompositeDatasetView::with_shared_scopes(vec![source], self.limits)
            .map_err(|error| exec_error(format!("admit atomic source: {error}")))?;
        Ok(AtomicLensState {
            augmented: AugmentedAtomicView { view, complement },
            carrier: Arc::new(carrier),
            source_focus,
        })
    }

    fn execution_identity(&self) -> (&str, &str, bool, [usize; 5]) {
        (
            &self.source_predicate,
            &self.view_predicate,
            self.inverse,
            [
                self.limits.max_sources,
                self.limits.max_terms,
                self.limits.max_rows,
                self.limits.max_payload_bytes,
                self.limits.max_auxiliary_bytes,
            ],
        )
    }
}

fn graph_catalogue(dataset: &RdfDataset) -> BTreeSet<TermValue> {
    dataset
        .named_graphs()
        .map(|id| dataset.term_value(id))
        .collect()
}

fn graph_catalogue_dataset(source: &RdfDataset) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    let mut importer = DatasetImporter::new(&mut builder, source);
    let graphs: Vec<_> = source.named_graphs().map(|id| importer.term(id)).collect();
    drop(importer);
    for graph in graphs {
        builder.declare_named_graph(graph);
    }
    builder
        .freeze()
        .map_err(|error| exec_error(format!("publish graph complement: {error}")))
}

/// Transfer only the selected focus and graph catalogue. Native term import is
/// memoized once across all selected rows, including nested RDF 1.2 terms.
fn rename_focus(
    source: &RdfDataset,
    from: &str,
    to: &str,
    inverse: bool,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    let predicate = builder.intern_iri(to);
    let mut importer = DatasetImporter::new(&mut builder, source);
    let mut rows = Vec::new();
    if let Some(selected) = source.term_id_by_iri(from) {
        for quad in source.quads_for_pattern(None, Some(selected), None, GraphMatch::Any) {
            let (subject, object) = if inverse {
                (quad.o, quad.s)
            } else {
                (quad.s, quad.o)
            };
            if matches!(source.resolve(subject), TermRef::Literal { .. }) {
                return Err(exec_error(
                    "atomic inversion would place a literal in subject position",
                ));
            }
            rows.push(QuadIds {
                s: importer.term(subject),
                p: predicate,
                o: importer.term(object),
                g: quad.g.map(|graph| importer.term(graph)),
            });
        }
    }
    let graphs: Vec<_> = source.named_graphs().map(|id| importer.term(id)).collect();
    drop(importer);
    for quad in rows {
        builder.push_quad(quad.s, quad.p, quad.o, quad.g);
    }
    for graph in graphs {
        builder.declare_named_graph(graph);
    }
    builder
        .freeze()
        .map_err(|error| exec_error(format!("publish atomic focus: {error}")))
}

impl AugmentedAtomicView {
    fn edited(view: Arc<RdfDataset>, complement: Arc<Complement>) -> gmeow_errors::Result<Self> {
        let predicate = view.term_id_by_iri(&complement.lens.view_predicate);
        if view.quads().any(|quad| Some(quad.p) != predicate)
            || view.reifier_quads().next().is_some()
            || view.annotation_quads().next().is_some()
            || graph_catalogue(&view) != complement.graphs
        {
            return Err(exec_error(
                "atomic edit is outside its predicate or graph catalogue",
            ));
        }
        Ok(Self { view, complement })
    }

    /// The projected RDF carrier. The complement is held separately in this
    /// typed value so it cannot be confused with view assertions or graph names.
    pub fn dataset(&self) -> &Arc<RdfDataset> {
        &self.view
    }

    /// Recover with the explicitly selected initial-state policy.
    ///
    /// # Errors
    /// Refuses native RDF or retention admission failures without a partial state.
    pub fn restore(&self, initial: AtomicInitialState) -> gmeow_errors::Result<AtomicLensState> {
        let AtomicInitialState::EmptyWithComplement = initial;
        let lens = &self.complement.lens;
        let focus = rename_focus(
            &self.view,
            &lens.view_predicate,
            &lens.source_predicate,
            lens.inverse,
        )?;
        // An edit cannot move this ordinary-assertion focus into a residual
        // reifier's annotation table: that would change the selected operation.
        for quad in focus.quads() {
            let residual = &self.complement.residual;
            if let Some(subject) = residual.term_id_by_value(&focus.term_value(quad.s)) {
                let graph = quad.g.map(|id| focus.term_value(id));
                if residual
                    .reifier_quads_of(subject)
                    .any(|binding| binding.g.map(|id| residual.term_value(id)) == graph)
                {
                    return Err(exec_error(
                        "atomic edit would change a residual statement annotation",
                    ));
                }
            }
        }
        let carrier = CompositeDatasetView::from_shared_sources(
            vec![
                CompositeSource::from_delta(Arc::clone(&self.complement.residual)),
                CompositeSource::new(Arc::clone(&focus)),
            ],
            lens.limits,
        )
        .map_err(|error| exec_error(format!("restore atomic source: {error}")))?;
        Ok(AtomicLensState {
            augmented: self.clone(),
            carrier: Arc::new(carrier),
            source_focus: Some(focus),
        })
    }
}

impl AtomicLensState {
    /// Borrow the complete source, including the untouched complement. Consumers
    /// can execute native queries against this view without materialization.
    pub fn carrier(&self) -> &CompositeDatasetView {
        &self.carrier
    }

    /// Acquire the current view and its exact required recovery complement.
    pub fn get(&self) -> AugmentedAtomicView {
        self.augmented.clone()
    }

    /// Replace the selected focus using independently edited view assertions and
    /// the actual prior source's complement. Both operands must already use the
    /// same explicit native blank-scope authority; this is not a parser boundary.
    ///
    /// # Errors
    /// Refuses edits outside the declared focus or fixed graph catalogue, RDF
    /// inversion failures, and native retention-limit overflow. Metadata edits
    /// require a lens that owns that metadata, never silent omission here.
    pub fn put_shared_scopes(&self, view: Arc<RdfDataset>) -> gmeow_errors::Result<Self> {
        AugmentedAtomicView::edited(view, Arc::clone(&self.augmented.complement))?
            .restore(AtomicInitialState::EmptyWithComplement)
    }

    /// Execute GetPut against the complete prior source.
    ///
    /// # Errors
    /// Returns admission or comparison-materialization errors, never success on
    /// a missing domain. This is one bounded case, not universal law authority.
    pub fn check_get_put(&self, identity: &str) -> gmeow_errors::Result<DischargeOutcome> {
        let recovered = self.put_shared_scopes(Arc::clone(self.get().dataset()))?;
        compare_states(identity, "put(get(s), s) = s", &recovered, self)
    }

    /// Execute PutGet on this independently supplied view and nonempty prior.
    ///
    /// # Errors
    /// Returns errors for an inadmissible edit; it cannot discharge a law.
    pub fn check_put_get(
        &self,
        identity: &str,
        view: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        let updated = self.put_shared_scopes(Arc::clone(&view))?;
        // Re-project the actual updated carrier. Reading the cached input view
        // here would turn the test into a tautology and conceal a faulty put.
        let source = updated
            .carrier
            .materialize()
            .map_err(|error| exec_error(error.to_string()))?;
        let lens = &self.augmented.complement.lens;
        let actual = rename_focus(
            &source,
            &lens.source_predicate,
            &lens.view_predicate,
            lens.inverse,
        )?;
        Ok(compare_graphs(
            &case(identity),
            "get(put(v, s)) = v",
            &actual,
            &view,
        ))
    }

    /// Execute PutPut on two independently supplied successive edits.
    ///
    /// # Errors
    /// Returns admission or comparison-materialization errors.
    pub fn check_put_put(
        &self,
        identity: &str,
        first: Arc<RdfDataset>,
        second: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        let sequential = self
            .put_shared_scopes(first)?
            .put_shared_scopes(Arc::clone(&second))?;
        let direct = self.put_shared_scopes(second)?;
        compare_states(
            identity,
            "put(v2, put(v1, s)) = put(v2, s)",
            &sequential,
            &direct,
        )
    }

    /// Execute section recovery from an augmented view under the given policy.
    ///
    /// # Errors
    /// Returns admission or comparison-materialization errors.
    pub fn check_section(
        &self,
        identity: &str,
        initial: AtomicInitialState,
    ) -> gmeow_errors::Result<DischargeOutcome> {
        compare_states(
            identity,
            "put(get(s), s0) = s",
            &self.get().restore(initial)?,
            self,
        )
    }
}

fn case(identity: &str) -> SeedGraph {
    SeedGraph {
        label: identity.to_owned(),
        quads: Vec::new(),
    }
}

fn compare_states(
    identity: &str,
    law: &str,
    actual: &AtomicLensState,
    expected: &AtomicLensState,
) -> gmeow_errors::Result<DischargeOutcome> {
    let actual = actual
        .carrier
        .materialize()
        .map_err(|error| exec_error(error.to_string()))?;
    let expected = expected
        .carrier
        .materialize()
        .map_err(|error| exec_error(error.to_string()))?;
    Ok(compare_graphs(&case(identity), law, &actual, &expected))
}
