// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selection of original source occurrences before native ingestion and lowering.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use purrdf::{DatasetView, GraphMatch, QuadIds, TermId, TermRef};

use crate::frontend::{
    AxiomSource, FormulaDisposition, OwnerDisposition, OwnerFamily, SourceCarrier, SourceEdgeRole,
    SourceNode, SourcePredicate, SourceStatementBinding, SourceUnitKind,
};
use crate::source_inventory::{
    SourceInventory, SourceObligationOrigin, SyntaxEvidence, SyntaxFamily, SyntaxIssue,
    SyntaxOwner, SyntaxOwnership, source_statements,
};

#[derive(Debug, Clone)]
pub(super) struct Boundary {
    pub(super) code: &'static str,
    pub(super) detail: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct Participation {
    included: bool,
    outside: bool,
    blocked_path: bool,
}

#[derive(Debug)]
enum Coordinate {
    None,
    Included,
    Outside,
    Blocked(Boundary),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Membership<'a> {
    Included,
    Outside,
    Blocked(&'a Boundary),
}

#[derive(Debug)]
struct ModuleFormulaLink {
    module: SyntaxOwner,
    target: Result<ModuleFormulaTarget, Boundary>,
}

#[derive(Debug, Clone, Copy)]
struct ModuleFormulaTarget {
    owner: SyntaxOwner,
    ordinal: usize,
}

/// Invocation-local masks retain original IR indices and lowering occurrences.
#[derive(Debug)]
pub(super) struct SelectionIndex {
    syntax: SyntaxOwnership,
    roots: BTreeSet<SyntaxOwner>,
    active: BTreeSet<SyntaxOwner>,
    participation: BTreeMap<SyntaxOwner, Participation>,
    coordinates: BTreeMap<SyntaxOwner, Coordinate>,
    selection_metadata: BTreeMap<SourceStatementBinding, BTreeSet<SyntaxOwner>>,
    formula_links: BTreeMap<SourceStatementBinding, ModuleFormulaLink>,
    blocked: BTreeMap<SyntaxOwner, Boundary>,
    unknown: Boundary,
    graph_boundary: Boundary,
    top_level_module_link: Boundary,
    pub(super) admitted_modules: BTreeSet<SourceNode>,
    pub(super) formula_occurrences: Vec<bool>,
    pub(super) axioms: Vec<bool>,
    pub(super) rules: Vec<bool>,
    pub(super) formulas: Vec<bool>,
    pub(super) ordinary: Vec<bool>,
    pub(super) annotations: Vec<bool>,
    pub(super) reifiers: Vec<bool>,
    pub(super) excluded_rows: BTreeSet<SourceStatementBinding>,
}

fn owner(node: SourceNode, family: SyntaxFamily) -> SyntaxOwner {
    SyntaxOwner { node, family }
}

fn owner_family(family: OwnerFamily) -> SyntaxFamily {
    match family {
        OwnerFamily::Rule => SyntaxFamily::Rule,
        OwnerFamily::Correspondence => SyntaxFamily::Correspondence,
        _ => SyntaxFamily::Owner,
    }
}

fn correspondence_field(
    inventory: &SourceInventory<'_>,
    unit: SyntaxOwner,
    edge: &crate::frontend::SourceEdge,
) -> bool {
    if unit.family != SyntaxFamily::Correspondence {
        return false;
    }
    let SourcePredicate::Stored(predicate) = edge.predicate else {
        return false;
    };
    inventory
        .correspondence_record_owners(SourceStatementBinding {
            canonical: QuadIds {
                s: edge.source.term,
                p: predicate,
                o: edge.target,
                g: edge.source.graph,
            },
            carrier: edge.carrier,
        })
        .any(|ordinal| inventory.theory().owner_lowerings()[ordinal].source == unit.node)
}

fn list_axiom(predicate: &str) -> bool {
    matches!(predicate, crate::lists::RDF_FIRST | crate::lists::RDF_REST)
}

fn module_formula_links(
    inventory: &SourceInventory<'_>,
    roots: &BTreeSet<SyntaxOwner>,
    independent_formulas: &mut BTreeSet<SourceNode>,
) -> BTreeMap<SourceStatementBinding, ModuleFormulaLink> {
    let theory = inventory.theory();
    let graph = theory.source().source_graph();
    let lowerings: BTreeMap<_, _> = theory
        .formula_lowerings()
        .iter()
        .enumerate()
        .map(|(ordinal, lowering)| (lowering.source, ordinal))
        .collect();
    graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.role == SourceEdgeRole::ExplicitFormulaSelection && edge.source.graph.is_none()
        })
        .filter_map(|edge| {
            let SourcePredicate::Stored(predicate) = edge.predicate else {
                return None;
            };
            let binding = SourceStatementBinding {
                canonical: QuadIds {
                    s: edge.source.term,
                    p: predicate,
                    o: edge.target,
                    g: edge.source.graph,
                },
                carrier: edge.carrier,
            };
            if !inventory.statement_selected(binding.canonical, binding.carrier) {
                return None;
            }
            let node = SourceNode {
                term: edge.target,
                graph: edge.source.graph,
            };
            if !graph.declares(edge.source, SourceUnitKind::Module) {
                if !graph.outgoing(edge.source).iter().any(|coordinate| {
                    matches!(
                        coordinate.role,
                        SourceEdgeRole::Module
                            | SourceEdgeRole::Standpoint
                            | SourceEdgeRole::Context
                            | SourceEdgeRole::Provenance
                    )
                }) {
                    // Preserve the existing top-level formula route through an
                    // unqualified nonmodule container.
                    independent_formulas.insert(node);
                }
                return None;
            }
            let target = lowerings
                .get(&node)
                .copied()
                .filter(|_| graph.declares(node, SourceUnitKind::Formula))
                .ok_or_else(|| Boundary {
                    code: "MISSING_MODULE_FORMULA_ROOT",
                    detail:
                        "module formula link requires an actual formula declaration and lowering"
                            .into(),
                })
                .and_then(|ordinal| {
                    let formula = owner(node, SyntaxFamily::Formula);
                    if !roots.contains(&formula) || graph.formula_is_owned(node) {
                        return Err(Boundary {
                            code: "MODULE_LINK_TO_OWNED_FORMULA",
                            detail:
                                "module cannot assert syntax already owned by another source unit"
                                    .into(),
                        });
                    }
                    if graph.outgoing(node).iter().any(|coordinate| {
                        coordinate.role == SourceEdgeRole::Module
                            && coordinate.target != edge.source.term
                    }) {
                        return Err(Boundary {
                            code: "CONFLICTING_FORMULA_MODULE_COORDINATE",
                            detail: "formula declares membership in a different module".into(),
                        });
                    }
                    Ok(ModuleFormulaTarget {
                        owner: formula,
                        ordinal,
                    })
                });
            Some((
                binding,
                ModuleFormulaLink {
                    module: owner(edge.source, SyntaxFamily::Module),
                    target,
                },
            ))
        })
        .collect()
}

impl SelectionIndex {
    pub(super) fn new(
        inventory: &SourceInventory<'_>,
        composed: bool,
        modules: &BTreeSet<TermId>,
        available: &BTreeSet<TermId>,
    ) -> Self {
        let theory = inventory.theory();
        let graph = theory.source().source_graph();
        let dataset = theory.source().dataset();
        let mut syntax = graph.syntax_ownership(dataset);
        for lowering in theory.owner_lowerings().iter().filter(|lowering| {
            lowering.family == OwnerFamily::Correspondence
                && matches!(lowering.disposition, OwnerDisposition::Emitted { .. })
        }) {
            for &binding in &lowering.record_support {
                syntax
                    .rows
                    .entry(binding)
                    .or_default()
                    .entry(owner(lowering.source, SyntaxFamily::Correspondence))
                    .or_default()
                    .insert(SyntaxEvidence::Statement(binding));
            }
        }
        // A recorded relationship is authority only when its actual carrier is
        // supplied by this inventory. Possible grammar families alone grant none.
        let evidence_selected = |evidence: &SyntaxEvidence| match *evidence {
            SyntaxEvidence::Statement(binding) => {
                inventory.statement_selected(binding.canonical, binding.carrier)
            }
            SyntaxEvidence::Edge(index) => {
                let edge = &graph.edges()[index];
                match edge.predicate {
                    SourcePredicate::Stored(p) => inventory.statement_selected(
                        QuadIds {
                            s: edge.source.term,
                            p,
                            o: edge.target,
                            g: edge.source.graph,
                        },
                        edge.carrier,
                    ),
                    SourcePredicate::Reifies => {
                        inventory.documents_selected(&inventory.node_documents(edge.source))
                    }
                }
            }
        };
        syntax.children.retain(|_, children| {
            children.retain(|_, evidence| {
                evidence.retain(&evidence_selected);
                !evidence.is_empty()
            });
            !children.is_empty()
        });
        syntax.rows.retain(|binding, owners| {
            if !inventory.statement_selected(binding.canonical, binding.carrier) {
                return false;
            }
            owners.retain(|_, evidence| {
                evidence.retain(&evidence_selected);
                !evidence.is_empty()
            });
            !owners.is_empty()
        });
        syntax.issues.retain(|_, issues| {
            issues.retain(|issue| {
                let source = match issue {
                    SyntaxIssue::MalformedCollection { source, .. }
                    | SyntaxIssue::CollectionCapacity { source, .. } => source,
                };
                inventory.statement_selected(source.canonical, source.carrier)
            });
            !issues.is_empty()
        });
        let mut roots = BTreeSet::new();
        for obligation in inventory.obligations() {
            match obligation.origin() {
                SourceObligationOrigin::ClassDefinition(node) => {
                    roots.insert(owner(node, SyntaxFamily::Class));
                }
                SourceObligationOrigin::PropertyCharacteristic(node) => {
                    roots.insert(owner(node, SyntaxFamily::Owner));
                }
                SourceObligationOrigin::Quotation { reifier, graph, .. } => {
                    roots.insert(owner(
                        SourceNode {
                            term: reifier,
                            graph,
                        },
                        SyntaxFamily::Atom,
                    ));
                }
                _ => {}
            }
        }
        for lowering in theory.owner_lowerings() {
            roots.insert(owner(lowering.source, owner_family(lowering.family)));
        }
        for lowering in theory.formula_lowerings() {
            roots.insert(owner(lowering.source, SyntaxFamily::Formula));
        }
        for (index, sources) in theory.axiom_sources().iter().enumerate() {
            for source in sources {
                match source {
                    AxiomSource::Reification(node) => {
                        roots.insert(owner(*node, SyntaxFamily::Atom));
                    }
                    AxiomSource::Collection { source, .. } => {
                        roots.insert(owner(
                            SourceNode {
                                term: source.constructor.canonical.s,
                                graph: source.constructor.canonical.g,
                            },
                            source.role.owner.family(),
                        ));
                    }
                    AxiomSource::RewrittenField {
                        source,
                        owner: role,
                    } => {
                        roots.insert(owner(
                            SourceNode {
                                term: source.canonical.s,
                                graph: source.canonical.g,
                            },
                            role.family(),
                        ));
                    }
                    AxiomSource::ClassExpression(node)
                        if !list_axiom(&theory.program().axioms[index].predicate) =>
                    {
                        roots.insert(owner(*node, SyntaxFamily::Class));
                    }
                    _ => {}
                }
            }
        }
        for unit in graph.units() {
            for kind in &unit.declared_kinds {
                roots.insert(owner(unit.node, declaration_family(*kind)));
            }
        }
        for (quad, _) in source_statements(dataset) {
            if let Some(family) = intrinsic_family(dataset, quad) {
                roots.insert(owner(
                    SourceNode {
                        term: quad.s,
                        graph: quad.g,
                    },
                    family,
                ));
            }
        }
        for edge in graph.edges() {
            if matches!(
                edge.role,
                SourceEdgeRole::ExclusiveFormulaUse | SourceEdgeRole::RecoveryOwnership
            ) {
                roots.insert(owner(
                    edge.source,
                    if edge.role == SourceEdgeRole::RecoveryOwnership {
                        SyntaxFamily::Correspondence
                    } else {
                        SyntaxFamily::Owner
                    },
                ));
            }
        }
        // Owners are roots only when they are not used as another owner's syntax.
        // A declared shared child remains support, never a new assertion.
        let children: BTreeSet<_> = syntax
            .children
            .values()
            .flat_map(|children| children.keys().copied())
            .collect();
        let all_roots = roots.clone();
        roots.retain(|root| !children.contains(root));
        let mut active = all_roots;
        let mut pending: VecDeque<_> = active.iter().copied().collect();
        while let Some(parent) = pending.pop_front() {
            if let Some(children) = syntax.children.get(&parent) {
                for &child in children.keys() {
                    if active.insert(child) {
                        pending.push_back(child);
                    }
                }
            }
        }
        let mut coordinates = BTreeMap::new();
        let mut selection_metadata = BTreeMap::new();
        let mut blocked = BTreeMap::new();
        let mut admitted_modules = BTreeSet::new();
        for &unit in &active {
            let coordinate = coordinate(
                inventory,
                unit,
                composed,
                modules,
                available,
                &mut selection_metadata,
            );
            match &coordinate {
                Coordinate::Included => {
                    admitted_modules.insert(unit.node);
                }
                Coordinate::Blocked(boundary) => {
                    blocked.insert(unit, boundary.clone());
                }
                Coordinate::None | Coordinate::Outside => {}
            }
            coordinates.insert(unit, coordinate);
        }
        for binding in syntax.rows.keys() {
            let unit = owner(
                SourceNode {
                    term: binding.canonical.s,
                    graph: binding.canonical.g,
                },
                SyntaxFamily::Schema,
            );
            coordinates.entry(unit).or_insert_with(|| {
                coordinate(
                    inventory,
                    unit,
                    composed,
                    modules,
                    available,
                    &mut selection_metadata,
                )
            });
        }
        let mut independent_formulas = BTreeSet::new();
        let formula_links = module_formula_links(inventory, &roots, &mut independent_formulas);
        for target in formula_links
            .values()
            .filter_map(|link| link.target.as_ref().ok())
        {
            if matches!(coordinates.get(&target.owner), Some(Coordinate::None))
                && !independent_formulas.contains(&target.owner.node)
            {
                roots.remove(&target.owner);
            }
        }
        selection_metadata.retain(|_, owners| {
            owners.retain(|unit| active.contains(unit));
            !owners.is_empty()
        });
        let mut index = Self { syntax, roots, active, coordinates,
            selection_metadata, formula_links,
            participation: BTreeMap::new(), blocked, admitted_modules,
            unknown: Boundary { code: "NO_EXECUTABLE_OWNER_PATH",
                detail: "owned syntax has no admitted executable owner path and cannot become an independent assertion".into() },
            graph_boundary: Boundary { code: "OUTSIDE_SELECTED_DEFAULT_GRAPH",
                detail: "named-graph syntax has no translation into the selected default logical world".into() },
            top_level_module_link: Boundary { code: "CONTEXT_ERASURE_FORBIDDEN",
                detail: "module formula link requires explicit module composition".into() },
            formula_occurrences: vec![false; theory.formula_lowerings().len()],
            axioms: vec![false; theory.program().axioms.len()],
            rules: vec![false; theory.program().rules.len()],
            formulas: vec![false; theory.program().formulas.len()],
            ordinary: Vec::new(), annotations: Vec::new(), reifiers: Vec::new(),
            excluded_rows: BTreeSet::new(),
        };
        index.propagate();
        // Failure belongs to the original owner, before reverse dependency
        // propagation marks every selected owner that requires it.
        for (&unit, issues) in &index.syntax.issues {
            if index
                .participation
                .get(&unit)
                .is_some_and(|state| state.included)
                && let Some(issue) = issues.first()
            {
                let (code, detail) = match issue {
                    SyntaxIssue::MalformedCollection { detail, .. } => {
                        ("MALFORMED_OWNED_COLLECTION", detail)
                    }
                    SyntaxIssue::CollectionCapacity { detail, .. } => {
                        ("SOURCE_COLLECTION_CAPACITY", detail)
                    }
                };
                index.blocked.insert(
                    unit,
                    Boundary {
                        code,
                        detail: detail.clone(),
                    },
                );
            }
        }
        index.block_contexts(inventory);
        for lowering in theory.owner_lowerings() {
            if !matches!(
                lowering.family,
                OwnerFamily::Rule | OwnerFamily::Contract | OwnerFamily::Correspondence
            ) {
                index.blocked.insert(
                    owner(lowering.source, SyntaxFamily::Owner),
                    Boundary {
                        code: "UNSUPPORTED_SELECTED_OWNER",
                        detail: format!(
                            "whole {:?} owner has no selected native execution contract",
                            lowering.family
                        ),
                    },
                );
            }
        }
        for diagnostic in 0..theory.diagnostics().len() {
            for unit in index.diagnostic_owners(inventory, diagnostic) {
                if index
                    .participation
                    .get(&unit)
                    .is_some_and(|state| state.included)
                {
                    index.blocked.entry(unit).or_insert_with(|| Boundary {
                        code: "COMPILER_OBSERVATION_ON_OWNER",
                        detail: format!(
                            "whole owner retains compiler observation {}: {}",
                            theory.diagnostics()[diagnostic].code,
                            theory.diagnostics()[diagnostic].message
                        ),
                    });
                }
            }
        }
        index.block_required_rows(inventory);
        index.block_required_children();
        // Inventory boundaries survive, but a blocked owner grants no raw EDB
        // reachability to its otherwise compatible siblings or descendants.
        index.propagate();
        index.lowering_masks(inventory);
        index.stream_masks(inventory);
        index
    }

    fn propagate(&mut self) {
        self.participation.clear();
        for &unit in &self.active {
            let mut state = Participation::default();
            match self.coordinates.get(&unit) {
                Some(Coordinate::Outside) => state.outside = true,
                Some(Coordinate::None | Coordinate::Included) if self.roots.contains(&unit) => {
                    if self.blocked.contains_key(&unit) {
                        state.blocked_path = true;
                    } else {
                        state.included = true;
                    }
                }
                Some(Coordinate::Blocked(_)) if self.roots.contains(&unit) => {
                    state.blocked_path = true
                }
                _ => {}
            }
            self.participation.insert(unit, state);
        }
        // A link can select a root, but a target cannot block its module or the
        // module's other assertions. Required syntax still propagates normally.
        for link in self.formula_links.values() {
            let Ok(target) = link.target else { continue };
            let mut state = self.participation[&target.owner];
            match self.formula_link_module(link) {
                Membership::Included if !self.blocked.contains_key(&target.owner) => {
                    state.included = true;
                }
                Membership::Outside => state.outside = true,
                Membership::Included | Membership::Blocked(_) => state.blocked_path = true,
            }
            self.participation.insert(target.owner, state);
        }
        let mut pending: VecDeque<_> = self
            .participation
            .iter()
            .filter(|(_, state)| state.included || state.outside || state.blocked_path)
            .map(|(unit, _)| *unit)
            .collect();
        while let Some(parent) = pending.pop_front() {
            let parent_state = self.participation[&parent];
            if let Some(children) = self.syntax.children.get(&parent) {
                for &child in children.keys() {
                    let mut incoming = Participation {
                        outside: parent_state.outside,
                        blocked_path: parent_state.blocked_path,
                        ..Participation::default()
                    };
                    if parent_state.included {
                        match self.coordinates.get(&child) {
                            Some(Coordinate::None | Coordinate::Included)
                                if !self.blocked.contains_key(&child) =>
                            {
                                incoming.included = true
                            }
                            Some(Coordinate::Outside) => incoming.outside = true,
                            _ => incoming.blocked_path = true,
                        }
                    }
                    let state = self.participation.entry(child).or_default();
                    let changed = (!state.included && incoming.included)
                        || (!state.outside && incoming.outside)
                        || (!state.blocked_path && incoming.blocked_path);
                    state.included |= incoming.included;
                    state.outside |= incoming.outside;
                    state.blocked_path |= incoming.blocked_path;
                    if changed {
                        pending.push_back(child);
                    }
                }
            }
        }
    }

    fn block_required_children(&mut self) {
        let mut parents = BTreeMap::<SyntaxOwner, Vec<SyntaxOwner>>::new();
        let mut pending = VecDeque::new();
        for (&parent, children) in &self.syntax.children {
            for &child in children.keys() {
                parents.entry(child).or_default().push(parent);
                if !matches!(self.owner_membership(child), Membership::Included) {
                    pending.push_back(child);
                }
            }
        }
        while let Some(child) = pending.pop_front() {
            for &parent in parents.get(&child).into_iter().flatten() {
                if self
                    .participation
                    .get(&parent)
                    .is_some_and(|state| state.included)
                    && !self.blocked.contains_key(&parent)
                {
                    self.blocked.insert(
                        parent,
                        Boundary {
                            code: "REQUIRED_CHILD_OUTSIDE_THEORY",
                            detail: format!(
                                "whole owner requires {child:?}; required syntax cannot be dropped"
                            ),
                        },
                    );
                    pending.push_back(parent);
                }
            }
        }
    }

    pub(super) fn owner_membership(&self, unit: SyntaxOwner) -> Membership<'_> {
        if matches!(self.coordinates.get(&unit), Some(Coordinate::Outside)) {
            return Membership::Outside;
        }
        match self.participation.get(&unit) {
            Some(state) if state.included && !self.blocked.contains_key(&unit) => {
                Membership::Included
            }
            Some(state) if state.outside && !state.blocked_path => Membership::Outside,
            _ => Membership::Blocked(self.blocked.get(&unit).unwrap_or(&self.unknown)),
        }
    }

    fn block_contexts(&mut self, inventory: &SourceInventory<'_>) {
        let untranslated = super::super::contexts::untranslated_formulas_with_modules(
            inventory.theory().source(),
            &self.admitted_modules,
        );
        let graph = inventory.theory().source().source_graph();
        let dataset = inventory.theory().source().dataset();
        for &unit in &self.active {
            if !self
                .participation
                .get(&unit)
                .is_some_and(|state| state.included)
            {
                continue;
            }
            let affected = if unit.family == SyntaxFamily::Formula {
                untranslated.contains(&unit.node)
            } else {
                graph.outgoing(unit.node).iter().any(|edge| {
                    if correspondence_field(inventory, unit, edge) {
                        return false;
                    }
                    let evidence = matches!(
                        unit.family,
                        SyntaxFamily::Rule | SyntaxFamily::Atom | SyntaxFamily::Module
                    );
                    edge.role == SourceEdgeRole::Standpoint
                        || (edge.role == SourceEdgeRole::Provenance && !evidence)
                        || (edge.role == SourceEdgeRole::Context
                            && !(evidence
                                && (edge.predicate.iri(dataset)
                                    == "https://blackcatinformatics.ca/logic/confidence"
                                    || edge.has_neutral_modality(dataset))))
                })
            };
            if affected {
                self.blocked.entry(unit).or_insert_with(|| Boundary {
                    code: "CONTEXT_ERASURE_FORBIDDEN",
                    detail:
                        "selected source scope remains untranslated after exact module admission"
                            .into(),
                });
            }
        }
    }

    fn block_required_rows(&mut self, inventory: &SourceInventory<'_>) {
        let graph = inventory.theory().source().source_graph();
        for (binding, owners) in &self.syntax.rows {
            let row_node = SourceNode {
                term: binding.canonical.s,
                graph: binding.canonical.g,
            };
            let source = owner(row_node, SyntaxFamily::Schema);
            let incompatible = matches!(
                self.coordinates.get(&source),
                Some(Coordinate::Outside | Coordinate::Blocked(_))
            );
            for &unit in owners.keys() {
                let support_outside_theory = incompatible
                    || graph.outgoing(row_node).iter().any(|edge| {
                        !correspondence_field(inventory, unit, edge)
                            && matches!(
                                edge.role,
                                SourceEdgeRole::Standpoint
                                    | SourceEdgeRole::Context
                                    | SourceEdgeRole::Provenance
                            )
                    });
                if support_outside_theory
                    && self
                        .participation
                        .get(&unit)
                        .is_some_and(|state| state.included)
                {
                    self.blocked.entry(unit).or_insert_with(|| Boundary {
                        code: "REQUIRED_SUPPORT_OUTSIDE_THEORY",
                        detail: "whole owner requires source support whose module or remaining \
                                 context is not admitted"
                            .into(),
                    });
                }
            }
        }
    }

    fn lowering_masks(&mut self, inventory: &SourceInventory<'_>) {
        let theory = inventory.theory();
        for (ordinal, lowering) in theory.formula_lowerings().iter().enumerate() {
            let admitted = lowering.source.graph.is_none()
                && matches!(
                    self.owner_membership(owner(lowering.source, SyntaxFamily::Formula)),
                    Membership::Included
                );
            self.formula_occurrences[ordinal] = admitted;
            if admitted && let FormulaDisposition::Formula(index) = lowering.disposition {
                self.formulas[index] = true;
            }
        }
        for lowering in theory.owner_lowerings() {
            if lowering.family == OwnerFamily::Rule
                && let OwnerDisposition::Emitted { index } = lowering.disposition
                && lowering.source.graph.is_none()
                && matches!(
                    self.owner_membership(owner(lowering.source, SyntaxFamily::Rule)),
                    Membership::Included
                )
            {
                self.rules[index] = true;
            }
        }
        for (index, sources) in theory.axiom_sources().iter().enumerate() {
            self.axioms[index] = sources.iter().any(|source| {
                matches!(
                    self.axiom_source_membership(inventory, source),
                    Some(Membership::Included)
                )
            });
        }
    }

    fn stream_masks(&mut self, inventory: &SourceInventory<'_>) {
        let dataset = inventory.theory().source().dataset();
        for (quad, carrier) in dataset
            .quads()
            .map(|q| (q, SourceCarrier::Statement))
            .chain(
                dataset
                    .annotation_quads()
                    .map(|q| (q, SourceCarrier::Annotation)),
            )
            .chain(dataset.reifier_quads().map(|q| (q, SourceCarrier::Reifier)))
        {
            let binding = SourceStatementBinding {
                canonical: quad,
                carrier,
            };
            let admitted = if carrier == SourceCarrier::Reifier {
                inventory.documents_selected(&inventory.node_documents(SourceNode {
                    term: quad.s,
                    graph: quad.g,
                })) && quad.g.is_none()
                    && matches!(
                        self.owner_membership(owner(
                            SourceNode {
                                term: quad.s,
                                graph: quad.g
                            },
                            SyntaxFamily::Atom
                        )),
                        Membership::Included
                    )
            } else {
                !self.selection_metadata.contains_key(&binding)
                    && !self.formula_links.contains_key(&binding)
                    && inventory
                        .correspondence_record_owners(binding)
                        .next()
                        .is_none()
                    && inventory.statement_selected(quad, carrier)
                    && matches!(
                        self.row_membership(inventory, binding),
                        Membership::Included
                    )
            };
            match carrier {
                SourceCarrier::Statement => self.ordinary.push(admitted),
                SourceCarrier::Annotation => self.annotations.push(admitted),
                SourceCarrier::Reifier => self.reifiers.push(admitted),
            }
            if !admitted {
                self.excluded_rows.insert(binding);
            }
        }
    }

    pub(super) fn obligation_membership(
        &self,
        inventory: &SourceInventory<'_>,
        origin: SourceObligationOrigin,
    ) -> Membership<'_> {
        let theory = inventory.theory();
        match origin {
            SourceObligationOrigin::Statement(binding) => self.row_membership(inventory, binding),
            SourceObligationOrigin::ClassDefinition(node) => {
                self.owner_membership(owner(node, SyntaxFamily::Class))
            }
            SourceObligationOrigin::PropertyCharacteristic(node) => {
                self.owner_membership(owner(node, SyntaxFamily::Owner))
            }
            SourceObligationOrigin::Owner(index) => {
                let lowering = &theory.owner_lowerings()[index];
                self.owner_membership(owner(lowering.source, owner_family(lowering.family)))
            }
            SourceObligationOrigin::Formula(index) => self.owner_membership(owner(
                theory.formula_lowerings()[index].source,
                SyntaxFamily::Formula,
            )),
            SourceObligationOrigin::Axiom(index) => self.combine_memberships(
                theory.axiom_sources()[index]
                    .iter()
                    .filter_map(|source| self.axiom_source_membership(inventory, source)),
            ),
            SourceObligationOrigin::Quotation { reifier, graph, .. } => {
                self.owner_membership(owner(
                    SourceNode {
                        term: reifier,
                        graph,
                    },
                    SyntaxFamily::Atom,
                ))
            }
            SourceObligationOrigin::Import(index) => self.owner_membership(owner(
                theory.source().source_graph().edges()[index].source,
                SyntaxFamily::Module,
            )),
            SourceObligationOrigin::Diagnostic(index) => {
                let affected = self.diagnostic_owners(inventory, index);
                if affected.is_empty() {
                    Membership::Included
                } else {
                    self.combine(affected.into_iter())
                }
            }
            SourceObligationOrigin::MissingRule(_) | SourceObligationOrigin::SelectionIssue(_) => {
                Membership::Included
            }
        }
    }

    fn diagnostic_owners(
        &self,
        inventory: &SourceInventory<'_>,
        index: usize,
    ) -> BTreeSet<SyntaxOwner> {
        let theory = inventory.theory();
        let owners = theory
            .owner_lowerings()
            .iter()
            .filter(|lowering| lowering.diagnostics.contains(&index))
            .map(|lowering| owner(lowering.source, owner_family(lowering.family)));
        let formulas = theory.formula_lowerings().iter().filter(|lowering|
            matches!(lowering.disposition, FormulaDisposition::Malformed { diagnostic } if diagnostic == index))
            .map(|lowering| owner(lowering.source, SyntaxFamily::Formula));
        let subjects = self.active.iter().copied().filter(|unit| {
            theory.source().source_graph().diagnostic_targets_node(
                theory.source().dataset(),
                &theory.diagnostics()[index],
                unit.node,
            )
        });
        owners.chain(formulas).chain(subjects).collect()
    }

    pub(super) fn axiom_source_membership(
        &self,
        inventory: &SourceInventory<'_>,
        source: &AxiomSource,
    ) -> Option<Membership<'_>> {
        let theory = inventory.theory();
        Some(match source {
            AxiomSource::Statement {
                subject,
                predicate,
                object,
            } => {
                let quad = QuadIds {
                    s: subject.term,
                    p: *predicate,
                    o: *object,
                    g: subject.graph,
                };
                if [SourceCarrier::Statement, SourceCarrier::Annotation]
                    .into_iter()
                    .any(|carrier| {
                        let binding = SourceStatementBinding {
                            canonical: quad,
                            carrier,
                        };
                        self.selection_metadata.contains_key(&binding)
                            || inventory
                                .correspondence_record_owners(binding)
                                .next()
                                .is_some()
                    })
                {
                    return None;
                }
                let memberships = [SourceCarrier::Statement, SourceCarrier::Annotation]
                    .into_iter()
                    .filter(|carrier| {
                        source_binding_exists(theory.source().dataset(), quad, *carrier)
                    })
                    .map(|carrier| {
                        self.row_membership(
                            inventory,
                            SourceStatementBinding {
                                canonical: quad,
                                carrier,
                            },
                        )
                    });
                self.combine_memberships(memberships)
            }
            AxiomSource::Reification(node) => {
                self.owner_membership(owner(*node, SyntaxFamily::Atom))
            }
            AxiomSource::Formula(node) => {
                self.owner_membership(owner(*node, SyntaxFamily::Formula))
            }
            AxiomSource::Collection { source, .. } => {
                if !inventory
                    .statement_selected(source.constructor.canonical, source.constructor.carrier)
                {
                    Membership::Outside
                } else {
                    let node = SourceNode {
                        term: source.constructor.canonical.s,
                        graph: source.constructor.canonical.g,
                    };
                    match self.owner_membership(owner(node, source.role.owner.family())) {
                        Membership::Included => {}
                        membership => return Some(membership),
                    }
                    let Some(&id) = self.syntax.collections.get(source) else {
                        return Some(Membership::Blocked(&self.unknown));
                    };
                    self.owner_membership(owner(node, SyntaxFamily::Collection(id)))
                }
            }
            AxiomSource::RewrittenField {
                source,
                owner: role,
            } => {
                if self.selection_metadata.contains_key(source) {
                    return None;
                }
                if !inventory.statement_selected(source.canonical, source.carrier) {
                    Membership::Outside
                } else {
                    self.owner_membership(owner(
                        SourceNode {
                            term: source.canonical.s,
                            graph: source.canonical.g,
                        },
                        role.family(),
                    ))
                }
            }
            AxiomSource::ClassExpression(node) => {
                self.owner_membership(owner(*node, SyntaxFamily::Class))
            }
        })
    }

    pub(super) fn row_membership(
        &self,
        inventory: &SourceInventory<'_>,
        binding: SourceStatementBinding,
    ) -> Membership<'_> {
        let quad = binding.canonical;
        if quad.g.is_some() {
            return Membership::Blocked(&self.graph_boundary);
        }
        if let Some(link) = self.formula_links.get(&binding) {
            return self.formula_link_membership(link);
        }
        if let Some(owners) = self.selection_metadata.get(&binding) {
            return self.combine(owners.iter().copied());
        }
        if inventory
            .correspondence_record_owners(binding)
            .next()
            .is_some()
        {
            let mut owners: BTreeSet<_> = inventory
                .correspondence_record_owners(binding)
                .map(|ordinal| {
                    owner(
                        inventory.theory().owner_lowerings()[ordinal].source,
                        SyntaxFamily::Correspondence,
                    )
                })
                .collect();
            if let Some(syntax) = self.syntax.rows.get(&binding) {
                owners.extend(
                    syntax
                        .keys()
                        .filter(|unit| self.active.contains(unit))
                        .copied(),
                );
            }
            let dataset = inventory.theory().source().dataset();
            let node = SourceNode {
                term: quad.s,
                graph: quad.g,
            };
            if let Some(family) = intrinsic_family(dataset, quad) {
                let unit = owner(node, family);
                if self.active.contains(&unit) {
                    owners.insert(unit);
                }
            }
            if let TermRef::Iri(predicate) = dataset.resolve(quad.p) {
                if let Some(families) = structural_families(predicate) {
                    owners.extend(
                        families
                            .iter()
                            .map(|&family| owner(node, family))
                            .filter(|unit| self.active.contains(unit)),
                    );
                }
                if matches!(
                    predicate,
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        | "https://blackcatinformatics.ca/logic/instanceOf"
                ) && let TermRef::Iri(marker) = dataset.resolve(quad.o)
                    && let Some(kind) =
                        crate::frontend::StructuralSourceGraph::declaration_kind(marker)
                {
                    let unit = owner(node, declaration_family(kind));
                    if self.active.contains(&unit) {
                        owners.insert(unit);
                    }
                }
            }
            return self.combine(owners.into_iter());
        }
        if let Some(owners) = self.syntax.rows.get(&binding) {
            let mut active = owners
                .keys()
                .filter(|unit| self.active.contains(unit))
                .copied()
                .peekable();
            if active.peek().is_some() {
                let source = owner(
                    SourceNode {
                        term: quad.s,
                        graph: quad.g,
                    },
                    SyntaxFamily::Schema,
                );
                if matches!(self.coordinates.get(&source), Some(Coordinate::Outside)) {
                    return Membership::Outside;
                }
                if let Some(Coordinate::Blocked(boundary)) = self.coordinates.get(&source) {
                    return Membership::Blocked(boundary);
                }
                return self.combine(active);
            }
        }
        let dataset = inventory.theory().source().dataset();
        let node = SourceNode {
            term: quad.s,
            graph: quad.g,
        };
        if let Some(family) = intrinsic_family(dataset, quad) {
            return self.owner_membership(owner(node, family));
        }
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            return Membership::Blocked(&self.unknown);
        };
        if matches!(
            predicate,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                | "https://blackcatinformatics.ca/logic/instanceOf"
        ) && let TermRef::Iri(marker) = dataset.resolve(quad.o)
            && let Some(kind) = crate::frontend::StructuralSourceGraph::declaration_kind(marker)
        {
            return self.owner_membership(owner(node, declaration_family(kind)));
        }
        if let Some(families) = structural_families(predicate) {
            let mut active = families
                .iter()
                .copied()
                .map(|family| owner(node, family))
                .filter(|unit| self.active.contains(unit))
                .peekable();
            if active.peek().is_some() {
                return self.combine(active);
            }
            return Membership::Blocked(&self.unknown);
        }
        // A plainly asserted domain row is its own top-level sentence. Merely
        // describing the same resource or a scoped reifier does not own it.
        Membership::Included
    }

    fn formula_link_module(&self, link: &ModuleFormulaLink) -> Membership<'_> {
        match self.coordinates.get(&link.module) {
            Some(Coordinate::None) => Membership::Blocked(&self.top_level_module_link),
            _ => self.owner_membership(link.module),
        }
    }

    fn formula_link_membership<'a>(&'a self, link: &'a ModuleFormulaLink) -> Membership<'a> {
        match self.formula_link_module(link) {
            Membership::Included => {}
            membership => return membership,
        }
        match &link.target {
            Ok(target) => self.owner_membership(target.owner),
            Err(boundary) => Membership::Blocked(boundary),
        }
    }

    pub(super) fn selected_module_formula(&self, binding: SourceStatementBinding) -> Option<usize> {
        let link = self.formula_links.get(&binding)?;
        if !matches!(self.formula_link_membership(link), Membership::Included) {
            return None;
        }
        link.target.as_ref().ok().map(|target| target.ordinal)
    }

    fn combine(&self, owners: impl Iterator<Item = SyntaxOwner>) -> Membership<'_> {
        self.combine_memberships(owners.map(|unit| self.owner_membership(unit)))
    }

    pub(super) fn correspondence_record_selected(
        &self,
        inventory: &SourceInventory<'_>,
        binding: SourceStatementBinding,
    ) -> bool {
        inventory
            .correspondence_record_owners(binding)
            .any(|ordinal| {
                matches!(
                    self.owner_membership(owner(
                        inventory.theory().owner_lowerings()[ordinal].source,
                        SyntaxFamily::Correspondence
                    )),
                    Membership::Included
                )
            })
    }

    fn combine_memberships<'a>(
        &'a self,
        memberships: impl Iterator<Item = Membership<'a>>,
    ) -> Membership<'a> {
        let mut blocked = None;
        let mut outside = false;
        for membership in memberships {
            match membership {
                Membership::Included => return Membership::Included,
                Membership::Outside => outside = true,
                Membership::Blocked(boundary) => {
                    blocked = Some(boundary);
                }
            }
        }
        if let Some(boundary) = blocked {
            Membership::Blocked(boundary)
        } else if outside {
            Membership::Outside
        } else {
            Membership::Blocked(&self.unknown)
        }
    }
}

fn source_binding_exists(
    dataset: &purrdf::RdfDataset,
    quad: QuadIds,
    carrier: SourceCarrier,
) -> bool {
    match carrier {
        SourceCarrier::Statement => dataset
            .quads_for_pattern(
                Some(quad.s),
                Some(quad.p),
                Some(quad.o),
                quad.g.map_or(GraphMatch::Default, GraphMatch::Named),
            )
            .next()
            .is_some(),
        SourceCarrier::Annotation => dataset
            .annotations_of_with_graph(quad.s)
            .any(|(p, o, g)| p == quad.p && o == quad.o && g == quad.g),
        SourceCarrier::Reifier => false,
    }
}

fn coordinate(
    inventory: &SourceInventory<'_>,
    unit: SyntaxOwner,
    composed: bool,
    modules: &BTreeSet<TermId>,
    available: &BTreeSet<TermId>,
    selection_metadata: &mut BTreeMap<SourceStatementBinding, BTreeSet<SyntaxOwner>>,
) -> Coordinate {
    let graph = inventory.theory().source().source_graph();
    let dataset = inventory.theory().source().dataset();
    if unit.node.graph.is_some() {
        return Coordinate::Blocked(Boundary {
            code: "OUTSIDE_SELECTED_DEFAULT_GRAPH",
            detail: "named-graph syntax has no translation into the selected default logical world"
                .into(),
        });
    }
    let targets: BTreeSet<_> = graph
        .outgoing(unit.node)
        .iter()
        .filter(|edge| edge.role == SourceEdgeRole::Module)
        .map(|edge| edge.target)
        .collect();
    if !composed {
        return if targets.is_empty() {
            Coordinate::None
        } else {
            Coordinate::Blocked(Boundary {
                code: "CONTEXT_ERASURE_FORBIDDEN",
                detail: "module scope remains untranslated in the top-level theory".into(),
            })
        };
    }
    if unit.family == SyntaxFamily::Module {
        if !available.contains(&unit.node.term) {
            return Coordinate::Blocked(Boundary {
                code: "UNDECLARED_IMPORT_OWNER",
                detail: "an import owner requires an actual supplied module declaration".into(),
            });
        }
        return if modules.contains(&unit.node.term) {
            Coordinate::Included
        } else {
            Coordinate::Outside
        };
    }
    if targets.is_empty() {
        return Coordinate::None;
    }
    if unit.node.graph.is_some() || targets.len() != 1 {
        return Coordinate::Blocked(Boundary {
            code: "UNTRANSLATED_MODULE_COORDINATE",
            detail: "module membership requires one exact default-graph module coordinate".into(),
        });
    }
    let target = *targets.first().expect("nonempty module target set");
    if !matches!(dataset.resolve(target), TermRef::Iri(_)) || !available.contains(&target) {
        return Coordinate::Blocked(Boundary {
            code: "UNAVAILABLE_MODULE_COORDINATE",
            detail: "module coordinate lacks an actual supplied native IRI declaration".into(),
        });
    }
    // Only the validated coordinate's actual physical inputs are selection
    // metadata. Their owner still determines inclusion, exclusion or blockage.
    for edge in graph
        .outgoing(unit.node)
        .iter()
        .filter(|edge| edge.role == SourceEdgeRole::Module)
    {
        if let SourcePredicate::Stored(predicate) = edge.predicate {
            let binding = SourceStatementBinding {
                canonical: QuadIds {
                    s: edge.source.term,
                    p: predicate,
                    o: edge.target,
                    g: edge.source.graph,
                },
                carrier: edge.carrier,
            };
            if inventory.statement_selected(binding.canonical, binding.carrier) {
                selection_metadata.entry(binding).or_default().insert(unit);
            }
        }
    }
    if modules.contains(&target) {
        Coordinate::Included
    } else {
        Coordinate::Outside
    }
}

fn intrinsic_family(dataset: &purrdf::RdfDataset, quad: QuadIds) -> Option<SyntaxFamily> {
    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
        return None;
    };
    if predicate == "https://blackcatinformatics.ca/logic/imports" {
        return Some(SyntaxFamily::Module);
    }
    if matches!(
        predicate,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            | "https://blackcatinformatics.ca/logic/instanceOf"
    ) && let TermRef::Iri(marker) = dataset.resolve(quad.o)
        && let Some(role) = gmeow_ns::schema_type_role(marker)
    {
        return Some(match role {
            gmeow_ns::SchemaTypeRole::Class | gmeow_ns::SchemaTypeRole::ClassExpression => {
                SyntaxFamily::Class
            }
            gmeow_ns::SchemaTypeRole::PropertySignature
            | gmeow_ns::SchemaTypeRole::PropertyCharacteristic
            | gmeow_ns::SchemaTypeRole::DisjointClasses
            | gmeow_ns::SchemaTypeRole::DisjointProperties
            | gmeow_ns::SchemaTypeRole::DistinctIndividuals => SyntaxFamily::Schema,
        });
    }
    gmeow_ns::schema_predicate_role(predicate).and_then(|role| {
        if matches!(
            gmeow_ns::canonical_calculus_symbol(predicate),
            Some(
                "https://blackcatinformatics.ca/logic/onClass"
                    | "https://blackcatinformatics.ca/logic/onDataRange"
            )
        ) {
            return None;
        }
        if matches!(
            role,
            gmeow_ns::SchemaPredicateRole::ClassList
                | gmeow_ns::SchemaPredicateRole::ValueList
                | gmeow_ns::SchemaPredicateRole::PropertyList
                | gmeow_ns::SchemaPredicateRole::FacetList
                | gmeow_ns::SchemaPredicateRole::Members
                | gmeow_ns::SchemaPredicateRole::DistinctMembers
        ) {
            return crate::frontend::collection_roles(
                dataset,
                SourceNode {
                    term: quad.s,
                    graph: quad.g,
                },
                predicate,
            )
            .into_iter()
            .flatten()
            .next()
            .map(|role| role.owner.family());
        }
        Some(if role == gmeow_ns::SchemaPredicateRole::Signature {
            SyntaxFamily::Schema
        } else {
            SyntaxFamily::Class
        })
    })
}

fn declaration_family(kind: SourceUnitKind) -> SyntaxFamily {
    match kind {
        SourceUnitKind::Formula => SyntaxFamily::Formula,
        SourceUnitKind::Rule => SyntaxFamily::Rule,
        SourceUnitKind::TermCarrier | SourceUnitKind::FunctionTerm => SyntaxFamily::Term,
        SourceUnitKind::ClassExpression => SyntaxFamily::Class,
        SourceUnitKind::ReifiedAssertion | SourceUnitKind::QuotedStatement => SyntaxFamily::Atom,
        SourceUnitKind::Module => SyntaxFamily::Module,
        SourceUnitKind::Correspondence => SyntaxFamily::Correspondence,
        _ => SyntaxFamily::Owner,
    }
}

/// A schema law about a resource cannot authorize that resource's unrelated
/// formula, term or rule grammar. Shared fields name only their real readers.
fn structural_families(predicate: &str) -> Option<&'static [SyntaxFamily]> {
    if matches!(
        predicate,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#object"
    ) {
        return Some(&[SyntaxFamily::Atom]);
    }
    if let Some(canonical) = gmeow_ns::canonical_calculus_symbol(predicate)
        && matches!(
            canonical,
            "https://blackcatinformatics.ca/logic/onClass"
                | "https://blackcatinformatics.ca/logic/onDataRange"
        )
    {
        return Some(&[SyntaxFamily::Class, SyntaxFamily::Owner]);
    }
    let local = predicate.strip_prefix("https://blackcatinformatics.ca/logic/");
    if let Some(local) = local {
        if local == "hasLens" {
            return Some(&[SyntaxFamily::Correspondence]);
        }
        if matches!(
            local,
            "termIndex"
                | "termIri"
                | "termVariable"
                | "termLiteral"
                | "termLiteralDatatype"
                | "termSequenceMarker"
                | "termApplication"
                | "functionSymbol"
        ) {
            return Some(&[SyntaxFamily::Term]);
        }
        if local == "argument" {
            return Some(&[
                SyntaxFamily::Formula,
                SyntaxFamily::Atom,
                SyntaxFamily::Term,
            ]);
        }
        if matches!(
            local,
            "relation" | "overAccessibility" | "inContext" | "quantifiedVariable"
        ) || crate::frontend::FORMULA_SUBLINKS.contains(&local)
        {
            return Some(&[SyntaxFamily::Formula]);
        }
        if matches!(
            local,
            "head"
                | "body"
                | "negatedBody"
                | "distinctBody"
                | "aggregateFunction"
                | "aggregateVariable"
                | "aggregateResult"
                | "groupKey"
        ) {
            return Some(&[SyntaxFamily::Rule]);
        }
    }
    match crate::frontend::StructuralSourceGraph::predicate_role(predicate) {
        Some(
            SourceEdgeRole::Module
            | SourceEdgeRole::Standpoint
            | SourceEdgeRole::Context
            | SourceEdgeRole::Provenance,
        ) => Some(&[
            SyntaxFamily::Formula,
            SyntaxFamily::Rule,
            SyntaxFamily::Atom,
            SyntaxFamily::Term,
            SyntaxFamily::Class,
            SyntaxFamily::Owner,
            SyntaxFamily::Correspondence,
            SyntaxFamily::Schema,
            SyntaxFamily::Module,
            SyntaxFamily::ClassList,
            SyntaxFamily::ValueList,
            SyntaxFamily::PropertyList,
            SyntaxFamily::FacetList,
            SyntaxFamily::UninterpretedList,
        ]),
        Some(_) => Some(&[SyntaxFamily::Owner]),
        None => None,
    }
}
