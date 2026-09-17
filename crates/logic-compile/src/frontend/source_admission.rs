// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit source selection and complete semantic-unit admission for classical FOF.
//!
//! A [`CompiledTheory`] is extraction evidence, not permission to treat every row as
//! one flat theory. This module binds a concrete document/root selection to the native
//! source graph and assigns every selected semantic unit a disposition. A projection
//! may run only from a complete admission; an unsupported required unit blocks the
//! whole problem instead of yielding a smaller, vacuously consistent theory.

use std::collections::BTreeSet;

use purrdf::{DatasetView, GraphMatch, QuadIds, RdfDataset, TermId, TermRef};

use super::{
    AxiomSource, CompiledTheory, FormulaDisposition, OwnerDisposition, OwnerFamily, Severity,
    SourceCarrier, SourceEdgeRole, SourceNode, SourceStatementBinding, SourceUnitKind,
    StructuralSourceGraph,
};
use crate::ir::{AtomicTerm, ContextualScope, Formula, LogicAxiom, LogicModality, Term};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const LOGIC_CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
const LOGIC_BUILTIN: &str = "https://blackcatinformatics.ca/logic/Builtin";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The only graph placement admitted by the initial external classical profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceGraphSelection {
    DefaultGraph,
}

/// Exact portable document selection. Anonymous single-dataset callers have no
/// document receipts and say so explicitly rather than pretending to know a path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DocumentSelection {
    AnonymousSource,
    Exact(BTreeSet<String>),
}

/// Selection of source roots within the chosen documents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RootSelection {
    None,
    All,
    Exact(BTreeSet<String>),
}

impl RootSelection {
    fn contains(&self, anchor: &str) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Exact(roots) => roots.contains(anchor),
        }
    }
}

/// One explicit proof-theory selection. Documents/imports, top-level formulas,
/// Horn rules and typed semantic owners remain separate choices.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceSelection {
    pub graph: SourceGraphSelection,
    pub documents: DocumentSelection,
    pub formulas: RootSelection,
    pub rules: RootSelection,
    pub owners: RootSelection,
}

impl SourceSelection {
    /// Select every recorded document, every top-level formula, every Horn rule,
    /// and every typed owner. A profile that cannot execute a selected owner must
    /// report it as a required-unit blocker.
    #[must_use]
    pub fn all_classical_roots(theory: &CompiledTheory) -> Self {
        let documents = if theory.source().origins().is_empty() {
            DocumentSelection::AnonymousSource
        } else {
            DocumentSelection::Exact(
                theory
                    .source()
                    .origins()
                    .iter()
                    .map(|origin| origin.document.path.clone())
                    .collect(),
            )
        };
        Self {
            graph: SourceGraphSelection::DefaultGraph,
            documents,
            formulas: RootSelection::All,
            rules: RootSelection::All,
            // Whole-source admission inventories typed owners even when this
            // profile cannot execute them. Their definitions/constraints must
            // become required-unit blockers rather than disappear behind the
            // fact that their child formulas were read as structural support.
            owners: RootSelection::All,
        }
    }

    /// Stable identity of the exact selection contract.
    #[must_use]
    pub fn digest(&self) -> String {
        digest_bytes(
            b"gmeow-source-selection-v1\0",
            &serde_json::to_vec(self).expect("SourceSelection serialization is infallible"),
        )
    }
}

/// Classical interpretation contract carried by every admission and projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FofSemanticProfile {
    pub id: String,
    pub graph: String,
    pub world: String,
    pub equality: String,
    pub blank_nodes: String,
    pub free_variables: String,
    pub context: String,
}

impl Default for FofSemanticProfile {
    fn default() -> Self {
        Self {
            id: "https://blackcatinformatics.ca/logic/profile/tptp-fof-source-v1".into(),
            graph: "selected default-graph assertions only".into(),
            world: "open-world classical first-order theory".into(),
            equality: "denotational equality without a unique-name assumption".into(),
            blank_nodes: "source-scoped existential blanks skolemized for satisfiability".into(),
            free_variables: "top-level free variables universally closed".into(),
            context: "unscoped sentences only; no standpoint, time, modality, confidence, provenance, or module erasure".into(),
        }
    }
}

/// Admission-wide verdict. Only `Complete` authorizes a proof problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceAdmissionStatus {
    Complete,
    Blocked,
    Malformed,
    Empty,
}

/// Semantic family of an inventoried source unit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum SemanticUnitKind {
    Assertion,
    Formula,
    Rule,
    PropertyCharacteristic,
    Import,
    StructuralSyntax,
    Quotation,
    Diagnostic,
    Owner,
}

/// A native RDF assertion selected for the classical theory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceAssertion {
    pub subject: AtomicTerm,
    pub predicate: String,
    pub object: AtomicTerm,
}

impl SourceAssertion {
    #[must_use]
    pub fn key(&self) -> String {
        frame(
            "assertion",
            [
                self.subject.key(),
                self.predicate.clone(),
                self.object.key(),
            ],
        )
    }
}

/// A supported property-characteristic record reconstructed from its typed source
/// node. Its component triples remain syntax; this value is the admitted law.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PropertyCharacteristic {
    pub property: String,
    pub characteristic: String,
}

/// Payload carried by an admitted semantic unit. Program indices are valid only
/// for the exact [`CompiledTheory`] whose digest the admission records.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdmittedSemanticUnit {
    Assertion(SourceAssertion),
    Formula { index: usize },
    Rule { index: usize },
    PropertyCharacteristic(PropertyCharacteristic),
}

/// Complete disposition of one source unit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SemanticUnitDisposition {
    Admitted(AdmittedSemanticUnit),
    StructuralSupport { detail: String },
    OutsideSelection { reason: String },
    QuotedUnasserted { detail: String },
    Blocked { code: String, reason: String },
    Malformed { code: String, reason: String },
    Observed { detail: String },
}

impl SemanticUnitDisposition {
    fn blocks(&self) -> bool {
        matches!(self, Self::Blocked { .. })
    }

    fn malformed(&self) -> bool {
        matches!(self, Self::Malformed { .. })
    }

    fn admitted(&self) -> bool {
        matches!(self, Self::Admitted(_))
    }
}

/// Portable accounting row. `anchor` is a canonical term/quad spelling, never a
/// persisted native `TermId`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticUnitAdmission {
    pub id: String,
    pub kind: SemanticUnitKind,
    pub anchor: String,
    pub documents: BTreeSet<String>,
    pub disposition: SemanticUnitDisposition,
}

/// A source-complete classical admission bound to one theory and one selection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceAdmission {
    pub profile: FofSemanticProfile,
    pub source_digest: String,
    pub program_digest: String,
    pub selection: SourceSelection,
    pub selection_digest: String,
    pub status: SourceAdmissionStatus,
    pub units: Vec<SemanticUnitAdmission>,
    pub declared_classes: BTreeSet<String>,
    pub declared_builtins: BTreeSet<String>,
}

impl SourceAdmission {
    /// Inventory and admit an exact selected source for classical FOF projection.
    #[must_use]
    pub fn classical_fof(theory: &CompiledTheory, selection: SourceSelection) -> Self {
        AdmissionBuilder::new(theory, selection).build()
    }

    #[must_use]
    pub fn blockers(&self) -> impl Iterator<Item = &SemanticUnitAdmission> {
        self.units
            .iter()
            .filter(|unit| unit.disposition.blocks() || unit.disposition.malformed())
    }

    #[must_use]
    pub fn admitted(&self) -> impl Iterator<Item = &SemanticUnitAdmission> {
        self.units.iter().filter(|unit| unit.disposition.admitted())
    }

    /// Whether this admission is still bound to this exact immutable source and
    /// compiled program.
    #[must_use]
    pub fn matches(&self, theory: &CompiledTheory) -> bool {
        self.source_digest == source_digest(theory)
            && self.program_digest
                == digest_bytes(
                    b"gmeow-logic-program-v1\0",
                    theory.program().canonical_key().as_bytes(),
                )
    }
}

struct AdmissionBuilder<'a> {
    theory: &'a CompiledTheory,
    selection: SourceSelection,
    selected_documents: BTreeSet<String>,
    units: Vec<SemanticUnitAdmission>,
    declared_classes: BTreeSet<String>,
    declared_builtins: BTreeSet<String>,
}

impl<'a> AdmissionBuilder<'a> {
    fn new(theory: &'a CompiledTheory, selection: SourceSelection) -> Self {
        let selected_documents = match &selection.documents {
            DocumentSelection::AnonymousSource => BTreeSet::new(),
            DocumentSelection::Exact(paths) => paths.clone(),
        };
        Self {
            theory,
            selection,
            selected_documents,
            units: Vec::new(),
            declared_classes: BTreeSet::new(),
            declared_builtins: BTreeSet::new(),
        }
    }

    fn build(mut self) -> SourceAdmission {
        self.validate_document_selection();
        self.scan_declarations();
        self.admit_source_statements();
        self.admit_imports();
        self.admit_property_characteristics();
        self.admit_formulas();
        self.admit_rules_and_owners();
        self.admit_remaining_axioms();
        self.record_diagnostics();
        self.units.sort_by(|left, right| {
            (left.kind, &left.anchor, &left.id).cmp(&(right.kind, &right.anchor, &right.id))
        });
        let malformed = self.units.iter().any(|unit| unit.disposition.malformed());
        let blocked = self.units.iter().any(|unit| unit.disposition.blocks());
        let admitted = self.units.iter().any(|unit| unit.disposition.admitted());
        let status = if malformed {
            SourceAdmissionStatus::Malformed
        } else if blocked {
            SourceAdmissionStatus::Blocked
        } else if admitted {
            SourceAdmissionStatus::Complete
        } else {
            SourceAdmissionStatus::Empty
        };
        let selection_digest = self.selection.digest();
        SourceAdmission {
            profile: FofSemanticProfile::default(),
            source_digest: source_digest(self.theory),
            program_digest: digest_bytes(
                b"gmeow-logic-program-v1\0",
                self.theory.program().canonical_key().as_bytes(),
            ),
            selection: self.selection,
            selection_digest,
            status,
            units: self.units,
            declared_classes: self.declared_classes,
            declared_builtins: self.declared_builtins,
        }
    }

    fn validate_document_selection(&mut self) {
        let available: BTreeSet<_> = self
            .theory
            .source()
            .origins()
            .iter()
            .map(|origin| origin.document.path.clone())
            .collect();
        match self.selection.documents.clone() {
            DocumentSelection::AnonymousSource if !available.is_empty() => self.push(
                SemanticUnitKind::Diagnostic,
                "selection:documents".into(),
                BTreeSet::new(),
                malformed(
                    "DOCUMENT_SELECTION_MISMATCH",
                    "anonymous selection cannot address a source with document receipts",
                ),
            ),
            DocumentSelection::Exact(_) if available.is_empty() => self.push(
                SemanticUnitKind::Diagnostic,
                "selection:documents".into(),
                BTreeSet::new(),
                malformed(
                    "DOCUMENT_SELECTION_MISMATCH",
                    "path selection cannot address an anonymous source",
                ),
            ),
            DocumentSelection::Exact(paths) => {
                for missing in paths.difference(&available) {
                    self.push(
                        SemanticUnitKind::Diagnostic,
                        format!("selection:document:{missing}"),
                        BTreeSet::new(),
                        malformed(
                            "UNKNOWN_SELECTED_DOCUMENT",
                            format!(
                                "selected document {missing:?} is absent from the source receipts"
                            ),
                        ),
                    );
                }
                if paths.is_empty() {
                    self.push(
                        SemanticUnitKind::Diagnostic,
                        "selection:documents".into(),
                        BTreeSet::new(),
                        malformed(
                            "EMPTY_DOCUMENT_SELECTION",
                            "an exact document selection must name at least one document",
                        ),
                    );
                }
            }
            DocumentSelection::AnonymousSource => {}
        }
    }

    fn scan_declarations(&mut self) {
        let dataset = self.theory.source().dataset();
        for quad in dataset.quads().filter(|quad| quad.g.is_none()) {
            if !self.statement_selected(quad, SourceCarrier::Statement) {
                continue;
            }
            let (TermRef::Iri(predicate), TermRef::Iri(object), TermRef::Iri(subject)) = (
                dataset.resolve(quad.p),
                dataset.resolve(quad.o),
                dataset.resolve(quad.s),
            ) else {
                continue;
            };
            if matches!(predicate, RDF_TYPE | LOGIC_INSTANCE_OF) {
                if matches!(object, LOGIC_CLASS | OWL_CLASS | RDFS_CLASS) {
                    self.declared_classes.insert(subject.to_owned());
                }
                if object == LOGIC_BUILTIN {
                    self.declared_builtins.insert(subject.to_owned());
                }
            }
        }
    }

    fn admit_source_statements(&mut self) {
        let dataset = self.theory.source().dataset();
        for quad in dataset.quads() {
            let documents = self.statement_documents(quad, SourceCarrier::Statement);
            let anchor = quad_anchor(dataset, quad);
            if quad.g.is_some() {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    outside("the selected profile admits the default graph only"),
                );
                continue;
            }
            if !self.documents_selected(&documents) {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    outside("the contributing documents are outside the exact selection"),
                );
                continue;
            }
            if self.is_structural_statement(quad) {
                self.push(
                    SemanticUnitKind::StructuralSyntax,
                    anchor,
                    documents,
                    SemanticUnitDisposition::StructuralSupport {
                        detail: "typed source syntax is accounted by its semantic owner".into(),
                    },
                );
                continue;
            }
            if self.references_class_expression(quad) {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    blocked(
                        "UNSUPPORTED_CLASS_EXPRESSION",
                        "the selected assertion reaches a restriction/enumeration whose complete FOF lowering is not admitted",
                    ),
                );
                continue;
            }
            match assertion_from_quad(dataset, quad) {
                Ok(assertion) => {
                    if self.is_procedural_predicate(&assertion.predicate) {
                        self.push(
                            SemanticUnitKind::Assertion,
                            anchor,
                            documents,
                            blocked(
                                "PROCEDURAL_RELATION_REQUIRES_BINDING_SEMANTICS",
                                format!(
                                    "{} is a procedural bound-term relation and cannot be asserted as denotational FOF",
                                    assertion.predicate
                                ),
                            ),
                        );
                    } else {
                        self.push(
                            SemanticUnitKind::Assertion,
                            anchor,
                            documents,
                            SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Assertion(
                                assertion,
                            )),
                        );
                    }
                }
                Err(reason) if matches!(dataset.resolve(quad.o), TermRef::Triple { .. }) => self
                    .push(
                        SemanticUnitKind::Quotation,
                        anchor,
                        documents,
                        SemanticUnitDisposition::Blocked {
                            code: "UNSUPPORTED_QUOTED_TERM_ARGUMENT".into(),
                            reason,
                        },
                    ),
                Err(reason) => self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    malformed("MALFORMED_SELECTED_ASSERTION", reason),
                ),
            }
        }

        // Statement-layer quotations are deliberately inventoried separately. Their
        // quoted payload is never promoted into the assertional graph.
        for (reifier, triple, graph) in dataset.reifiers_with_graph() {
            let node = SourceNode {
                term: reifier,
                graph,
            };
            let documents = self.node_documents(node);
            let anchor = format!(
                "reifier|{}|{}",
                term_anchor(dataset, dataset.resolve(reifier)),
                term_anchor(dataset, dataset.resolve(triple))
            );
            self.push(
                SemanticUnitKind::Quotation,
                anchor,
                documents,
                if graph.is_some() || !self.documents_selected(&self.node_documents(node)) {
                    outside("quoted statement is outside the selected default-graph documents")
                } else {
                    SemanticUnitDisposition::QuotedUnasserted {
                        detail: "RDF 1.2 quotation retained without asserting its payload".into(),
                    }
                },
            );
        }
    }

    fn admit_imports(&mut self) {
        let dataset = self.theory.source().dataset();
        for edge in self
            .theory
            .source()
            .source_graph()
            .edges()
            .iter()
            .filter(|edge| edge.role == SourceEdgeRole::Import)
        {
            let documents = self.node_documents(edge.source);
            let anchor = format!(
                "import|{}|{}",
                node_anchor(dataset, edge.source),
                term_anchor(dataset, dataset.resolve(edge.target))
            );
            if edge.source.graph.is_some() || !self.documents_selected(&documents) {
                self.push(
                    SemanticUnitKind::Import,
                    anchor,
                    documents,
                    outside("the importing module is outside the selected default-graph documents"),
                );
                continue;
            }
            let target = SourceNode {
                term: edge.target,
                graph: edge.source.graph,
            };
            let target_documents = self.node_documents(target);
            if target_documents.is_empty() || !self.documents_selected(&target_documents) {
                self.push(
                    SemanticUnitKind::Import,
                    anchor,
                    documents,
                    blocked(
                        "UNADMITTED_IMPORT",
                        "a selected module import has no target in the exact selected document set",
                    ),
                );
            } else {
                self.push(
                    SemanticUnitKind::Import,
                    anchor,
                    documents,
                    SemanticUnitDisposition::StructuralSupport {
                        detail:
                            "selected import target is present in the same exact source selection"
                                .into(),
                    },
                );
            }
        }
    }

    fn admit_property_characteristics(&mut self) {
        let dataset = self.theory.source().dataset();
        let units: Vec<_> = self
            .theory
            .source()
            .source_graph()
            .units()
            .filter(|unit| {
                unit.declared_kinds
                    .contains(&SourceUnitKind::PropertyCharacteristicAssertion)
            })
            .map(|unit| unit.node)
            .collect();
        for node in units {
            let documents = self.node_documents(node);
            let anchor = node_anchor(dataset, node);
            if node.graph.is_some() || !self.documents_selected(&documents) {
                self.push(
                    SemanticUnitKind::PropertyCharacteristic,
                    anchor,
                    documents,
                    outside(
                        "the characteristic record is outside the selected default-graph documents",
                    ),
                );
                continue;
            }
            let property = single_iri_object(dataset, node, &format!("{LOGIC}characterizes"));
            let characteristic =
                single_iri_object(dataset, node, &format!("{LOGIC}characteristicSort"));
            let (Ok(property), Ok(characteristic)) = (property, characteristic) else {
                self.push(
                    SemanticUnitKind::PropertyCharacteristic,
                    anchor,
                    documents,
                    malformed(
                        "MALFORMED_PROPERTY_CHARACTERISTIC",
                        "a characteristic record requires exactly one IRI-valued logic:characterizes and logic:characteristicSort",
                    ),
                );
                continue;
            };
            if !supported_characteristic(&characteristic) {
                self.push(
                    SemanticUnitKind::PropertyCharacteristic,
                    anchor,
                    documents,
                    blocked(
                        "UNSUPPORTED_PROPERTY_CHARACTERISTIC",
                        format!(
                            "the classical profile has no certified lowering for {characteristic}"
                        ),
                    ),
                );
                continue;
            }
            self.push(
                SemanticUnitKind::PropertyCharacteristic,
                anchor,
                documents,
                SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::PropertyCharacteristic(
                    PropertyCharacteristic {
                        property,
                        characteristic,
                    },
                )),
            );
        }
    }

    fn admit_formulas(&mut self) {
        let dataset = self.theory.source().dataset();
        for lowering in self.theory.formula_lowerings() {
            let anchor = node_anchor(dataset, lowering.source);
            let documents = self.node_documents(lowering.source);
            if lowering.source.graph.is_some()
                || !self.documents_selected(&documents)
                || !self.selection.formulas.contains(&anchor)
            {
                self.push(
                    SemanticUnitKind::Formula,
                    anchor,
                    documents,
                    outside(
                        "the formula root is outside the selected documents, graph, or root set",
                    ),
                );
                continue;
            }
            let disposition = match &lowering.disposition {
                FormulaDisposition::Formula(index) => {
                    self.theory.program().formulas.get(*index).map_or_else(
                        || malformed("MISSING_FORMULA_IR", "formula trace index is out of range"),
                        |formula| self.formula_disposition(*index, formula),
                    )
                }
                FormulaDisposition::Axiom(index) => {
                    self.theory.program().axioms.get(*index).map_or_else(
                        || {
                            malformed(
                                "MISSING_AXIOM_IR",
                                "formula-routed axiom index is out of range",
                            )
                        },
                        |axiom| match self.axiom_disposition(axiom) {
                            SemanticUnitDisposition::Blocked { code, reason } => blocked(
                                "UNSUPPORTED_SELECTED_FORMULA",
                                format!("formula lowering was blocked by {code}: {reason}"),
                            ),
                            disposition => disposition,
                        },
                    )
                }
                FormulaDisposition::ReadForOwner => SemanticUnitDisposition::StructuralSupport {
                    detail: "formula is exclusively owned and requires explicit owner selection"
                        .into(),
                },
                FormulaDisposition::Malformed { diagnostic } => malformed(
                    "MALFORMED_SELECTED_FORMULA",
                    format!(
                        "selected formula failed compiler admission at diagnostic {diagnostic}"
                    ),
                ),
                FormulaDisposition::OutsideDefaultGraph => {
                    outside("formula is outside the selected default graph")
                }
            };
            self.push(SemanticUnitKind::Formula, anchor, documents, disposition);
        }
    }

    fn admit_rules_and_owners(&mut self) {
        let dataset = self.theory.source().dataset();
        let mut seen_rules = BTreeSet::new();
        for lowering in self.theory.owner_lowerings() {
            let anchor = node_anchor(dataset, lowering.source);
            let documents = self.node_documents(lowering.source);
            let selected = lowering.source.graph.is_none() && self.documents_selected(&documents);
            if lowering.family == OwnerFamily::Rule {
                if let OwnerDisposition::Emitted { index } = &lowering.disposition {
                    seen_rules.insert(*index);
                }
                if !selected || !self.selection.rules.contains(&anchor) {
                    self.push(
                        SemanticUnitKind::Rule,
                        anchor,
                        documents,
                        outside(
                            "the rule root is outside the selected documents, graph, or root set",
                        ),
                    );
                    continue;
                }
                let disposition = match &lowering.disposition {
                    OwnerDisposition::Emitted { index } => {
                        self.theory.program().rules.get(*index).map_or_else(
                            || malformed("MISSING_RULE_IR", "rule trace index is out of range"),
                            |rule| rule_disposition(*index, rule, &self.declared_builtins),
                        )
                    }
                    OwnerDisposition::Rejected => malformed(
                        "MALFORMED_SELECTED_RULE",
                        format!(
                            "selected rule failed compiler admission at diagnostics {:?}",
                            lowering.diagnostics
                        ),
                    ),
                    OwnerDisposition::OutsideDefaultGraph => {
                        outside("rule is outside the selected default graph")
                    }
                };
                self.push(SemanticUnitKind::Rule, anchor, documents, disposition);
            } else if selected && self.selection.owners.contains(&anchor) {
                self.push(
                    SemanticUnitKind::Owner,
                    anchor,
                    documents,
                    blocked(
                        "UNSUPPORTED_SELECTED_OWNER",
                        format!(
                            "the classical profile has no execution contract for owner family {:?}",
                            lowering.family
                        ),
                    ),
                );
            }
        }
        for index in 0..self.theory.program().rules.len() {
            if !seen_rules.contains(&index) {
                self.push(
                    SemanticUnitKind::Rule,
                    format!("program:rule:{index}"),
                    BTreeSet::new(),
                    malformed(
                        "RULE_WITHOUT_SOURCE_TRACE",
                        "compiled rule has no exact source-owner lowering",
                    ),
                );
            }
        }
    }

    fn admit_remaining_axioms(&mut self) {
        for (index, sources) in self.theory.axiom_sources().iter().enumerate() {
            let handled_formula = self.theory.formula_lowerings().iter().any(|lowering| {
                matches!(&lowering.disposition, FormulaDisposition::Axiom(found) if *found == index)
            });
            if handled_formula
                || sources
                    .iter()
                    .all(|source| matches!(source, AxiomSource::Statement { .. }))
            {
                continue;
            }
            let documents: BTreeSet<_> = sources
                .iter()
                .flat_map(|source| self.axiom_source_documents(source))
                .collect();
            let anchor = format!("program:axiom:{index}");
            if sources
                .iter()
                .any(|source| matches!(source, AxiomSource::Reification(_)))
            {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    blocked(
                        "CONTEXTUAL_REIFICATION_REQUIRES_TYPED_TRANSLATION",
                        "a reified assertion cannot be flattened without preserving its standpoint/provenance/context envelope",
                    ),
                );
            } else if sources
                .iter()
                .any(|source| matches!(source, AxiomSource::ClassExpression(_)))
            {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    blocked(
                        "UNSUPPORTED_CLASS_EXPRESSION",
                        "class-expression lowering is not yet certified for the classical external profile",
                    ),
                );
            } else if let Some(axiom) = self.theory.program().axioms.get(index) {
                let disposition = self.axiom_disposition(axiom);
                self.push(SemanticUnitKind::Assertion, anchor, documents, disposition);
            } else {
                self.push(
                    SemanticUnitKind::Assertion,
                    anchor,
                    documents,
                    malformed(
                        "MISSING_AXIOM_IR",
                        "axiom source trace index is out of range",
                    ),
                );
            }
        }
    }

    fn record_diagnostics(&mut self) {
        for (index, diagnostic) in self.theory.diagnostics().iter().enumerate() {
            let disposition = if diagnostic.severity == Severity::Error {
                malformed(
                    "COMPILER_ERROR_IN_SELECTED_THEORY",
                    format!("[{}] {}", diagnostic.code, diagnostic.message),
                )
            } else {
                SemanticUnitDisposition::Observed {
                    detail: format!(
                        "{} [{}] {}",
                        diagnostic.severity.as_str(),
                        diagnostic.code,
                        diagnostic.message
                    ),
                }
            };
            self.push(
                SemanticUnitKind::Diagnostic,
                format!("compiler-diagnostic:{index}"),
                BTreeSet::new(),
                disposition,
            );
        }
    }

    fn axiom_disposition(&self, axiom: &LogicAxiom) -> SemanticUnitDisposition {
        if axiom.negated {
            return blocked(
                "NEGATION_AS_FAILURE_IS_NOT_CLASSICAL_NEGATION",
                "a compact negated axiom is procedural NAF and cannot become classical not",
            );
        }
        if !scope_is_flat(&axiom.scope) {
            return blocked(
                "CONTEXT_ERASURE_FORBIDDEN",
                "standpoint/time/modality/confidence/provenance/module scope requires a typed translation",
            );
        }
        if self.is_procedural_predicate(&axiom.predicate) {
            return blocked(
                "PROCEDURAL_RELATION_REQUIRES_BINDING_SEMANTICS",
                format!(
                    "{} is a procedural bound-term relation and cannot become denotational equality or a classical predicate",
                    axiom.predicate
                ),
            );
        }
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Assertion(SourceAssertion {
            subject: AtomicTerm::resource(axiom.subject.clone()),
            predicate: axiom.predicate.clone(),
            object: axiom.obj.clone(),
        }))
    }

    fn formula_disposition(&self, index: usize, formula: &Formula) -> SemanticUnitDisposition {
        if let Some(reason) = unsupported_formula(formula, &self.declared_builtins) {
            blocked("UNSUPPORTED_SELECTED_FORMULA", reason)
        } else {
            SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Formula { index })
        }
    }

    fn is_structural_statement(&self, quad: QuadIds) -> bool {
        let dataset = self.theory.source().dataset();
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            return true;
        };
        if StructuralSourceGraph::predicate_role(predicate).is_some() {
            return true;
        }
        let node = SourceNode {
            term: quad.s,
            graph: quad.g,
        };
        let Some(unit) = self.theory.source().source_graph().unit(node) else {
            return false;
        };
        if matches!(predicate, RDF_TYPE | LOGIC_INSTANCE_OF) && unit.declarations.contains(&quad.o)
        {
            return true;
        }
        false
    }

    fn references_class_expression(&self, quad: QuadIds) -> bool {
        self.theory.source().source_graph().declares(
            SourceNode {
                term: quad.o,
                graph: quad.g,
            },
            SourceUnitKind::ClassExpression,
        )
    }

    fn is_procedural_predicate(&self, predicate: &str) -> bool {
        is_known_procedural(predicate) || self.declared_builtins.contains(predicate)
    }

    fn statement_documents(&self, quad: QuadIds, carrier: SourceCarrier) -> BTreeSet<String> {
        self.theory
            .source()
            .origins()
            .iter()
            .filter(|origin| {
                origin
                    .statements
                    .binary_search(&SourceStatementBinding {
                        canonical: quad,
                        carrier,
                    })
                    .is_ok()
            })
            .map(|origin| origin.document.path.clone())
            .collect()
    }

    fn statement_selected(&self, quad: QuadIds, carrier: SourceCarrier) -> bool {
        let documents = self.statement_documents(quad, carrier);
        self.documents_selected(&documents)
    }

    fn node_documents(&self, node: SourceNode) -> BTreeSet<String> {
        self.theory
            .source()
            .origins()
            .iter()
            .filter(|origin| {
                origin
                    .bindings
                    .iter()
                    .any(|binding| binding.canonical == node)
            })
            .map(|origin| origin.document.path.clone())
            .collect()
    }

    fn documents_selected(&self, documents: &BTreeSet<String>) -> bool {
        match &self.selection.documents {
            DocumentSelection::AnonymousSource => self.theory.source().origins().is_empty(),
            DocumentSelection::Exact(_) => documents
                .iter()
                .any(|document| self.selected_documents.contains(document)),
        }
    }

    fn axiom_source_documents(&self, source: &AxiomSource) -> BTreeSet<String> {
        match source {
            AxiomSource::Statement { subject, .. }
            | AxiomSource::ClassExpression(subject)
            | AxiomSource::Reification(subject)
            | AxiomSource::Formula(subject) => self.node_documents(*subject),
        }
    }

    fn push(
        &mut self,
        kind: SemanticUnitKind,
        anchor: String,
        documents: BTreeSet<String>,
        disposition: SemanticUnitDisposition,
    ) {
        let encoded = serde_json::to_vec(&(kind, &anchor, &documents, &disposition))
            .expect("semantic admission row serialization is infallible");
        self.units.push(SemanticUnitAdmission {
            id: digest_bytes(b"gmeow-semantic-unit-v1\0", &encoded),
            kind,
            anchor,
            documents,
            disposition,
        });
    }
}

fn rule_disposition(
    index: usize,
    rule: &crate::ir::LogicRule,
    builtins: &BTreeSet<String>,
) -> SemanticUnitDisposition {
    if rule.aggregation.is_some() {
        return blocked(
            "AGGREGATION_IS_NOT_FIRST_ORDER",
            "a reduce/GROUP BY rule has no classical FOF sentence",
        );
    }
    if !rule.distinct_pairs.is_empty() {
        return blocked(
            "RULE_TERM_DISTINCT_REQUIRES_BINDING_SEMANTICS",
            "logic:distinctBody is a bound-term guard, not denotational inequality",
        );
    }
    if !scope_is_flat(&rule.scope)
        || !scope_is_flat(&rule.head.scope)
        || rule.body.iter().any(|atom| !scope_is_flat(&atom.scope))
    {
        return blocked(
            "CONTEXT_ERASURE_FORBIDDEN",
            "rule or atom context requires a typed standpoint/modal translation",
        );
    }
    if rule.body.iter().any(|atom| atom.negated) || rule.head.negated {
        return blocked(
            "NEGATION_AS_FAILURE_IS_NOT_CLASSICAL_NEGATION",
            "a NAF rule cannot be strengthened into classical negation",
        );
    }
    if std::iter::once(&rule.head)
        .chain(rule.body.iter())
        .any(|atom| is_known_procedural(&atom.predicate) || builtins.contains(&atom.predicate))
    {
        return blocked(
            "PROCEDURAL_RELATION_REQUIRES_BINDING_SEMANTICS",
            "a rule uses a procedural built-in whose SPARQL binding/value/error contract has no FOF transfer",
        );
    }
    SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Rule { index })
}

fn unsupported_formula(formula: &Formula, builtins: &BTreeSet<String>) -> Option<String> {
    match formula {
        Formula::Atom { relation, args } => {
            let Term::Iri(relation) = relation else {
                return Some("formula relation is not an IRI".into());
            };
            if is_known_procedural(relation) || builtins.contains(relation) {
                return Some(format!(
                    "{relation} is a procedural built-in; translating it as a classical predicate or equality would be unsound"
                ));
            }
            if args.iter().any(term_has_sequence_marker) {
                return Some(
                    "Common Logic sequence markers are variadic binders and cannot collapse to constants"
                        .into(),
                );
            }
            None
        }
        Formula::Not(inner) => unsupported_formula(inner, builtins),
        Formula::And(parts) | Formula::Or(parts) => parts
            .iter()
            .find_map(|part| unsupported_formula(part, builtins)),
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            unsupported_formula(left, builtins).or_else(|| unsupported_formula(right, builtins))
        }
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            unsupported_formula(body, builtins)
        }
    }
}

fn term_has_sequence_marker(term: &Term) -> bool {
    match term {
        Term::SequenceMarker(_) => true,
        Term::App { args, .. } => args.iter().any(term_has_sequence_marker),
        Term::Var(_) | Term::Iri(_) | Term::Literal(_) => false,
    }
}

fn is_known_procedural(predicate: &str) -> bool {
    matches!(
        predicate.strip_prefix(LOGIC),
        Some("termEqual" | "termDistinct" | "termIn" | "termRegex" | "directType")
    )
}

fn supported_characteristic(characteristic: &str) -> bool {
    matches!(
        characteristic.strip_prefix(LOGIC),
        Some(
            "functionalProperty"
                | "inverseFunctionalProperty"
                | "transitiveProperty"
                | "symmetricProperty"
                | "asymmetricProperty"
                | "reflexiveProperty"
                | "irreflexiveProperty"
        )
    )
}

fn scope_is_flat(scope: &ContextualScope) -> bool {
    scope.modality == LogicModality::None
        && scope.standpoint.is_none()
        && scope.time.is_none()
        && scope.confidence.is_none()
        && scope.provenance.is_none()
        && scope.module.is_none()
}

fn single_iri_object(
    dataset: &RdfDataset,
    node: SourceNode,
    predicate: &str,
) -> Result<String, ()> {
    let Some(predicate) = dataset.term_id_by_iri(predicate) else {
        return Err(());
    };
    let graph = node.graph.map_or(GraphMatch::Default, GraphMatch::Named);
    let mut values = dataset.quads_for_pattern(Some(node.term), Some(predicate), None, graph);
    let Some(first) = values.next() else {
        return Err(());
    };
    if values.next().is_some() {
        return Err(());
    }
    match dataset.resolve(first.o) {
        TermRef::Iri(iri) => Ok(iri.to_owned()),
        _ => Err(()),
    }
}

fn assertion_from_quad(dataset: &RdfDataset, quad: QuadIds) -> Result<SourceAssertion, String> {
    let subject = atomic_term(dataset, quad.s, false)?;
    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
        return Err("RDF predicate is not an IRI".into());
    };
    let object = atomic_term(dataset, quad.o, true)?;
    Ok(SourceAssertion {
        subject,
        predicate: predicate.to_owned(),
        object,
    })
}

fn atomic_term(
    dataset: &RdfDataset,
    id: TermId,
    literal_allowed: bool,
) -> Result<AtomicTerm, String> {
    match dataset.resolve(id) {
        TermRef::Iri(iri) => Ok(AtomicTerm::Iri(iri.to_owned())),
        TermRef::Blank { label, scope } => Ok(AtomicTerm::Blank(
            scope.qualify_label(label).trim_start_matches("_:").to_owned(),
        )),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } if literal_allowed => {
            let TermRef::Iri(datatype) = dataset.resolve(datatype) else {
                return Err("literal datatype is not an IRI".into());
            };
            let language = language.map(str::to_owned);
            Ok(AtomicTerm::Literal(purrdf::RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype: if language.is_some() || datatype == XSD_STRING {
                    None
                } else {
                    Some(datatype.to_owned())
                },
                language,
                direction,
            }))
        }
        TermRef::Literal { .. } => Err("literal cannot occur in assertion subject position".into()),
        TermRef::Triple { .. } => Err(
            "quoted triple term requires an explicit proposition-term translation and remains unasserted"
                .into(),
        ),
    }
}

fn source_digest(theory: &CompiledTheory) -> String {
    let dataset = theory.source().dataset();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-prepared-source-v2\0");
    for quad in dataset.quads() {
        hasher.update(quad_anchor(dataset, quad).as_bytes());
        hasher.update(b"\n");
    }
    for (reifier, triple, graph) in dataset.reifiers_with_graph() {
        hasher.update(b"R|");
        hasher.update(term_anchor(dataset, dataset.resolve(reifier)).as_bytes());
        hasher.update(b"|");
        hasher.update(term_anchor(dataset, dataset.resolve(triple)).as_bytes());
        if let Some(graph) = graph {
            hasher.update(b"|");
            hasher.update(term_anchor(dataset, dataset.resolve(graph)).as_bytes());
        }
        hasher.update(b"\n");
    }
    for (reifier, predicate, object, graph) in dataset.annotations_with_graph() {
        hasher.update(b"A|");
        for term in [reifier, predicate, object] {
            hasher.update(term_anchor(dataset, dataset.resolve(term)).as_bytes());
            hasher.update(b"|");
        }
        if let Some(graph) = graph {
            hasher.update(term_anchor(dataset, dataset.resolve(graph)).as_bytes());
        }
        hasher.update(b"\n");
    }
    let mut receipts: Vec<_> = theory
        .source()
        .origins()
        .iter()
        .map(|origin| {
            serde_json::to_vec(&origin.document)
                .expect("source document receipt serialization is infallible")
        })
        .collect();
    receipts.sort_unstable();
    for receipt in receipts {
        hasher.update(&(receipt.len() as u64).to_be_bytes());
        hasher.update(&receipt);
    }
    hasher.finalize().to_hex().to_string()
}

fn quad_anchor(dataset: &RdfDataset, quad: QuadIds) -> String {
    frame(
        "quad",
        [
            term_anchor(dataset, dataset.resolve(quad.s)),
            term_anchor(dataset, dataset.resolve(quad.p)),
            term_anchor(dataset, dataset.resolve(quad.o)),
            quad.g
                .map(|graph| term_anchor(dataset, dataset.resolve(graph)))
                .unwrap_or_default(),
        ],
    )
}

fn node_anchor(dataset: &RdfDataset, node: SourceNode) -> String {
    frame(
        "node",
        [
            term_anchor(dataset, dataset.resolve(node.term)),
            node.graph
                .map(|graph| term_anchor(dataset, dataset.resolve(graph)))
                .unwrap_or_default(),
        ],
    )
}

fn term_anchor(dataset: &RdfDataset, term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(iri) => frame("I", [iri.to_owned()]),
        TermRef::Blank { label, scope } => frame("B", [scope.qualify_label(label).into_owned()]),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => frame(
            "L",
            [
                lexical.to_owned(),
                match dataset.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    other => format!("{other:?}"),
                },
                language.unwrap_or_default().to_owned(),
                direction
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            ],
        ),
        TermRef::Triple { s, p, o } => frame(
            "T",
            [
                term_anchor(dataset, dataset.resolve(s)),
                term_anchor(dataset, dataset.resolve(p)),
                term_anchor(dataset, dataset.resolve(o)),
            ],
        ),
    }
}

fn frame(tag: &str, fields: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from(tag);
    for field in fields {
        out.push('|');
        out.push_str(&field.len().to_string());
        out.push(':');
        out.push_str(&field);
    }
    out
}

fn digest_bytes(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value);
    hasher.finalize().to_hex().to_string()
}

fn blocked(code: impl Into<String>, reason: impl Into<String>) -> SemanticUnitDisposition {
    SemanticUnitDisposition::Blocked {
        code: code.into(),
        reason: reason.into(),
    }
}

fn malformed(code: impl Into<String>, reason: impl Into<String>) -> SemanticUnitDisposition {
    SemanticUnitDisposition::Malformed {
        code: code.into(),
        reason: reason.into(),
    }
}

fn outside(reason: impl Into<String>) -> SemanticUnitDisposition {
    SemanticUnitDisposition::OutsideSelection {
        reason: reason.into(),
    }
}
