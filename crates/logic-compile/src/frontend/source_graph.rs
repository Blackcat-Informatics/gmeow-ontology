// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Structural source units and uses, recorded before extraction and lowering.
//!
//! This graph describes source syntax and ownership claims. A declaration is not
//! admission to an engine, and a reference is not an execution certificate. Native
//! IDs remain local to the one immutable source dataset; they are never serialized.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use purrdf::{RdfDataset, TermId, TermRef};

use super::{Diagnostic, FORMULA_SUBLINKS};
use crate::graphutil::{RDF_REIFIES, RDF_STATEMENT, is_structural_type_predicate};
use crate::ir::LOGIC_NAMESPACE;

/// How a source's admitted base IRI was established.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SourceBaseOrigin {
    Caller,
    Directive { line: u32, column: u32 },
    Enclosing,
}

/// Exact base admission, retained separately from graph or module identity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceBase {
    pub iri: String,
    pub origin: SourceBaseOrigin,
}

/// Portable original-document receipt. The role inventories source provenance;
/// selecting a role for execution requires a separate reasoning contract.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceDocument {
    pub path: String,
    pub content_digest: String,
    pub role: String,
    pub base: Option<SourceBase>,
}

/// Position in the original document's RDF statements, not an execution role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceOccurrencePosition {
    Subject,
    Object,
    AnnotationSubject,
    AnnotationObject,
    Reifier,
    QuotedStatement,
}

/// Exact original-to-prepared structural-node binding. Both sides remain native
/// invocation-local anchors; portable documents are stored once in a separate pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceNodeBinding {
    pub original: SourceNode,
    pub canonical: SourceNode,
    pub position: SourceOccurrencePosition,
}

/// One document and all its structural occurrences before union deduplication.
#[derive(Debug)]
pub struct SourceDocumentOccurrences {
    pub document: SourceDocument,
    pub bindings: BTreeSet<SourceNodeBinding>,
    /// Every ordinary or annotation row contributed by this document. Duplicate
    /// aggregate rows deliberately occur in more than one document inventory.
    pub statements: Vec<SourceStatementBinding>,
}

/// One structural node or quoted statement in an exact native source dataset.
/// Graph identity is separate from module and standpoint references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceNode {
    /// Invocation-local resource identity, including blank-node scope.
    pub term: TermId,
    /// Original RDF graph placement, never a pipeline transport graph.
    pub graph: Option<TermId>,
}

/// Structural unit family; original declaration IRIs remain in the source graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceUnitKind {
    Formula,
    TermCarrier,
    FunctionTerm,
    Rule,
    Constraint,
    ConstraintSugar,
    Correspondence,
    CorrespondenceComposition,
    FinitePresentation,
    PresentationContext,
    PresentationSymbol,
    PresentationEvidence,
    PresentationBinding,
    ScopedBlankReference,
    PresentationSentence,
    PresentationMap,
    PresentationMerge,
    RecoveryCase,
    ReasoningProgram,
    TransactionProgram,
    AbductiveSchema,
    ContextualEvaluationRequest,
    ReasoningContract,
    ReasoningPreset,
    ClosureEntry,
    ProbabilityModel,
    PropertyCharacteristicAssertion,
    PathShape,
    JoinLeg,
    LawClaim,
    ExpressivenessBoundary,
    Module,
    ClassExpression,
    KeyAssertion,
    ReifiedAssertion,
    QuotedStatement,
}

/// The role of a source edge, independent of any verdict about its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEdgeRole {
    FormulaComponent,
    TermComponent,
    ExclusiveFormulaUse,
    RecoveryOwnership,
    RuleComponent,
    Configuration,
    Backing,
    Target,
    ExplicitFormulaSelection,
    Module,
    Standpoint,
    Context,
    Import,
    Provenance,
    Reification,
    StatementComponent,
}

/// Native storage position is retained even if the same RDF assertion also has
/// an ordinary statement-table representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceCarrier {
    Statement,
    Annotation,
    Reifier,
}

/// One RDF statement after native composition/canonicalization.
///
/// The row is invocation-local. The surrounding [`SourceDocument`] supplies the
/// portable identity; this binding proves which selected document contributed an
/// exact row without serializing or reparsing the aggregate dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceStatementBinding {
    pub canonical: purrdf::QuadIds,
    pub carrier: SourceCarrier,
}

impl Ord for SourceStatementBinding {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.canonical.s,
            self.canonical.p,
            self.canonical.o,
            self.canonical.g,
            self.carrier,
        )
            .cmp(&(
                other.canonical.s,
                other.canonical.p,
                other.canonical.o,
                other.canonical.g,
                other.carrier,
            ))
    }
}

impl PartialOrd for SourceStatementBinding {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Native reifier bindings need no synthetic dictionary entry for `rdf:reifies`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourcePredicate {
    Stored(TermId),
    Reifies,
}

impl SourcePredicate {
    /// Resolve the predicate without allocating or altering the source dictionary.
    pub fn iri(self, dataset: &RdfDataset) -> &str {
        match self {
            Self::Stored(term) => match dataset.resolve(term) {
                TermRef::Iri(iri) => iri,
                _ => unreachable!("source predicate is an RDF IRI"),
            },
            Self::Reifies => RDF_REIFIES,
        }
    }
}

/// A declared or structurally referenced unit. An undeclared reference target is
/// retained so a malformed owner cannot erase the missing dependency.
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub node: SourceNode,
    /// Exact authored type terms, without a global `instanceOf`/`rdf:type` rewrite.
    pub declarations: BTreeSet<TermId>,
    pub declared_kinds: BTreeSet<SourceUnitKind>,
    /// Families required by incoming uses, not inferred declarations or authority.
    pub required_kinds: BTreeSet<SourceUnitKind>,
}

/// One original edge. Literal and quoted-triple targets remain typed native terms
/// even when the selected grammar requires a resource and must reject them.
#[derive(Debug, Clone)]
pub struct SourceEdge {
    pub source: SourceNode,
    pub predicate: SourcePredicate,
    pub carrier: SourceCarrier,
    pub target: TermId,
    pub role: SourceEdgeRole,
    pub required_owner: Option<SourceUnitKind>,
    pub required_target: Option<SourceUnitKind>,
}

/// Indexed structural syntax for one immutable dataset, across all original graphs.
/// Ordinary schema/data assertions remain in that same native dataset; this index
/// is not, on its own, a complete logical-law denominator.
#[derive(Debug)]
pub struct StructuralSourceGraph {
    units: BTreeMap<SourceNode, SourceUnit>,
    edges: Vec<SourceEdge>,
    owned_formulas: HashSet<SourceNode>,
    owned_recoveries: HashSet<SourceNode>,
    default_statement_annotations: BTreeMap<TermId, BTreeSet<TermId>>,
}

impl StructuralSourceGraph {
    /// Classify an exact source predicate using the same ownership vocabulary as
    /// the structural inventory, without constructing or lowering that inventory.
    /// The role describes authored syntax; it does not admit its execution.
    #[must_use]
    pub fn predicate_role(predicate: &str) -> Option<SourceEdgeRole> {
        edge_spec(predicate).map(|spec| spec.role)
    }

    pub(crate) fn new(dataset: &RdfDataset) -> Self {
        let mut graph = Self {
            units: BTreeMap::new(),
            edges: Vec::new(),
            owned_formulas: HashSet::new(),
            owned_recoveries: HashSet::new(),
            default_statement_annotations: BTreeMap::new(),
        };
        for (quad, _) in source_statements(dataset) {
            let (TermRef::Iri(predicate), TermRef::Iri(class)) =
                (dataset.resolve(quad.p), dataset.resolve(quad.o))
            else {
                continue;
            };
            if is_structural_type_predicate(predicate, class)
                && let Some(kind) = class_kind(class)
            {
                let unit = graph.unit_mut(SourceNode {
                    term: quad.s,
                    graph: quad.g,
                });
                unit.declarations.insert(quad.o);
                unit.declared_kinds.insert(kind);
            }
        }
        for (quad, carrier) in source_statements(dataset) {
            let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
                continue;
            };
            let Some(spec) = edge_spec(predicate) else {
                continue;
            };
            let source = SourceNode {
                term: quad.s,
                graph: quad.g,
            };
            graph.unit_mut(source);
            if let Some(kind) = spec.owner {
                graph.unit_mut(source).required_kinds.insert(kind);
            }
            if let Some(kind) = spec.target
                && (is_resource(dataset, quad.o)
                    || (kind == SourceUnitKind::QuotedStatement
                        && matches!(dataset.resolve(quad.o), TermRef::Triple { .. })))
            {
                graph
                    .unit_mut(SourceNode {
                        term: quad.o,
                        graph: quad.g,
                    })
                    .required_kinds
                    .insert(kind);
            }
            let target = SourceNode {
                term: quad.o,
                graph: quad.g,
            };
            if matches!(
                spec.role,
                SourceEdgeRole::FormulaComponent | SourceEdgeRole::ExclusiveFormulaUse
            ) {
                // Even an invalid ownership claim must never promote its formula
                // into an unconditional assertion. The invalid claim gets an error.
                graph.owned_formulas.insert(target);
            }
            if spec.role == SourceEdgeRole::RecoveryOwnership
                && graph.declares(source, SourceUnitKind::Correspondence)
            {
                graph.owned_recoveries.insert(target);
            }
            graph.edges.push(SourceEdge {
                source,
                predicate: SourcePredicate::Stored(quad.p),
                carrier,
                target: quad.o,
                role: spec.role,
                required_owner: spec.owner,
                required_target: spec.target,
            });
        }
        for (reifier, triple, context) in dataset.reifiers_with_graph() {
            let source = SourceNode {
                term: reifier,
                graph: context,
            };
            graph
                .unit_mut(source)
                .required_kinds
                .insert(SourceUnitKind::ReifiedAssertion);
            graph
                .unit_mut(SourceNode {
                    term: triple,
                    graph: context,
                })
                .required_kinds
                .insert(SourceUnitKind::QuotedStatement);
            graph.edges.push(SourceEdge {
                source,
                predicate: SourcePredicate::Reifies,
                carrier: SourceCarrier::Reifier,
                target: triple,
                role: SourceEdgeRole::Reification,
                required_owner: None,
                required_target: Some(SourceUnitKind::QuotedStatement),
            });
        }
        // Source-local admission must not rescan the whole corpus per sentence.
        // Group the existing edge storage, without a second adjacency payload.
        graph.edges.sort_by_key(|edge| edge.source);
        // Keep statement metadata attached to its original asserted source row.
        // This small reference index is shared by semantic owners; it neither
        // asserts quoted triples nor copies their native payloads.
        let mut annotations = BTreeMap::<TermId, BTreeSet<TermId>>::new();
        let mut record = |owner: TermId, quoted: TermId| {
            if let TermRef::Triple { s, p, o } = dataset.resolve(quoted)
                && crate::graphutil::default_graph_pattern(dataset, Some(s), Some(p), Some(o))
                    .next()
                    .is_some()
            {
                annotations.entry(s).or_default().insert(owner);
            }
        };
        for unit in graph
            .units
            .values()
            .filter(|unit| unit.node.graph.is_none())
        {
            record(unit.node.term, unit.node.term);
        }
        for edge in graph
            .edges
            .iter()
            .filter(|edge| edge.source.graph.is_none() && edge.role == SourceEdgeRole::Reification)
        {
            record(edge.source.term, edge.target);
        }
        graph.default_statement_annotations = annotations;
        graph
    }

    fn unit_mut(&mut self, node: SourceNode) -> &mut SourceUnit {
        self.units.entry(node).or_insert_with(|| SourceUnit {
            node,
            declarations: BTreeSet::new(),
            declared_kinds: BTreeSet::new(),
            required_kinds: BTreeSet::new(),
        })
    }

    /// Every declared or referenced structural unit, retaining graph placement.
    pub fn units(&self) -> impl Iterator<Item = &SourceUnit> {
        self.units.values()
    }

    /// Borrow a source unit by its exact dataset-local anchor.
    pub fn unit(&self, node: SourceNode) -> Option<&SourceUnit> {
        self.units.get(&node)
    }

    /// Original ownership, backing, target, context and import relationships.
    pub fn edges(&self) -> &[SourceEdge] {
        &self.edges
    }

    /// Borrow only this exact source node's relationships, including annotations.
    /// Graph placement is part of the key; the lookup never unions named graphs.
    pub fn outgoing(&self, node: SourceNode) -> &[SourceEdge] {
        let first = self.edges.partition_point(|edge| edge.source < node);
        let count = self.edges[first..].partition_point(|edge| edge.source == node);
        &self.edges[first..first + count]
    }

    /// Metadata subjects describing actual default-graph statements by `subject`.
    /// Unasserted quotations and reifiers in another graph are not scope imports.
    /// Following these references also exposes metadata on metadata without
    /// flattening any quoted statement into an assertion.
    pub fn default_statement_annotations(
        &self,
        subject: TermId,
    ) -> impl Iterator<Item = TermId> + '_ {
        self.default_statement_annotations
            .get(&subject)
            .into_iter()
            .flat_map(|annotations| annotations.iter().copied())
    }

    /// Test explicit structural typing; required kinds never satisfy declarations.
    pub fn declares(&self, node: SourceNode, kind: SourceUnitKind) -> bool {
        self.units
            .get(&node)
            .is_some_and(|unit| unit.declared_kinds.contains(&kind))
    }

    pub(crate) fn formula_is_owned(&self, node: SourceNode) -> bool {
        self.owned_formulas.contains(&node)
    }

    pub(crate) fn recovery_is_owned(&self, node: SourceNode) -> bool {
        self.owned_recoveries.contains(&node)
    }

    /// Owner grammar violations in the frontend's explicitly selected default
    /// graph. Named-graph units remain inventoried, not implicitly asserted here.
    pub(crate) fn ownership_diagnostics(&self, dataset: &RdfDataset) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut reported_uses = HashSet::new();
        for unit in self.units.values().filter(|unit| unit.node.graph.is_none()) {
            if unit.required_kinds.contains(&SourceUnitKind::Formula)
                && !unit.declared_kinds.contains(&SourceUnitKind::Formula)
                && !self.formula_is_owned(unit.node)
            {
                let focus = focus(dataset, unit.node.term);
                diagnostics.push(Diagnostic::error(
                    "UNDECLARED_FORMULA_ROOT",
                    format!("{focus} has formula structure or an explicit formula selection but no logic:Formula declaration"),
                    Some(focus),
                ));
            }
        }
        for edge in self.edges.iter().filter(|edge| edge.source.graph.is_none()) {
            if matches!(
                edge.role,
                SourceEdgeRole::ExclusiveFormulaUse | SourceEdgeRole::RecoveryOwnership
            ) {
                if !reported_uses.insert((edge.source, edge.predicate, edge.target)) {
                    continue;
                }
                let predicate = edge.predicate.iri(dataset);
                let focus = focus(dataset, edge.source.term);
                if let Some(owner) = edge.required_owner
                    && !self.declares(edge.source, owner)
                {
                    diagnostics.push(Diagnostic::error(
                        "UNDECLARED_SEMANTIC_OWNER",
                        format!("{focus} uses {predicate} but has no declared {owner:?} owner; its target is not an independent assertion"),
                        Some(focus.clone()),
                    ));
                }
                if !is_resource(dataset, edge.target) {
                    diagnostics.push(Diagnostic::error(
                        "MALFORMED_SEMANTIC_TARGET",
                        format!(
                            "{focus} requires a resource-valued {predicate} target; found {:?}",
                            dataset.resolve(edge.target)
                        ),
                        Some(focus),
                    ));
                }
            }
        }
        diagnostics
    }
}

fn is_resource(dataset: &RdfDataset, term: TermId) -> bool {
    matches!(
        dataset.resolve(term),
        TermRef::Iri(_) | TermRef::Blank { .. }
    )
}

fn source_statements(
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

pub(crate) fn focus(dataset: &RdfDataset, term: TermId) -> String {
    match dataset.resolve(term) {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, scope } => scope.qualify_label(label).into_owned(),
        other => format!("{other:?}"),
    }
}

fn class_kind(iri: &str) -> Option<SourceUnitKind> {
    use SourceUnitKind as K;
    if iri == RDF_STATEMENT {
        return Some(K::ReifiedAssertion);
    }
    Some(match iri.strip_prefix(LOGIC_NAMESPACE)? {
        "Formula" => K::Formula,
        "TermCarrier" => K::TermCarrier,
        "FunctionTerm" => K::FunctionTerm,
        "Rule" => K::Rule,
        "Constraint" => K::Constraint,
        "Correspondence" => K::Correspondence,
        "CorrespondenceComposition" => K::CorrespondenceComposition,
        "FinitePresentation" => K::FinitePresentation,
        "PresentationContext" => K::PresentationContext,
        "PresentationSymbol" => K::PresentationSymbol,
        "PresentationEvidence" => K::PresentationEvidence,
        "PresentationBinding" => K::PresentationBinding,
        "ScopedBlankReference" => K::ScopedBlankReference,
        "PresentationSentence" => K::PresentationSentence,
        "PresentationMap" => K::PresentationMap,
        "PresentationMerge" => K::PresentationMerge,
        "RecoveryCase" => K::RecoveryCase,
        "ReasoningProgram" => K::ReasoningProgram,
        "TransactionProgram" => K::TransactionProgram,
        "AbductiveSchema" => K::AbductiveSchema,
        "ContextualEvaluationRequest" => K::ContextualEvaluationRequest,
        "ReasoningContract" => K::ReasoningContract,
        "ReasoningPreset" => K::ReasoningPreset,
        "ClosureEntry" => K::ClosureEntry,
        "ProbabilityModel" => K::ProbabilityModel,
        "PropertyCharacteristicAssertion" => K::PropertyCharacteristicAssertion,
        "PathShape" => K::PathShape,
        "JoinLeg" => K::JoinLeg,
        "LawClaim" => K::LawClaim,
        "ExpressivenessBoundary" => K::ExpressivenessBoundary,
        "Module" => K::Module,
        "Restriction" | "Enumeration" | "DatatypeRestriction" => K::ClassExpression,
        "KeyAssertion" => K::KeyAssertion,
        "ChoiceGroupConstraint"
        | "GuardedImplicationConstraint"
        | "DisjunctiveRequirednessConstraint"
        | "PathValueTypeConstraint"
        | "CrossNodeConstraint"
        | "ForbiddenPatternConstraint"
        | "ValueRangeConstraint"
        | "AggregateConstraint"
        | "JoinAggregateConstraint"
        | "AggregateBalanceConstraint"
        | "ComparisonConstraint"
        | "PathNodeKindConstraint"
        | "SelfJoinUniquenessConstraint"
        | "InverseExistenceConstraint"
        | "TransitiveReachabilityConstraint"
        | "AcyclicConstraint"
        | "ValueSetMembershipConstraint"
        | "StringPatternConstraint"
        | "UniqueLangConstraint" => K::ConstraintSugar,
        _ => return None,
    })
}

struct EdgeSpec {
    role: SourceEdgeRole,
    owner: Option<SourceUnitKind>,
    target: Option<SourceUnitKind>,
}

fn edge_spec(iri: &str) -> Option<EdgeSpec> {
    use SourceEdgeRole as R;
    use SourceUnitKind as K;
    let (role, owner, target) = if let Some(local) = iri.strip_prefix(LOGIC_NAMESPACE) {
        if FORMULA_SUBLINKS.contains(&local) {
            (R::FormulaComponent, Some(K::Formula), Some(K::Formula))
        } else {
            match local {
                "argument" | "quantifiedVariable" => (R::TermComponent, None, Some(K::TermCarrier)),
                "termApplication" => (
                    R::TermComponent,
                    Some(K::TermCarrier),
                    Some(K::FunctionTerm),
                ),
                "integrity" => (
                    R::ExclusiveFormulaUse,
                    Some(K::Constraint),
                    Some(K::Formula),
                ),
                "presentationFormula" => (
                    R::ExclusiveFormulaUse,
                    Some(K::PresentationSentence),
                    Some(K::Formula),
                ),
                "presentationContext" | "symbolContext" | "sentenceContext" => {
                    (R::Configuration, None, Some(K::PresentationContext))
                }
                "presentationSymbol" | "bindingSymbol" | "bindingSource" | "bindingTarget" => {
                    (R::Backing, None, Some(K::PresentationSymbol))
                }
                "sentenceEvidence" => (
                    R::Backing,
                    Some(K::PresentationSentence),
                    Some(K::PresentationEvidence),
                ),
                "sentenceBinding" => (
                    R::Configuration,
                    Some(K::PresentationSentence),
                    Some(K::PresentationBinding),
                ),
                "generatorBinding" => (
                    R::Configuration,
                    Some(K::PresentationMap),
                    Some(K::PresentationBinding),
                ),
                "presentationSentence" => (
                    R::Backing,
                    Some(K::FinitePresentation),
                    Some(K::PresentationSentence),
                ),
                "presentationContract" => (
                    R::Configuration,
                    Some(K::FinitePresentation),
                    Some(K::ReasoningContract),
                ),
                "mapSource" | "mapTarget" => (
                    R::Target,
                    Some(K::PresentationMap),
                    Some(K::FinitePresentation),
                ),
                "mergeLeft" | "mergeRight" => (
                    R::Target,
                    Some(K::PresentationMerge),
                    Some(K::PresentationMap),
                ),
                "hasPresentationMerge" => (R::Backing, None, Some(K::PresentationMerge)),
                "recoveryTransform" => (
                    R::ExclusiveFormulaUse,
                    Some(K::RecoveryCase),
                    Some(K::Formula),
                ),
                "clause" | "programQuery" | "verdictProbe" => (
                    R::ExclusiveFormulaUse,
                    Some(K::ReasoningProgram),
                    Some(K::Formula),
                ),
                "completenessFormula" => (
                    R::ExclusiveFormulaUse,
                    Some(K::AbductiveSchema),
                    Some(K::Formula),
                ),
                "queryFormula" => (
                    R::ExclusiveFormulaUse,
                    Some(K::ContextualEvaluationRequest),
                    Some(K::Formula),
                ),
                "recoveryCase" => (
                    R::RecoveryOwnership,
                    Some(K::Correspondence),
                    Some(K::RecoveryCase),
                ),
                "hasFormula" => (R::ExplicitFormulaSelection, None, Some(K::Formula)),
                "head" | "body" | "negatedBody" | "distinctBody" => {
                    (R::RuleComponent, Some(K::Rule), None)
                }
                "closureEntry" => (R::Configuration, None, Some(K::ClosureEntry)),
                "compositionFirst" | "compositionSecond" | "compositionResult" => (
                    R::Target,
                    Some(K::CorrespondenceComposition),
                    Some(K::Correspondence),
                ),
                "hasComposition" => (R::Backing, None, Some(K::CorrespondenceComposition)),
                "hasLawClaim" => (R::Backing, Some(K::Correspondence), Some(K::LawClaim)),
                "expressivenessBoundary" => (R::Backing, None, Some(K::ExpressivenessBoundary)),
                "relation" | "overAccessibility" => (R::Target, Some(K::Formula), None),
                "formalizes" | "termIri" | "functionSymbol" | "variableSort" | "getLeg"
                | "putLeg" | "characterizes" => (R::Target, None, None),
                "inModule" => (R::Module, None, Some(K::Module)),
                "standpoint" | "accordingTo" => (R::Standpoint, None, None),
                "world" | "time" | "path" | "confidence" | "modality" | "inContext"
                | "queryContext" => (R::Context, None, None),
                "imports" => (R::Import, None, Some(K::Module)),
                "provenance" => (R::Provenance, None, None),
                _ if super::is_reserved_source_syntax_predicate(local) => {
                    (R::Configuration, None, None)
                }
                _ => return None,
            }
        }
    } else {
        match iri {
            "https://blackcatinformatics.ca/gmeow/accordingTo" => (R::Standpoint, None, None),
            "https://blackcatinformatics.ca/math/memberCondition" => {
                (R::ExclusiveFormulaUse, None, Some(K::Formula))
            }
            "https://blackcatinformatics.ca/math/definingLaw"
            | "https://blackcatinformatics.ca/math/preservesStructure" => (R::Backing, None, None),
            RDF_REIFIES => (
                R::Reification,
                Some(K::ReifiedAssertion),
                Some(K::QuotedStatement),
            ),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#object" => {
                (R::StatementComponent, Some(K::ReifiedAssertion), None)
            }
            // Typed correspondence transaction-program syntax lives in the
            // GMEOW namespace. These predicates describe the selected path AST;
            // they are not domain relations to assert in the FOF problem.
            "https://blackcatinformatics.ca/gmeow/path"
            | "https://blackcatinformatics.ca/gmeow/pathStep"
            | "https://blackcatinformatics.ca/gmeow/pathSteps"
            | "https://blackcatinformatics.ca/gmeow/pathAlts"
            | "https://blackcatinformatics.ca/gmeow/pathItem"
            | "https://blackcatinformatics.ca/gmeow/pathNext" => (R::Configuration, None, None),
            _ => return None,
        }
    };
    Some(EdgeSpec {
        role,
        owner,
        target,
    })
}

#[cfg(test)]
mod tests;
