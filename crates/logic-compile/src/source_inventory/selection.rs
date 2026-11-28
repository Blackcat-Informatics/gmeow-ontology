// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compiler-issued resolution of explicit module roots against one immutable source.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{TermId, TermRef};

use super::{SourceIdentity, SourceInventory, source_statements};
use crate::frontend::{
    CompiledTheory, RootSelection, SourceCarrier, SourceEdge, SourceEdgeRole, SourceNode,
    SourcePredicate, SourceUnitKind,
};
use crate::ir::{ModuleIri, TheorySelection};

const LOGIC_MODULE: &str = "https://blackcatinformatics.ca/logic/Module";
const THEORY_SELECTION_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-resolved-theory-selection-v1\0");
const THEORY_EXECUTION_MASKS_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-theory-execution-masks-v2\0");

mod index;
mod view;

pub use view::{SelectedProgramView, SelectedSourceView, TheoryUnitSelection};

/// Portable evidence of an actual admitted module-import edge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleImportObservation {
    pub source: ModuleIri,
    pub target: ModuleIri,
    pub carrier: SourceCarrier,
    pub documents: BTreeSet<String>,
}

/// A resolved request is observable data; decoding it cannot restore authority.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TheorySelectionObservation {
    pub request: TheorySelection,
    pub includes_top_level: bool,
    pub modules: BTreeSet<ModuleIri>,
    pub declarations: BTreeMap<ModuleIri, BTreeSet<String>>,
    pub imports: Vec<ModuleImportObservation>,
    pub source_digest: String,
    pub program_digest: String,
    pub document_selection_digest: String,
    pub effective_masks_digest: String,
    pub identity: String,
}

impl TheorySelectionObservation {
    /// Verify the portable evidence seal without issuing a live selection.
    ///
    /// # Errors
    /// Refuses a record whose selected source fields do not match its identity.
    pub fn verify_identity(&self) -> gmeow_errors::Result<()> {
        let mut unsigned = self.clone();
        unsigned.identity.clear();
        if unsigned.compute_identity()? != self.identity {
            return Err(selection_error(
                "FOREIGN_THEORY_SELECTION",
                "portable selection fields do not match their recorded identity",
            ));
        }
        Ok(())
    }

    fn compute_identity(&self) -> gmeow_errors::Result<String> {
        let mut hasher = purrdf_hash::blake3::Hasher::new();
        hasher.update(THEORY_SELECTION_DOMAIN.as_bytes());
        serde_json::to_writer(&mut hasher, self).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Frontend::caused_by(
                "cannot encode the resolved module selection identity".into(),
                error,
            ))
        })?;
        Ok(purrdf_hash::hex::encode(hasher.finalize().as_bytes()))
    }
}

#[derive(Debug)]
struct ResolvedSelectionData {
    source: SourceIdentity,
    modules: BTreeSet<TermId>,
    observation: TheorySelectionObservation,
    index: index::SelectionIndex,
}

/// Live compiler authority for a request resolved against one exact source.
/// No decoder, public index constructor or digest string can issue this value.
#[derive(Debug, Clone)]
pub struct ResolvedTheorySelection(Arc<ResolvedSelectionData>);

impl ResolvedTheorySelection {
    #[must_use]
    pub fn observation(&self) -> &TheorySelectionObservation {
        &self.0.observation
    }

    #[must_use]
    pub fn request(&self) -> &TheorySelection {
        &self.0.observation.request
    }

    /// Check the invocation-local source before borrowing its native coordinates.
    ///
    /// # Errors
    /// Refuses a separately compiled source, even when its content is identical.
    pub fn authorize_source(&self, theory: &CompiledTheory) -> gmeow_errors::Result<()> {
        if !self.0.source.matches(&theory.source().identity()) {
            return Err(selection_error(
                "FOREIGN_THEORY_SELECTION",
                "module selection belongs to a different immutable compiler source",
            ));
        }
        Ok(())
    }

    /// Check both the live source and the exact document/root selection.
    ///
    /// # Errors
    /// Refuses foreign source coordinates or a different source selection.
    pub fn authorize_inventory(&self, inventory: &SourceInventory<'_>) -> gmeow_errors::Result<()> {
        self.authorize_source(inventory.theory())?;
        if inventory.source_digest() != self.observation().source_digest
            || inventory.program_digest() != self.observation().program_digest
        {
            return Err(selection_error(
                "FOREIGN_COMPILATION_SELECTION",
                "resolved theory belongs to different source or program identities",
            ));
        }
        if inventory.selection_digest() != self.observation().document_selection_digest {
            return Err(selection_error(
                "FOREIGN_DOCUMENT_SELECTION",
                "resolved theory belongs to a different document/root selection",
            ));
        }
        Ok(())
    }

    /// Test an exact native module target after checking its source binding.
    ///
    /// # Errors
    /// Refuses a foreign source instead of interpreting its invocation-local ID.
    pub fn contains_module(
        &self,
        theory: &CompiledTheory,
        module: TermId,
    ) -> gmeow_errors::Result<bool> {
        self.authorize_source(theory)?;
        Ok(self.0.modules.contains(&module))
    }

    /// Borrow the issuer's actual native module closure after source validation.
    pub(crate) fn module_terms(
        &self,
        theory: &CompiledTheory,
    ) -> gmeow_errors::Result<&BTreeSet<TermId>> {
        self.authorize_source(theory)?;
        Ok(&self.0.modules)
    }
}

fn selection_error(code: &str, detail: impl std::fmt::Display) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Frontend::new(format!("{code}: {detail}")))
}

fn module_identifier(theory: &CompiledTheory, term: TermId) -> gmeow_errors::Result<ModuleIri> {
    let TermRef::Iri(iri) = theory.source().dataset().resolve(term) else {
        return Err(selection_error(
            "MALFORMED_MODULE_TARGET",
            "a module coordinate requires a native IRI, not a literal, blank or quoted term",
        ));
    };
    ModuleIri::try_from(iri.to_owned())
}

impl SourceInventory<'_> {
    /// Resolve roots and actual transitive imports without admitting execution.
    ///
    /// # Errors
    /// Refuses malformed selections, absent declarations and scoped dependencies.
    pub fn resolve_theory_selection(
        &self,
        request: &TheorySelection,
    ) -> gmeow_errors::Result<ResolvedTheorySelection> {
        let source_selection = self.selection();
        if [
            &source_selection.formulas,
            &source_selection.rules,
            &source_selection.owners,
        ]
        .into_iter()
        .any(|roots| *roots != RootSelection::All)
        {
            return Err(selection_error(
                "UNSUPPORTED_SOURCE_ROOT_SELECTION",
                "module composition requires all source formula, rule and owner roots; arbitrary root filters have no execution contract",
            ));
        }
        if !self.selection_issues().is_empty() {
            return Err(selection_error(
                "INVALID_DOCUMENT_SELECTION",
                self.selection_issues()
                    .iter()
                    .map(|issue| format!("{}: {}", issue.code, issue.reason))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let mut available = BTreeMap::<TermId, (ModuleIri, BTreeSet<String>)>::new();
        let theory = self.theory();
        let dataset = theory.source().dataset();
        if self.selection().documents
            != crate::frontend::SourceSelection::all_classical_roots(theory).documents
        {
            return Err(selection_error(
                "UNSUPPORTED_DOCUMENT_SUBSELECTION",
                "resolved theory execution requires every actually loaded source document; narrower document filtering has no execution contract",
            ));
        }
        if request.is_composed() {
            if !request.includes_all_modules() && request.module_roots().next().is_none() {
                return Err(selection_error(
                    "EMPTY_MODULE_SELECTION",
                    "a composed theory requires at least one explicit module root",
                ));
            }
            for (quad, carrier) in source_statements(dataset) {
                if quad.g.is_some()
                    || !matches!(dataset.resolve(quad.o), TermRef::Iri(iri) if iri == LOGIC_MODULE)
                    || !matches!(dataset.resolve(quad.p), TermRef::Iri(iri) if matches!(iri,
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        | "https://blackcatinformatics.ca/logic/instanceOf"))
                    || !theory.source().source_graph().declares(
                        SourceNode {
                            term: quad.s,
                            graph: None,
                        },
                        SourceUnitKind::Module,
                    )
                    || !self.statement_selected(quad, carrier)
                {
                    continue;
                }
                let TermRef::Iri(iri) = dataset.resolve(quad.s) else {
                    if request.includes_all_modules() {
                        return Err(selection_error(
                            "MALFORMED_MODULE_TARGET",
                            "complete source requires an absolute IRI for each module declaration",
                        ));
                    }
                    continue;
                };
                let iri = ModuleIri::try_from(iri.to_owned())?;
                available
                    .entry(quad.s)
                    .or_insert_with(|| (iri, BTreeSet::new()))
                    .1
                    .extend(self.statement_documents(quad, carrier));
            }
        }
        let mut pending = if request.includes_all_modules() {
            available.keys().copied().collect()
        } else {
            BTreeSet::new()
        };
        for root in request.module_roots() {
            let term = dataset
                .term_id_by_iri(root.as_str())
                .filter(|term| available.contains_key(term))
                .ok_or_else(|| selection_error("UNAVAILABLE_MODULE_ROOT", root.as_str()))?;
            pending.insert(term);
        }
        let mut modules = BTreeSet::new();
        let mut imports = Vec::new();
        let graph = theory.source().source_graph();
        while let Some(module) = pending.pop_first() {
            if !modules.insert(module) {
                continue;
            }
            let node = SourceNode {
                term: module,
                graph: None,
            };
            for edge in graph.outgoing(node).iter().filter(|edge| {
                matches!(
                    edge.role,
                    SourceEdgeRole::Module | SourceEdgeRole::Standpoint
                ) || (edge.role == SourceEdgeRole::Context
                    && edge.predicate.iri(dataset)
                        != "https://blackcatinformatics.ca/logic/confidence")
            }) {
                if self.selected_edge_documents(edge)?.is_some() {
                    return Err(selection_error(
                        "SCOPED_MODULE_IMPORTS",
                        available[&module].0.as_str(),
                    ));
                }
            }
            for edge in graph
                .outgoing(node)
                .iter()
                .filter(|edge| edge.role == SourceEdgeRole::Import)
            {
                let Some(documents) = self.selected_edge_documents(edge)? else {
                    continue;
                };
                let target = module_identifier(theory, edge.target)?;
                if !available.contains_key(&edge.target) {
                    return Err(selection_error(
                        "UNAVAILABLE_IMPORTED_MODULE",
                        target.as_str(),
                    ));
                }
                imports.push(ModuleImportObservation {
                    source: available[&module].0.clone(),
                    target,
                    carrier: edge.carrier,
                    documents,
                });
                pending.insert(edge.target);
            }
        }
        imports.sort_by(|left, right| {
            (
                &left.source,
                &left.target,
                left.carrier.as_str(),
                &left.documents,
            )
                .cmp(&(
                    &right.source,
                    &right.target,
                    right.carrier.as_str(),
                    &right.documents,
                ))
        });
        let index = index::SelectionIndex::new(
            self,
            request.is_composed(),
            &modules,
            &available.keys().copied().collect(),
        );
        let mut mask_hasher = purrdf_hash::blake3::Hasher::new();
        mask_hasher.update(THEORY_EXECUTION_MASKS_DOMAIN.as_bytes());
        serde_json::to_writer(
            &mut mask_hasher,
            &(
                &index.ordinary,
                &index.annotations,
                &index.reifiers,
                &index.axioms,
                &index.rules,
                &index.forbidden_patterns,
                &index.formulas,
                &index.formula_occurrences,
            ),
        )
        .map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Frontend::caused_by(
                "cannot encode the effective theory execution masks".into(),
                error,
            ))
        })?;
        let mut observation = TheorySelectionObservation {
            request: request.clone(),
            includes_top_level: true,
            modules: modules
                .iter()
                .map(|term| available[term].0.clone())
                .collect(),
            declarations: modules.iter().map(|term| available[term].clone()).collect(),
            imports,
            source_digest: self.source_digest().into(),
            program_digest: self.program_digest().into(),
            document_selection_digest: self.selection_digest().into(),
            identity: String::new(),
            effective_masks_digest: purrdf_hash::hex::encode(mask_hasher.finalize().as_bytes()),
        };
        observation.identity = observation.compute_identity()?;
        Ok(ResolvedTheorySelection(Arc::new(ResolvedSelectionData {
            source: theory.source().identity(),
            modules,
            observation,
            index,
        })))
    }

    fn selected_edge_documents(
        &self,
        edge: &SourceEdge,
    ) -> gmeow_errors::Result<Option<BTreeSet<String>>> {
        let SourcePredicate::Stored(predicate) = edge.predicate else {
            return Err(selection_error(
                "MALFORMED_SOURCE_EDGE_CARRIER",
                "a selected module dependency requires an actual stored assertion predicate",
            ));
        };
        let quad = purrdf::QuadIds {
            s: edge.source.term,
            p: predicate,
            o: edge.target,
            g: edge.source.graph,
        };
        Ok(self
            .statement_selected(quad, edge.carrier)
            .then(|| self.statement_documents(quad, edge.carrier)))
    }
}
