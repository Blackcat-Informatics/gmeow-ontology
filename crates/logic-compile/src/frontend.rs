// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Front-end parser: a `logic:` RDF 1.2 source graph → [`LogicProgram`].
//!
//! The `logic:` front-end parser; the Python duplicate
//! (`logic_frontend.py`) has since been retired.  It parses a `logic:`-vocabulary
//! RDF graph (Turtle text or a parsed wasm-clean `RdfDataset`) into a typed
//! [`LogicProgram`] plus a list of [`Diagnostic`] messages.
//!
//! # Parse contract
//!
//! * **Preserve diagnostics** — malformed rules, formulas and contracts are
//!   excluded with error diagnostics. A malformed rule body never becomes a weaker
//!   rule. Recoverable projection/annotation issues retain their named warnings.
//! * **Raise** ([`Diag`]) on unparsable input or supplied range bounds outside the
//!   supported finite numeric contract. Empty graphs and unreadable files also fail.
//! * **Never silently skip** — every skipped element produces a named diagnostic.
//!
//! # Recognised RDF patterns
//!
//! 1. **Axioms** — triples whose predicate is in the `logic:` namespace, and
//!    `rdf:type` triples whose object is a `logic:` class.
//! 2. **Scoped axioms** — RDF 1.2 reifier nodes (`rdf:reifies` → triple term) and
//!    classic `rdf:Statement` reifications carrying `logic:` scope annotations.
//! 3. **Reasoning contracts** — `rdf:type logic:ReasoningContract` /
//!    `logic:ReasoningPreset` declarations, with their facet selection.
//! 4. **Rules** — `logic:Rule` nodes with `logic:head` / `logic:body` /
//!    `logic:negatedBody` / `logic:distinctBody` links.

mod json;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use gmeow_errors::Diag;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermValue, parse_dataset};

mod formula_reader;
pub(crate) use formula_reader::FormulaReader;

mod admission;
mod temporal;
pub use admission::{
    FORMULA_SOURCE_ADMISSION_FRAGMENT, MAX_FORMULA_SOURCE_DEPTH, MAX_FORMULA_SOURCE_EXPANSION,
};
pub use temporal::{FINITE_AT_OR_AFTER, FINITE_NEXT, FINITE_STRICTLY_BEFORE};

mod source_shapes;
use source_shapes::*;
pub use source_shapes::{
    FunctionalCarrierViolation, derive_validation_shapes, functional_carrier_integrity,
    functional_carrier_property_iris, functional_properties_missing_logic_carrier,
    is_authoring_namespace,
};

mod presentation;
mod source_graph;
pub use source_graph::{
    SourceBase, SourceBaseOrigin, SourceCarrier, SourceDocument, SourceDocumentOccurrences,
    SourceEdge, SourceEdgeRole, SourceNode, SourceNodeBinding, SourceOccurrencePosition,
    SourcePredicate, SourceStatementBinding, SourceUnit, SourceUnitKind, StructuralSourceGraph,
};
pub(crate) use source_graph::{
    collection_roles, collection_sources, rewritten_owner_role, source_fields,
};

mod collections;
pub(crate) use collections::collection_origins;
use collections::lift_list_statement;

mod emission;
use emission::{AxiomEmission, EmissionTrace, OwnerEmission, capture_owner, finish_owners};
pub use emission::{
    AxiomSource, CollectionField, CollectionMembers, CollectionRole, CollectionSource,
    CompiledTheory, FormulaDisposition, FormulaLowering, OwnerDisposition, OwnerFamily,
    OwnerLowering, SourceCompilation, SourceOwnerRole,
};

mod claims;
pub use claims::{ClaimCarrier, ClaimResolver, ClaimStatement};

mod prepared;
pub use crate::graphutil::default_graph_pattern as default_source_statements;
pub use crate::graphutil::source_graph_pattern as selected_source_statements;
pub use prepared::PreparedLogicSource;

mod source_admission;
pub use source_admission::{
    AdmittedSemanticUnit, DocumentSelection, FOF_SOURCE_PROFILE, FofSemanticProfile,
    PropertyCharacteristic, RootSelection, SemanticUnitAdmission, SemanticUnitDisposition,
    SemanticUnitKind, SourceAdmission, SourceAdmissionStatus, SourceAssertion,
    SourceClassDefinition, SourceContractDeclaration, SourceGraphSelection, SourceSelection,
};

use super::compat;
use super::graphutil::{
    Iri, Node, RDF_OBJECT, RDF_PREDICATE, RDF_REIFIES, RDF_STATEMENT, RDF_SUBJECT, RDF_TYPE,
    Subject, canonicalize_blank_nodes, default_graph_quads, has_structural_class, is_empty, nn,
    objects, structural_classes, subject_id, subject_is_blank, subject_str,
    subjects_of_structural_class, subjects_with, term_as_subject, term_str, value,
};
use super::ir::{
    ANNOTATION_LIFT_PREDS, AggregateBalance, AggregateComparator, AggregateComparison,
    AggregateRhs, AggregateSpec, ComplexityClass, ConstraintComponent, ConstraintIr,
    ConstraintProvenance, ContextualScope, Correspondence, EvaluationMode, Formula, JoinAggregate,
    JoinLeg, LOGIC_NAMESPACE, LogicAxiom, LogicModality, LogicProgram, LogicRule, NodeKind,
    PathBase, PathShapeIr, PropertyConstraintIr, ReasoningContract, ReasoningProgramIr,
    SemanticProfileId, ShaclNodeKind, ShaclSeverity, ShapeTarget, ShapeValue, Term,
    ValidationShapeIr, VariableSortScope, X_GMEOW_ENGLISH_TAG, annotation_pred_is_load_bearing,
    subject_is_gmeow_authored,
};
use super::restriction;

/// Re-export the CGIF reader alongside CLIF (the conceptual-graph dialect inverse).
pub use crate::cgif::parse_cgif_str;
/// Re-export the CLIF reader so the FOL-text inverse sits alongside the other frontend
/// entry points (`gmeow_logic_compile::frontend::parse_clif_str` resolves, as does the
/// canonical `gmeow_logic_compile::clif::parse_clif_str`).
pub use crate::clif::parse_clif_str;
/// Re-export the XCL reader so the XML-dialect inverse sits alongside CLIF/CGIF
/// (`gmeow_logic_compile::frontend::parse_xcl_str` resolves, as does the canonical
/// `gmeow_logic_compile::xcl::parse_xcl_str`).
pub use crate::xcl::parse_xcl_str;

fn logic_iri(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

fn owner_source(store: &RdfDataset, node: &Subject) -> SourceNode {
    SourceNode {
        term: subject_id(store, node).expect("owner was discovered in this source"),
        graph: None,
    }
}

// --------------------------------------------------------------------------- //
// Diagnostics
// --------------------------------------------------------------------------- //

/// Severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    /// A hard error: accepting the offending structure would change program meaning.
    Error,
    /// A recoverable issue: the offending element was skipped.
    Warning,
    /// Informational note.
    Info,
}

impl Severity {
    /// The canonical token (`"ERROR"` / `"WARNING"` / `"INFO"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
        }
    }
}

/// A structured diagnostic emitted during parsing (mirrors the Python
/// `Diagnostic` dataclass).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    /// Severity token.
    pub severity: Severity,
    /// A short machine-readable code (e.g. `"MALFORMED_AXIOM"`).
    pub code: String,
    /// A human-readable description.
    pub message: String,
    /// The IRI / blank-node id of the problematic element, or `None`.
    pub subject: Option<String>,
}

impl Diagnostic {
    fn warning(code: &str, message: impl Into<String>, subject: Option<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_owned(),
            message: message.into(),
            subject,
        }
    }

    pub(crate) fn error(code: &str, message: impl Into<String>, subject: Option<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_owned(),
            message: message.into(),
            subject,
        }
    }
}

// --------------------------------------------------------------------------- //
// Scope extraction
// --------------------------------------------------------------------------- //

fn confidence_from_term(term: &Node) -> Option<f64> {
    if let Node::Lit(purrdf::RdfLiteral {
        lexical_form: lexical,
        ..
    }) = term
        && let Ok(val) = lexical.parse::<f64>()
        && (0.0..=1.0).contains(&val)
    {
        return Some(val);
    }
    None
}

fn modality_from_term(term: Option<&Node>) -> Option<LogicModality> {
    let Some(term) = term else {
        return Some(LogicModality::None);
    };
    let mut raw = term_str(term);
    if let Some(stripped) = raw.strip_prefix(LOGIC_NAMESPACE) {
        raw = stripped.to_owned();
    }
    LogicModality::from_str_value(&raw.to_lowercase())
}

/// A total, storage-order-independent identity for a scope term: its kind, its
/// canonical text, and the complete term so that terms differing only in
/// datatype, language or blank-node scope keep a distinct, fixed place in the
/// order. The debug text is a sound tiebreak only while `Debug` stays derived
/// for the term types.
fn term_identity(term: &Node) -> (u8, String, String) {
    let kind = match term {
        Node::Iri(_) => 0,
        Node::Blank { .. } => 1,
        Node::Lit(_) => 2,
        Node::Triple(_) => 3,
    };
    (kind, term_str(term), format!("{term:?}"))
}

/// Read every default-graph value of one scope coordinate, ordered by term
/// identity (kind, then canonical text, then the full term) so the result never
/// depends on which physical table the store returned a value from.
///
/// Every distinct term is returned: an IRI, a literal and a blank node that share
/// the same text are three terms here, so that `confidence` and `modality`
/// validate each of them. `scope_from_node` then reduces the values to the ones
/// that produce distinct scopes.
///
/// `Some(empty)` means the coordinate is absent. `None` means the node was
/// refused (a diagnostic has been recorded): a proposition-valued term on any of
/// the values withholds the whole carrier, never just that value.
fn scope_values(
    store: &RdfDataset,
    node: &Subject,
    field: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Node>> {
    let (Some(subject), Some(predicate)) = (
        subject_id(store, node),
        store.term_id_by_iri(&logic_iri(field)),
    ) else {
        return Some(Vec::new());
    };
    let mut values: Vec<Node> =
        crate::graphutil::default_graph_pattern(store, Some(subject), Some(predicate), None)
            .map(|row| crate::graphutil::node_of(store, row.o))
            .collect();
    if values.iter().any(|value| matches!(value, Node::Triple(_))) {
        diagnostics.push(Diagnostic::error(
            "UNSUPPORTED_SCOPE_TERM",
            format!("logic:{field} is a proposition term; this scope requires typed term lowering"),
            Some(subject_str(node)),
        ));
        return None;
    }
    values.sort_by_cached_key(term_identity);
    values.dedup_by_key(|value| term_identity(value));
    Some(values)
}

/// The scope under construction while the coordinates are fanned out.
#[derive(Clone)]
struct ScopeDraft {
    standpoint: Option<String>,
    time: Option<String>,
    confidence: Option<f64>,
    modality: LogicModality,
    provenance: Option<String>,
    module: Option<String>,
}

/// Cross each draft with every value of one coordinate. An absent coordinate
/// (no values) leaves the drafts unchanged.
fn fan_out<T>(
    drafts: Vec<ScopeDraft>,
    values: &[T],
    set: impl Fn(&mut ScopeDraft, &T),
) -> Vec<ScopeDraft> {
    if values.is_empty() {
        return drafts;
    }
    let set = &set;
    drafts
        .into_iter()
        .flat_map(|draft| {
            values.iter().map(move |value| {
                let mut next = draft.clone();
                set(&mut next, value);
                next
            })
        })
        .collect()
}

/// Keep the first term of each distinct text, preserving order. Terms with one
/// text are not adjacent in identity order, so this cannot be an adjacent dedup.
fn first_per_text(values: Vec<Node>) -> Vec<Node> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(term_str(value)))
        .collect()
}

/// Extract every scope dimension together and lower the carrier into its scopes.
///
/// A carrier with one value per coordinate yields one scope. A carrier with
/// several values of exactly one coordinate yields one scope per value, in the
/// order of the values, with every other coordinate shared. Values are distinct
/// exactly when the scopes they produce are distinct. `standpoint`, `time`,
/// `provenance` and `inModule` are held as text in a scope, so terms with one
/// text are one value: the first of them in term-identity order is kept, and the
/// values are ordered by that identity. `confidence` and `modality` validate every
/// term and are then counted by parsed value: `"0.5"` and `"0.50"` are one
/// confidence, and the bare local name and the full IRI of one modality are one
/// modality. Confidences are ordered by numeric value and modalities by the
/// declaration order of `LogicModality`.
/// A carrier with several values of two or more coordinates is refused whole:
/// the node gives no pairing between the values of different coordinates, and
/// their product would assert combinations no author wrote. An invalid value of
/// any coordinate (a confidence outside [0, 1], an unknown modality, a
/// proposition-valued term) also withholds the whole claim or rule; no value is
/// dropped to let the others through. `None` always carries an error diagnostic.
fn scope_from_node(
    store: &RdfDataset,
    node: &Subject,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<ContextualScope>> {
    let start = diagnostics.len();
    let standpoint = scope_values(store, node, "standpoint", diagnostics);
    let time = scope_values(store, node, "time", diagnostics);
    let confidence_nodes = scope_values(store, node, "confidence", diagnostics);
    let modality_nodes = scope_values(store, node, "modality", diagnostics);
    let provenance = scope_values(store, node, "provenance", diagnostics);
    let module = scope_values(store, node, "inModule", diagnostics);
    let (
        Some(standpoint),
        Some(time),
        Some(confidence_nodes),
        Some(modality_nodes),
        Some(provenance),
        Some(module),
    ) = (
        standpoint,
        time,
        confidence_nodes,
        modality_nodes,
        provenance,
        module,
    )
    else {
        return None;
    };
    let [standpoint, time, provenance, module] =
        [standpoint, time, provenance, module].map(first_per_text);

    // Every term is validated; only then are equal parsed values merged, so one
    // invalid spelling still withholds the claim.
    let mut confidences = Vec::with_capacity(confidence_nodes.len());
    for value in &confidence_nodes {
        match confidence_from_term(value) {
            // Exact equality of the parsed value; -0.0 is folded into 0.0 first.
            Some(confidence) => confidences.push(if confidence == 0.0 { 0.0 } else { confidence }),
            None => diagnostics.push(Diagnostic::error(
                "INVALID_CONFIDENCE",
                format!(
                    "confidence value {:?} is not a float in [0, 1]; scope excluded",
                    term_str(value)
                ),
                Some(subject_str(node)),
            )),
        }
    }
    let mut modalities = Vec::with_capacity(modality_nodes.len());
    for value in &modality_nodes {
        match modality_from_term(Some(value)) {
            Some(modality) => modalities.push(modality),
            None => diagnostics.push(Diagnostic::error(
                "UNKNOWN_MODALITY",
                "unrecognized logic:modality; scope excluded",
                Some(subject_str(node)),
            )),
        }
    }

    // Total order on the validated range (no NaN), so the sort cannot panic.
    confidences.sort_by(f64::total_cmp);
    confidences.dedup();
    modalities.sort_unstable();
    modalities.dedup();

    let multi_valued: Vec<&str> = [
        ("standpoint", standpoint.len()),
        ("time", time.len()),
        ("confidence", confidences.len()),
        ("modality", modalities.len()),
        ("provenance", provenance.len()),
        ("inModule", module.len()),
    ]
    .into_iter()
    .filter_map(|(field, count)| (count > 1).then_some(field))
    .collect();
    if multi_valued.len() > 1 {
        let fields: Vec<String> = multi_valued
            .iter()
            .map(|field| format!("logic:{field}"))
            .collect();
        diagnostics.push(Diagnostic::error(
            "AMBIGUOUS_SCOPE_COORDINATES",
            format!(
                "{} each carry several values on one node, which gives no pairing between \
                 them; write one reifier or rule per intended combination",
                fields.join(" and ")
            ),
            Some(subject_str(node)),
        ));
    }
    if diagnostics.len() != start {
        return None;
    }

    let seed = ScopeDraft {
        standpoint: None,
        time: None,
        confidence: None,
        modality: LogicModality::None,
        provenance: None,
        module: None,
    };
    // At most one coordinate has several values (checked above), so crossing the
    // coordinates one at a time fans that coordinate out and nothing else.
    let drafts = fan_out(vec![seed], &standpoint, |d, v| {
        d.standpoint = Some(term_str(v));
    });
    let drafts = fan_out(drafts, &time, |d, v| d.time = Some(term_str(v)));
    let drafts = fan_out(drafts, &confidences, |d, v| d.confidence = Some(*v));
    let drafts = fan_out(drafts, &modalities, |d, v| d.modality = *v);
    let drafts = fan_out(drafts, &provenance, |d, v| {
        d.provenance = Some(term_str(v));
    });
    let drafts = fan_out(drafts, &module, |d, v| d.module = Some(term_str(v)));
    Some(
        drafts
            .into_iter()
            .map(|d| {
                ContextualScope::new(
                    d.standpoint,
                    d.time,
                    d.confidence,
                    d.modality,
                    d.provenance,
                    d.module,
                )
                .expect("every confidence coordinate has been validated")
            })
            .collect(),
    )
}

// --------------------------------------------------------------------------- //
// Axiom extraction
// --------------------------------------------------------------------------- //

/// The set of `logic:` predicate-local names that carry reasoning-contract /
/// preset / closure *meta-configuration*.  When such a predicate's
/// subject is a `logic:ReasoningContract` / `logic:ReasoningPreset` /
/// `logic:ClosureEntry` node, the triple is contract configuration consumed by
/// [`extract_contracts`] — NOT a domain fact — and must NOT leak into `prog.axioms`
/// (where it would pollute the Datalog / N3 / ledger projections).
fn is_facet_config_predicate(prop_local: &str) -> bool {
    FACET_PROPERTIES.contains(&prop_local)
        || matches!(
            prop_local,
            "closureEntry"
                | "closureKey"
                | "closureValue"
                | "closureIndividual"
                | "complexityClass"
        )
}

/// The reserved `logic:` predicate-local names that build the full-FOL formula AST.
/// Like the rule-structural predicates, these are consumed by
/// [`extract_formulas`] to reconstruct [`Formula`] trees and must NOT leak into
/// `prog.axioms` (where they would pollute the Datalog / N3 / ledger projections and
/// break the canonical round-trip).
fn is_formula_structural_predicate(prop_local: &str) -> bool {
    matches!(
        prop_local,
        "hasFormula"
            | "relation"
            | "argument"
            | "and"
            | "or"
            | "not"
            | "antecedent"
            | "consequent"
            | "iff"
            | "forall"
            | "exists"
            | "necessarily"
            | "possibly"
            | "next"
            | "eventually"
            | "globally"
            | "until"
            | "untilLeft"
            | "overAccessibility"
            | "inContext"
            | "queryFormula"
            | "queryContext"
            | "quantifiedVariable"
            | "termIndex"
            | "termIri"
            | "termVariable"
            | "termLiteral"
            | "termLiteralDatatype"
            | "termSequenceMarker"
            | "termApplication"
            | "functionSymbol"
    )
}

/// Recovery-case ownership links are correspondence-calculus structure, not domain
/// axioms.  Their formula tree is reconstructed into [`RecoveryCaseIr`] by the shared
/// correspondence reader; retaining these edges in `LogicProgram::axioms` as well would
/// duplicate one semantic object across the generic projection and correspondence lanes.
fn is_recovery_case_structural_predicate(prop_local: &str) -> bool {
    matches!(prop_local, "recoveryCase" | "recoveryTransform")
}

/// The reserved `logic:` predicate-local names that carry a `logic:Constraint` (or a compact
/// constraint-sugar record) — its integrity/severity/message/formalizes annotations, the sugar
/// pattern parameters, and the aggregate-comparison satellite. Like the formula-structural
/// predicates, these are consumed by the constraint / sugar readers and must NOT leak into
/// `prog.axioms` (where they would pollute the Datalog / N3 / ledger projections). A constraint's
/// integrity formula tree is already excluded (its root is in the referenced set), so this covers
/// the constraint-level annotations and the sugar surface.
fn is_constraint_structural_predicate(prop_local: &str) -> bool {
    matches!(
        prop_local,
        // Core logic:Constraint annotations.
        "integrity" | "severity" | "message" | "formalizes" | "adviceSourceField"
            // Shared sugar target.
            | "onClass"
            // P1 choice-group.
            | "choicePredicate" | "choiceMode"
            // P2 guarded implication.
            | "trigger" | "triggerValue" | "requires"
            // P3 disjunctive requiredness.
            | "anyOf"
            // P4 path-value type / fixed-value membership.
            | "valuePath" | "valueClass" | "valuePredicate" | "valueObject"
            // P5 cross-node co-occurrence / inequality.
            | "roleA" | "roleB" | "crossMode"
            // P7 forbidden-pattern.
            | "forbiddenPredicate" | "forbiddenValue"
            // P8 value-range (inclusive OR exclusive numeric bounds over a path).
            | "minInclusiveBound" | "maxInclusiveBound"
            | "minExclusiveBound" | "maxExclusiveBound"
            // Aggregate-comparison satellite.
            | "aggFunction" | "aggDistinct" | "aggPath" | "aggComparator" | "aggCompareTo"
            // Join-aggregate satellite (multi-hop join + product aggregate + threshold).
            | "joinPath" | "aggThreshold" | "legRecordType" | "legSource" | "legTarget" | "legValue"
            // Aggregate-balance satellite (double-entry balance: partitioned two-sum equality).
            | "balancePostingPredicate" | "balancePartitionPredicate" | "balanceDebitValue"
            | "balanceCreditValue" | "balanceAmountNodePredicate" | "balanceValuePredicate"
            | "balanceGroupPredicate"
            // Comparison constraint (two focus-property values compared).
            | "leftPath" | "rightPath" | "compareOp"
            // Path node-kind constraint.
            | "nodeKind"
            // Self-join uniqueness (no two distinct siblings share a value).
            | "siblingPredicate" | "sharedPredicate"
            // Inverse-existence (must be the object of a predicate from a typed subject).
            | "inversePredicate" | "subjectClass"
            // Transitive reachability / acyclicity (a one-or-more property-path walk).
            | "viaPredicate" | "pathPredicate" | "reachTarget"
            // Value-set membership (sh:in-style enumerated-value restriction over a property).
            | "memberValue" | "membershipMode"
            // String pattern / prefix test over a property value.
            | "stringPattern" | "stringOp"
    )
}

/// The `logic:` class local names that TYPE a constraint or a compact constraint-sugar record.
/// Their `rdf:type` triples are consumed by the constraint / sugar readers and must NOT leak into
/// `prog.axioms` as spurious class-membership facts.
fn is_constraint_sugar_class(local: &str) -> bool {
    matches!(
        local,
        "Constraint"
            | "ChoiceGroupConstraint"
            | "GuardedImplicationConstraint"
            | "DisjunctiveRequirednessConstraint"
            | "PathValueTypeConstraint"
            | "CrossNodeConstraint"
            | "ForbiddenPatternConstraint"
            | "ValueRangeConstraint"
            | "AggregateConstraint"
            | "JoinAggregateConstraint"
            | "JoinLeg"
            | "AggregateBalanceConstraint"
            | "ComparisonConstraint"
            | "PathNodeKindConstraint"
            | "SelfJoinUniquenessConstraint"
            | "InverseExistenceConstraint"
            | "TransitiveReachabilityConstraint"
            | "AcyclicConstraint"
            | "ValueSetMembershipConstraint"
            | "StringPatternConstraint"
            | "UniqueLangConstraint"
    )
}

/// The reserved `logic:` predicate-local names that carry a `logic:AbductiveSchema`'s repair
/// mechanism — the discipline back-link, the repair-strategy selector, and the completeness
/// formula root. Like the constraint annotations, these are consumed by the abductive advice
/// producer (which queries the reasoned RDF dataset directly) and must NOT leak into
/// `prog.axioms`; in particular `completenessFormula` reaches a `logic:Formula` root that is
/// excluded from the top-level formula set in [`extract_formulas`], so authoring a completeness
/// condition never changes what the reasoner entails about the live model.
fn is_abductive_schema_structural_predicate(prop_local: &str) -> bool {
    matches!(
        prop_local,
        "repairsDiscipline" | "repairStrategy" | "completenessFormula"
    )
}

/// The reserved `logic:` predicate-local names that carry a `logic:ReasoningProgram`'s
/// clause set, goal, verdict probes, evaluation-strategy selector, and per-variable
/// order-sort declarations. Like the formula-structural predicates, these are consumed by
/// [`extract_reasoning_programs`] and must NOT leak into `prog.axioms` (where they would
/// pollute the Datalog / N3 / ledger projections and break the canonical round-trip).
fn is_reasoning_program_structural_predicate(prop_local: &str) -> bool {
    matches!(
        prop_local,
        "clause" | "programQuery" | "verdictProbe" | "evaluationMode" | "variableSort"
    )
}

/// The reserved `logic:` predicate-local names that carry a rule's aggregation (reduce) spec.
/// Like the formula-structural predicates, these are consumed by [`extract_rules`] (not
/// [`extract_axioms`]) and must NOT leak into `prog.axioms`.
fn is_rule_aggregation_predicate(prop_local: &str) -> bool {
    matches!(
        prop_local,
        "aggregateFunction" | "aggregateVariable" | "aggregateResult" | "groupKey"
    )
}

/// Every reserved source predicate consumed as typed frontend syntax rather than
/// as an assertion in the logical theory.
///
/// This inventory is deliberately predicate-based. A resource may be both a
/// structural owner and the subject of ordinary domain assertions; declaring the
/// resource as (for example) a `logic:Formula` must never make those unrelated
/// assertions disappear. Keeping the grammar here also gives source admission and
/// extraction one definition of the reserved syntax surface.
fn is_reserved_source_syntax_predicate(prop_local: &str) -> bool {
    is_facet_config_predicate(prop_local)
        || is_formula_structural_predicate(prop_local)
        || is_recovery_case_structural_predicate(prop_local)
        || is_constraint_structural_predicate(prop_local)
        || is_abductive_schema_structural_predicate(prop_local)
        || is_reasoning_program_structural_predicate(prop_local)
        || is_rule_aggregation_predicate(prop_local)
        || matches!(
            prop_local,
            // Class-expression records consumed by the restriction/enumeration
            // readers. The semantic anchor predicates (`subClassOf`, `domain`,
            // `range`, ...) are intentionally absent.
            "onProperty"
                | "someValuesFrom"
                | "allValuesFrom"
                | "hasValue"
                | "onDataRange"
                | "minCardinality"
                | "maxCardinality"
                | "cardinality"
                | "minQualifiedCardinality"
                | "maxQualifiedCardinality"
                | "qualifiedCardinality"
                | "onDatatype"
                | "withRestrictions"
                | "unionOf"
                | "intersectionOf"
                | "disjointUnionOf"
                | "complementOf"
                | "oneOf"
                | "members"
                // Native characteristic/key declarations.
                | "characteristicSort"
                | "keyClass"
                | "keyProperty"
                // Path-shape records.
                | "pathStepPredicate"
                | "pathWildcard"
                | "pathMinDepth"
                | "pathMaxDepth"
                | "pathNamespaceScope"
                | "pathDepthParam"
                // Finite-presentation records. Resource-valued edges are also
                // classified more specifically by `source_graph::edge_spec`.
                | "presentationWorld"
                | "presentationStandpoint"
                | "presentationTime"
                | "presentationPath"
                | "presentationModule"
                | "presentationModality"
                | "symbolName"
                | "individualSymbolRole"
                | "variadicRelationSymbolRole"
                | "variadicFunctionSymbolRole"
                | "relationSymbolArity"
                | "functionSymbolArity"
                | "evidenceOrigin"
                | "preservationKind"
                | "unsupportedConstruct"
                | "sourceBlankLabel"
                | "sourceBlankScope"
                | "sentenceSign"
                | "sentenceKind"
                | "bindingName"
                // Correspondence calculus, law evidence, and certified
                // composition records.
                | "hasCorrespondence"
                | "hasLens"
                | "hasPreservation"
                | "correspondenceRelation"
                | "morphismClass"
                | "morphismKind"
                | "mnemomorphic"
                | "hasDeterminacy"
                | "sourceEndpoint"
                | "targetEndpoint"
                | "evidenceStrength"
                | "weight"
                | "probability"
                | "lawClaimed"
                | "lawDischargeVerdict"
                | "lawDischargeCondition"
                | "hasCaveat"
                | "lossyDrop"
                | "evidenceSource"
                | "evidenceScale"
                | "crossChainProbabilityModel"
                | "compositionAxisRule"
                | "confidenceIndependenceEvidence"
                | "probabilityIndependenceEvidence"
                // Contract-wide probability capability declaration.
                | "probabilityModel"
        )
}

/// Read a rule node's aggregation (reduce) spec, when present. Requires the function, the
/// aggregated variable, and the result variable; the group keys are optional (a reduce with no
/// group key aggregates the whole relation). A node carrying a partial spec is a hard skip with
/// a diagnostic, never a silent drop.
fn aggregation_from_node(
    store: &RdfDataset,
    node: &Subject,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<AggregateSpec> {
    let fields = ["aggregateFunction", "aggregateVariable", "aggregateResult"]
        .map(|local| objects(store, node, &nn(&logic_iri(local))));
    let group_terms = objects(store, node, &nn(&logic_iri("groupKey")));
    if fields.iter().all(Vec::is_empty) && group_terms.is_empty() {
        return None;
    }
    if fields.iter().any(|values| values.len() != 1) {
        diagnostics.push(Diagnostic::error(
            "MALFORMED_RULE_AGGREGATION",
            "logic:Rule aggregation requires exactly one function, input variable and result variable; entire rule excluded",
            Some(subject_str(node)),
        ));
        return None;
    }
    let [Ok(function), Ok(aggregate_var), Ok(result_var)] =
        fields.map(|values| rule_field_text(&values[0]))
    else {
        diagnostics.push(Diagnostic::error(
            "MALFORMED_RULE_AGGREGATION",
            "aggregation fields must be textual; entire rule excluded",
            Some(subject_str(node)),
        ));
        return None;
    };
    let Ok(group_keys) = group_terms
        .iter()
        .map(rule_field_text)
        .collect::<gmeow_errors::Result<Vec<String>>>()
    else {
        diagnostics.push(Diagnostic::error(
            "MALFORMED_RULE_AGGREGATION",
            "aggregation group keys must be textual; entire rule excluded",
            Some(subject_str(node)),
        ));
        return None;
    };
    if !matches!(function.as_str(), "SUM" | "COUNT" | "MIN" | "MAX" | "AVG")
        || !aggregate_var.starts_with('?')
        || !result_var.starts_with('?')
        || group_keys.iter().any(|key| !key.starts_with('?'))
    {
        diagnostics.push(Diagnostic::error(
            "MALFORMED_RULE_AGGREGATION",
            "logic:Rule aggregation requires a supported function and variable input, result and group keys; entire rule excluded",
            Some(subject_str(node)),
        ));
        return None;
    }

    Some(AggregateSpec::new(
        function,
        aggregate_var,
        result_var,
        group_keys,
    ))
}

/// Collect the IRIs / blank-node ids of every subject typed
/// `logic:ReasoningContract`, `logic:ReasoningPreset`, OR `logic:ClosureEntry`.
/// These are the meta-configuration nodes whose facet-config triples must be kept
/// out of the domain axiom set.
///
/// An object of `logic:closureEntry` on a `logic:ReasoningContract` is configuration
/// whether or not it is typed `logic:ClosureEntry`.
fn collect_contract_config_subjects(store: &RdfDataset) -> HashSet<String> {
    let mut subjects: HashSet<String> = HashSet::new();
    for class_local in ["ReasoningContract", "ReasoningPreset", "ClosureEntry"] {
        let class_term = Node::iri(logic_iri(class_local));
        for subj in subjects_of_structural_class(store, &class_term) {
            subjects.insert(subject_str(&subj));
        }
    }
    for contract in subjects_of_structural_class(store, &Node::iri(logic_iri("ReasoningContract")))
    {
        for entry in objects(store, &contract, &nn(&logic_iri("closureEntry"))) {
            if let Some(entry) = term_as_subject(&entry) {
                subjects.insert(subject_str(&entry));
            }
        }
    }
    subjects
}

/// Lift the RDFS/SKOS annotation surface (`ANNOTATION_LIFT_PREDS`) into first-class
/// [`NodeKind::Annotation`] axioms — the inbound half of `logic: isSupersetOf SKOS/RDFS`.
/// Each `<term> <annotation-pred> "literal"@x-gmeow-english` triple becomes one annotation
/// axiom carrying the surface predicate verbatim, so the SKOS/RDFS annotation surface
/// round-trips through the canonical IR and the generated SKOS surface is a projection of
/// these axioms rather than an authored second source.
///
/// The `@x-gmeow-english` carrier discipline is honored: ONLY carrier-tagged literals are
/// lifted — an annotation literal carrying a *different* language tag (an `@en` example
/// label, a foreign literal) or an untagged/typed literal is skipped, never silently
/// retagged. The authoritative fail-closed carrier guard is the structural lint (the
/// validate `x-gmeow-` language-tag check), scoped to the shipped core-term graphs, which
/// flags a genuine core violation; internal terms are always carrier-tagged.
fn extract_annotation_axioms(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AxiomEmission> {
    let mut axioms: Vec<AxiomEmission> = Vec::new();
    for (source_quad, quad) in crate::graphutil::default_graph_quads_with_ids(store) {
        let p_str = quad.predicate.as_str();
        if !ANNOTATION_LIFT_PREDS.contains(&p_str) {
            continue;
        }
        // Annotations belong on named terms; a blank-node subject is a structural
        // interior (a restriction / formula node), never a term carrying a display label.
        if subject_is_blank(&quad.subject) {
            continue;
        }
        // Only GMEOW-authored subjects are lifted: a foreign alignment-target / example
        // subject carries its own external-vocabulary label (@en, …), which is that
        // vocabulary's metadata, not GMEOW's SKOS/RDFS surface, and must not be lifted or
        // carrier-checked (mirrors the structural-lint scoping).
        if !subject_is_gmeow_authored(&subject_str(&quad.subject)) {
            continue;
        }
        let Node::Lit(literal) = &quad.object else {
            // A non-literal object on an annotation predicate is malformed authoring
            // (e.g. rdfs:seeAlso-style IRI object); it is not a lift target.
            continue;
        };
        // Only the internal carrier surface is lifted. Any other language tag (an `@en`
        // example/demonstration label, a foreign literal) or an untagged/typed literal is
        // NOT part of GMEOW's carrier SKOS/RDFS surface, so it is skipped — never lifted and
        // never treated as a hard error here. The authoritative fail-closed carrier-discipline
        // guard is the structural lint (validate-gts), which is scoped to the shipped
        // core-term graphs and flags a genuine core violation (R2/AC2); the compile-logic
        // corpus also carries example/test subjects the lint deliberately does not police, so
        // rejecting their @en annotations here would be stricter than the guard itself.
        if literal.language.as_deref() != Some(X_GMEOW_ENGLISH_TAG) {
            continue;
        }
        match LogicAxiom::new(
            subject_str(&quad.subject),
            p_str,
            crate::ir::AtomicTerm::Literal(literal.clone()),
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(AxiomEmission::new(
                ax.with_node_kind(NodeKind::Annotation)
                    .with_load_bearing(annotation_pred_is_load_bearing(p_str)),
                AxiomSource::statement(source_quad),
            )),
            Err(exc) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_ANNOTATION",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }
    axioms
}

/// Lift only the selected default-graph annotation surface into canonical IR axioms.
///
/// Reuses the full compiler's annotation extractor, literal admission and diagnostics.
/// Named annotation subjects and literal objects need no structural blank-node identity,
/// so this operation neither canonicalizes the source graph nor constructs a logic program.
/// Full source compilation and its diagnostic publication remain separate obligations.
/// A nonempty source with no selected annotations has an empty, valid selection.
///
/// # Errors
/// Returns the existing frontend error for an empty source. Recoverable annotation
/// diagnostics retain their original severity, code, message and subject in the result.
pub fn lift_annotation_axioms(
    dataset: &RdfDataset,
) -> Result<(Vec<LogicAxiom>, Vec<Diagnostic>), Diag> {
    require_nonempty_source(dataset)?;
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    let mut axioms: Vec<LogicAxiom> = extract_annotation_axioms(dataset, &mut diagnostics)
        .into_iter()
        .map(|emission| emission.axiom)
        .filter(|axiom| seen.insert(content_dedup_key(axiom)))
        .collect();
    axioms.sort_by_cached_key(LogicAxiom::sort_key);
    Ok((axioms, diagnostics))
}

fn extract_axioms(
    store: &RdfDataset,
    source_graph: &StructuralSourceGraph,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AxiomEmission> {
    let mut axioms: Vec<AxiomEmission> = Vec::new();

    // Meta-config subjects (contracts / presets / closure entries): facet-config
    // triples on these are contract configuration, not domain facts.
    let config_subjects = collect_contract_config_subjects(store);
    // Presentation syntax belongs to the typed declaration catalog. It must not
    // additionally assert its signed sentences or configuration as global facts.
    let presentation_subjects: HashSet<_> = source_graph
        .units()
        .filter(|unit| {
            unit.node.graph.is_none()
                && unit
                    .declared_kinds
                    .iter()
                    .chain(&unit.required_kinds)
                    .any(presentation::owns_kind)
        })
        .map(|unit| unit.node.term)
        .collect();

    // The declaration and scope satellites of a Rule belong to its typed IR.
    // Re-emitting the source declaration as a domain axiom creates a second,
    // headless Rule when a projection mints the executable rule's canonical IRI.
    let rule_subjects: HashSet<String> =
        subjects_of_structural_class(store, &Node::iri(logic_iri("Rule")))
            .iter()
            .map(subject_str)
            .collect();
    let mut reifier_subjects: HashSet<_> =
        subjects_with(store, &nn(RDF_TYPE), &Node::iri(RDF_STATEMENT))
            .iter()
            .map(subject_str)
            .collect();
    if let Some(predicate) = store.term_id_by_iri(RDF_REIFIES) {
        reifier_subjects.extend(
            crate::graphutil::default_graph_pattern(store, None, Some(predicate), None)
                .map(|quad| subject_str(&crate::graphutil::subject_of(store, quad.s))),
        );
    }

    // Lift anonymous dependencies through one shared owner map. Restriction keys
    // include the final filler identities; no parse-local blank label may enter
    // them. Retry only pending nodes, retaining diagnostics only at the fixed point.
    let logic_vocab = restriction::RestrictionVocab::logic();
    let mut pending_restrictions: BTreeMap<String, Subject> =
        restriction::restriction_nodes(store, &logic_vocab)
            .into_iter()
            .map(|node| (subject_str(&node), node))
            .collect();
    let mut anchored: BTreeSet<String> = pending_restrictions.keys().cloned().collect();
    anchored.extend(restriction::enumeration_node_labels(store, &logic_vocab));
    anchored.extend(restriction::datarange_node_labels(store, &logic_vocab));
    let mut skolems: BTreeMap<String, String> = BTreeMap::new();
    // Enumerations and facet dataranges retain their existing admitted boundaries.
    let mut lifted_class_exprs =
        restriction::skolemize_enumerations(store, &logic_vocab, diagnostics, &mut skolems);
    lifted_class_exprs.extend(restriction::skolemize_dataranges(
        store,
        &logic_vocab,
        diagnostics,
        &mut skolems,
    ));
    let anonymous_owners = crate::lists::anonymous_owner_nodes(store, &anchored);
    let mut pending_owners = anonymous_owners.clone();
    loop {
        let pending_before = pending_restrictions.len() + pending_owners.len();
        let mut deferred_diagnostics = Vec::new();
        let lifted_restrictions = restriction::skolemize_restrictions(
            store,
            &logic_vocab,
            &pending_restrictions,
            &mut deferred_diagnostics,
            &mut skolems,
        );
        let resolved_sources: BTreeSet<_> = lifted_restrictions
            .iter()
            .flat_map(|triple| {
                triple
                    .sources
                    .iter()
                    .map(|source| source.source_node().term)
            })
            .collect();
        pending_restrictions.retain(|_, node| {
            !resolved_sources.contains(&subject_id(store, node).expect("selected restriction root"))
        });
        lifted_class_exprs.extend(lifted_restrictions);
        lifted_class_exprs.extend(crate::lists::skolemize_anonymous_owners(
            store,
            &pending_owners,
            &mut skolems,
            &mut deferred_diagnostics,
        ));
        pending_owners.retain(|label, _| !skolems.contains_key(label));
        let pending_after = pending_restrictions.len() + pending_owners.len();
        if pending_after == 0 {
            break;
        }
        if pending_after == pending_before {
            diagnostics.extend(deferred_diagnostics);
            break;
        }
    }
    // Failed owners are already diagnosed. Do not retain an anchor emitted by
    // their successful child as a weaker, blank-labelled parent representation.
    lifted_class_exprs.retain(|triple| {
        !pending_restrictions.contains_key(&triple.subject)
            && !pending_owners.contains_key(&triple.subject)
    });
    // Anchors can themselves be anonymous owners resolved in a later wave.
    // Redirect them only after every ready dependency has obtained its identity.
    for triple in &mut lifted_class_exprs {
        if let Some(iri) = skolems.get(&triple.subject) {
            triple.subject.clone_from(iri);
        }
    }
    let mut rnodes = anchored.clone();
    rnodes.extend(anonymous_owners.keys().cloned());
    for lifted in lifted_class_exprs {
        if let Ok(ax) = LogicAxiom::new(
            lifted.subject,
            lifted.predicate,
            lifted.obj,
            false,
            ContextualScope::default(),
        ) {
            axioms.push(AxiomEmission {
                axiom: ax,
                sources: lifted.sources,
            });
        }
    }

    // 1. Domain triples with a logic: predicate. Canonical structural typing is
    // handled with RDF typing in pass 2 so both surfaces share one ownership check.
    for (source_quad, quad) in crate::graphutil::default_graph_quads_with_ids(store) {
        if presentation_subjects.contains(&source_quad.s) {
            continue;
        }
        let p_str = quad.predicate.as_str();
        if !p_str.starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        if matches!(quad.object, Node::Triple(_)) {
            diagnostics.push(Diagnostic::error(
                "UNSUPPORTED_AXIOM_TERM",
                "a proposition object requires typed axiom lowering; it cannot be stringified",
                Some(subject_str(&quad.subject)),
            ));
            continue;
        }
        if p_str == "https://blackcatinformatics.ca/logic/instanceOf"
            && matches!(&quad.object, Node::Iri(class) if class.starts_with(LOGIC_NAMESPACE))
        {
            continue;
        }
        // Class-expression internals + anchor edges are owned by the skolemizers above.
        // Skip a triple whose subject is an anonymous class-expression node, and a
        // subClassOf / equivalentClass edge whose object is a restriction, enumeration or
        // datarange node (re-emitted redirected to the skolem node).
        if rnodes.contains(&subject_str(&quad.subject)) {
            continue;
        }
        let p_local = &p_str[LOGIC_NAMESPACE.len()..];
        if reifier_subjects.contains(&subject_str(&quad.subject))
            && matches!(
                p_local,
                "standpoint" | "time" | "confidence" | "modality" | "provenance" | "inModule"
            )
        {
            continue;
        }
        if rule_subjects.contains(&subject_str(&quad.subject))
            && matches!(
                p_local,
                "head"
                    | "body"
                    | "negatedBody"
                    | "distinctBody"
                    | "standpoint"
                    | "time"
                    | "confidence"
                    | "modality"
                    | "provenance"
                    | "inModule"
            )
        {
            continue;
        }
        if matches!(p_local, "subClassOf" | "equivalentClass")
            && anchored.contains(&term_str(&quad.object))
        {
            continue;
        }
        // Skip contract/preset/closure facet-config triples: they are consumed by
        // extract_contracts and must not pollute the domain axiom set.
        if is_facet_config_predicate(p_local)
            && config_subjects.contains(&subject_str(&quad.subject))
        {
            continue;
        }
        // Formula-AST structural triples are consumed by extract_formulas; they are
        // never domain facts.
        if is_formula_structural_predicate(p_local) {
            continue;
        }
        if is_recovery_case_structural_predicate(p_local) {
            continue;
        }
        // Rule-aggregation triples (the reduce spec carried on a logic:Rule node) are consumed
        // by extract_rules; they are rule structure, never domain facts, and must not pollute
        // the axiom set or the canonical round-trip.
        if is_rule_aggregation_predicate(p_local) {
            continue;
        }
        // Constraint / sugar structural triples are consumed by the constraint + sugar readers;
        // they are never domain facts.
        if is_constraint_structural_predicate(p_local) {
            continue;
        }
        // Reasoning-program structural triples are consumed by extract_reasoning_programs;
        // they are never domain facts.
        if is_reasoning_program_structural_predicate(p_local) {
            continue;
        }
        // Abductive-schema structural triples are consumed by the abductive advice producer;
        // they are never domain facts (and completenessFormula's root is excluded from the
        // top-level formula set in extract_formulas).
        if is_abductive_schema_structural_predicate(p_local) {
            continue;
        }
        // Any other edge naming an anonymous class expression names its lifted identity.
        // One that could not be lifted was disclosed by its skolemizer and is not
        // referenced through a dangling blank node.
        let object = match &quad.object {
            Node::Blank { label, .. } => match skolems.get(label) {
                Some(iri) => Node::Iri(iri.clone()),
                None if rnodes.contains(label) => continue,
                None => quad.object.clone(),
            },
            other => other.clone(),
        };
        // A list-valued constructor, or any other edge whose object is an RDF list,
        // carries its list in the single IR encoding (see `crate::lists`).
        let list_ctor = crate::lists::constructor(p_str);
        if list_ctor.is_some()
            || (crate::lists::is_list_head(store, &quad.object)
                && !matches!(&quad.object, Node::Iri(iri) if iri == crate::lists::RDF_NIL))
        {
            lift_list_statement(
                store,
                source_quad,
                &quad.subject,
                p_str,
                list_ctor.unwrap_or(&crate::lists::UNREGISTERED_LIST),
                &quad.object,
                &skolems,
                &mut axioms,
                diagnostics,
            );
            continue;
        }
        match native_axiom(
            subject_str(&quad.subject),
            p_str,
            &object,
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(AxiomEmission::new(ax, AxiomSource::statement(source_quad))),
            Err(exc) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_AXIOM",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }

    // 2. Both admitted typing surfaces for a canonical logic class. Preserve the
    // authored predicate on domain axioms; record discovery never equates the relations.
    let mut reported_orphan_cases = HashSet::new();
    for (source_quad, quad) in crate::graphutil::default_graph_quads_with_ids(store) {
        if presentation_subjects.contains(&source_quad.s) {
            continue;
        }
        let type_predicate = quad.predicate.as_str();
        if type_predicate != RDF_TYPE
            && !(type_predicate == "https://blackcatinformatics.ca/logic/instanceOf"
                && matches!(&quad.object, Node::Iri(class) if class.starts_with(LOGIC_NAMESPACE)))
        {
            continue;
        }
        // The `<r> rdf:type logic:Restriction` typing is owned by the skolemizer.
        if rnodes.contains(&subject_str(&quad.subject)) {
            continue;
        }
        let o_str = term_str(&quad.object);
        if !o_str.starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        // Skip the type triple that DEFINES a contract-config subject
        // (`?s rdf:type logic:{ReasoningContract,ReasoningPreset,ClosureEntry}`): it is
        // consumed by extract_contracts, exactly like the facet-config predicates in
        // step 1. Retaining it (for a non-`logic:` subject, whose type triple step 1's
        // predicate filter never reaches) re-extracts as a spurious empty contract on
        // every round-trip — a canonical-RDF-1.2 non-idempotence.
        let o_local = &o_str[LOGIC_NAMESPACE.len()..];
        if matches!(
            o_local,
            "ReasoningContract" | "ReasoningPreset" | "ClosureEntry"
        ) && config_subjects.contains(&subject_str(&quad.subject))
        {
            continue;
        }
        // A `logic:Formula` type triple declares a typed formula-tree node and is owned by
        // `extract_formulas`. Keeping it as a generic class-membership axiom as well would give
        // the same authored node two IR homes; the CL writer would then emit a constructorless
        // duplicate under the source IRI alongside the content-addressed formula tree.
        // A contextual request likewise belongs to its source-owned evaluation;
        // its declaration is not a domain membership asserted by that evaluation.
        if matches!(
            o_local,
            "Formula" | "TermCarrier" | "Rule" | "ContextualEvaluationRequest"
        ) {
            continue;
        }
        // A `logic:ReasoningProgram` type triple declares a typed reasoning-program node and
        // is owned by `extract_reasoning_programs`, exactly like Formula/TermCarrier above.
        if o_local == "ReasoningProgram" {
            continue;
        }
        // A `logic:RecoveryCase` type triple is owned by the correspondence that reaches it
        // via `logic:recoveryCase`, exactly like Formula/TermCarrier above. A RecoveryCase that
        // is NOT reached that way is unowned recovery evidence — an authoring error, not
        // something to swallow — so it is hard-failed here instead of silently vanishing.
        if o_local == "RecoveryCase" {
            let case_iri = subject_str(&quad.subject);
            if !source_graph.recovery_is_owned(SourceNode {
                term: subject_id(store, &quad.subject).expect("source subject is present"),
                graph: None,
            }) && reported_orphan_cases.insert(case_iri.clone())
            {
                diagnostics.push(Diagnostic::error(
                    "ORPHAN_RECOVERY_CASE",
                    format!(
                        "{case_iri:?} is typed logic:RecoveryCase but is not referenced by any \
                         logic:Correspondence via logic:recoveryCase; unowned recovery evidence \
                         must not disappear silently"
                    ),
                    Some(case_iri),
                ));
            }
            continue;
        }
        // A `logic:Constraint` / constraint-sugar type triple is consumed by the constraint +
        // sugar readers; it is never a domain class-membership fact.
        if is_constraint_sugar_class(o_local) {
            continue;
        }
        // A compiler-minted anonymous class expression / n-ary axiom keeps its typing: it is
        // part of the node's content (re-read from the canonical projection), not `logic:`
        // vocabulary self-description.
        if type_predicate == RDF_TYPE
            && subject_str(&quad.subject).starts_with(LOGIC_NAMESPACE)
            && !crate::lists::is_minted_owner(&subject_str(&quad.subject))
        {
            continue;
        }
        match LogicAxiom::new(
            subject_str(&quad.subject),
            type_predicate,
            crate::ir::AtomicTerm::resource(o_str),
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(AxiomEmission::new(ax, AxiomSource::statement(source_quad))),
            Err(exc) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_AXIOM",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }

    axioms
}

// --------------------------------------------------------------------------- //
// RDF 1.2 + classic reified-statement scope extraction
// --------------------------------------------------------------------------- //

fn extract_scoped_axioms(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<AxiomEmission> {
    let mut axioms: Vec<AxiomEmission> = Vec::new();
    let mut rule_atoms = BTreeSet::new();
    for rule in subjects_of_structural_class(store, &Node::iri(logic_iri("Rule"))) {
        for link in ["head", "body", "negatedBody"] {
            for atom in objects(store, &rule, &nn(&logic_iri(link))) {
                if let Some(atom) = term_as_subject(&atom)
                    && let Some(id) = subject_id(store, &atom)
                {
                    rule_atoms.insert(id);
                }
            }
        }
    }

    // RDF 1.2 style: reifier node with rdf:reifies → triple term.
    if let Some(predicate) = store.term_id_by_iri(RDF_REIFIES) {
        for quad in crate::graphutil::default_graph_pattern(store, None, Some(predicate), None) {
            let reifier = crate::graphutil::subject_of(store, quad.s);
            if rule_atoms.contains(&quad.s) {
                continue;
            }
            let Some(scopes) = scope_from_node(store, &reifier, diagnostics) else {
                continue;
            };
            // Quotation alone never asserts its proposition. The same explicit
            // scope admission applies to both native and classic reification.
            // A carrier whose only scope is the default stays an unasserted
            // quotation, so `none` beside another modality yields only the other claim.
            let scopes: Vec<_> = scopes
                .into_iter()
                .filter(|scope| *scope != ContextualScope::default())
                .collect();
            if scopes.is_empty() {
                continue;
            }
            if let Node::Triple(triple) = crate::graphutil::node_of(store, quad.o) {
                if matches!(triple.object, Node::Triple(_)) {
                    diagnostics.push(Diagnostic::error(
                        "UNSUPPORTED_SCOPED_TERM",
                        "a nested proposition requires typed term lowering; \
                         it cannot be stringified into a scoped axiom",
                        Some(subject_str(&reifier)),
                    ));
                    continue;
                }
                // One claim per scope, all anchored to the authored reifier. The
                // claim is withheld whole: every scope's axiom is built first and
                // none is emitted unless all of them are.
                let lowered: gmeow_errors::Result<Vec<_>> = scopes
                    .into_iter()
                    .map(|scope| native_quoted_axiom(store, &reifier, &triple, false, scope))
                    .collect();
                match lowered {
                    Ok(lowered) => axioms.extend(lowered.into_iter().map(|ax| {
                        AxiomEmission::new(
                            ax,
                            AxiomSource::Reification(SourceNode {
                                term: quad.s,
                                graph: quad.g,
                            }),
                        )
                    })),
                    Err(exc) => diagnostics.push(Diagnostic::error(
                        "MALFORMED_SCOPED_AXIOM",
                        exc.message().to_owned(),
                        Some(subject_str(&reifier)),
                    )),
                }
            }
        }
    }

    // Classic reification: rdf:Statement nodes with logic: scope annotations.
    let rdf_type_term = Node::iri(RDF_STATEMENT);
    for stmt in subjects_with(store, &nn(RDF_TYPE), &rdf_type_term) {
        if subject_id(store, &stmt).is_some_and(|id| rule_atoms.contains(&id)) {
            continue;
        }
        let Some(scopes) = scope_from_node(store, &stmt, diagnostics) else {
            continue;
        };
        // A carrier whose only scope is the default stays an unasserted quotation,
        // so `none` beside another modality yields only the other claim.
        let scopes: Vec<_> = scopes
            .into_iter()
            .filter(|scope| *scope != ContextualScope::default())
            .collect();
        if scopes.is_empty() {
            continue;
        }
        let t_p = value(store, &stmt, &nn(RDF_PREDICATE));
        let Some(t_p) = t_p else {
            diagnostics.push(Diagnostic::error(
                "MISSING_PREDICATE",
                "rdf:Statement node has no rdf:predicate; skipped",
                Some(subject_str(&stmt)),
            ));
            continue;
        };
        let Node::Iri(p_str) = t_p else {
            diagnostics.push(Diagnostic::error(
                "MALFORMED_SCOPED_AXIOM",
                "scoped axiom rdf:predicate must be an IRI",
                Some(subject_str(&stmt)),
            ));
            continue;
        };
        let t_s = value(store, &stmt, &nn(RDF_SUBJECT));
        let Some(subject @ (Node::Iri(_) | Node::Blank { .. })) = t_s.as_ref() else {
            diagnostics.push(Diagnostic::error(
                "MALFORMED_SCOPED_AXIOM",
                "scoped axiom rdf:subject must be an IRI or blank node",
                Some(subject_str(&stmt)),
            ));
            continue;
        };
        let subj = term_str(subject);
        let t_o = value(store, &stmt, &nn(RDF_OBJECT));
        let Some(object) = t_o.as_ref() else {
            diagnostics.push(Diagnostic::error(
                "MALFORMED_SCOPED_AXIOM",
                "scoped axiom requires rdf:object",
                Some(subject_str(&stmt)),
            ));
            continue;
        };
        // One claim per scope, all anchored to the authored statement node. The
        // claim is withheld whole: every scope's axiom is built first and none is
        // emitted unless all of them are.
        let lowered: gmeow_errors::Result<Vec<_>> = scopes
            .into_iter()
            .map(|scope| native_axiom(subj.clone(), p_str.clone(), object, false, scope))
            .collect();
        match lowered {
            Ok(lowered) => axioms.extend(lowered.into_iter().map(|ax| {
                AxiomEmission::new(
                    ax,
                    AxiomSource::Reification(SourceNode {
                        term: subject_id(store, &stmt).expect("selected reification root"),
                        graph: None,
                    }),
                )
            })),
            Err(exc) => diagnostics.push(Diagnostic::error(
                "MALFORMED_SCOPED_AXIOM",
                exc.message().to_owned(),
                Some(subject_str(&stmt)),
            )),
        }
    }

    axioms
}

// --------------------------------------------------------------------------- //
// Reasoning-contract extraction
// --------------------------------------------------------------------------- //

/// The direct facet properties whose object is a single facet value individual.
/// Each is read AND the `logic:expandsToFacet` bundle is read; every value is
/// routed into the right contract field by its `rdf:type` facet-class.
const FACET_PROPERTIES: [&str; 16] = [
    "formulaFragment",
    "modelSemantics",
    "truthAlgebra",
    "admissibleValuation",
    "designatedValues",
    "evolution",
    "argumentation",
    "revision",
    "equalityPolicy",
    "defaultClosure",
    "negationOperator",
    "contextAxis",
    "uncertaintyMeasure",
    "resourcePolicy",
    "projectionTarget",
    "expandsToFacet",
];

/// The local name of `value`'s `rdf:type` that is a facet value-class (i.e. not
/// `owl:NamedIndividual`, not a preset/contract type). Returns the first matching
/// recognised facet class, or `None` if the value carries no recognised facet type.
fn facet_class_of(store: &RdfDataset, value_iri: &str) -> Option<String> {
    let subject = Subject::Iri(value_iri.to_owned());
    for ty in structural_classes(store, &subject) {
        let ty_str = term_str(&ty);
        if let Some(local) = ty_str.strip_prefix(LOGIC_NAMESPACE)
            && is_facet_class(local)
        {
            return Some(local.to_owned());
        }
    }
    None
}

/// The facet value-class a DIRECT facet property routes to, independent of the
/// value individual's `rdf:type`.  This is the round-trip path: the
/// canonical RDF 1.2 projection emits facet selections as bare
/// `logic:<facetProp> logic:<Value>` triples WITHOUT re-typing each value
/// individual, so the parser routes by the property name itself.  `expandsToFacet`
/// is intentionally absent — it carries mixed facet kinds and must route by the
/// value's `rdf:type` ([`facet_class_of`]).
fn facet_class_for_property(prop_local: &str) -> Option<&'static str> {
    Some(match prop_local {
        "formulaFragment" => "FormulaFragment",
        "modelSemantics" => "ModelSemantics",
        "truthAlgebra" => "TruthAlgebra",
        "admissibleValuation" => "AdmissibleValuationPolicy",
        "designatedValues" => "DesignatedValueSet",
        "evolution" => "EvolutionMode",
        "argumentation" => "ArgumentationSemantics",
        "revision" => "RevisionPolicy",
        "equalityPolicy" => "EqualityPolicy",
        "defaultClosure" => "ClosureValue",
        "negationOperator" => "NegationOperator",
        "contextAxis" => "ContextAxis",
        "uncertaintyMeasure" => "UncertaintyMeasure",
        "resourcePolicy" => "ResourcePolicy",
        "projectionTarget" => "ProjectionTarget",
        _ => return None,
    })
}

/// Whether `local` is one of the recognised facet value-class local names.
fn is_facet_class(local: &str) -> bool {
    matches!(
        local,
        "FormulaFragment"
            | "ModelSemantics"
            | "TruthAlgebra"
            | "AdmissibleValuationPolicy"
            | "DesignatedValueSet"
            | "NegationOperator"
            | "ClosureValue"
            | "ContextAxis"
            | "EvolutionMode"
            | "UncertaintyMeasure"
            | "ArgumentationSemantics"
            | "RevisionPolicy"
            | "EqualityPolicy"
            | "ResourcePolicy"
            | "ProjectionTarget"
    )
}

/// Route a facet value (its local name + its facet-class local name) into the
/// correct field of `contract`. Single-valued facets reject conflicting values;
/// repeated identical declarations and the set-valued facets remain idempotent.
/// Refusals join the source compiler's error observations with their exact owner.
fn route_facet_value(
    contract: &mut ReasoningContract,
    facet_class: &str,
    value_local: String,
    owner: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let singleton = match facet_class {
        "FormulaFragment" => Some(&mut contract.formula_fragment),
        "ModelSemantics" => Some(&mut contract.model_semantics),
        "TruthAlgebra" => Some(&mut contract.truth_algebra),
        "AdmissibleValuationPolicy" => Some(&mut contract.admissible_valuation),
        "DesignatedValueSet" => Some(&mut contract.designated_values),
        "EvolutionMode" => Some(&mut contract.evolution),
        "ArgumentationSemantics" => Some(&mut contract.argumentation),
        "RevisionPolicy" => Some(&mut contract.revision),
        "EqualityPolicy" => Some(&mut contract.equality_policy),
        "ClosureValue" => Some(&mut contract.default_closure),
        _ => None,
    };
    if let Some(slot) = singleton {
        if let Some(previous) = slot.as_ref() {
            if previous != &value_local {
                diagnostics.push(Diagnostic::error(
                    "INVALID_CONTRACT_FACET",
                    format!("single-valued {facet_class} has conflicting values {previous:?} and {value_local:?}"),
                    Some(owner.to_owned()),
                ));
                return;
            }
        } else {
            *slot = Some(value_local);
        }
        return;
    }
    let values = match facet_class {
        "NegationOperator" => &mut contract.negation_operators,
        "ContextAxis" => &mut contract.context_axes,
        "UncertaintyMeasure" => &mut contract.uncertainty_measures,
        "ResourcePolicy" => &mut contract.resource_policies,
        "ProjectionTarget" => &mut contract.projection_targets,
        _ => {
            diagnostics.push(Diagnostic::error(
                "INVALID_CONTRACT_FACET",
                format!("unknown contract facet class {facet_class:?}"),
                Some(owner.to_owned()),
            ));
            return;
        }
    };
    values.insert(value_local);
}

/// A closure entry has one key and one value; never select an arbitrary field.
fn closure_field(
    store: &RdfDataset,
    entry: &Subject,
    property: &str,
    owner: &str,
) -> Result<Option<Node>, Diagnostic> {
    let mut values = objects(store, entry, &nn(&logic_iri(property))).into_iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.any(|value| value != first) {
        return Err(Diagnostic::error(
            "AMBIGUOUS_CLOSURE_ENTRY",
            format!(
                "reasoning contract {owner:?} closure entry {:?} has multiple distinct logic:{property} values",
                subject_str(entry)
            ),
            Some(owner.to_owned()),
        ));
    }
    Ok(Some(first))
}

/// Admit a complete source key before interpreting its exact semantic name.
fn closure_key(store: &RdfDataset, entry: &Subject, owner: &str) -> Result<String, Diagnostic> {
    let term = closure_field(store, entry, "closureKey", owner)?.ok_or_else(|| {
        Diagnostic::error(
            "MALFORMED_CLOSURE_ENTRY",
            format!(
                "closure entry {:?} missing logic:closureKey",
                subject_str(entry)
            ),
            Some(owner.to_owned()),
        )
    })?;
    match term {
        Node::Iri(iri) => Ok(iri),
        other => selector_literal_value(other, "closureKey").map_err(|error| {
            Diagnostic::error(
                "MALFORMED_CLOSURE_ENTRY",
                format!(
                    "closure entry {:?}: {}",
                    subject_str(entry),
                    error.message()
                ),
                Some(owner.to_owned()),
            )
        }),
    }
}

/// Extract configuration owners with their source emissions and diagnostic observations.
/// Invalid facets remain errors on their owner; declarations never become domain axioms.
fn extract_contracts(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<ReasoningContract>> {
    let mut contracts: Vec<OwnerEmission<ReasoningContract>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Subjects typed logic:ReasoningContract OR logic:ReasoningPreset.
    let mut subjects: Vec<Subject> = Vec::new();
    for class_local in ["ReasoningContract", "ReasoningPreset"] {
        let class_term = Node::iri(logic_iri(class_local));
        subjects.extend(subjects_of_structural_class(store, &class_term));
    }

    for individual in subjects {
        let iri_str = subject_str(&individual);
        if !seen.insert(iri_str.clone()) {
            continue;
        }

        if let Some(emitted) = capture_owner(
            owner_source(store, &individual),
            OwnerFamily::Contract,
            diagnostics,
            lowerings,
            |diagnostics| {
                let mut contract = ReasoningContract::new();

                // If it is a recognised preset, record its preset id; an unrecognised
                // preset is a hard-skip with a diagnostic (behaviour-preserving).
                let is_preset = has_structural_class(
                    store,
                    &individual,
                    &Node::iri(logic_iri("ReasoningPreset")),
                );
                if is_preset {
                    let preset = iri_str
                        .strip_prefix(LOGIC_NAMESPACE)
                        .and_then(SemanticProfileId::from_local);
                    match preset {
                        Some(p) => contract.preset = Some(p),
                        None => {
                            // Greenfield: an unrecognised preset reference is
                            // a hard error, not a silent approximation to a nearby preset.
                            diagnostics.push(Diagnostic::error(
                                "UNKNOWN_PROFILE",
                                format!(
                                    "{iri_str:?} is declared as logic:ReasoningPreset but is not a \
                             recognised named individual; skipped"
                                ),
                                Some(iri_str.clone()),
                            ));
                            return Err(Vec::new());
                        }
                    }
                }

                // Direct facet properties + the expandsToFacet bundle. A DIRECT facet
                // property (everything but expandsToFacet) routes by the PROPERTY name —
                // so the rdf:type-less canonical RDF 1.2 projection round-trips; the
                // value's own facet-class rdf:type, when present, is used as a fallback /
                // for expandsToFacet (which carries mixed facet kinds).
                for prop_local in FACET_PROPERTIES {
                    for value_term in objects(store, &individual, &nn(&logic_iri(prop_local))) {
                        let value_iri = term_str(&value_term);
                        let value_local = value_iri
                            .strip_prefix(LOGIC_NAMESPACE)
                            .unwrap_or(&value_iri)
                            .to_owned();
                        let facet_class = facet_class_for_property(prop_local)
                            .map(str::to_owned)
                            .or_else(|| facet_class_of(store, &value_iri));
                        match facet_class {
                    Some(facet_class) => {
                        route_facet_value(&mut contract, &facet_class, value_local, &iri_str, diagnostics);
                    }
                    None => diagnostics.push(Diagnostic::error(
                        "UNKNOWN_PROFILE",
                        format!(
                            "{iri_str:?} references {value_iri:?} via logic:{prop_local}, but it \
                             is not a recognised facet value; ignored"
                        ),
                        Some(iri_str.clone()),
                    )),
                }
                    }
                }

                // Closure entries: logic:closureEntry → ClosureEntry node with
                // logic:closureKey (IRI or literal name) + logic:closureValue (ClosureValue individual).
                for entry_term in objects(store, &individual, &nn(&logic_iri("closureEntry"))) {
                    // HARD verdict: a malformed closure entry — a non-node
                    // object, or a node missing logic:closureKey / logic:closureValue — is a
                    // Severity::Error (consistent with UNSUPPORTED_CONTRACT above), never a
                    // silent skip, so the compile Report is not ok.
                    let Some(entry_node) = term_as_subject(&entry_term) else {
                        diagnostics.push(Diagnostic::error(
                    "MALFORMED_CLOSURE_ENTRY",
                    format!(
                        "reasoning contract {iri_str:?} has a logic:closureEntry whose object \
                         {:?} is not a node (expected a logic:ClosureEntry with logic:closureKey \
                         + logic:closureValue)",
                        term_str(&entry_term)
                    ),
                    Some(iri_str.clone()),
                ));
                        continue;
                    };
                    // The contract closure map binds a key for the whole contract; an
                    // entry scoped to one individual is world evidence for the
                    // foundation evaluator and has no contract-wide reading.
                    if !objects(store, &entry_node, &nn(&logic_iri("closureIndividual"))).is_empty()
                    {
                        diagnostics.push(Diagnostic::error(
                            "MALFORMED_CLOSURE_ENTRY",
                            format!(
                                "reasoning contract {iri_str:?} closure entry {:?} is scoped to \
                                 an individual by logic:closureIndividual, which a contract-wide \
                                 closure map cannot bind",
                                subject_str(&entry_node)
                            ),
                            Some(iri_str.clone()),
                        ));
                        continue;
                    }
                    let key = match closure_key(store, &entry_node, &iri_str) {
                        Ok(key) => Some(key),
                        Err(diagnostic) => {
                            diagnostics.push(diagnostic);
                            continue;
                        }
                    };
                    let val = match closure_field(store, &entry_node, "closureValue", &iri_str) {
                        Ok(value) => value,
                        Err(diagnostic) => {
                            diagnostics.push(diagnostic);
                            continue;
                        }
                    }
                    .map(|t| {
                        let v = term_str(&t);
                        v.strip_prefix(LOGIC_NAMESPACE).unwrap_or(&v).to_owned()
                    });
                    match (key, val) {
                        (Some(key), Some(val)) => {
                            if let Some(previous) = contract.closure_entries.get(&key) {
                                if previous != &val {
                                    diagnostics.push(Diagnostic::error(
                                        "AMBIGUOUS_CLOSURE_ENTRY",
                                        format!("reasoning contract {iri_str:?} has conflicting closure values {previous:?} and {val:?} for key {key:?}"),
                                        Some(iri_str.clone()),
                                    ));
                                }
                            } else {
                                contract.closure_entries.insert(key, val);
                            }
                        }
                        (key, val) => {
                            let mut missing: Vec<&str> = Vec::new();
                            if key.is_none() {
                                missing.push("logic:closureKey");
                            }
                            if val.is_none() {
                                missing.push("logic:closureValue");
                            }
                            diagnostics.push(Diagnostic::error(
                                "MALFORMED_CLOSURE_ENTRY",
                                format!(
                                    "reasoning contract {iri_str:?} has a logic:closureEntry node \
                             {:?} missing {}",
                                    subject_str(&entry_node),
                                    missing.join(" + ")
                                ),
                                Some(iri_str.clone()),
                            ));
                        }
                    }
                }

                // Carried decidability data: logic:complexityClass.
                if let Some(cn) = value(store, &individual, &nn(&logic_iri("complexityClass"))) {
                    let label = term_str(&cn).trim().to_owned();
                    match ComplexityClass::new(label.clone()) {
                        Ok(cc) => contract.complexity = Some(cc),
                        Err(_) => diagnostics.push(Diagnostic::warning(
                            "INVALID_COMPLEXITY_CLASS",
                            format!(
                                "complexityClass {label:?} is not a recognised ComplexityClass \
                         value; ignored"
                            ),
                            Some(iri_str.clone()),
                        )),
                    }
                }

                // ── Compatibility feature model ─────────────────────────────────────
                // HARD verdict: an unsupported contract is a Severity::Error
                // finding, so the compile Report is not ok and the program is never
                // silently approximated to a nearby semantics.
                if let compat::ContractVerdict::Unsupported(reasons) = compat::check(&contract) {
                    diagnostics.push(Diagnostic::error(
                        "UNSUPPORTED_CONTRACT",
                        format!(
                            "reasoning contract {iri_str:?} is not soundly evaluable: {}",
                            reasons.join("; ")
                        ),
                        Some(iri_str.clone()),
                    ));
                }

                // This reads a configuration declaration, not a query request.
                // The probabilistic evaluator requires the selected query's
                // model; an arbitrary model elsewhere in this graph cannot
                // license execution, nor must a reusable declaration provide one.

                Ok(contract)
            },
        ) {
            contracts.push(emitted);
        }
    }

    contracts
}

// --------------------------------------------------------------------------- //
// Rule extraction (forward-compatible: absent logic:Rule → empty list)
// --------------------------------------------------------------------------- //

/// Read a textual rule field without panicking or erasing a triple-valued term.
/// Rich typed terms require an admitted typed atom representation; the string-based
/// Horn record cannot silently reinterpret them as an IRI or plain literal.
fn rule_field_text(term: &Node) -> gmeow_errors::Result<String> {
    if matches!(term, Node::Triple(_)) {
        return Err(Diag::of_kind(crate::error::Frontend::new(
            "triple-valued rule field is not representable by the selected Horn atom record"
                .to_owned(),
        )));
    }
    Ok(term_str(term))
}

/// Read one required structural field without choosing among conflicting values.
fn required_atom_field(
    store: &RdfDataset,
    node: &Subject,
    predicate: &str,
) -> gmeow_errors::Result<Node> {
    let mut fields = objects(store, node, &nn(predicate));
    if fields.len() != 1 {
        return Err(Diag::of_kind(crate::error::Frontend::new(format!(
            "reified atom requires exactly one {predicate}; found {}",
            fields.len()
        ))));
    }
    Ok(fields.remove(0))
}

fn native_axiom(
    subject: impl Into<String>,
    predicate: impl Into<String>,
    object: &Node,
    negated: bool,
    scope: ContextualScope,
) -> gmeow_errors::Result<LogicAxiom> {
    LogicAxiom::new(
        subject,
        predicate,
        crate::graphutil::atomic_object(object)?,
        negated,
        scope,
    )
}

/// Read a resource-valued rule term through the shared, typed carrier reader.
fn rule_resource_term(
    store: &RdfDataset,
    owner: &Subject,
    resource: &Subject,
) -> gmeow_errors::Result<String> {
    match formula_reader::read_atomic_carrier(store, owner, resource)? {
        Some(crate::ir::AtomicTerm::Var(value) | crate::ir::AtomicTerm::Iri(value)) => Ok(value),
        Some(crate::ir::AtomicTerm::Blank(value)) => Ok(format!("_:{value}")),
        Some(crate::ir::AtomicTerm::Literal(_)) => Err(Diag::of_kind(crate::error::Frontend::new(
            "a rule subject or predicate cannot be a literal".into(),
        ))),
        None => Ok(subject_str(resource)),
    }
}

/// Read one RDF 1.2 statement-layer target. Missing or ambiguous targets are
/// errors; quotation preserves the atom without asserting its proposition.
fn read_reified_axiom(
    store: &RdfDataset,
    node: &Subject,
    negated: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> gmeow_errors::Result<LogicAxiom> {
    let target = required_atom_field(store, node, RDF_REIFIES)?;
    let Node::Triple(triple) = target else {
        return Err(Diag::of_kind(crate::error::Frontend::new(
            "rule atom rdf:reifies must target a triple term".into(),
        )));
    };
    let mut scopes = scope_from_node(store, node, diagnostics).ok_or_else(|| {
        Diag::of_kind(crate::error::Frontend::new(
            "invalid rule atom scope".into(),
        ))
    })?;
    // A rule atom is one conjunct of its rule, so it carries exactly one scope:
    // lowering several scopes of one atom into several atoms would turn the rule
    // into a conjunction, a different rule. A carrier refused by `scope_from_node`
    // for two or more multi-valued coordinates reaches this site as the earlier
    // error above, not this one.
    if scopes.len() != 1 {
        diagnostics.push(Diagnostic::error(
            "AMBIGUOUS_RULE_ATOM_SCOPE",
            "a rule atom carries several values of one scope coordinate; write one rule \
             per intended atom value, each atom with its own reifier (a scope on the rule \
             node scopes the rule as a whole, not its atoms)",
            Some(subject_str(node)),
        ));
        return Err(Diag::of_kind(crate::error::Frontend::new(
            "rule atom scope has several values".into(),
        )));
    }
    let scope = scopes.remove(0);
    native_quoted_axiom(store, node, &triple, negated, scope)
}

fn native_quoted_axiom(
    store: &RdfDataset,
    node: &Subject,
    triple: &crate::graphutil::TripleTerm,
    negated: bool,
    scope: ContextualScope,
) -> gmeow_errors::Result<LogicAxiom> {
    let subject = rule_resource_term(store, node, &triple.subject)?;
    let predicate =
        rule_resource_term(store, node, &Subject::Iri(triple.predicate.as_str().into()))?;
    let object = match &triple.object {
        Node::Iri(_) | Node::Blank { .. } => {
            let carrier = term_as_subject(&triple.object).expect("resource term");
            formula_reader::read_atomic_carrier(store, node, &carrier)?
                .unwrap_or(crate::graphutil::atomic_object(&triple.object)?)
        }
        _ => crate::graphutil::atomic_object(&triple.object)?,
    };
    LogicAxiom::new(subject, predicate, object, negated, scope)
}

fn extract_rules(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<LogicRule>> {
    let mut rules: Vec<OwnerEmission<LogicRule>> = Vec::new();

    let logic_rule = Node::iri(logic_iri("Rule"));
    let logic_head = nn(&logic_iri("head"));
    let logic_body = nn(&logic_iri("body"));
    let logic_negated_body = nn(&logic_iri("negatedBody"));
    let logic_distinct_body = nn(&logic_iri("distinctBody"));

    for rule_node in subjects_of_structural_class(store, &logic_rule) {
        if let Some(emitted) = capture_owner(
            owner_source(store, &rule_node),
            OwnerFamily::Rule,
            diagnostics,
            lowerings,
            |diagnostics| {
                let diagnostic_start = diagnostics.len();
                let Some(scopes) = scope_from_node(store, &rule_node, diagnostics) else {
                    return Err(Vec::new());
                };

                // Head.
                let heads = objects(store, &rule_node, &logic_head);
                let Some(head_term) = heads.first() else {
                    diagnostics.push(Diagnostic::error(
                        "MISSING_RULE_HEAD",
                        "logic:Rule node has no logic:head; skipped",
                        Some(subject_str(&rule_node)),
                    ));
                    return Err(Vec::new());
                };
                if heads.len() != 1 {
                    diagnostics.push(Diagnostic::error(
                        "MALFORMED_RULE_HEAD",
                        "logic:Rule requires exactly one logic:head; ambiguous rule excluded",
                        Some(subject_str(&rule_node)),
                    ));
                    return Err(Vec::new());
                }
                let Some(head_node) = term_as_subject(head_term) else {
                    diagnostics.push(Diagnostic::error(
                        "MALFORMED_RULE_HEAD",
                        "logic:head must name an RDF 1.2 reifier; skipped",
                        Some(subject_str(&rule_node)),
                    ));
                    return Err(Vec::new());
                };
                let head_axiom = match read_reified_axiom(store, &head_node, false, diagnostics) {
                    Ok(ax) => ax,
                    Err(msg) => {
                        let message = msg.message().to_owned();
                        diagnostics.push(Diagnostic::error(
                            "MALFORMED_RULE_HEAD",
                            message,
                            Some(subject_str(&rule_node)),
                        ));
                        return Err(Vec::new());
                    }
                };

                // Body (positive then negated).
                let mut body_axioms: Vec<LogicAxiom> = Vec::new();
                for (body_predicate, negated) in [(&logic_body, false), (&logic_negated_body, true)]
                {
                    for body_term in objects(store, &rule_node, body_predicate) {
                        let Some(body_node) = term_as_subject(&body_term) else {
                            diagnostics.push(Diagnostic::error(
                                "MALFORMED_RULE_BODY",
                                "logic:body must name an RDF 1.2 reifier; entire rule excluded",
                                Some(subject_str(&rule_node)),
                            ));
                            continue;
                        };
                        match read_reified_axiom(store, &body_node, negated, diagnostics) {
                            Ok(ax) => body_axioms.push(ax),
                            Err(msg) => {
                                let message = msg.message().to_owned();
                                diagnostics.push(Diagnostic::error(
                                    "MALFORMED_RULE_BODY",
                                    message,
                                    Some(subject_str(&rule_node)),
                                ));
                            }
                        }
                    }
                }

                // Ordered variable pairs retain RDF-term identity inequality. The
                // procedural termDistinct relation instead compares SPARQL values.
                let mut distinct_pairs: Vec<(String, String)> = Vec::new();
                for distinct_term in objects(store, &rule_node, &logic_distinct_body) {
                    let Some(distinct_node) = term_as_subject(&distinct_term) else {
                        diagnostics.push(Diagnostic::error(
                            "MALFORMED_RULE_BODY",
                            "logic:distinctBody must name a guard argument structure; entire rule excluded",
                            Some(subject_str(&rule_node)),
                        ));
                        continue;
                    };
                    let pair = match formula_reader::read_rule_distinct_pair(store, &distinct_node)
                    {
                        Ok(guard) => guard,
                        Err(error) => {
                            diagnostics.push(Diagnostic::error(
                                "MALFORMED_RULE_BODY",
                                error.message(),
                                Some(subject_str(&rule_node)),
                            ));
                            continue;
                        }
                    };
                    distinct_pairs.push(pair);
                }

                let aggregation = aggregation_from_node(store, &rule_node, diagnostics);
                // One rule per scope of the authored rule node.
                let lowered: Vec<LogicRule> = scopes
                    .into_iter()
                    .map(|scope| {
                        let rule = LogicRule::new(
                            head_axiom.clone(),
                            body_axioms.clone(),
                            distinct_pairs.clone(),
                            scope,
                        );
                        match &aggregation {
                            Some(agg) => rule.with_aggregation(agg.clone()),
                            None => rule,
                        }
                    })
                    .collect();
                // A malformed conjunct, guard, aggregation or scope cannot turn this into
                // a weaker rule. Preserve all diagnostics, and withhold the whole rule.
                let rule_diagnostics = &mut diagnostics[diagnostic_start..];
                if rule_diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity != Severity::Info)
                {
                    for diagnostic in rule_diagnostics {
                        if diagnostic.severity == Severity::Warning {
                            diagnostic.severity = Severity::Error;
                        }
                    }
                    return Err(Vec::new());
                }
                Ok(lowered)
            },
        ) {
            // Every lowered rule keeps the authored rule node as its source.
            for value in emitted.value {
                rules.push(OwnerEmission {
                    source: emitted.source,
                    family: emitted.family,
                    diagnostics: emitted.diagnostics.clone(),
                    value,
                    record_support: Default::default(),
                });
            }
        }
    }

    rules
}

// --------------------------------------------------------------------------- //
// Public API
// --------------------------------------------------------------------------- //

// --------------------------------------------------------------------------- //
// Full first-order formula extraction (absent logic:Formula → empty list)
// --------------------------------------------------------------------------- //

/// The sub-formula link predicates: a `logic:Formula` reached through any of these is a
/// COMPONENT of another formula, so it is not a top-level assertion (`logic:hasFormula`).
/// `necessarily`/`possibly` are the modal-operator body links: a modal node's body is a
/// component reached by the standard-translation expansion, never a free-standing top-level
/// formula.
pub const FORMULA_SUBLINKS: [&str; 15] = [
    "not",
    "and",
    "or",
    "antecedent",
    "consequent",
    "iff",
    "forall",
    "exists",
    "necessarily",
    "possibly",
    "next",
    "eventually",
    "globally",
    "until",
    "untilLeft",
];

/// The base world at which a top-level modal `logic:Formula` is evaluated: the standard
/// translation ST(φ, w) begins here and threads a fresh world variable through each modal
/// operator. A module-level constant so the parse-time expansion and any downstream reader
/// name the same actual world.
const ACTUAL_WORLD_IRI: &str = "https://blackcatinformatics.ca/logic/actualWorld";

/// The six typed accessibility relations a modal node may pin on `logic:overAccessibility`.
/// The bare `logic:accessibleFrom` superproperty is intentionally EXCLUDED: it licenses no
/// cross-type entailment, so a well-formed modal node must pin exactly one typed relation and
/// the standard translation is taken over it, never over the blurred union.
const MODAL_ACCESSIBILITY_RELATIONS: [&str; 6] = [
    "https://blackcatinformatics.ca/logic/epistemicallyPossible",
    "https://blackcatinformatics.ca/logic/doxasticallyAccessible",
    "https://blackcatinformatics.ca/logic/deonticallyIdeal",
    "https://blackcatinformatics.ca/logic/temporallySucceeds",
    "https://blackcatinformatics.ca/logic/counterfactuallyCloser",
    "https://blackcatinformatics.ca/gmeow/sharpens",
];

/// Read top-level `logic:Formula` trees into [`Formula`]s. A malformed node emits an
/// error-grade `MALFORMED_FORMULA` finding and is skipped: accepting a partial or ambiguous
/// tree would change its logical meaning. A returned formula MAY be trivially-Horn (a reified
/// ordinary triple) — the caller ([`parse_logic_dataset`]) partitions those out to
/// [`LogicProgram::axioms`] via [`Formula::as_horn_axiom`] so the `with_formulas` invariant
/// is enforced by routing, not by assuming the projection never authors one.
struct SourceFormula {
    source: SourceNode,
    formula: Formula,
}

struct FormulaExtraction {
    presentation_formulas: BTreeMap<SourceNode, std::sync::Arc<Formula>>,
    formulas: Vec<SourceFormula>,
    malformed: BTreeMap<String, usize>,
    lowerings: Vec<FormulaLowering>,
}

fn extract_formulas(
    reader: &mut FormulaReader<'_>,
    source_graph: &StructuralSourceGraph,
    diagnostics: &mut Vec<Diagnostic>,
) -> FormulaExtraction {
    let store = reader.dataset();
    let formula_ty = Node::iri(logic_iri("Formula"));
    let subjects = subjects_of_structural_class(store, &formula_ty);

    // Ownership claims remain distinct from mathematical backing references.
    let mut member_uses = HashSet::new();
    for edge in source_graph
        .edges()
        .iter()
        .filter(|edge| edge.source.graph.is_none())
    {
        if edge.predicate.iri(store) == "https://blackcatinformatics.ca/math/memberCondition"
            && member_uses.insert((edge.source, edge.target))
        {
            let formula = source_graph::focus(store, edge.target);
            diagnostics.push(Diagnostic::warning(
                "OWNER_SCOPED_MEMBER_CONDITION",
                format!("membership condition {formula} belongs to set expression {}; its denotation requires owner-aware evaluation and is not an independent assertion", source_graph::focus(store, edge.source.term)),
                Some(formula),
            ));
        }
    }

    // Validate every declared formula, not only roots. This catches a cycle whose every node is
    // referenced (and therefore has no root), as well as malformed constraint-owned subtrees.
    // Only returned roots need an owned tree here. The reader bounds intermediate reuse.
    let presentation_roots: BTreeSet<_> = source_graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.source.graph.is_none()
                && edge.predicate.iri(store) == logic_iri("presentationFormula")
        })
        .map(|edge| SourceNode {
            term: edge.target,
            graph: None,
        })
        .collect();
    let mut presentation_formulas = BTreeMap::new();
    let mut formulas = Vec::new();
    let mut lowerings: Vec<_> = source_graph
        .units()
        .filter(|unit| {
            unit.node.graph.is_some() && unit.declared_kinds.contains(&SourceUnitKind::Formula)
        })
        .map(|unit| FormulaLowering {
            source: unit.node,
            disposition: FormulaDisposition::OutsideDefaultGraph,
        })
        .collect();
    let mut malformed: BTreeMap<String, (&str, String, Vec<SourceNode>)> = BTreeMap::new();
    for subj in &subjects {
        let source = SourceNode {
            term: subject_id(store, subj)
                .expect("declared formula belongs to the selected dataset"),
            graph: None,
        };
        match reader.read(subj) {
            Ok(f) => {
                if !source_graph.formula_is_owned(source) {
                    formulas.push(SourceFormula { source, formula: f });
                } else {
                    if presentation_roots.contains(&source) {
                        presentation_formulas.insert(source, std::sync::Arc::new(f));
                    }
                    lowerings.push(FormulaLowering {
                        source,
                        disposition: FormulaDisposition::ReadForOwner,
                    });
                }
            }
            Err(error) => {
                let focus = error
                    .inner()
                    .source_ctx
                    .focus
                    .as_ref()
                    .map(|focus| focus.0.clone())
                    .unwrap_or_else(|| subject_str(subj));
                malformed
                    .entry(focus)
                    .or_insert_with(|| {
                        (
                            if error.code() == crate::error::FormulaAdmission::register() {
                                "FORMULA_ADMISSION_EXHAUSTED"
                            } else {
                                "MALFORMED_FORMULA"
                            },
                            error.message().to_owned(),
                            Vec::new(),
                        )
                    })
                    .2
                    .push(source);
            }
        }
    }

    let mut malformed_indices = BTreeMap::new();
    for (focus, (code, message, sources)) in &malformed {
        let diagnostic = diagnostics.len();
        malformed_indices.insert(focus.clone(), diagnostic);
        diagnostics.push(Diagnostic::error(code, message, Some(focus.clone())));
        lowerings.extend(sources.iter().map(|&source| FormulaLowering {
            source,
            disposition: FormulaDisposition::Malformed { diagnostic },
        }));
    }

    FormulaExtraction {
        presentation_formulas,
        formulas,
        malformed: malformed_indices,
        lowerings,
    }
}

/// Borrowed source coordinate shared by formula admission and reconstruction.
trait FormulaSource {
    type Dataset: purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized;
    fn dataset(&self) -> &Self::Dataset;
    fn source_graph(&self) -> GraphMatch<<Self::Dataset as DatasetView>::Id>;
    fn observe(&self, _quad: purrdf::QuadIds<<Self::Dataset as DatasetView>::Id>) {}
}

impl<D: purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized> FormulaSource for D {
    type Dataset = D;
    fn dataset(&self) -> &D {
        self
    }
    fn source_graph(&self) -> GraphMatch<D::Id> {
        GraphMatch::Default
    }
}

#[derive(Clone)]
struct SelectedFormulaGraph<
    'a,
    D: purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized = RdfDataset,
> {
    dataset: &'a D,
    graph: GraphMatch<D::Id>,
    reads: Option<formula_reader::ReadRecorder<D::Id>>,
}

impl<D: purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized> FormulaSource
    for SelectedFormulaGraph<'_, D>
{
    type Dataset = D;
    fn dataset(&self) -> &D {
        self.dataset
    }
    fn source_graph(&self) -> GraphMatch<D::Id> {
        self.graph
    }
    fn observe(&self, quad: purrdf::QuadIds<D::Id>) {
        if let Some(reads) = &self.reads {
            reads.row(quad);
        }
    }
}

/// Every admitted object of a `logic:<link>` property in the selected source.
fn formula_objects(store: &impl FormulaSource, node: &Subject, link: &str) -> Vec<Node> {
    crate::graphutil::objects_in_graph_observed(
        store.dataset(),
        node,
        &nn(&logic_iri(link)),
        store.source_graph(),
        |quad| store.observe(quad),
    )
}

/// Attach a typed frontend failure to its exact source focus.
fn formula_err_for(focus: impl Into<String>, detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Frontend::new(detail.into())).with_focus(focus)
}

fn formula_err(node: &Subject, detail: impl Into<String>) -> Diag {
    formula_err_for(subject_str(node), detail)
}

/// Recursively reconstruct a [`Formula`] rooted at `node`.
///
/// The parser is deliberately strict: every node has exactly one constructor family; singleton
/// constructors have exactly one child; `and`/`or` have at least two children; `iff` has exactly
/// two; implication has one antecedent and one consequent; and recursive cycles are rejected.
pub(crate) fn parse_formula(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<Formula> {
    FormulaReader::new(store).read(node)
}

/// Reconstruct the [`Formula`] rooted at the `logic:Formula` node named `root_iri` from `store`.
///
/// The public entry for consumers that hold a formula-root IRI resolved from the reasoned RDF
/// dataset and need its first-order [`Formula`] IR without compiling the whole document — notably
/// the abductive advice producer, which reads a `logic:AbductiveSchema`'s
/// `logic:completenessFormula` root. That root is deliberately kept out of the top-level formula
/// set (see [`extract_formulas`]), so it is unreachable through `parse_logic_*`; this reconstructs
/// exactly the one subtree, applying the same strict well-formedness checks as every other formula.
pub fn reconstruct_formula(store: &RdfDataset, root_iri: &str) -> gmeow_errors::Result<Formula> {
    parse_formula(store, &Subject::Iri(root_iri.to_owned()))
}

/// Reconstruct a contextual query through the canonical shared reader. The
/// selected context scopes every connective; nested context selectors and modal
/// binders retain their own scope. Reading a query asserts no formula.
pub fn reconstruct_formula_in_context(
    store: &(impl purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized),
    root_iri: &str,
    context_iri: &str,
) -> gmeow_errors::Result<Formula> {
    FormulaReader::new(store)
        .read_in_context(&Subject::Iri(root_iri.to_owned()), Term::iri(context_iri)?)
}

/// Reconstruct and admit a contextual query from exactly one named source graph.
/// Missing formula fields cannot be supplied by default or foreign graphs.
pub fn reconstruct_formula_in_named_context(
    dataset: &(impl purrdf::DatasetView<ReadError = std::convert::Infallible> + ?Sized),
    source_graph_iri: &str,
    root_iri: &str,
    context_iri: &str,
) -> gmeow_errors::Result<Formula> {
    let graph = crate::graphutil::resident(
        dataset.term_id_by_value(&TermValue::Iri(source_graph_iri.to_owned())),
    )
    .ok_or_else(|| {
        formula_err_for(
            root_iri,
            format!("selected formula source graph {source_graph_iri} is absent"),
        )
    })?;
    FormulaReader::in_graph(dataset, GraphMatch::Named(graph))
        .read_in_context(&Subject::Iri(root_iri.to_owned()), Term::iri(context_iri)?)
}

// --------------------------------------------------------------------------- //
// Reasoning-program extraction (`logic:ReasoningProgram`)
// --------------------------------------------------------------------------- //

/// Collect `(variable name, sort IRI)` pairs from every `logic:variableSort`-bearing
/// `logic:TermCarrier` reachable from `node`, a `logic:Formula` tree already known
/// well-formed ([`read_reasoning_program`] only calls this after [`parse_formula`] returned
/// `Ok` for the same node). Appends into `out`; duplicate or conflicting pairs are resolved
/// by [`ReasoningProgramIr::new`], not here.
fn collect_variable_sorts(
    store: &RdfDataset,
    node: &Subject,
    out: &mut Vec<(String, String)>,
) -> gmeow_errors::Result<()> {
    for link in FORMULA_SUBLINKS {
        for obj in formula_objects(store, node, link) {
            if let Some(child) = term_as_subject(&obj) {
                collect_variable_sorts(store, &child, out)?;
            }
        }
    }
    for link in ["argument", "quantifiedVariable"] {
        for carrier_term in formula_objects(store, node, link) {
            if let Some(carrier) = term_as_subject(&carrier_term) {
                collect_carrier_variable_sort(store, &carrier, out)?;
            }
        }
    }
    Ok(())
}

/// Record a `logic:TermCarrier`'s `logic:variableSort` (when the carrier holds a
/// `logic:termVariable`), then recurse into a `logic:termApplication` carrier's own
/// argument carriers so a sort declared on a nested compound-term variable (`s(X)`,
/// `cons(H, T)`) is not missed.
fn collect_carrier_variable_sort(
    store: &RdfDataset,
    carrier: &Subject,
    out: &mut Vec<(String, String)>,
) -> gmeow_errors::Result<()> {
    let var_values = formula_objects(store, carrier, "termVariable");
    if let Some(var_value) = var_values.first() {
        let var_name = term_str(var_value);
        for sort_obj in formula_objects(store, carrier, "variableSort") {
            let Node::Iri(sort_iri) = sort_obj else {
                return Err(formula_err(
                    carrier,
                    format!(
                        "logic:TermCarrier {} requires an IRI-valued logic:variableSort",
                        subject_str(carrier)
                    ),
                ));
            };
            out.push((var_name.clone(), sort_iri));
        }
    }
    if let Some(app_term) = formula_objects(store, carrier, "termApplication")
        .into_iter()
        .next()
        && let Some(app_node) = term_as_subject(&app_term)
    {
        for carrier_term in formula_objects(store, &app_node, "argument") {
            if let Some(nested) = term_as_subject(&carrier_term) {
                collect_carrier_variable_sort(store, &nested, out)?;
            }
        }
    }
    Ok(())
}

/// Collect every `Term::Iri` CONSTANT appearing in argument position across `f`'s tree
/// (never `Formula::Atom`'s own `relation` — a reified predicate/relation IRI, not a
/// sortable individual — see [`ReasoningProgramIr::constant_sorts`]), appending into `out`.
/// Recurses into `Term::App` argument lists so a constant nested inside a function-term
/// application (`s(one)`, `cons(a, cons(b, nil))`) is not missed, and into every
/// `Formula` connective/quantifier so a constant anywhere in a rule's
/// antecedent/consequent is captured. A pure in-memory walk of the already-parsed
/// [`Formula`]/[`Term`] AST — no second RDF-link traversal, unlike
/// [`collect_variable_sorts`] (which must walk `logic:TermCarrier` links because a
/// variable's declared sort lives on the carrier, not in the `Term` AST itself).
fn collect_constant_iris(f: &Formula, out: &mut Vec<String>) {
    match f {
        Formula::Atom { args, .. } => {
            for a in args {
                collect_constant_iris_from_term(a, out);
            }
        }
        Formula::Not(inner) => collect_constant_iris(inner, out),
        Formula::And(fs) | Formula::Or(fs) => {
            for g in fs {
                collect_constant_iris(g, out);
            }
        }
        Formula::Implies(a, b) | Formula::Iff(a, b) => {
            collect_constant_iris(a, out);
            collect_constant_iris(b, out);
        }
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            collect_constant_iris(body, out);
        }
    }
}

/// The [`Term`] half of [`collect_constant_iris`]: an IRI is a constant; a compound
/// application recurses into its arguments; a variable/literal/sequence-marker carries no
/// constant identity to sort-type.
fn collect_constant_iris_from_term(t: &Term, out: &mut Vec<String>) {
    match t {
        Term::Iri(iri) => out.push(iri.clone()),
        Term::App { args, .. } => {
            for a in args {
                collect_constant_iris_from_term(a, out);
            }
        }
        Term::Var(_) | Term::Literal(_) | Term::SequenceMarker(_) => {}
    }
}

/// Reconstruct one [`ReasoningProgramIr`] rooted at a `logic:ReasoningProgram` node, or
/// return the reason it is malformed (surfaced as one `MALFORMED_REASONING_PROGRAM` error
/// diagnostic by [`extract_reasoning_programs`], which then skips the program). Reuses
/// [`parse_formula`] for every clause/query/probe root — there is no second clause reader —
/// and [`collect_variable_sorts`] to harvest each clause/query's `logic:variableSort`
/// declarations.
fn read_reasoning_program(
    reader: &mut FormulaReader<'_>,
    node: &Subject,
) -> gmeow_errors::Result<ReasoningProgramIr> {
    let store = reader.dataset();
    let iri = subject_str(node);

    // Clause set: zero-or-more logic:clause roots, each an existing logic:Formula tree.
    // Cardinality (at least one) is enforced by ReasoningProgramIr::new, not duplicated here.
    let mut clause_nodes = Vec::new();
    for obj in formula_objects(store, node, "clause") {
        let clause_node = term_as_subject(&obj).ok_or_else(|| {
            formula_err(
                node,
                format!("logic:ReasoningProgram {iri} has a non-resource logic:clause object"),
            )
        })?;
        clause_nodes.push(clause_node);
    }
    let mut clauses = Vec::with_capacity(clause_nodes.len());
    for clause_node in &clause_nodes {
        clauses.push(reader.read(clause_node)?);
    }

    // Goal: exactly one logic:programQuery root — never zero, never more than one.
    let query_objs = formula_objects(store, node, "programQuery");
    if query_objs.len() != 1 {
        return Err(formula_err(
            node,
            format!(
                "logic:ReasoningProgram {iri} requires exactly one logic:programQuery; found {}",
                query_objs.len()
            ),
        ));
    }
    let query_node = term_as_subject(&query_objs[0]).ok_or_else(|| {
        formula_err(
            node,
            format!("logic:ReasoningProgram {iri} has a non-resource logic:programQuery object"),
        )
    })?;
    let query = reader.read(&query_node)?;

    // Verdict probes: zero-or-more logic:verdictProbe roots. Atomicity is enforced by
    // ReasoningProgramIr::new, not duplicated here.
    let mut probe_nodes = Vec::new();
    for obj in formula_objects(store, node, "verdictProbe") {
        let probe_node = term_as_subject(&obj).ok_or_else(|| {
            formula_err(
                node,
                format!(
                    "logic:ReasoningProgram {iri} has a non-resource logic:verdictProbe object"
                ),
            )
        })?;
        probe_nodes.push(probe_node);
    }
    let mut verdict_probes = Vec::with_capacity(probe_nodes.len());
    for probe_node in &probe_nodes {
        verdict_probes.push(reader.read(probe_node)?);
    }

    // Evaluation strategy: exactly one logic:evaluationMode, drawn from the closed
    // logic:EvaluationMode set. Absent, duplicated, or unrecognized is a hard fail — NEVER a
    // silent default to logic:BackwardEvaluation.
    let mode_objs = formula_objects(store, node, "evaluationMode");
    if mode_objs.len() != 1 {
        return Err(formula_err(
            node,
            format!(
                "logic:ReasoningProgram {iri} requires exactly one logic:evaluationMode; found {}",
                mode_objs.len()
            ),
        ));
    }
    let Node::Iri(mode_iri) = &mode_objs[0] else {
        return Err(formula_err(
            node,
            format!("logic:ReasoningProgram {iri} requires an IRI-valued logic:evaluationMode"),
        ));
    };
    let mode_local = mode_iri
        .strip_prefix(LOGIC_NAMESPACE)
        .unwrap_or(mode_iri.as_str());
    let mode = EvaluationMode::from_local(mode_local).ok_or_else(|| {
        formula_err(
            node,
            format!(
                "logic:ReasoningProgram {iri} logic:evaluationMode {mode_iri:?} is not a \
                 recognized logic:EvaluationMode value (only logic:BackwardEvaluation is \
                 supported today)"
            ),
        )
    })?;

    // Per-variable order-sort declarations, harvested from the clause set's and query's term
    // carriers (logic:variableSort on a logic:TermCarrier). Both are already known
    // well-formed (parse_formula above returned Ok), so this is a read-only companion walk
    // over the SAME validated links, not a second clause reader.
    // Each clause / the query / each probe is a SEPARATE variable scope (a fresh set of
    // metavariables): the same authored name in two clauses is two unrelated variables, so a
    // sort declared on `X` in one scope must not constrain an unrelated `X` in another. The
    // scope is identified by the owning clause/probe's `Formula::content_key` (stable across
    // the canonical sort `ReasoningProgramIr::new` applies), so it survives reordering.
    let mut variable_sorts: Vec<(VariableSortScope, String, String)> = Vec::new();
    // Every position a `logic:variableSort` can attach to, paired with the scope that owns it.
    // Probes must be ground (enforced by `ReasoningProgramIr::new`), so their walk collects
    // nothing for a well-formed program; including them keeps the scope taxonomy total.
    // The scope key is the owning clause/probe's `Formula::content_key` PLUS an occurrence
    // index — the number of PRIOR clauses/probes sharing that same content_key — so two
    // structurally-identical clauses carrying DIFFERENT `logic:variableSort` declarations key
    // DISTINCT scopes rather than colliding. Both facets are stable across the canonical sort
    // `ReasoningProgramIr::new` applies: that sort is stable and same-content_key ⟹ same
    // sort_key, so each clause's occurrence index is preserved between here and the lowerer.
    let scoped_roots = clause_nodes
        .iter()
        .zip(clauses.iter())
        .enumerate()
        .map(|(idx, (node, clause))| {
            let key = clause.content_key();
            let occurrence = clauses[..idx]
                .iter()
                .filter(|prior| prior.content_key() == key)
                .count();
            (node, VariableSortScope::Clause { key, occurrence })
        })
        .chain(std::iter::once((&query_node, VariableSortScope::Query)))
        .chain(
            probe_nodes
                .iter()
                .zip(verdict_probes.iter())
                .enumerate()
                .map(|(idx, (node, probe))| {
                    let key = probe.content_key();
                    let occurrence = verdict_probes[..idx]
                        .iter()
                        .filter(|prior| prior.content_key() == key)
                        .count();
                    (node, VariableSortScope::Probe { key, occurrence })
                }),
        );
    for (root, scope) in scoped_roots {
        let mut pairs = Vec::new();
        collect_variable_sorts(store, root, &mut pairs)?;
        for (name, sort) in pairs {
            variable_sorts.push((scope.clone(), name, sort));
        }
    }

    // Per-constant order-sort declarations: every `Term::Iri` constant referenced in
    // argument position across the clause set, query, and verdict probes, paired with
    // EVERY `rdf:type` object IRI asserted on it in `store`. The raw `ex:one a
    // math:Integer` triple is otherwise dropped by the stage's L3 fold (it is ordinary
    // domain data, not `logic:` structural vocabulary), so it MUST be captured here or an
    // order-sorted demonstrator downstream cannot discriminate a typed constant from an
    // untyped one. An unsorted constant is legal (it stays order-sort top) — never a hard
    // fail.
    let mut constant_iris = Vec::new();
    for clause in &clauses {
        collect_constant_iris(clause, &mut constant_iris);
    }
    collect_constant_iris(&query, &mut constant_iris);
    for probe in &verdict_probes {
        collect_constant_iris(probe, &mut constant_iris);
    }
    constant_iris.sort();
    constant_iris.dedup();
    let mut constant_sorts = Vec::new();
    for constant_iri in &constant_iris {
        for ty in objects(store, &Subject::Iri(constant_iri.clone()), &nn(RDF_TYPE)) {
            if let Node::Iri(sort_iri) = ty {
                constant_sorts.push((constant_iri.clone(), sort_iri));
            }
        }
    }

    ReasoningProgramIr::new(
        iri,
        mode,
        clauses,
        query,
        verdict_probes,
        variable_sorts,
        constant_sorts,
    )
}

/// Read every authored `logic:ReasoningProgram` individual into a [`ReasoningProgramIr`] —
/// the authored clause-set-plus-goal surface a downstream compiler lowers directly to the
/// native reasoning engine. **Hard-fails** on any structural malformation (an `Error`-grade
/// `MALFORMED_REASONING_PROGRAM` diagnostic, mirroring `MALFORMED_FORMULA` /
/// `ORPHAN_RECOVERY_CASE`): the malformed program is skipped — never silently repaired or
/// completed with a default — while every other authored program still compiles. See
/// [`read_reasoning_program`] for the exact structural requirements.
///
/// A reasoning-program-free source yields an empty vector, and
/// `with_reasoning_programs(vec![])` is a no-op in [`LogicProgram::canonical_key`] (the
/// segment is append-only) — so adding this stage to every parse leaves every existing
/// artifact byte-identical.
fn extract_reasoning_programs(
    reader: &mut FormulaReader<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<ReasoningProgramIr>> {
    let store = reader.dataset();
    let program_ty = Node::iri(logic_iri("ReasoningProgram"));
    let mut programs = Vec::new();
    for subj in subjects_of_structural_class(store, &program_ty) {
        if let Some(emitted) = capture_owner(
            owner_source(store, &subj),
            OwnerFamily::ReasoningProgram,
            diagnostics,
            lowerings,
            |diagnostics| {
                read_reasoning_program(reader, &subj).map_err(|err| {
                    diagnostics.push(Diagnostic::error(
                        "MALFORMED_REASONING_PROGRAM",
                        err.message().to_owned(),
                        Some(subject_str(&subj)),
                    ));
                    Vec::new()
                })
            },
        ) {
            programs.push(emitted);
        }
    }
    programs
}

/// Read every authored `logic:Correspondence` individual into the IR — the input the
/// five conformance gates run on. Fail-soft like the other extractors: a malformed cell
/// emits a `MALFORMED_CORRESPONDENCE` warning and is skipped, never silently dropped (the
/// hard-fail discipline belongs to the cache re-derivation, not the authoring front-end).
///
/// A correspondence-free source yields an empty vector, and `with_correspondences(vec![])`
/// is a no-op in `LogicProgram::canonical_key` (the segment is append-only) — so adding
/// this stage to every parse leaves every existing artifact byte-identical.
fn extract_correspondences(
    reader: &mut FormulaReader<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<Correspondence>> {
    let store = reader.dataset();
    let mut correspondences = Vec::new();
    for subject in subjects_of_structural_class(store, &Node::iri(logic_iri("Correspondence"))) {
        if let Some(emitted) = capture_owner(
            owner_source(store, &subject),
            OwnerFamily::Correspondence,
            diagnostics,
            lowerings,
            |diagnostics| {
                crate::projections::correspondence::read_source_correspondence(reader, &subject)
                    .map_err(|err| {
                        diagnostics.push(Diagnostic::warning(
                            if subject_is_blank(&subject) {
                                "UNLOWERED_CORRESPONDENCE_IDENTITY"
                            } else {
                                "MALFORMED_CORRESPONDENCE"
                            },
                            err.message().to_owned(),
                            Some(subject_str(&subject)),
                        ));
                        Vec::new()
                    })
            },
        ) {
            correspondences.push(OwnerEmission {
                source: emitted.source,
                family: emitted.family,
                diagnostics: emitted.diagnostics,
                record_support: emitted.value.record_support,
                value: emitted.value.value,
            });
        }
    }
    correspondences
}

fn extract_compositions(
    store: &RdfDataset,
    source_graph: &StructuralSourceGraph,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<crate::ir::CorrespondenceComposition>> {
    let mut compositions = Vec::new();
    // Invalid membership targets are not structural resource units, but must still
    // produce an owned rejection rather than silently selecting no declaration.
    for edge in source_graph.edges().iter().filter(|edge| {
        edge.source.graph.is_none()
            && edge.predicate.iri(store) == "https://blackcatinformatics.ca/logic/hasComposition"
            && !matches!(store.resolve(edge.target), purrdf::TermRef::Iri(_))
    }) {
        let diagnostic = diagnostics.len();
        diagnostics.push(Diagnostic::error(
            "MALFORMED_CORRESPONDENCE_COMPOSITION",
            "logic:hasComposition requires a named composition identity",
            Some(source_graph::focus(store, edge.source.term)),
        ));
        lowerings.push(OwnerLowering {
            source: edge.source,
            family: OwnerFamily::CorrespondenceComposition,
            disposition: OwnerDisposition::Rejected,
            diagnostics: vec![diagnostic],
            record_support: Default::default(),
        });
    }
    for unit in source_graph.units().filter(|unit| {
        unit.node.graph.is_none()
            && (unit
                .declared_kinds
                .contains(&SourceUnitKind::CorrespondenceComposition)
                || unit
                    .required_kinds
                    .contains(&SourceUnitKind::CorrespondenceComposition))
    }) {
        if let Some(emitted) = capture_owner(
            unit.node,
            OwnerFamily::CorrespondenceComposition,
            diagnostics,
            lowerings,
            |diagnostics| {
                crate::projections::correspondence::read_source_composition(store, unit.node)
                    .map_err(|error| {
                        diagnostics.push(Diagnostic::error(
                            "MALFORMED_CORRESPONDENCE_COMPOSITION",
                            error.message().to_owned(),
                            Some(source_graph::focus(store, unit.node.term)),
                        ));
                        Vec::new()
                    })
            },
        ) {
            compositions.push(emitted);
        }
    }
    compositions
}

fn extract_owned_leg_programs(
    store: &RdfDataset,
    correspondences: &[OwnerEmission<Correspondence>],
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<crate::ir::TransactionProgramIr>> {
    // These are original source references, captured before correspondence IDs can
    // be projected or rewritten. A shared leg is read only once.
    let legs: BTreeSet<_> = correspondences
        .iter()
        .flat_map(|emission| {
            emission
                .value
                .get_leg
                .iter()
                .chain(emission.value.put_leg.iter())
        })
        .collect();
    let mut programs = Vec::new();
    for iri in legs {
        let source = SourceNode {
            term: store
                .term_id_by_iri(iri)
                .expect("leg reference came from the source"),
            graph: None,
        };
        if let Some(emitted) = capture_owner(
            source,
            OwnerFamily::TransactionProgram,
            diagnostics,
            lowerings,
            |diagnostics| {
                crate::projections::correspondence::read_source_leg_program(store, iri).map_err(
                    |error| {
                        diagnostics.push(Diagnostic::warning(
                            "UNLOWERED_TRANSACTION_PROGRAM",
                            error.message().to_owned(),
                            Some(iri.clone()),
                        ));
                        Vec::new()
                    },
                )
            },
        ) {
            programs.push(emitted);
        }
    }
    programs
}

/// Read every authored `logic:Constraint` individual into a [`ConstraintIr`] — the typed
/// home for a closed-world procedural integrity condition (whose violation is a finding).
/// Fail-soft like the other extractors: a malformed constraint emits a
/// `MALFORMED_CONSTRAINT` warning and is skipped, never silently dropped.
///
/// A constraint node carries its integrity condition via `logic:integrity` (a top-level
/// [`Formula`] tree, reconstructed by the shared [`parse_formula`] reader — no duplicated
/// formula parsing), its `sh:severity` via a `logic:severity "Violation"|"Warning"|"Info"`
/// literal (absent ⇒ the SHACL default `Violation`), an advisory `logic:message`
/// literal set, and an optional `logic:formalizes` gmeow-domain back-reference. The focus
/// [`ShapeTarget`] is DERIVED from the integrity's `∀` guard by [`ConstraintIr::new`], which
/// hard-fails (⇒ a `MALFORMED_CONSTRAINT` warning) on a non-range-restricted formula.
///
/// A constraint-free source yields an empty vector, and `with_constraints(vec![])` is a no-op
/// in [`LogicProgram::canonical_key`] (the segment is append-only) — so adding this stage to
/// every parse leaves every existing artifact byte-identical.
fn extract_constraints(
    reader: &mut FormulaReader<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    malformed_formulas: &BTreeMap<String, usize>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<ConstraintIr>> {
    let store = reader.dataset();
    let constraint_ty = Node::iri(logic_iri("Constraint"));
    let mut constraints = Vec::new();
    for subj in subjects_of_structural_class(store, &constraint_ty) {
        if let Some(emitted) = capture_owner(
            owner_source(store, &subj),
            OwnerFamily::Constraint,
            diagnostics,
            lowerings,
            |diagnostics| {
                read_constraint(reader, &subj).map_err(|err| {
                    let focus = err
                        .inner()
                        .source_ctx
                        .focus
                        .as_ref()
                        .map(|focus| focus.0.as_str());
                    if let Some(diagnostic) = focus.and_then(|focus| malformed_formulas.get(focus))
                    {
                        return vec![*diagnostic];
                    }
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_CONSTRAINT",
                        err.message().to_owned(),
                        Some(subject_str(&subj)),
                    ));
                    Vec::new()
                })
            },
        ) {
            constraints.push(emitted);
        }
    }
    constraints
}

/// Extract every authored `logic:Constraint` individual AND compact constraint-sugar record
/// (P1–P7 + the aggregate satellite) from `store`, each expanded to a canonical [`ConstraintIr`],
/// together with any `MALFORMED_CONSTRAINT` diagnostics. This is the whole-dataset entry point the
/// pipeline uses to gather procedural constraints from the MERGED authored ontology (every slice
/// `module.ttl`), not just the `logic:` terminal module — so a `logic:Constraint`/sugar record may
/// be authored in the slice that owns the constrained class (the constraint peer of
/// [`derive_validation_shapes`], which already reads the merged dataset). The returned vector is
/// unsorted; [`LogicProgram::with_constraints`] canonicalizes it.
///
/// # Errors
/// Refuses supplied range bounds outside the supported finite numeric contract.
pub fn extract_all_constraints(
    store: &RdfDataset,
) -> gmeow_errors::Result<(Vec<ConstraintIr>, Vec<Diagnostic>)> {
    extract_constraints_from_source(store, &StructuralSourceGraph::new(store))
}

/// Extract procedural constraints with the prepared source's ownership diagnostics.
/// Malformed recoverable records remain excluded with their original diagnostics.
///
/// # Errors
/// Propagates mandatory range-bound admission failures instead of returning partial constraints.
fn extract_constraints_from_source(
    store: &RdfDataset,
    source_graph: &StructuralSourceGraph,
) -> gmeow_errors::Result<(Vec<ConstraintIr>, Vec<Diagnostic>)> {
    let mut reader = FormulaReader::new(store);
    let mut diagnostics = source_graph.ownership_diagnostics(store);
    let malformed_formulas =
        extract_formulas(&mut reader, source_graph, &mut diagnostics).malformed;
    let mut lowerings = Vec::new();
    let mut constraints = extract_constraints(
        &mut reader,
        &mut diagnostics,
        &malformed_formulas,
        &mut lowerings,
    );
    constraints.extend(extract_sugar_constraints(
        store,
        &mut diagnostics,
        &mut lowerings,
    )?);
    Ok((
        constraints
            .into_iter()
            .map(|emission| emission.value)
            .collect(),
        diagnostics,
    ))
}

/// Build a `MALFORMED_CONSTRAINT`-grade frontend [`Diag`] from a message. The constraint-sugar
/// readers surface a fail-soft reason (rendered as one warning) as the structured [`Diag`] that is
/// the sole first-party error type (the Phase-6 Diag substrate); a malformed record degrades to a
/// warning rather than aborting the parse.
fn sugar_err(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Frontend::new(detail.into()))
}

const GMEOW_ENFORCES_FAILURE_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

/// The distinct failure-class IRIs on one canonical source node. RDF datasets
/// may carry the same assertion more than once; repeated identical values are
/// one set member, while non-IRI objects are malformed metadata.
fn distinct_failure_classes(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut classes = BTreeSet::new();
    for value in objects(store, node, &nn(GMEOW_ENFORCES_FAILURE_CLASS)) {
        match value {
            Node::Iri(iri) => {
                classes.insert(iri);
            }
            _ => {
                return Err(sugar_err(format!(
                    "{} gmeow:enforcesFailureClass value must be an IRI",
                    subject_str(node)
                )));
            }
        }
    }
    Ok(classes)
}

/// Retain every supplied message literal and reject malformed advisory metadata.
fn read_messages(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<Vec<purrdf::RdfLiteral>> {
    let mut messages = objects(store, node, &nn(&logic_iri("message")))
        .into_iter()
        .map(|value| match value {
            Node::Lit(literal) => Ok(literal),
            other => Err(
                sugar_err(format!("logic:message must be a literal, found {other:?}"))
                    .with_focus(subject_str(node)),
            ),
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    crate::ir::literal_serde::normalize_messages(&mut messages).map_err(|detail| {
        Diag::of_kind(crate::error::Frontend::caused_by(
            format!("logic:message: {detail}"),
            detail,
        ))
        .with_focus(subject_str(node))
    })?;
    Ok(messages)
}

/// Reconstruct one [`ConstraintIr`] rooted at a `logic:Constraint` node, or return a
/// human-readable reason the constraint is malformed (surfaced as one `MALFORMED_CONSTRAINT`
/// warning by [`extract_constraints`]).
fn read_constraint(
    reader: &mut FormulaReader<'_>,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let store = reader.dataset();
    let iri = subject_str(node);
    // Validate constraint-owned metadata before parsing the integrity tree. If both the formula
    // and an independent constraint annotation are malformed, each receives its own authoritative
    // diagnostic instead of the formula failure masking the constraint defect.
    // Absent severity ⇒ the SHACL default `Violation`; an unrecognized token is a hard error.
    let severity = sugar_severity(store, node).map_err(|err| err.with_focus(iri.clone()))?;
    let messages = read_messages(store, node)?;
    let formalizes_all = sugar_iri_list(store, node, "formalizes");
    let failure_classes =
        distinct_failure_classes(store, node).map_err(|err| err.with_focus(iri.clone()))?;
    if failure_classes.len() > 1 {
        return Err(sugar_err(format!(
            "logic:Constraint {iri} has distinct gmeow:enforcesFailureClass values"
        ))
        .with_focus(iri));
    }

    let integrity_values = formula_objects(store, node, "integrity");
    if integrity_values.len() != 1 {
        return Err(sugar_err(format!(
            "logic:Constraint {} requires exactly one logic:integrity formula; found {}",
            subject_str(node),
            integrity_values.len()
        ))
        .with_focus(subject_str(node)));
    }
    let integrity_node = term_as_subject(&integrity_values[0]).ok_or_else(|| {
        sugar_err(format!(
            "logic:Constraint {} requires a resource-valued logic:integrity formula",
            subject_str(node)
        ))
        .with_focus(subject_str(node))
    })?;
    // Preserve the malformed formula's own focus through the constraint reader. The formula
    // extractor has already emitted MALFORMED_FORMULA for that identity, so the caller can
    // suppress only the redundant constraint wrapper while retaining independent defects.
    let integrity = reader.read(&integrity_node)?;

    let mut constraint = ConstraintIr::new(&iri, integrity, severity, messages)
        .map_err(|err| err.with_focus(iri.clone()))?;
    if let Some((primary, rest)) = formalizes_all.split_first() {
        constraint = constraint
            .with_formalizes(primary.clone())
            .map_err(|err| err.with_focus(iri.clone()))?;
        constraint = constraint
            .with_also_formalizes(rest.to_vec())
            .map_err(|err| err.with_focus(iri.clone()))?;
    }
    if let Some(failure_class) = failure_classes.first() {
        constraint = constraint
            .with_failure_class(failure_class)
            .map_err(|err| err.with_focus(iri.clone()))?;
    }
    Ok(constraint)
}

// --------------------------------------------------------------------------- //
// Compact constraint-sugar records — a few authoring shortcuts the compiler expands into a
// canonical `logic:Constraint` + integrity `Formula`, so an author never hand-writes a formula
// tree for the common patterns. Sugar is FRONTEND-ONLY: each record expands to exactly one
// `ConstraintIr` (carrying the honest reified FOL integrity + a required `logic:formalizes`), so
// there is one canonical home per authored constraint. The expansion reuses `Formula`/`Term`
// constructors and `ConstraintIr::new` (which derives the target from the `∀` guard) — never a
// parallel constraint AST.
// --------------------------------------------------------------------------- //

/// A bound variable term.
fn t_var(n: &str) -> Term {
    Term::Var(n.to_owned())
}

/// A binary atom `rel(a, b)` (relation is always an IRI).
fn f_atom2(rel: &str, a: Term, b: Term) -> gmeow_errors::Result<Formula> {
    Formula::atom(Term::iri(rel)?, vec![a, b])
}

/// An existential `∃ var . pred(this, var)` — "the focus has some value on `pred`".
fn f_pred_exists(pred: &str, var: &str) -> gmeow_errors::Result<Formula> {
    Ok(Formula::Exists {
        vars: vec![var.to_owned()],
        body: Box::new(f_atom2(pred, t_var("this"), t_var(var))?),
    })
}

/// A class-membership guard atom `rdf:type(this, class)`.
fn f_guard_class(class: &str) -> gmeow_errors::Result<Formula> {
    f_atom2(RDF_TYPE, t_var("this"), Term::iri(class)?)
}

/// Wrap `consequent` in the canonical range-restricted guarded universal
/// `∀ this . guard → consequent`, from which [`ConstraintIr::new`] derives the target.
fn f_forall_this(guard: Formula, consequent: Formula) -> Formula {
    Formula::Forall {
        vars: vec!["this".to_owned()],
        body: Box::new(Formula::Implies(Box::new(guard), Box::new(consequent))),
    }
}

/// The required `logic:onClass` target of a sugar record.
fn sugar_target_class(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<String> {
    value(store, node, &nn(&logic_iri("onClass")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("constraint-sugar record requires logic:onClass (the target class)")
        })
}

/// The OPTIONAL `logic:onClass` target of a sugar record — `None` when omitted, so a sugar that
/// admits a predicate-presence range restriction (a guarded implication guarded by the mere
/// presence of a trigger predicate rather than a class) can derive a `sh:targetSubjectsOf` target
/// from its trigger atom instead of a `sh:targetClass`.
fn sugar_optional_target_class(store: &RdfDataset, node: &Subject) -> Option<String> {
    value(store, node, &nn(&logic_iri("onClass"))).map(|t| term_str(&t))
}

/// One distinct, untagged string/token severity (absent ⇒ the SHACL default `Violation`).
fn sugar_severity(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ShaclSeverity> {
    let mut severities = objects(store, node, &nn(&logic_iri("severity"))).into_iter();
    let Some(term) = severities.next() else {
        return Ok(ShaclSeverity::Violation);
    };
    if severities.any(|other| other != term) {
        return Err(sugar_err(
            "logic:severity requires exactly one distinct value",
        ));
    }
    // graphutil::node_of represents the native xsd:string datatype as None.
    let token = match &term {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        }) if matches!(
            datatype.as_deref(),
            None | Some(
                "http://www.w3.org/2001/XMLSchema#string"
                    | "http://www.w3.org/2001/XMLSchema#token"
            )
        ) =>
        {
            lexical_form
        }
        _ => {
            return Err(sugar_err(
                "logic:severity requires an untagged xsd:string or xsd:token literal",
            ));
        }
    };
    ShaclSeverity::from_local(token).ok_or_else(|| {
        sugar_err(format!(
            "logic:severity '{token}' is not one of Violation/Warning/Info"
        ))
    })
}

/// The ordered, deduped IRI objects of `pred` on `node` (sorted for byte-deterministic expansion
/// regardless of the source triple order).
fn sugar_iri_list(store: &RdfDataset, node: &Subject, pred_local: &str) -> Vec<String> {
    let mut v: Vec<String> = objects(store, node, &nn(&logic_iri(pred_local)))
        .iter()
        .map(term_str)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Finalize a sugar expansion: build the [`ConstraintIr`] from the integrity formula and attach the
/// REQUIRED `logic:formalizes` back-reference (so the projected shape passes the purity gate).
fn finalize_sugar(
    store: &RdfDataset,
    node: &Subject,
    integrity: Formula,
) -> gmeow_errors::Result<ConstraintIr> {
    let severity = sugar_severity(store, node)?;
    let messages = read_messages(store, node)?;
    let formalizes_all = sugar_iri_list(store, node, "formalizes");
    let (primary, rest) = formalizes_all.split_first().ok_or_else(|| {
        sugar_err(
            "constraint-sugar record requires logic:formalizes (the gmeow term it formalizes)",
        )
    })?;
    let mut constraint = ConstraintIr::new(subject_str(node), integrity, severity, messages)?
        .with_formalizes(primary.clone())?
        .with_also_formalizes(rest.to_vec())?;
    let failure_classes = distinct_failure_classes(store, node)?;
    if failure_classes.len() > 1 {
        return Err(sugar_err(format!(
            "constraint-sugar record {} has distinct gmeow:enforcesFailureClass values",
            subject_str(node)
        )));
    }
    if let Some(failure_class) = failure_classes.first() {
        constraint = constraint.with_failure_class(failure_class)?;
    }
    Ok(constraint)
}

/// Admit one distinct complete selector literal without changing its lexical text.
fn sugar_selector_literal(
    store: &RdfDataset,
    node: &Subject,
    pred_local: &str,
) -> gmeow_errors::Result<String> {
    let mut values = objects(store, node, &nn(&logic_iri(pred_local))).into_iter();
    let term = values.next().ok_or_else(|| {
        sugar_err(format!(
            "constraint-sugar record requires logic:{pred_local}"
        ))
    })?;
    if values.any(|other| other != term) {
        return Err(sugar_err(format!(
            "logic:{pred_local} requires exactly one distinct value"
        )));
    }
    selector_literal_value(term, pred_local)
}

/// Validate the native string/token carrier without changing its supplied lexical text.
fn selector_literal_value(term: Node, pred_local: &str) -> gmeow_errors::Result<String> {
    let Node::Lit(purrdf::RdfLiteral {
        lexical_form,
        datatype,
        language: None,
        direction: None,
    }) = term
    else {
        return Err(sugar_err(format!(
            "logic:{pred_local} requires an untagged xsd:string or xsd:token literal"
        )));
    };
    // Native xsd:string resolves to None in graphutil::node_of.
    match datatype.as_deref() {
        None | Some("http://www.w3.org/2001/XMLSchema#string") => {
            purrdf::xsd::simple::read_xml_string(&lexical_form)
        }
        Some("http://www.w3.org/2001/XMLSchema#token") => {
            purrdf::xsd::simple::read_token(&lexical_form)
        }
        _ => {
            return Err(sugar_err(format!(
                "logic:{pred_local} requires an untagged xsd:string or xsd:token literal"
            )));
        }
    }
    .map_err(|reason| sugar_err(format!("logic:{pred_local}: {reason}")))?;
    Ok(lexical_form)
}

/// P1 — choice-group cardinality: a target class + a set of predicates + a mode
/// (`exactly-one` / `at-most-one` / `at-least-one`; `exactly-one-of-N` is an alias of
/// `exactly-one`). Expands to the XOR / at-most / at-least formula over `∃`-requiredness of
/// each predicate.
fn read_choice_group(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let preds = sugar_iri_list(store, node, "choicePredicate");
    if preds.len() < 2 {
        return Err(sugar_err(
            "logic:ChoiceGroupConstraint needs at least two logic:choicePredicate",
        ));
    }
    let mode = sugar_selector_literal(store, node, "choiceMode")?
        .trim()
        .to_ascii_lowercase();
    let ex = |i: usize| f_pred_exists(&preds[i], &format!("v{i}"));
    let consequent = match mode.as_str() {
        "exactly-one" | "exactly-one-of-n" => {
            // OR over i of (∃pᵢ ∧ AND over j≠i of ¬∃pⱼ) — exactly one predicate present.
            let mut branches = Vec::with_capacity(preds.len());
            for i in 0..preds.len() {
                let mut conj = vec![ex(i)?];
                for j in 0..preds.len() {
                    if j != i {
                        conj.push(Formula::Not(Box::new(ex(j)?)));
                    }
                }
                branches.push(Formula::And(conj));
            }
            Formula::Or(branches)
        }
        "at-least-one" => {
            // OR over i of ∃pᵢ — at least one alternative predicate present (the disjunctive
            // existence obligation: a node missing EVERY alternative violates).
            let mut branches = Vec::with_capacity(preds.len());
            for i in 0..preds.len() {
                branches.push(ex(i)?);
            }
            Formula::Or(branches)
        }
        "at-most-one" => {
            // AND over pairs (i<j) of ¬(∃pᵢ ∧ ∃pⱼ) — no two predicates present together.
            let mut pairs = Vec::new();
            for i in 0..preds.len() {
                for j in (i + 1)..preds.len() {
                    pairs.push(Formula::Not(Box::new(Formula::And(vec![ex(i)?, ex(j)?]))));
                }
            }
            if pairs.len() == 1 {
                pairs.pop().expect("one pair")
            } else {
                Formula::And(pairs)
            }
        }
        other => {
            return Err(sugar_err(format!(
                "logic:choiceMode '{other}' must be exactly-one / at-most-one / at-least-one / \
                 exactly-one-of-N"
            )));
        }
    };
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, consequent),
    )
}

/// P2 — guarded implication: a trigger predicate (optionally pinned to a `logic:triggerValue`)
/// with one or more required companion predicates, optionally range-restricted to a target class.
/// Expands to `∀ this . C(this) ∧ trigger(this, …) → ∃ companion` when a `logic:onClass` is
/// authored, or to the **predicate-presence** form `∀ this . trigger(this, …) → ∃ companion`
/// when it is omitted — the guard is then the trigger atom alone, from which
/// [`ConstraintIr::new`] derives a `sh:targetSubjectsOf trigger` target (the "subjects of a
/// predicate must carry a companion" pattern the grounding slices lean on, e.g. a claim
/// carrying one field must declare the field that grounds it).
fn read_guarded_implication(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_optional_target_class(store, node);
    let trigger = value(store, node, &nn(&logic_iri("trigger")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:GuardedImplicationConstraint requires logic:trigger"))?;
    let companions = sugar_iri_list(store, node, "requires");
    if companions.is_empty() {
        return Err(sugar_err(
            "logic:GuardedImplicationConstraint requires at least one logic:requires companion \
             predicate",
        ));
    }
    // The trigger atom: pinned to a fixed object value when `logic:triggerValue` is present, else
    // an existential occurrence `trigger(this, ?t)` (the mere presence of the trigger predicate).
    let trigger_atom = match value(store, node, &nn(&logic_iri("triggerValue"))) {
        Some(v) => {
            let obj = match &v {
                Node::Iri(i) => Term::iri(i)?,
                Node::Lit(literal) => Term::rdf_literal(literal.clone())?,
                _ => return Err(sugar_err("logic:triggerValue must be an IRI or a literal")),
            };
            f_atom2(&trigger, t_var("this"), obj)?
        }
        None => f_atom2(&trigger, t_var("this"), t_var("t"))?,
    };
    // With a target class the guard conjoins the class-membership guard with the trigger; without
    // one the trigger atom IS the guard (a predicate-presence range restriction → SubjectsOf).
    let guard = match &class {
        Some(c) => Formula::And(vec![f_guard_class(c)?, trigger_atom]),
        None => trigger_atom,
    };
    // The consequent: one required companion, or the conjunction of their existentials.
    let mut req: Vec<Formula> = Vec::with_capacity(companions.len());
    for (i, c) in companions.iter().enumerate() {
        req.push(f_pred_exists(c, &format!("c{i}"))?);
    }
    let consequent = if req.len() == 1 {
        req.pop().expect("one companion")
    } else {
        Formula::And(req)
    };
    finalize_sugar(store, node, f_forall_this(guard, consequent))
}

/// P3 — disjunctive requiredness: a target class + a set of predicates, at least one required.
/// Expands to `∀ this . C(this) → (∃p₁ ∨ … ∨ ∃pₙ)`.
fn read_disjunctive_requiredness(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let preds = sugar_iri_list(store, node, "anyOf");
    if preds.is_empty() {
        return Err(sugar_err(
            "logic:DisjunctiveRequirednessConstraint requires at least one logic:anyOf predicate",
        ));
    }
    let mut disj: Vec<Formula> = Vec::with_capacity(preds.len());
    for (i, p) in preds.iter().enumerate() {
        disj.push(f_pred_exists(p, &format!("v{i}"))?);
    }
    let consequent = if disj.len() == 1 {
        disj.pop().expect("one predicate")
    } else {
        Formula::Or(disj)
    };
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, consequent),
    )
}

/// P4 — path-value membership: a target class + a path `P` + the membership every value on `P`
/// must satisfy. Two variants, one required:
///
/// * `logic:valueClass D` (type membership) → `∀ this . C(this) → ∀ v . P(this, v) → D(v)`;
/// * `logic:valuePredicate Q` + `logic:valueObject o` (a fixed predicate=value check, e.g. every
///   inducing form must be `math:definiteness math:positiveDefinite`) →
///   `∀ this . C(this) → ∀ v . P(this, v) → Q(v, o)`.
///
/// The fixed-value variant reuses the identical nested-`∀` lowering as the class variant (only the
/// consequent atom differs), so the projector needs no new lowering — a value reached by a path
/// must carry a given predicate=value, not only `rdf:type C`.
fn read_path_value_type(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let path = value(store, node, &nn(&logic_iri("valuePath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:PathValueTypeConstraint requires logic:valuePath"))?;
    // The membership atom over the bound value `v`: a class (`rdf:type(v, D)`) or a fixed
    // predicate=value (`Q(v, o)`). Exactly one form must be authored.
    let value_class = value(store, node, &nn(&logic_iri("valueClass"))).map(|t| term_str(&t));
    let value_predicate =
        value(store, node, &nn(&logic_iri("valuePredicate"))).map(|t| term_str(&t));
    let membership = match (value_class, value_predicate) {
        (Some(d), None) => f_atom2(RDF_TYPE, t_var("v"), Term::iri(&d)?)?,
        (None, Some(q)) => {
            let obj = match value(store, node, &nn(&logic_iri("valueObject"))) {
                Some(Node::Iri(i)) => Term::iri(&i)?,
                Some(Node::Lit(literal)) => Term::rdf_literal(literal.clone())?,
                _ => {
                    return Err(sugar_err(
                        "logic:PathValueTypeConstraint with logic:valuePredicate requires a \
                         logic:valueObject (an IRI or a literal)",
                    ));
                }
            };
            f_atom2(&q, t_var("v"), obj)?
        }
        (Some(_), Some(_)) => {
            return Err(sugar_err(
                "logic:PathValueTypeConstraint takes EITHER logic:valueClass OR \
                 logic:valuePredicate, not both",
            ));
        }
        (None, None) => {
            return Err(sugar_err(
                "logic:PathValueTypeConstraint requires logic:valueClass (a class) or \
                 logic:valuePredicate + logic:valueObject (a fixed predicate=value)",
            ));
        }
    };
    let inner = Formula::Forall {
        vars: vec!["v".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(f_atom2(&path, t_var("this"), t_var("v"))?),
            Box::new(membership),
        )),
    };
    finalize_sugar(store, node, f_forall_this(f_guard_class(&class)?, inner))
}

/// P5 — cross-node co-occurrence / inequality: a target class + two roles that must
/// **co-occur** (present together) or **differ** (never bind the same value). Expands to the
/// join + inequality (`logic:termDistinct` / `logic:termEqual` filter) or bi-implication form.
fn read_cross_node(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let role_a = value(store, node, &nn(&logic_iri("roleA")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:CrossNodeConstraint requires logic:roleA"))?;
    let role_b = value(store, node, &nn(&logic_iri("roleB")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:CrossNodeConstraint requires logic:roleB"))?;
    let mode = sugar_selector_literal(store, node, "crossMode")?
        .trim()
        .to_ascii_lowercase();
    let consequent = match mode.as_str() {
        // Co-occur: role A present iff role B present (each implies the other).
        "co-occur" | "cooccur" => {
            let a = || f_pred_exists(&role_a, "a");
            let b = || f_pred_exists(&role_b, "b");
            Formula::And(vec![
                Formula::Implies(Box::new(a()?), Box::new(b()?)),
                Formula::Implies(Box::new(b()?), Box::new(a()?)),
            ])
        }
        // Differ: no value bound by both roles — ¬∃a,b. (roleA(this,a) ∧ roleB(this,b) ∧ a = b).
        "differ" | "distinct" => Formula::Not(Box::new(Formula::Exists {
            vars: vec!["a".to_owned(), "b".to_owned()],
            body: Box::new(Formula::And(vec![
                f_atom2(&role_a, t_var("this"), t_var("a"))?,
                f_atom2(&role_b, t_var("this"), t_var("b"))?,
                f_atom2(&logic_iri("termEqual"), t_var("a"), t_var("b"))?,
            ])),
        })),
        other => {
            return Err(sugar_err(format!(
                "logic:crossMode '{other}' must be co-occur or differ"
            )));
        }
    };
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, consequent),
    )
}

/// P7 — forbidden pattern: a target class + a forbidden predicate (optionally pinned to a
/// forbidden value). Expands to `∀ this . C(this) → ¬∃ b . forbidden(this, b)` (or the
/// pinned-value form `¬ forbidden(this, value)`).
fn read_forbidden_pattern(
    store: &RdfDataset,
    node: &Subject,
    source: SourceNode,
    support: &mut BTreeSet<SourceStatementBinding>,
) -> gmeow_errors::Result<ConstraintIr> {
    let kind = logic_iri("ForbiddenPatternConstraint");
    support.extend(source_fields(store, source, None).filter(|binding| {
        let quad = binding.canonical;
        matches!(store.resolve(quad.o), purrdf::TermRef::Iri(iri) if iri == kind.as_str())
            && matches!(store.resolve(quad.p), purrdf::TermRef::Iri(predicate)
                if crate::graphutil::is_structural_type_predicate(predicate, &kind))
    }));
    let ForbiddenPattern {
        class,
        predicate,
        pin,
    } = forbidden_pattern_fields(store, source, support)?;
    let consequent = match pin {
        Some(pin) => {
            let object = match pin {
                ShapeValue::Iri(iri) => Term::iri(&iri)?,
                ShapeValue::Literal(literal) => Term::rdf_literal(literal)?,
            };
            Formula::Not(Box::new(f_atom2(&predicate, t_var("this"), object)?))
        }
        None => Formula::Not(Box::new(f_pred_exists(&predicate, "b")?)),
    };
    let constraint = finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, consequent),
    )?;
    for field in [
        logic_iri("formalizes"),
        logic_iri("severity"),
        logic_iri("message"),
        GMEOW_ENFORCES_FAILURE_CLASS.to_owned(),
    ] {
        if let Some(predicate) = store.term_id_by_iri(&field) {
            support.extend(source_fields(store, source, Some(predicate)));
        }
    }
    Ok(constraint)
}

struct ForbiddenPattern {
    class: String,
    predicate: String,
    pin: Option<ShapeValue>,
}

fn forbidden_pattern_fields(
    store: &RdfDataset,
    source: SourceNode,
    support: &mut BTreeSet<SourceStatementBinding>,
) -> gmeow_errors::Result<ForbiddenPattern> {
    let class = forbidden_pattern_iri(store, source, "onClass", support)?;
    let predicate = forbidden_pattern_iri(store, source, "forbiddenPredicate", support)?;
    let pin = match forbidden_pattern_field(store, source, "forbiddenValue", support)? {
        Some(Node::Iri(iri)) => Some(ShapeValue::Iri(iri)),
        Some(Node::Lit(literal)) => Some(ShapeValue::Literal(literal)),
        None => None,
        _ => {
            return Err(sugar_err(
                "logic:forbiddenValue must be an IRI or a literal",
            ));
        }
    };
    Ok(ForbiddenPattern {
        class,
        predicate,
        pin,
    })
}

fn forbidden_pattern_iri(
    store: &RdfDataset,
    source: SourceNode,
    local: &str,
    support: &mut BTreeSet<SourceStatementBinding>,
) -> gmeow_errors::Result<String> {
    match forbidden_pattern_field(store, source, local, support)? {
        Some(Node::Iri(iri)) => Ok(iri),
        _ => Err(sugar_err(format!(
            "logic:ForbiddenPatternConstraint requires one IRI for logic:{local}"
        ))),
    }
}

/// Retain all physical carriers for the one semantic value read from this graph.
fn forbidden_pattern_field(
    store: &RdfDataset,
    source: SourceNode,
    local: &str,
    support: &mut BTreeSet<SourceStatementBinding>,
) -> gmeow_errors::Result<Option<Node>> {
    let Some(predicate) = store.term_id_by_iri(&logic_iri(local)) else {
        return Ok(None);
    };
    let mut selected = None;
    for binding in source_fields(store, source, Some(predicate)) {
        let object = binding.canonical.o;
        if selected.is_some_and(|prior| prior != object) {
            return Err(sugar_err(format!(
                "logic:ForbiddenPatternConstraint has multiple distinct logic:{local} values"
            )));
        }
        selected = Some(object);
        support.insert(binding);
    }
    Ok(selected.map(|object| crate::graphutil::node_of(store, object)))
}

/// P6 / aggregate — an aggregate-comparison constraint: a target class + an aggregate function
/// over a path + a comparator + a right-hand side (a compared property of the focus, or a
/// literal). Expands to a [`ConstraintIr`] carrying BOTH the honest reified FOL integrity (so the
/// Convert an authored object [`Node`] (IRI or literal) to a FOL [`Term`], preserving the
/// distinction so a set member / string pattern round-trips as the right SPARQL token.
fn node_to_term(n: &Node) -> gmeow_errors::Result<Term> {
    match n {
        Node::Iri(i) => Term::iri(i),
        Node::Lit(literal) => Term::rdf_literal(literal.clone()),
        other => Err(sugar_err(format!(
            "a set member must be an IRI or literal, not {}",
            term_str(other)
        ))),
    }
}

/// A value-set membership constraint (`sh:in`-style): every value on `logic:valuePath P` must be in
/// (mode `required`, the default) — or must NOT be in (mode `forbidden`) — the enumerated set given
/// by `logic:memberValue v…`. With `logic:onClass C` the guard is class membership; without one the
/// value-path presence is the range restriction (→ `sh:targetSubjectsOf P`). Integrity:
/// required  `∀this. guard → ∀v. P(this,v) → termIn(v, {v…})`;
/// forbidden `∀this. guard → ¬∃v. P(this,v) ∧ termIn(v, {v…})`.
fn read_value_set_membership(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_optional_target_class(store, node);
    let path = value(store, node, &nn(&logic_iri("valuePath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:ValueSetMembershipConstraint requires logic:valuePath"))?;
    let members: Vec<Term> = {
        let mut nodes = objects(store, node, &nn(&logic_iri("memberValue")));
        // Deterministic, source-order-independent expansion.
        nodes.sort_by_key(term_str);
        nodes.dedup_by_key(|n| term_str(n));
        nodes.iter().map(node_to_term).collect::<Result<_, _>>()?
    };
    if members.is_empty() {
        return Err(sugar_err(
            "logic:ValueSetMembershipConstraint requires at least one logic:memberValue",
        ));
    }
    let mode = value(store, node, &nn(&logic_iri("membershipMode")))
        .map(|t| term_str(&t))
        .unwrap_or_else(|| "required".to_owned());
    let mut in_args = vec![t_var("v")];
    in_args.extend(members);
    let in_atom = Formula::atom(Term::iri(logic_iri("termIn"))?, in_args)?;
    let inner = match mode.trim() {
        "required" => Formula::Forall {
            vars: vec!["v".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(f_atom2(&path, t_var("this"), t_var("v"))?),
                Box::new(in_atom),
            )),
        },
        "forbidden" => Formula::Not(Box::new(Formula::Exists {
            vars: vec!["v".to_owned()],
            body: Box::new(Formula::And(vec![
                f_atom2(&path, t_var("this"), t_var("v"))?,
                in_atom,
            ])),
        })),
        other => {
            return Err(sugar_err(format!(
                "logic:membershipMode '{other}' is not one of required/forbidden"
            )));
        }
    };
    let guard = match &class {
        Some(c) => f_guard_class(c)?,
        None => f_atom2(&path, t_var("this"), t_var("g"))?,
    };
    finalize_sugar(store, node, f_forall_this(guard, inner))
}

/// A string pattern / prefix constraint: every value on `logic:valuePath P` must match (mode
/// `…Required`) — or must NOT match (mode `…Forbidden`) — the `logic:stringPattern "…"` under the
/// `logic:stringOp` test (`regex…` → `REGEX`, `prefix…` → `STRSTARTS`). Integrity:
/// required  `∀this. guard → ∀v. P(this,v) → rel(v, pattern)`;
/// forbidden `∀this. guard → ¬∃v. P(this,v) ∧ rel(v, pattern)`.
fn read_string_pattern(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_optional_target_class(store, node);
    let path = value(store, node, &nn(&logic_iri("valuePath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:StringPatternConstraint requires logic:valuePath"))?;
    let pattern = match value(store, node, &nn(&logic_iri("stringPattern"))) {
        Some(Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            ..
        })) => lexical,
        _ => {
            return Err(sugar_err(
                "logic:StringPatternConstraint requires a literal logic:stringPattern",
            ));
        }
    };
    let op = value(store, node, &nn(&logic_iri("stringOp")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err(
                "logic:StringPatternConstraint requires logic:stringOp \
                 (regexRequired/regexForbidden/prefixRequired/prefixForbidden)",
            )
        })?;
    let (relation, forbidden) = match op.trim() {
        "regexRequired" => ("termRegex", false),
        "regexForbidden" => ("termRegex", true),
        "prefixRequired" => ("termStrStarts", false),
        "prefixForbidden" => ("termStrStarts", true),
        other => {
            return Err(sugar_err(format!(
                "logic:stringOp '{other}' is not one of \
                 regexRequired/regexForbidden/prefixRequired/prefixForbidden"
            )));
        }
    };
    let test_atom = Formula::atom(
        Term::iri(logic_iri(relation))?,
        vec![t_var("v"), Term::literal(pattern, None)?],
    )?;
    let inner = if forbidden {
        Formula::Not(Box::new(Formula::Exists {
            vars: vec!["v".to_owned()],
            body: Box::new(Formula::And(vec![
                f_atom2(&path, t_var("this"), t_var("v"))?,
                test_atom,
            ])),
        }))
    } else {
        Formula::Forall {
            vars: vec!["v".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(f_atom2(&path, t_var("this"), t_var("v"))?),
                Box::new(test_atom),
            )),
        }
    };
    let guard = match &class {
        Some(c) => f_guard_class(c)?,
        None => f_atom2(&path, t_var("this"), t_var("g"))?,
    };
    finalize_sugar(store, node, f_forall_this(guard, inner))
}

/// FOL canon is complete + the target derives) AND the structured [`AggregateComparison`] satellite
/// (which drives the real `GROUP BY`/`HAVING` SPARQL projection).
fn read_aggregate_constraint(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let function = sugar_selector_literal(store, node, "aggFunction")?;
    let path = value(store, node, &nn(&logic_iri("aggPath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:AggregateConstraint requires logic:aggPath"))?;
    let comparator =
        AggregateComparator::from_symbol(&sugar_selector_literal(store, node, "aggComparator")?)
            .ok_or_else(|| {
                sugar_err(
                    "logic:AggregateConstraint requires a logic:aggComparator in =/!=/</<=/>/>=",
                )
            })?;
    let distinct = value(store, node, &nn(&logic_iri("aggDistinct")))
        .map(|t| term_str(&t).trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // The right-hand side: an IRI object is a compared PROPERTY of the focus; a literal is a fixed
    // comparison value.
    let compare_to = match value(store, node, &nn(&logic_iri("aggCompareTo"))) {
        Some(Node::Iri(p)) => AggregateRhs::Property(p),
        Some(Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            datatype,
            ..
        })) => AggregateRhs::Literal { lexical, datatype },
        _ => {
            return Err(sugar_err(
                "logic:AggregateConstraint requires logic:aggCompareTo (a property IRI or a literal)",
            ));
        }
    };
    let agg = AggregateComparison::new(function, distinct, path, comparator, compare_to)?;

    // The honest reified FOL integrity: a single reified `aggregateComparison` predication over
    // the focus, guarded by the target class so the target derives. It carries the function, the
    // DISTINCT flag, the path, the comparator, and the right-hand side, so the FOL canon is
    // complete (the SPARQL projection uses the satellite for a real GROUP BY/HAVING instead).
    let rhs_term = match &agg.compare_to {
        AggregateRhs::Property(p) => Term::iri(p)?,
        AggregateRhs::Literal { lexical, datatype } => {
            Term::literal(lexical.clone(), datatype.clone())?
        }
    };
    let reified = Formula::atom(
        Term::iri(logic_iri("aggregateComparison"))?,
        vec![
            t_var("this"),
            Term::literal(agg.function.clone(), None)?,
            Term::literal(agg.distinct.to_string(), None)?,
            Term::iri(&agg.path)?,
            Term::literal(agg.comparator.as_sparql(), None)?,
            rhs_term,
        ],
    )?;
    let integrity = f_forall_this(f_guard_class(&class)?, reified);
    Ok(finalize_sugar(store, node, integrity)?.with_aggregate(agg))
}

/// Read ONE `logic:JoinLeg` structural predicate (`legSource`/`legTarget`/`legValue`/
/// `legRecordType`), enforcing it names an IRI — these are record→endpoint/value PREDICATES, not
/// data literals, so a literal or blank-node value is malformed, not a value to stringify.
/// `Ok(None)` ⇒ the predicate is absent (the caller decides whether that is a hard-skip); a
/// PRESENT non-IRI value is rejected outright via `Err` rather than silently stringified.
fn read_leg_iri(
    store: &RdfDataset,
    leg: &Subject,
    local: &str,
) -> gmeow_errors::Result<Option<String>> {
    match value(store, leg, &nn(&logic_iri(local))) {
        None => Ok(None),
        Some(Node::Iri(iri)) => Ok(Some(iri)),
        Some(other) => Err(sugar_err(format!(
            "logic:JoinLeg.{local} must be an IRI (a record → endpoint/value predicate), not {}",
            term_str(&other)
        ))),
    }
}

/// Read ONE `logic:JoinLeg` (a member of a `logic:joinPath` list): the required source / target /
/// value predicates plus the optional record type. A leg missing any of the three predicates is a
/// hard skip (the whole join-aggregate record degrades to one MALFORMED_CONSTRAINT warning); a
/// leg carrying a non-IRI (literal or blank node) value for any of the four is likewise rejected —
/// see [`read_leg_iri`].
fn read_join_leg(store: &RdfDataset, leg: &Subject) -> gmeow_errors::Result<JoinLeg> {
    let source = read_leg_iri(store, leg, "legSource")?.ok_or_else(|| {
        sugar_err("logic:JoinLeg requires logic:legSource (record → source endpoint)")
    })?;
    let target = read_leg_iri(store, leg, "legTarget")?.ok_or_else(|| {
        sugar_err("logic:JoinLeg requires logic:legTarget (record → target endpoint)")
    })?;
    let val = read_leg_iri(store, leg, "legValue")?.ok_or_else(|| {
        sugar_err("logic:JoinLeg requires logic:legValue (record → numeric leaf value)")
    })?;
    let record_type = read_leg_iri(store, leg, "legRecordType")?;
    JoinLeg::new(record_type, source, target, val)
}

/// P9 / join-aggregate — a multi-hop-join product-aggregate constraint: a `logic:onClass` focus, an
/// ordered `logic:joinPath` of at least two `logic:JoinLeg`s (each a reified relation record with a
/// source / target / value predicate), a `logic:aggFunction` (SUM for ∂²), a `logic:aggComparator`
/// invariant, and a fixed literal `logic:aggThreshold`. Expands to a [`ConstraintIr`] carrying BOTH
/// the honest reified FOL integrity (`joinAggregateComparison(this, fn, cmp, thr)`, guarded by the
/// class so the target derives) AND the structured [`JoinAggregate`] satellite (which drives the
/// real multi-hop-join `GROUP BY`/`HAVING` SPARQL projection). Generalizes
/// [`read_aggregate_constraint`] from a single-predicate focus aggregate to a chained-join product
/// aggregate; it is the sanctioned home of the general-CW ∂²=0 conformance check.
fn read_join_aggregate_constraint(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let function = sugar_selector_literal(store, node, "aggFunction")?;
    let comparator = AggregateComparator::from_symbol(&sugar_selector_literal(
        store,
        node,
        "aggComparator",
    )?)
    .ok_or_else(|| {
        sugar_err("logic:JoinAggregateConstraint requires a logic:aggComparator in =/!=/</<=/>/>=")
    })?;
    let (threshold_lexical, threshold_datatype) = match value(
        store,
        node,
        &nn(&logic_iri("aggThreshold")),
    ) {
        Some(Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            datatype,
            ..
        })) => (lexical, datatype),
        _ => {
            return Err(sugar_err(
                "logic:JoinAggregateConstraint requires a literal logic:aggThreshold (the fixed \
                 comparison value, e.g. 0)",
            ));
        }
    };
    // The ordered join path: an rdf:List whose members are the JoinLeg records, read in list order
    // (order is semantic — the chain composes leg[k].target into leg[k+1].source).
    let head = value(store, node, &nn(&logic_iri("joinPath"))).ok_or_else(|| {
        sugar_err("logic:JoinAggregateConstraint requires logic:joinPath (an rdf:List of ≥2 logic:JoinLeg)")
    })?;
    let leg_subjects = read_list_member_subjects(store, &head);
    let mut legs = Vec::with_capacity(leg_subjects.len());
    for leg in &leg_subjects {
        legs.push(read_join_leg(store, leg)?);
    }
    let ja = JoinAggregate::new(
        function,
        legs,
        comparator,
        threshold_lexical,
        threshold_datatype,
    )?;

    // The honest reified FOL integrity: a single reified `joinAggregateComparison` predication over
    // the focus, guarded by the target class so the target derives. It carries the aggregate
    // function, the comparator, and the threshold; the structured leg chain lives in the satellite,
    // which drives the real multi-hop-join GROUP BY/HAVING SPARQL projection.
    let threshold_term =
        Term::literal(ja.threshold_lexical.clone(), ja.threshold_datatype.clone())?;
    let reified = Formula::atom(
        Term::iri(logic_iri("joinAggregateComparison"))?,
        vec![
            t_var("this"),
            Term::literal(ja.function.clone(), None)?,
            Term::literal(ja.comparator.as_sparql(), None)?,
            threshold_term,
        ],
    )?;
    let integrity = f_forall_this(f_guard_class(&class)?, reified);
    Ok(finalize_sugar(store, node, integrity)?.with_join_aggregate(ja))
}

/// The double-entry balance sugar (`logic:AggregateBalanceConstraint`): a target class + the seven
/// predicate/value bindings of an [`AggregateBalance`]. Expands to a canonical guarded universal
/// carrying an honest reified `balancedByGroup` FOL predication (so the FOL canon is complete + the
/// target derives) PLUS the structured [`AggregateBalance`] satellite (which drives the real
/// `GROUP BY`/`HAVING` SPARQL projection).
fn read_aggregate_balance_constraint(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let read = |local: &str| -> gmeow_errors::Result<String> {
        value(store, node, &nn(&logic_iri(local)))
            .map(|t| term_str(&t))
            .ok_or_else(|| {
                sugar_err(format!(
                    "logic:AggregateBalanceConstraint requires logic:{local}"
                ))
            })
    };
    let posting = read("balancePostingPredicate")?;
    let partition = read("balancePartitionPredicate")?;
    let debit = read("balanceDebitValue")?;
    let credit = read("balanceCreditValue")?;
    let amount_node = read("balanceAmountNodePredicate")?;
    let value_pred = read("balanceValuePredicate")?;
    let group = read("balanceGroupPredicate")?;
    let balance = AggregateBalance::new(
        posting.clone(),
        partition.clone(),
        debit.clone(),
        credit.clone(),
        amount_node.clone(),
        value_pred.clone(),
        group.clone(),
    )?;
    // Honest reified FOL integrity: a single `balancedByGroup` predication over the focus carrying
    // every binding, guarded by the target class so the target derives. The realized FOL core has
    // no aggregate node, so the SPARQL projection uses the satellite for a real GROUP BY/HAVING.
    let reified = Formula::atom(
        Term::iri(logic_iri("balancedByGroup"))?,
        vec![
            t_var("this"),
            Term::iri(&posting)?,
            Term::iri(&partition)?,
            Term::iri(&debit)?,
            Term::iri(&credit)?,
            Term::iri(&amount_node)?,
            Term::iri(&value_pred)?,
            Term::iri(&group)?,
        ],
    )?;
    let integrity = f_forall_this(f_guard_class(&class)?, reified);
    Ok(finalize_sugar(store, node, integrity)?.with_aggregate_balance(balance))
}

/// Map an authored `logic:compareOp` symbol to the `logic:` comparison relation local name the
/// projector recognizes (the FORBIDDEN relation whose satisfaction is the violation).
fn compare_op_relation(op: &str) -> Option<&'static str> {
    match op.trim() {
        "=" | "==" => Some("termEqual"),
        "!=" | "≠" | "<>" => Some("termDistinct"),
        "<" => Some("termLess"),
        "<=" | "≤" => Some("termLessEqual"),
        ">" => Some("termGreater"),
        ">=" | "≥" => Some("termGreaterEqual"),
        _ => None,
    }
}

/// Map an authored `logic:nodeKind` token to the unary `logic:` node-kind relation local name the
/// projector recognizes (the REQUIRED kind; its failure is the violation).
fn node_kind_relation(kind: &str) -> Option<&'static str> {
    crate::ir::TermKindPredicate::from_authored_token(kind)
        .map(crate::ir::TermKindPredicate::local_name)
}

/// The SHACL node-kind an authored `logic:nodeKind` token names — the DECLARATIVE peer of
/// [`node_kind_relation`] (which names the same token's `logic:` term relation for the procedural
/// lowering). Accepts exactly the same token spellings, so the two projections of one record can
/// never disagree about which kinds are authorable; an unrecognized token yields `None` in both
/// (the procedural reader then raises the authoritative `MALFORMED_CONSTRAINT`).
fn shacl_node_kind_of(kind: &str) -> Option<ShaclNodeKind> {
    match node_kind_relation(kind)? {
        "termIsIri" => Some(ShaclNodeKind::Iri),
        "termIsLiteral" => Some(ShaclNodeKind::Literal),
        "termIsBlankOrIri" => Some(ShaclNodeKind::BlankNodeOrIri),
        _ => None,
    }
}

/// A comparison constraint: two focus-node property values compared. `logic:leftPath P` +
/// `logic:rightPath Q` + `logic:compareOp OP` where OP is the FORBIDDEN relation, so the violation
/// is `∃ l, r . P(this, l) ∧ Q(this, r) ∧ OP(l, r)` — e.g. a `gmeow:ScoreScale` whose
/// `gmeow:scaleMin >= gmeow:scaleMax`. Integrity: `∀ this . C(this) → ¬∃ l, r . (…)`.
fn read_comparison(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let left = value(store, node, &nn(&logic_iri("leftPath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:ComparisonConstraint requires logic:leftPath"))?;
    let right = value(store, node, &nn(&logic_iri("rightPath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:ComparisonConstraint requires logic:rightPath"))?;
    let op = value(store, node, &nn(&logic_iri("compareOp")))
        .map(|t| term_str(&t))
        .and_then(|s| compare_op_relation(&s))
        .ok_or_else(|| {
            sugar_err("logic:ComparisonConstraint requires a logic:compareOp in =/!=/</<=/>/>=")
        })?;
    let forbidden = Formula::Not(Box::new(Formula::Exists {
        vars: vec!["l".to_owned(), "r".to_owned()],
        body: Box::new(Formula::And(vec![
            f_atom2(&left, t_var("this"), t_var("l"))?,
            f_atom2(&right, t_var("this"), t_var("r"))?,
            f_atom2(&logic_iri(op), t_var("l"), t_var("r"))?,
        ])),
    }));
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, forbidden),
    )
}

/// A path node-kind constraint: every value on `logic:valuePath P` must be of the given
/// `logic:nodeKind` (`IRI` / `BlankNodeOrIRI` / `Literal`). Integrity:
/// `∀ this . guard → ∀ v . P(this, v) → kind(v)`. With `logic:onClass` the guard is class
/// membership; without it the trigger predicate `P` is the range restriction (→ `sh:targetSubjectsOf
/// P`, the "subjects of a predicate — its value must be an IRI" pattern).
fn read_path_node_kind(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_optional_target_class(store, node);
    let path = value(store, node, &nn(&logic_iri("valuePath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:PathNodeKindConstraint requires logic:valuePath"))?;
    let kind = value(store, node, &nn(&logic_iri("nodeKind")))
        .map(|t| term_str(&t))
        .and_then(|s| node_kind_relation(&s))
        .ok_or_else(|| {
            sugar_err("logic:PathNodeKindConstraint requires a logic:nodeKind in IRI/BlankNodeOrIRI/Literal")
        })?;
    let kind_atom = Formula::atom(Term::iri(logic_iri(kind))?, vec![t_var("v")])?;
    let inner = Formula::Forall {
        vars: vec!["v".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(f_atom2(&path, t_var("this"), t_var("v"))?),
            Box::new(kind_atom),
        )),
    };
    // With a target class the guard is class membership; without one the value-path atom IS the
    // guard (a predicate-presence range restriction → SubjectsOf).
    let guard = match &class {
        Some(c) => f_guard_class(c)?,
        None => f_atom2(&path, t_var("this"), t_var("g"))?,
    };
    finalize_sugar(store, node, f_forall_this(guard, inner))
}

/// A self-join uniqueness constraint: no two DISTINCT siblings reached through
/// `logic:siblingPredicate P` may share the same value on `logic:sharedPredicate Q`. Violation:
/// `∃ s1, s2, i . P(this, s1) ∧ P(this, s2) ∧ Q(s1, i) ∧ Q(s2, i) ∧ s1 ≠ s2` — e.g. two
/// argument slots with the same slot index. Integrity: `∀ this . guard → ¬∃ (…)`. With
/// `logic:onClass` the guard is class membership; without it `P` is the range restriction (→
/// `sh:targetSubjectsOf P`).
fn read_self_join_uniqueness(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_optional_target_class(store, node);
    let sibling = value(store, node, &nn(&logic_iri("siblingPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:SelfJoinUniquenessConstraint requires logic:siblingPredicate")
        })?;
    let shared = value(store, node, &nn(&logic_iri("sharedPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:SelfJoinUniquenessConstraint requires logic:sharedPredicate")
        })?;
    let forbidden = Formula::Not(Box::new(Formula::Exists {
        vars: vec!["s1".to_owned(), "s2".to_owned(), "i".to_owned()],
        body: Box::new(Formula::And(vec![
            f_atom2(&sibling, t_var("this"), t_var("s1"))?,
            f_atom2(&sibling, t_var("this"), t_var("s2"))?,
            f_atom2(&shared, t_var("s1"), t_var("i"))?,
            f_atom2(&shared, t_var("s2"), t_var("i"))?,
            f_atom2(&logic_iri("termDistinct"), t_var("s1"), t_var("s2"))?,
        ])),
    }));
    let guard = match &class {
        Some(c) => f_guard_class(c)?,
        None => f_atom2(&sibling, t_var("this"), t_var("g"))?,
    };
    finalize_sugar(store, node, f_forall_this(guard, forbidden))
}

/// An inverse-existence constraint: every `logic:onClass C` must be the OBJECT of
/// `logic:inversePredicate P` from some subject, optionally typed `logic:subjectClass T`.
/// Integrity: `∀ this . C(this) → ∃ s . ([T(s) ∧] P(s, this))` — e.g. a `lang:FeatureValue` must
/// be the `lang:denotationTarget` of some `lang:Denotation`.
fn read_inverse_existence(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let inverse = value(store, node, &nn(&logic_iri("inversePredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:InverseExistenceConstraint requires logic:inversePredicate")
        })?;
    let subject_class = value(store, node, &nn(&logic_iri("subjectClass"))).map(|t| term_str(&t));
    let mut conj = Vec::new();
    if let Some(t) = &subject_class {
        conj.push(f_atom2(RDF_TYPE, t_var("s"), Term::iri(t)?)?);
    }
    conj.push(f_atom2(&inverse, t_var("s"), t_var("this"))?);
    let existence = Formula::Exists {
        vars: vec!["s".to_owned()],
        body: Box::new(if conj.len() == 1 {
            conj.pop().expect("one atom")
        } else {
            Formula::And(conj)
        }),
    };
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, existence),
    )
}

/// The `logic:` transitive-reachability relation the projector lowers to a `subject <Q>+ target`
/// property path.
fn f_transitive_reach(subject: Term, path: &str, target: Term) -> gmeow_errors::Result<Formula> {
    Formula::atom(
        Term::iri(logic_iri("transitiveReach"))?,
        vec![subject, Term::iri(path)?, target],
    )
}

/// A transitive-reachability constraint: every value on `logic:viaPredicate P` must reach
/// `logic:reachTarget T` along a one-or-more `logic:pathPredicate Q` walk. Integrity:
/// `∀ this . C(this) → ∀ v . P(this, v) → v Q+ T` — e.g. a flagship scenario's enforced failure
/// class must transitively `rdfs:subClassOf+` the conformance-failure root.
fn read_transitive_reachability(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let via = value(store, node, &nn(&logic_iri("viaPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:TransitiveReachabilityConstraint requires logic:viaPredicate")
        })?;
    let path = value(store, node, &nn(&logic_iri("pathPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:TransitiveReachabilityConstraint requires logic:pathPredicate")
        })?;
    let target = value(store, node, &nn(&logic_iri("reachTarget")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:TransitiveReachabilityConstraint requires logic:reachTarget")
        })?;
    let inner = Formula::Forall {
        vars: vec!["v".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(f_atom2(&via, t_var("this"), t_var("v"))?),
            Box::new(f_transitive_reach(t_var("v"), &path, Term::iri(&target)?)?),
        )),
    };
    finalize_sugar(store, node, f_forall_this(f_guard_class(&class)?, inner))
}

/// An acyclicity constraint: no `logic:onClass C` may reach itself along a one-or-more
/// `logic:pathPredicate Q` walk. Integrity: `∀ this . C(this) → ¬ (this Q+ this)` — e.g. a form
/// slot may not depend (transitively) on itself.
fn read_acyclic(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let path = value(store, node, &nn(&logic_iri("pathPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:AcyclicConstraint requires logic:pathPredicate"))?;
    let forbidden = Formula::Not(Box::new(f_transitive_reach(
        t_var("this"),
        &path,
        t_var("this"),
    )?));
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, forbidden),
    )
}

/// P8 — value range: a target class + a value path + an inclusive lower and/or upper LITERAL
/// bound every value on the path must satisfy. Expands to
/// `∀ this, v . C(this) ∧ P(this, v) → v ≥ min ∧ v ≤ max` (one conjunct per authored bound) —
/// the exact formula shape the hand-authored range constraints carry (guard = class atom ∧
/// path atom, consequent = `logic:termGreaterEqual` / `logic:termLessEqual` filter atoms).
/// This is the DECIDABLE, validation-only home of a bounded numeric range: the OWL rendering
/// (a faceted-datatype `owl:allValuesFrom` filler) is undecidable for the native reasoner the
/// moment a literal is asserted on the constrained path, so it must never sit in the reasoning
/// core. The declarative twin (`sh:minInclusive`/`sh:maxInclusive` on the target class shape)
/// is lowered by [`derive_validation_shapes`].
fn read_value_range(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let path = value(store, node, &nn(&logic_iri("valuePath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:ValueRangeConstraint requires logic:valuePath"))?;
    let bound = |local: &str| -> gmeow_errors::Result<Option<Term>> {
        match value(store, node, &nn(&logic_iri(local))) {
            None => Ok(None),
            Some(Node::Lit(literal)) => Ok(Some(Term::rdf_literal(literal)?)),
            Some(_) => Err(sugar_err(format!(
                "logic:{local} must be a literal (an inclusive numeric bound)"
            ))),
        }
    };
    let mut cmps: Vec<Formula> = Vec::with_capacity(2);
    if let Some(min) = bound("minInclusiveBound")? {
        cmps.push(f_atom2(&logic_iri("termGreaterEqual"), t_var("v"), min)?);
    } else if let Some(min) = bound("minExclusiveBound")? {
        cmps.push(f_atom2(&logic_iri("termGreater"), t_var("v"), min)?);
    }
    if let Some(max) = bound("maxInclusiveBound")? {
        cmps.push(f_atom2(&logic_iri("termLessEqual"), t_var("v"), max)?);
    } else if let Some(max) = bound("maxExclusiveBound")? {
        cmps.push(f_atom2(&logic_iri("termLess"), t_var("v"), max)?);
    }
    let consequent = match cmps.len() {
        0 => {
            return Err(sugar_err(
                "logic:ValueRangeConstraint requires logic:minInclusiveBound / \
                 logic:minExclusiveBound and/or logic:maxInclusiveBound / logic:maxExclusiveBound",
            ));
        }
        1 => cmps.pop().expect("one bound"),
        _ => Formula::And(cmps),
    };
    let guard = Formula::And(vec![
        f_guard_class(&class)?,
        f_atom2(&path, t_var("this"), t_var("v"))?,
    ]);
    finalize_sugar(
        store,
        node,
        Formula::Forall {
            vars: vec!["this".to_owned(), "v".to_owned()],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(consequent))),
        },
    )
}

/// Read every compact constraint-sugar record (P1–P5, P7, P8, and the aggregate P6) into
/// [`ConstraintIr`]s. Supplied numeric range bounds must be admitted before extraction;
/// an unsupported bound refuses the source rather than weakening its constraint.
/// Other malformed records retain the `MALFORMED_CONSTRAINT` warning policy.
///
/// # Errors
/// Refuses any supplied range bound outside the finite numeric projection contract.
fn extract_sugar_constraints(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> gmeow_errors::Result<Vec<OwnerEmission<ConstraintIr>>> {
    type Reader = fn(&RdfDataset, &Subject) -> gmeow_errors::Result<ConstraintIr>;
    enum SugarReader {
        Ordinary(Reader),
        ForbiddenPattern,
    }
    use SugarReader::{ForbiddenPattern, Ordinary};
    let readers = [
        ("ChoiceGroupConstraint", Ordinary(read_choice_group)),
        (
            "GuardedImplicationConstraint",
            Ordinary(read_guarded_implication),
        ),
        (
            "DisjunctiveRequirednessConstraint",
            Ordinary(read_disjunctive_requiredness),
        ),
        ("PathValueTypeConstraint", Ordinary(read_path_value_type)),
        ("CrossNodeConstraint", Ordinary(read_cross_node)),
        ("ForbiddenPatternConstraint", ForbiddenPattern),
        ("ValueRangeConstraint", Ordinary(read_value_range)),
        ("AggregateConstraint", Ordinary(read_aggregate_constraint)),
        (
            "JoinAggregateConstraint",
            Ordinary(read_join_aggregate_constraint),
        ),
        (
            "AggregateBalanceConstraint",
            Ordinary(read_aggregate_balance_constraint),
        ),
        ("ComparisonConstraint", Ordinary(read_comparison)),
        ("PathNodeKindConstraint", Ordinary(read_path_node_kind)),
        (
            "SelfJoinUniquenessConstraint",
            Ordinary(read_self_join_uniqueness),
        ),
        (
            "InverseExistenceConstraint",
            Ordinary(read_inverse_existence),
        ),
        (
            "TransitiveReachabilityConstraint",
            Ordinary(read_transitive_reachability),
        ),
        ("AcyclicConstraint", Ordinary(read_acyclic)),
        (
            "ValueSetMembershipConstraint",
            Ordinary(read_value_set_membership),
        ),
        ("StringPatternConstraint", Ordinary(read_string_pattern)),
    ];
    let mut out: Vec<OwnerEmission<ConstraintIr>> = Vec::new();
    for (class_local, reader) in readers {
        let class_ty = Node::iri(logic_iri(class_local));
        for subj in subjects_of_structural_class(store, &class_ty) {
            if class_local == "ValueRangeConstraint" {
                // Validate every supplied endpoint, including a shadowed exclusive peer,
                // outside the recoverable reader boundary. Failure must abort the source.
                for local in [
                    "minInclusiveBound",
                    "minExclusiveBound",
                    "maxInclusiveBound",
                    "maxExclusiveBound",
                ] {
                    numeric_range_bound(store, &subj, &nn(&logic_iri(local)))?;
                }
            }
            let source = owner_source(store, &subj);
            let mut support = BTreeSet::new();
            if let Some(mut emitted) = capture_owner(
                source,
                OwnerFamily::Constraint,
                diagnostics,
                lowerings,
                |diagnostics| {
                    let result = match &reader {
                        Ordinary(read) => read(store, &subj),
                        ForbiddenPattern => {
                            read_forbidden_pattern(store, &subj, source, &mut support)
                        }
                    };
                    result.map_err(|err| {
                        diagnostics.push(Diagnostic::warning(
                            "MALFORMED_CONSTRAINT",
                            err.message().to_owned(),
                            Some(subject_str(&subj)),
                        ));
                        Vec::new()
                    })
                },
            ) {
                emitted.record_support = support;
                out.push(emitted);
            }
        }
    }
    Ok(out)
}

/// Parse a `logic:` RDF source already loaded into a wasm-clean [`RdfDataset`]
/// (default graph) into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_dataset(
    dataset: &RdfDataset,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), Diag> {
    require_nonempty_source(dataset)?;

    // Ordinary callers retain the mapping-free canonicalization path. Source-aware
    // callers use PreparedLogicSource and share the same extraction boundary below.
    let canon = canonicalize_blank_nodes(dataset)?;
    compile_canonical_source(canon.as_ref(), source_iri)
}

fn require_nonempty_source(dataset: &RdfDataset) -> Result<(), Diag> {
    if is_empty(dataset) {
        return Err(Diag::of_kind(crate::error::Frontend::new(
            "Source graph is empty — nothing to parse.  Pass a non-empty graph or a \
             Turtle file with logic: triples."
                .to_owned(),
        )));
    }

    Ok(())
}

fn compile_canonical_source(
    store: &RdfDataset,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), Diag> {
    let (program, diagnostics, _) =
        compile_source_graph(store, &StructuralSourceGraph::new(store), source_iri)?;
    Ok((program, diagnostics))
}

/// Compile source units into a canonical program with diagnostics and ownership traces.
/// Recoverable exclusions retain diagnostics rather than contributing weaker records.
///
/// # Errors
/// Propagates mandatory construction and range-bound admission failures.
fn compile_source_graph(
    store: &RdfDataset,
    source_graph: &StructuralSourceGraph,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>, EmissionTrace), Diag> {
    let mut diagnostics = source_graph.ownership_diagnostics(store);
    let mut owner_lowerings = emission::excluded_owners(source_graph);
    let mut reader = FormulaReader::new(store);

    let plain_axioms = extract_axioms(store, source_graph, &mut diagnostics);
    let scoped_axioms = extract_scoped_axioms(store, &mut diagnostics);
    // Lift the RDFS/SKOS annotation surface into first-class NodeKind::Annotation axioms
    // (logic: isSupersetOf SKOS/RDFS); fail-closed on a non-carrier language tag.
    let annotation_axioms = extract_annotation_axioms(store, &mut diagnostics);

    // Read the top-level `logic:Formula` trees, then ROUTE a trivially-Horn leaf (a reified
    // ordinary triple: an IRI relation over two non-sequence-marker arguments) to its axiom
    // home. `LogicProgram::with_formulas` hard-asserts against such a leaf — a fact would get
    // two content-addressed identities — so the invariant is enforced by routing rather than by
    // the (previously false) assumption that the projection never authors one. The regeneration
    // projection never emits a trivially-Horn top-level formula, so this partition is a no-op on
    // the pipeline corpus (byte-identical); a hand-authored / hostile candidate document CAN
    // emit one, and now becomes a fact (ground) or a rule-shaped axiom (variable) instead of
    // panicking. A degenerate binary atom with a literal / sequence-marker subject cannot be a
    // triple subject, so it is neither a formula nor an axiom: emit `MALFORMED_FORMULA`.
    let FormulaExtraction {
        presentation_formulas,
        formulas: extracted_formulas,
        malformed: malformed_formulas,
        lowerings: mut formula_lowerings,
    } = extract_formulas(&mut reader, source_graph, &mut diagnostics);
    let mut formulas: Vec<SourceFormula> = Vec::new();
    let mut horn_axioms: Vec<AxiomEmission> = Vec::new();
    for SourceFormula { source, formula: f } in extracted_formulas {
        if f.is_trivially_horn() {
            match f.as_horn_axiom() {
                Some(ax) => horn_axioms.push(AxiomEmission::new(ax, AxiomSource::Formula(source))),
                None => {
                    formula_lowerings.push(FormulaLowering {
                        source,
                        disposition: FormulaDisposition::Malformed {
                            diagnostic: diagnostics.len(),
                        },
                    });
                    diagnostics.push(Diagnostic::error(
                    "MALFORMED_FORMULA",
                    "trivially-Horn logic:Formula has a non-subject term (a literal or sequence \
                     marker) in argument position 0; it is neither a formula nor a fact; skipped",
                    Some(source_graph::focus(store, source.term)),
                ));
                }
            }
        } else {
            formulas.push(SourceFormula { source, formula: f });
        }
    }

    // Merge + dedup by full content (mirrors the Python `set(...) | set(...)`),
    // preserving first-occurrence order (plain, then scoped, then the routed Horn leaves) for
    // deterministic tie-breaking; `LogicProgram::new` then sorts canonically.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut all_axioms: Vec<AxiomEmission> = Vec::new();
    for emitted in plain_axioms
        .into_iter()
        .chain(scoped_axioms)
        .chain(horn_axioms)
        .chain(annotation_axioms)
    {
        let next = all_axioms.len();
        let index = *seen
            .entry(content_dedup_key(&emitted.axiom))
            .or_insert(next);
        if index == next {
            all_axioms.push(emitted);
        } else {
            all_axioms[index].sources.extend(emitted.sources);
        }
    }

    let mut contracts = extract_contracts(store, &mut diagnostics, &mut owner_lowerings);
    let mut rules = extract_rules(store, &mut diagnostics, &mut owner_lowerings);
    let mut path_shapes = extract_path_shapes(store, &mut diagnostics, &mut owner_lowerings);
    let mut correspondences =
        extract_correspondences(&mut reader, &mut diagnostics, &mut owner_lowerings);
    let mut compositions =
        extract_compositions(store, source_graph, &mut diagnostics, &mut owner_lowerings);
    // Resolve each leg IRI to its `gm:path` body so the round-trip gate can compose the
    // actual leg paths (not IRI strings). Leg-body-free corpora yield an empty registry and
    // leave the canonical key byte-identical (the segment is append-only).
    let transaction_programs = extract_owned_leg_programs(
        store,
        &correspondences,
        &mut diagnostics,
        &mut owner_lowerings,
    );

    // Authored `logic:Constraint` individuals PLUS the compact constraint-sugar records (P1–P5,
    // P7, and the aggregate P6), each expanded to one canonical `ConstraintIr`. Both feed the same
    // `LogicProgram::constraints` home (which sorts + content-keys them canonically).
    let mut constraints = extract_constraints(
        &mut reader,
        &mut diagnostics,
        &malformed_formulas,
        &mut owner_lowerings,
    );
    constraints.extend(extract_sugar_constraints(
        store,
        &mut diagnostics,
        &mut owner_lowerings,
    )?);

    // Authored `logic:ReasoningProgram` clause-set-plus-goal individuals. A structurally
    // malformed program emits an error-grade MALFORMED_REASONING_PROGRAM diagnostic above
    // and is excluded here; every other authored program still compiles.
    let mut reasoning_programs =
        extract_reasoning_programs(&mut reader, &mut diagnostics, &mut owner_lowerings);

    // Sort values WITH their origins, then detach them once. The normal constructor
    // receives empty axiom/formula vectors so neither trees nor keys are rebuilt and
    // the already canonical vectors need no second sort. No generated-IRI matching.
    all_axioms.sort_by_cached_key(|emission| emission.axiom.sort_key());
    let mut axiom_sources = Vec::with_capacity(all_axioms.len());
    let mut axioms = Vec::with_capacity(all_axioms.len());
    for (index, mut emission) in all_axioms.into_iter().enumerate() {
        emission.sources.sort_unstable();
        emission.sources.dedup();
        for source in &emission.sources {
            if let AxiomSource::Formula(source) = source {
                formula_lowerings.push(FormulaLowering {
                    source: *source,
                    disposition: FormulaDisposition::Axiom(index),
                });
            }
        }
        axioms.push(emission.axiom);
        axiom_sources.push(emission.sources);
    }
    formulas.sort_by_cached_key(|emission| emission.formula.sort_key());
    let formulas = formulas
        .into_iter()
        .enumerate()
        .map(|(index, emission)| {
            formula_lowerings.push(FormulaLowering {
                source: emission.source,
                disposition: FormulaDisposition::Formula(index),
            });
            emission.formula
        })
        .collect();
    formula_lowerings.sort_by_key(|lowering| lowering.source);

    rules.sort_by_cached_key(|emission| emission.value.sort_key());
    contracts.sort_by_cached_key(|emission| emission.value.sort_key());
    path_shapes.sort_by_cached_key(|emission| emission.value.sort_key());
    correspondences.sort_by(|a, b| a.value.iri.cmp(&b.value.iri));
    compositions.sort_by(|a, b| a.value.cmp(&b.value));
    constraints.sort_by(|a, b| a.value.iri.cmp(&b.value.iri));
    reasoning_programs.sort_by(|a, b| a.value.iri.cmp(&b.value.iri));
    // Leg extraction already visits sorted unique source IRIs.
    let mut program = LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri)
        .with_ordered_correspondences(finish_owners(correspondences, &mut owner_lowerings))?
        .with_ordered_constraints(finish_owners(constraints, &mut owner_lowerings))
        .with_ordered_reasoning_programs(finish_owners(reasoning_programs, &mut owner_lowerings));
    program.rules = finish_owners(rules, &mut owner_lowerings);
    program.contracts = finish_owners(contracts, &mut owner_lowerings);
    program.path_shapes = finish_owners(path_shapes, &mut owner_lowerings);
    program.transaction_programs = finish_owners(transaction_programs, &mut owner_lowerings);
    program.correspondence_compositions = finish_owners(compositions, &mut owner_lowerings);
    program.axioms = axioms;
    program.formulas = formulas;
    program.presentations = presentation::extract(
        store,
        source_graph,
        &program,
        &presentation_formulas,
        &mut diagnostics,
        &mut owner_lowerings,
    );
    owner_lowerings.sort_by_key(|lowering| (lowering.source, lowering.family));
    Ok((
        program,
        diagnostics,
        EmissionTrace {
            presentation_formulas,
            axiom_sources,
            formulas: formula_lowerings,
            owners: owner_lowerings,
        },
    ))
}

/// Parse Turtle source text into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_str(
    turtle: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), Diag> {
    // Native codec parse → frozen wasm-clean IR dataset, straight into the parser
    // (no oxigraph Store hop).
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None).map_err(|e| {
        Diag::of_kind(crate::error::SourceParse {
            format: "Turtle",
            cause: std::sync::Arc::new(e),
        })
    })?;
    parse_logic_dataset(dataset.as_ref(), source_iri)
}

/// Parse a Turtle file into a [`LogicProgram`] + diagnostics.  When `source_iri`
/// is `None`, the file URI is recorded as the program's provenance source.
///
/// Native-only: `wasm32` has no filesystem, so the wasm-able compiler exposes only
/// the in-memory `parse_logic_str` / `parse_logic_dataset` entry points.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_logic_path(
    path: &Path,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), Diag> {
    let source_iri = source_iri.or_else(|| path_to_file_uri(path));
    let turtle = std::fs::read_to_string(path).map_err(|e| {
        Diag::of_kind(crate::error::SourceRead {
            path: path.to_path_buf(),
            cause: std::sync::Arc::new(e),
        })
    })?;
    parse_logic_str(&turtle, source_iri)
        .map_err(|error| error.with_context(format!("source file {}", path.display())))
}

/// A content-dedup key including scope (the Rust analogue of frozen-dataclass
/// equality used by the Python `set` merge).
fn content_dedup_key(ax: &LogicAxiom) -> String {
    let conf = ax
        .scope
        .confidence
        .map(|c| {
            let c = if c == 0.0 { 0.0 } else { c }; // collapse -0.0 -> 0.0
            c.to_string()
        })
        .unwrap_or_default();
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        ax.sort_key(),
        ax.scope.standpoint.as_deref().unwrap_or(""),
        ax.scope.time.as_deref().unwrap_or(""),
        conf,
        ax.scope.modality.as_str(),
        ax.scope.provenance.as_deref().unwrap_or(""),
        ax.scope.module.as_deref().unwrap_or(""),
    )
}

/// Build a `file://` URI for a path (best-effort; mirrors `Path.as_uri()`).
#[cfg(not(target_arch = "wasm32"))]
fn path_to_file_uri(path: &Path) -> Option<String> {
    let abs = std::fs::canonicalize(path).ok()?;
    #[cfg(windows)]
    {
        let mut s = abs.to_string_lossy().into_owned();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            s = stripped.to_string();
        }
        let s = s.replace('\\', "/");
        let s = if s.starts_with('/') {
            s
        } else {
            format!("/{s}")
        };
        Some(format!("file://{s}"))
    }
    #[cfg(not(windows))]
    {
        Some(format!("file://{}", abs.display()))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shape_derivation_tests;

#[cfg(test)]
mod native_statement_tests;

#[cfg(test)]
mod native_literal_tests;

#[cfg(test)]
mod compact_term_tests;
