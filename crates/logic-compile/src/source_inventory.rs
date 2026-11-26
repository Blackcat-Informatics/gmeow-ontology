// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One immutable source inventory shared by independent admission profiles.
//!
//! Rows contain only invocation-local source coordinates and lowering indexes.
//! Portable anchors and document sets are rendered on demand. A declaration or
//! lowering trace is an obligation, never evidence that an engine executed it.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{QuadIds, RdfDataset, TermId, TermRef};

use crate::frontend::{
    AxiomSource, CollectionField, CollectionRole, CompiledTheory, DocumentSelection,
    FormulaDisposition, OwnerDisposition, OwnerFamily, SemanticUnitKind, SourceCarrier,
    SourceEdgeRole, SourceNode, SourceSelection, SourceStatementBinding, SourceUnitKind,
};

mod contexts;
mod identity;
mod origins;
mod ownership;
mod selection;
mod trace;

pub(crate) use ownership::{
    CollectionId, SyntaxEvidence, SyntaxFamily, SyntaxIssue, SyntaxOwner, SyntaxOwnership,
};
pub use selection::{
    ModuleImportObservation, ResolvedTheorySelection, SelectedProgramView, SelectedSourceView,
    TheorySelectionObservation, TheoryUnitSelection,
};

pub(crate) use identity::{
    XSD_STRING, assertion_from_quad, atomic_term, class_field, digest_bytes, frame, node_anchor,
    quad_anchor, single_iri_object, source_digest, term_anchor,
};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const SOURCE_OBLIGATION_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-source-obligation-v1\0");

pub(crate) fn source_statements(
    dataset: &RdfDataset,
) -> impl Iterator<Item = (purrdf::QuadIds, SourceCarrier)> + '_ {
    dataset
        .quads()
        .map(|quad| (quad, SourceCarrier::Statement))
        .chain(
            dataset
                .annotations_with_graph()
                .map(|(s, p, o, g)| (purrdf::QuadIds { s, p, o, g }, SourceCarrier::Annotation)),
        )
}

/// Invocation-local ownership of one immutable compiler source. A portable digest
/// describes content; this opaque token identifies the actual prepared input.
#[derive(Debug, Clone)]
pub struct SourceIdentity(std::sync::Arc<()>);

impl SourceIdentity {
    pub(crate) fn new() -> Self {
        Self(std::sync::Arc::new(()))
    }
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

/// An exact source obligation, independent of any profile's disposition.
/// IDs within these coordinates are valid only for the inventory's compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceObligationOrigin {
    ClassDefinition(SourceNode),
    Owner(usize),
    MissingRule(usize),
    Statement(SourceStatementBinding),
    Quotation {
        reifier: TermId,
        triple: TermId,
        graph: Option<TermId>,
    },
    Import(usize),
    PropertyCharacteristic(SourceNode),
    Formula(usize),
    Axiom(usize),
    Diagnostic(usize),
    SelectionIssue(usize),
}

/// A malformed document selection retained as an obligation of every profile.
pub struct SourceSelectionIssue {
    pub anchor: String,
    pub code: &'static str,
    pub reason: String,
}

pub(crate) struct SourceDeclaration {
    pub disposition: crate::frontend::SemanticUnitDisposition,
    pub support: BTreeSet<SourceStatementBinding>,
}

fn selection_issues(
    theory: &CompiledTheory,
    selection: &SourceSelection,
) -> Vec<SourceSelectionIssue> {
    let available: BTreeSet<_> = theory
        .source()
        .origins()
        .iter()
        .map(|origin| origin.document.path.as_str())
        .collect();
    let issue = |code, reason: &str| SourceSelectionIssue {
        anchor: "selection:documents".into(),
        code,
        reason: reason.into(),
    };
    match &selection.documents {
        DocumentSelection::AnonymousSource if !available.is_empty() => vec![issue(
            "DOCUMENT_SELECTION_MISMATCH",
            "anonymous selection cannot address a source with document receipts",
        )],
        DocumentSelection::Exact(_) if available.is_empty() => vec![issue(
            "DOCUMENT_SELECTION_MISMATCH",
            "path selection cannot address an anonymous source",
        )],
        DocumentSelection::Exact(paths) => {
            let mut issues: Vec<_> = paths
                .iter()
                .filter(|path| !available.contains(path.as_str()))
                .map(|missing| SourceSelectionIssue {
                    anchor: format!("selection:document:{missing}"),
                    code: "UNKNOWN_SELECTED_DOCUMENT",
                    reason: format!(
                        "selected document {missing:?} is absent from the source receipts"
                    ),
                })
                .collect();
            if paths.is_empty() {
                issues.push(issue(
                    "EMPTY_DOCUMENT_SELECTION",
                    "an exact document selection must name at least one document",
                ));
            }
            issues
        }
        DocumentSelection::AnonymousSource => Vec::new(),
    }
}

/// Profile-independent identity of an authored obligation and its lowering trace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct SourceObligationId(String);

impl SourceObligationId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Borrowed compilation, one occurrence index and compact obligation coordinates.
/// No parsed dataset, formula tree or portable admission row is copied here.
pub struct SourceInventory<'source> {
    theory: &'source CompiledTheory,
    origins: origins::OriginIndex<'source>,
    selection: SourceSelection,
    source_digest: String,
    program_digest: String,
    trace_digest: String,
    selection_digest: String,
    obligations: Vec<SourceObligationOrigin>,
    declared_classes: BTreeSet<String>,
    declared_builtins: BTreeSet<String>,
    selection_issues: Vec<SourceSelectionIssue>,
    declarations: BTreeMap<(SourceNode, usize), SourceDeclaration>,
    correspondence_records: BTreeMap<SourceStatementBinding, BTreeSet<usize>>,
    forbidden_pattern_records: BTreeMap<SourceStatementBinding, usize>,
    collection_constructors: BTreeMap<SourceStatementBinding, BTreeSet<CollectionRole>>,
    untranslated_formula_contexts: BTreeSet<SourceNode>,
}

impl<'source> SourceInventory<'source> {
    /// Enumerate every source pass used by complete admission, not just typed nodes.
    #[must_use]
    pub fn new(theory: &'source CompiledTheory, selection: SourceSelection) -> Self {
        let mut inventory = Self {
            theory,
            origins: origins::OriginIndex::new(theory.source().origins()),
            selection_digest: selection.digest(),
            selection_issues: selection_issues(theory, &selection),
            selection,
            source_digest: source_digest(theory),
            program_digest: digest_bytes(
                b"gmeow-logic-program-v1\0",
                theory.program().canonical_key().as_bytes(),
            ),
            trace_digest: trace::digest(theory),
            obligations: Vec::new(),
            declared_classes: BTreeSet::new(),
            declared_builtins: BTreeSet::new(),
            declarations: BTreeMap::new(),
            correspondence_records: BTreeMap::new(),
            forbidden_pattern_records: BTreeMap::new(),
            collection_constructors: BTreeMap::new(),
            untranslated_formula_contexts: contexts::untranslated_formulas(theory.source()),
        };
        let dataset = theory.source().dataset();
        let mut classes = BTreeSet::new();
        let mut characteristics = BTreeSet::new();
        for unit in theory.source().source_graph().units() {
            if unit
                .declared_kinds
                .contains(&SourceUnitKind::ClassExpression)
            {
                classes.insert(unit.node);
            }
            if unit
                .declared_kinds
                .contains(&SourceUnitKind::PropertyCharacteristicAssertion)
            {
                characteristics.insert(unit.node);
            }
        }
        for source in theory.axiom_sources().iter().flatten() {
            if let Some(node) = source.class_definition() {
                classes.insert(node);
            }
            if let AxiomSource::Collection {
                source,
                field: CollectionField::Constructor,
            } = source
            {
                inventory
                    .collection_constructors
                    .entry(source.constructor)
                    .or_default()
                    .insert(source.role);
            }
        }
        for (quad, carrier) in source_statements(dataset) {
            inventory
                .obligations
                .push(SourceObligationOrigin::Statement(SourceStatementBinding {
                    canonical: quad,
                    carrier,
                }));
            if quad.g.is_some() {
                continue;
            }
            if carrier == SourceCarrier::Statement
                && let TermRef::Iri(predicate) = dataset.resolve(quad.p)
                && predicate.strip_prefix(LOGIC).is_some_and(|local| {
                    class_field(local) && !matches!(local, "onClass" | "onDataRange")
                })
            {
                classes.insert(SourceNode {
                    term: quad.s,
                    graph: None,
                });
            }
            if carrier != SourceCarrier::Statement || !inventory.statement_selected(quad, carrier) {
                continue;
            }
            if let (TermRef::Iri(predicate), TermRef::Iri(object), TermRef::Iri(subject)) = (
                dataset.resolve(quad.p),
                dataset.resolve(quad.o),
                dataset.resolve(quad.s),
            ) && matches!(
                predicate,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    | "https://blackcatinformatics.ca/logic/instanceOf"
            ) {
                if matches!(
                    object,
                    "https://blackcatinformatics.ca/logic/Class"
                        | "http://www.w3.org/2002/07/owl#Class"
                        | "http://www.w3.org/2000/01/rdf-schema#Class"
                ) {
                    inventory.declared_classes.insert(subject.to_owned());
                }
                if object == "https://blackcatinformatics.ca/logic/Builtin" {
                    inventory.declared_builtins.insert(subject.to_owned());
                }
            }
        }
        inventory.obligations.extend(
            classes
                .into_iter()
                .map(SourceObligationOrigin::ClassDefinition),
        );
        inventory.obligations.extend(
            characteristics
                .into_iter()
                .map(SourceObligationOrigin::PropertyCharacteristic),
        );
        inventory
            .obligations
            .extend(
                dataset
                    .reifiers_with_graph()
                    .map(
                        |(reifier, triple, graph)| SourceObligationOrigin::Quotation {
                            reifier,
                            triple,
                            graph,
                        },
                    ),
            );
        inventory.obligations.extend(
            theory
                .source()
                .source_graph()
                .edges()
                .iter()
                .enumerate()
                .filter(|(_, edge)| edge.role == SourceEdgeRole::Import)
                .map(|(index, _)| SourceObligationOrigin::Import(index)),
        );
        let mut seen_rules = BTreeSet::new();
        for (index, lowering) in theory.owner_lowerings().iter().enumerate() {
            inventory
                .obligations
                .push(SourceObligationOrigin::Owner(index));
            if lowering.family == OwnerFamily::Correspondence
                && matches!(lowering.disposition, OwnerDisposition::Emitted { .. })
            {
                for binding in &lowering.record_support {
                    inventory
                        .correspondence_records
                        .entry(*binding)
                        .or_default()
                        .insert(index);
                }
            }
            if inventory.is_forbidden_pattern(index) {
                inventory.forbidden_pattern_records.extend(
                    lowering
                        .record_support
                        .iter()
                        .map(|&binding| (binding, index)),
                );
            }
            if lowering.family == OwnerFamily::Rule
                && let OwnerDisposition::Emitted { index } = &lowering.disposition
            {
                seen_rules.insert(*index);
            }
            if lowering.family == OwnerFamily::Contract
                && let OwnerDisposition::Emitted { index } = &lowering.disposition
            {
                let declaration = inventory.read_contract_declaration(lowering.source, *index);
                inventory
                    .declarations
                    .insert((lowering.source, *index), declaration);
            }
        }
        inventory.obligations.extend(
            (0..theory.program().rules.len())
                .filter(|index| !seen_rules.contains(index))
                .map(SourceObligationOrigin::MissingRule),
        );
        inventory
            .obligations
            .extend((0..theory.formula_lowerings().len()).map(SourceObligationOrigin::Formula));
        let formula_axioms: BTreeSet<_> = theory
            .formula_lowerings()
            .iter()
            .filter_map(|lowering| match lowering.disposition {
                FormulaDisposition::Axiom(index) => Some(index),
                _ => None,
            })
            .collect();
        inventory.obligations.extend(
            theory
                .axiom_sources()
                .iter()
                .enumerate()
                .filter(|(index, sources)| {
                    !formula_axioms.contains(index)
                        && !sources.iter().all(|source| {
                            matches!(
                                source,
                                AxiomSource::Statement { .. }
                                    | AxiomSource::Collection {
                                        field: CollectionField::Constructor,
                                        ..
                                    }
                            )
                        })
                })
                .map(|(index, _)| SourceObligationOrigin::Axiom(index)),
        );
        inventory
            .obligations
            .extend((0..theory.diagnostics().len()).map(SourceObligationOrigin::Diagnostic));
        inventory.obligations.extend(
            (0..inventory.selection_issues.len()).map(SourceObligationOrigin::SelectionIssue),
        );
        inventory
    }

    #[must_use]
    pub fn theory(&self) -> &'source CompiledTheory {
        self.theory
    }
    #[must_use]
    pub fn selection(&self) -> &SourceSelection {
        &self.selection
    }

    pub(crate) fn collection_roles(
        &self,
        binding: SourceStatementBinding,
    ) -> impl Iterator<Item = CollectionRole> + '_ {
        self.collection_constructors
            .get(&binding)
            .into_iter()
            .flat_map(|roles| roles.iter().copied())
    }
    #[must_use]
    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }
    #[must_use]
    pub fn program_digest(&self) -> &str {
        &self.program_digest
    }
    #[must_use]
    pub fn selection_digest(&self) -> &str {
        &self.selection_digest
    }
    pub fn obligations(&self) -> impl ExactSizeIterator<Item = SourceObligation<'_>> {
        self.obligations.iter().map(|origin| SourceObligation {
            inventory: self,
            origin: *origin,
        })
    }

    /// Original obligation identity of a compiler formula-lowering occurrence.
    #[must_use]
    pub fn formula_obligation_id(&self, ordinal: usize) -> Option<SourceObligationId> {
        self.theory.formula_lowerings().get(ordinal)?;
        Some(self.identity(SourceObligationOrigin::Formula(ordinal)))
    }
    #[must_use]
    pub fn selection_issues(&self) -> &[SourceSelectionIssue] {
        &self.selection_issues
    }

    /// Actual successful correspondence owners of this exact physical record row.
    /// The ordinals address this inventory's immutable compiler trace, not a
    /// same-subject declaration or a detached serialized observation.
    pub fn correspondence_record_owners(
        &self,
        binding: SourceStatementBinding,
    ) -> impl Iterator<Item = usize> + '_ {
        self.correspondence_records
            .get(&binding)
            .into_iter()
            .flat_map(|owners| owners.iter().copied())
    }

    pub(crate) fn forbidden_pattern_owner(&self, binding: SourceStatementBinding) -> Option<usize> {
        self.forbidden_pattern_records.get(&binding).copied()
    }

    pub(crate) fn is_forbidden_pattern(&self, ordinal: usize) -> bool {
        let lowering = &self.theory.owner_lowerings()[ordinal];
        let dataset = self.theory.source().dataset();
        const MARKER: &str = "https://blackcatinformatics.ca/logic/ForbiddenPatternConstraint";
        lowering.family == OwnerFamily::Constraint
            && matches!(lowering.disposition, OwnerDisposition::Emitted { .. })
            && lowering.record_support.iter().any(|binding| {
                let quad = binding.canonical;
                matches!(dataset.resolve(quad.o), TermRef::Iri(MARKER))
                    && matches!(dataset.resolve(quad.p), TermRef::Iri(predicate)
                        if crate::graphutil::is_structural_type_predicate(predicate, MARKER))
            })
    }
    #[must_use]
    pub(crate) fn contract_declaration(
        &self,
        root: SourceNode,
        index: usize,
    ) -> Option<(
        &crate::frontend::SemanticUnitDisposition,
        &BTreeSet<SourceStatementBinding>,
    )> {
        self.declarations
            .get(&(root, index))
            .map(|record| (&record.disposition, &record.support))
    }
    /// Source contexts not represented by the formula IR, including descendants.
    #[must_use]
    pub fn untranslated_formula_contexts(&self) -> &BTreeSet<SourceNode> {
        &self.untranslated_formula_contexts
    }
    /// Read the exact native terms of a statement without reparsing the source.
    fn assertion(&self, quad: QuadIds) -> gmeow_errors::Result<crate::frontend::SourceAssertion> {
        assertion_from_quad(self.theory.source().dataset(), quad)
    }
    /// Source grammar belongs to its inventoried semantic owner, not a second fact.
    #[must_use]
    fn is_structural_statement(&self, quad: QuadIds, carrier: SourceCarrier) -> bool {
        if self.declarations.values().any(|declaration| {
            declaration.support.contains(&SourceStatementBinding {
                canonical: quad,
                carrier,
            })
        }) {
            return true;
        }
        let dataset = self.theory.source().dataset();
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            return true;
        };
        if crate::frontend::StructuralSourceGraph::predicate_role(predicate).is_some() {
            return true;
        }
        let node = SourceNode {
            term: quad.s,
            graph: quad.g,
        };
        self.theory
            .source()
            .source_graph()
            .unit(node)
            .is_some_and(|unit| {
                matches!(
                    predicate,
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        | "https://blackcatinformatics.ca/logic/instanceOf"
                ) && unit.declarations.contains(&quad.o)
            })
    }
    #[must_use]
    pub(crate) fn declared_classes(&self) -> &BTreeSet<String> {
        &self.declared_classes
    }
    #[must_use]
    pub(crate) fn declared_builtins(&self) -> &BTreeSet<String> {
        &self.declared_builtins
    }

    #[must_use]
    pub fn documents_selected(&self, documents: &BTreeSet<String>) -> bool {
        match &self.selection.documents {
            DocumentSelection::AnonymousSource => self.theory.source().origins().is_empty(),
            DocumentSelection::Exact(paths) => documents.iter().any(|path| paths.contains(path)),
        }
    }
    #[must_use]
    pub fn node_documents(&self, node: SourceNode) -> BTreeSet<String> {
        self.origins.node_documents(node)
    }
    #[must_use]
    pub fn statement_documents(&self, quad: QuadIds, carrier: SourceCarrier) -> BTreeSet<String> {
        self.origins.statement_documents(SourceStatementBinding {
            canonical: quad,
            carrier,
        })
    }
    #[must_use]
    pub fn statement_selected(&self, quad: QuadIds, carrier: SourceCarrier) -> bool {
        self.documents_selected(&self.statement_documents(quad, carrier))
    }

    /// Portable source anchor; graph and storage carrier remain distinct coordinates.
    #[must_use]
    fn anchor(&self, obligation: SourceObligationOrigin) -> String {
        let dataset = self.theory.source().dataset();
        match obligation {
            SourceObligationOrigin::ClassDefinition(node)
            | SourceObligationOrigin::PropertyCharacteristic(node) => node_anchor(dataset, node),
            SourceObligationOrigin::Owner(index) => {
                node_anchor(dataset, self.theory.owner_lowerings()[index].source)
            }
            SourceObligationOrigin::MissingRule(index) => format!("program:rule:{index}"),
            SourceObligationOrigin::Statement(binding) => match binding.carrier {
                SourceCarrier::Statement => quad_anchor(dataset, binding.canonical),
                SourceCarrier::Annotation => {
                    frame("annotation", [quad_anchor(dataset, binding.canonical)])
                }
                SourceCarrier::Reifier => {
                    frame("reifier-row", [quad_anchor(dataset, binding.canonical)])
                }
            },
            SourceObligationOrigin::Quotation {
                reifier, triple, ..
            } => format!(
                "reifier|{}|{}",
                term_anchor(dataset, dataset.resolve(reifier)),
                term_anchor(dataset, dataset.resolve(triple))
            ),
            SourceObligationOrigin::Import(index) => {
                let edge = &self.theory.source().source_graph().edges()[index];
                format!(
                    "import|{}|{}",
                    node_anchor(dataset, edge.source),
                    term_anchor(dataset, dataset.resolve(edge.target))
                )
            }
            SourceObligationOrigin::Formula(index) => {
                node_anchor(dataset, self.theory.formula_lowerings()[index].source)
            }
            SourceObligationOrigin::Axiom(index) => format!("program:axiom:{index}"),
            SourceObligationOrigin::Diagnostic(index) => format!("compiler-diagnostic:{index}"),
            SourceObligationOrigin::SelectionIssue(index) => {
                self.selection_issues[index].anchor.clone()
            }
        }
    }

    /// Identity includes source occurrences and compiler trace, never profile judgment.
    #[must_use]
    fn identity(&self, obligation: SourceObligationOrigin) -> SourceObligationId {
        #[derive(serde::Serialize)]
        enum TraceCoordinate<'a> {
            Source {
                kind: SemanticUnitKind,
            },
            Owner {
                ordinal: usize,
            },
            Formula {
                ordinal: usize,
            },
            Axiom {
                ordinal: usize,
            },
            Quotation {
                graph: Option<String>,
            },
            Selection {
                selection: &'a str,
                code: &'a str,
                reason: &'a str,
            },
        }
        #[derive(serde::Serialize)]
        struct Identity<'a> {
            schema_version: u32,
            source: &'a str,
            program: &'a str,
            compiler_trace: &'a str,
            anchor: String,
            documents: BTreeSet<String>,
            coordinate: TraceCoordinate<'a>,
        }
        let trace = match obligation {
            SourceObligationOrigin::Owner(ordinal) => TraceCoordinate::Owner { ordinal },
            SourceObligationOrigin::Formula(ordinal) => TraceCoordinate::Formula { ordinal },
            SourceObligationOrigin::Axiom(ordinal) => TraceCoordinate::Axiom { ordinal },
            SourceObligationOrigin::Quotation { graph, .. } => TraceCoordinate::Quotation {
                graph: graph.map(|graph| {
                    term_anchor(
                        self.theory.source().dataset(),
                        self.theory.source().dataset().resolve(graph),
                    )
                }),
            },
            SourceObligationOrigin::SelectionIssue(index) => TraceCoordinate::Selection {
                selection: self.selection_digest(),
                code: self.selection_issues[index].code,
                reason: &self.selection_issues[index].reason,
            },
            other => TraceCoordinate::Source {
                kind: self.kind(other),
            },
        };
        let mut hasher = purrdf_hash::blake3::Hasher::new();
        hasher.update(SOURCE_OBLIGATION_DOMAIN.as_bytes());
        serde_json::to_writer(
            &mut hasher,
            &Identity {
                schema_version: 1,
                source: self.source_digest(),
                program: self.program_digest(),
                compiler_trace: &self.trace_digest,
                anchor: self.anchor(obligation),
                documents: self.documents(obligation),
                coordinate: trace,
            },
        )
        .expect("source obligation fields serialize to an infallible digest sink");
        SourceObligationId(purrdf_hash::hex::encode(hasher.finalize().as_bytes()))
    }

    #[must_use]
    fn kind(&self, obligation: SourceObligationOrigin) -> SemanticUnitKind {
        match obligation {
            SourceObligationOrigin::ClassDefinition(_) => SemanticUnitKind::ClassDefinition,
            SourceObligationOrigin::Owner(index)
                if self.theory.owner_lowerings()[index].family == OwnerFamily::Rule =>
            {
                SemanticUnitKind::Rule
            }
            SourceObligationOrigin::Owner(_) => SemanticUnitKind::Owner,
            SourceObligationOrigin::MissingRule(_) => SemanticUnitKind::Rule,
            SourceObligationOrigin::Statement(_) | SourceObligationOrigin::Axiom(_) => {
                SemanticUnitKind::Assertion
            }
            SourceObligationOrigin::Quotation { .. } => SemanticUnitKind::Quotation,
            SourceObligationOrigin::Import(_) => SemanticUnitKind::Import,
            SourceObligationOrigin::PropertyCharacteristic(_) => {
                SemanticUnitKind::PropertyCharacteristic
            }
            SourceObligationOrigin::Formula(_) => SemanticUnitKind::Formula,
            SourceObligationOrigin::Diagnostic(_) => SemanticUnitKind::Diagnostic,
            SourceObligationOrigin::SelectionIssue(_) => SemanticUnitKind::Diagnostic,
        }
    }

    #[must_use]
    fn documents(&self, obligation: SourceObligationOrigin) -> BTreeSet<String> {
        match obligation {
            SourceObligationOrigin::ClassDefinition(node)
            | SourceObligationOrigin::PropertyCharacteristic(node) => self.node_documents(node),
            SourceObligationOrigin::Owner(index) => {
                self.node_documents(self.theory.owner_lowerings()[index].source)
            }
            SourceObligationOrigin::Statement(binding) => {
                self.statement_documents(binding.canonical, binding.carrier)
            }
            SourceObligationOrigin::Quotation { reifier, graph, .. } => {
                self.node_documents(SourceNode {
                    term: reifier,
                    graph,
                })
            }
            SourceObligationOrigin::Import(index) => {
                self.node_documents(self.theory.source().source_graph().edges()[index].source)
            }
            SourceObligationOrigin::Formula(index) => {
                self.node_documents(self.theory.formula_lowerings()[index].source)
            }
            SourceObligationOrigin::Axiom(index) => self.theory.axiom_sources()[index]
                .iter()
                .flat_map(|source| match source {
                    AxiomSource::Statement { subject, .. }
                    | AxiomSource::ClassExpression(subject)
                    | AxiomSource::Reification(subject)
                    | AxiomSource::Formula(subject) => self.node_documents(*subject),
                    AxiomSource::Collection { source, .. } => self.statement_documents(
                        source.constructor.canonical,
                        source.constructor.carrier,
                    ),
                    AxiomSource::RewrittenField { source, .. } => {
                        self.statement_documents(source.canonical, source.carrier)
                    }
                })
                .collect(),
            SourceObligationOrigin::MissingRule(_)
            | SourceObligationOrigin::Diagnostic(_)
            | SourceObligationOrigin::SelectionIssue(_) => BTreeSet::new(),
        }
    }
}

/// Borrowed obligation from its owning immutable inventory. Public consumers can
/// inspect coordinates but cannot manufacture a foreign inventory lookup.
#[derive(Clone, Copy)]
pub struct SourceObligation<'inventory> {
    inventory: &'inventory SourceInventory<'inventory>,
    origin: SourceObligationOrigin,
}

impl SourceObligation<'_> {
    /// Actual successfully lowered constructor roles, retaining the row's carrier.
    /// A role is source evidence, not native or classical law admission.
    pub fn collection_roles(&self) -> impl Iterator<Item = CollectionRole> + '_ {
        let roles = match self.origin {
            SourceObligationOrigin::Statement(binding) => {
                self.inventory.collection_constructors.get(&binding)
            }
            _ => None,
        };
        roles.into_iter().flat_map(|roles| roles.iter().copied())
    }

    /// Whether this row authors class-constructor syntax, including annotation rows.
    #[must_use]
    pub fn requires_class_law(&self) -> bool {
        let SourceObligationOrigin::Statement(binding) = self.origin else {
            return false;
        };
        let dataset = self.inventory.theory.source().dataset();
        matches!(dataset.resolve(binding.canonical.p), TermRef::Iri(predicate) if predicate.strip_prefix(LOGIC).is_some_and(class_field))
    }
    /// Exact property-characteristic record read through the shared compiler boundary.
    pub fn property_characteristic(
        &self,
    ) -> Option<gmeow_errors::Result<crate::frontend::PropertyCharacteristic>> {
        let SourceObligationOrigin::PropertyCharacteristic(node) = self.origin else {
            return None;
        };
        let dataset = self.inventory.theory.source().dataset();
        Some((|| {
            Ok(crate::frontend::PropertyCharacteristic {
                property: single_iri_object(dataset, node, &format!("{LOGIC}characterizes"))?,
                characteristic: single_iri_object(
                    dataset,
                    node,
                    &format!("{LOGIC}characteristicSort"),
                )?,
            })
        })())
    }
    /// Native statement terms, addressed only through this inventory's sealed view.
    pub fn assertion(&self) -> Option<gmeow_errors::Result<crate::frontend::SourceAssertion>> {
        match self.origin {
            SourceObligationOrigin::Statement(binding) => {
                Some(self.inventory.assertion(binding.canonical))
            }
            _ => None,
        }
    }
    #[must_use]
    pub fn is_structural_statement(&self) -> bool {
        match self.origin {
            SourceObligationOrigin::Statement(binding) => self
                .inventory
                .is_structural_statement(binding.canonical, binding.carrier),
            _ => false,
        }
    }
    /// Authored configuration retained by the shared compiler reader.
    #[must_use]
    pub fn configuration_declaration(&self) -> Option<&crate::frontend::SemanticUnitDisposition> {
        let SourceObligationOrigin::Owner(ordinal) = self.origin else {
            return None;
        };
        let lowering = &self.inventory.theory.owner_lowerings()[ordinal];
        let OwnerDisposition::Emitted { index } = lowering.disposition else {
            return None;
        };
        self.inventory
            .contract_declaration(lowering.source, index)
            .map(|(disposition, _)| disposition)
    }
    #[must_use]
    pub fn origin(&self) -> SourceObligationOrigin {
        self.origin
    }
    #[must_use]
    pub fn identity(&self) -> SourceObligationId {
        self.inventory.identity(self.origin)
    }
    #[must_use]
    pub fn anchor(&self) -> String {
        self.inventory.anchor(self.origin)
    }
    #[must_use]
    pub fn kind(&self) -> SemanticUnitKind {
        self.inventory.kind(self.origin)
    }
    #[must_use]
    pub fn documents(&self) -> BTreeSet<String> {
        self.inventory.documents(self.origin)
    }
}
