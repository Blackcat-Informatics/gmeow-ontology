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
//! * **Fail-soft** on recoverable issues — a malformed axiom, a rule with no
//!   head, an unrecognised profile IRI — emits a `WARNING` [`Diagnostic`] and is
//!   skipped.
//! * **Raise** ([`LogicParseError`]) on unparsable input: an empty graph or a
//!   file that cannot be read/parsed.
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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use gmeow_errors::Diag;
use purrdf::{RdfDataset, parse_dataset};

use super::compat;
use super::graphutil::{
    Iri, Node, RDF_OBJECT, RDF_PREDICATE, RDF_REIFIES, RDF_STATEMENT, RDF_SUBJECT, RDF_TYPE,
    Subject, canonicalize_blank_nodes, contains, default_graph_quads, has_predicate,
    has_predicate_object, is_empty, nn, objects, subject_is_blank, subject_str, subjects_with,
    term_as_subject, term_is_literal, term_str, value,
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

// --------------------------------------------------------------------------- //
// Diagnostics
// --------------------------------------------------------------------------- //

/// Severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    fn error(code: &str, message: impl Into<String>, subject: Option<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_owned(),
            message: message.into(),
            subject,
        }
    }
}

/// Raised for unparsable input (empty graph, unreadable / malformed file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicParseError(pub String);

impl fmt::Display for LogicParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LogicParseError {}

// --------------------------------------------------------------------------- //
// Scope extraction
// --------------------------------------------------------------------------- //

fn confidence_from_term(term: &Node) -> Option<f64> {
    if let Node::Lit { lexical, .. } = term
        && let Ok(val) = lexical.parse::<f64>()
        && (0.0..=1.0).contains(&val)
    {
        return Some(val);
    }
    None
}

fn modality_from_term(term: Option<&Node>) -> LogicModality {
    let Some(term) = term else {
        return LogicModality::None;
    };
    let mut raw = term_str(term);
    if let Some(stripped) = raw.strip_prefix(LOGIC_NAMESPACE) {
        raw = stripped.to_owned();
    }
    LogicModality::from_str_value(&raw.to_lowercase()).unwrap_or(LogicModality::None)
}

/// Extract a [`ContextualScope`] from `logic:` annotations on `node`.
fn scope_from_node(
    store: &RdfDataset,
    node: &Subject,
    diagnostics: &mut Vec<Diagnostic>,
) -> ContextualScope {
    let standpoint = value(store, node, &nn(&logic_iri("standpoint"))).map(|t| term_str(&t));
    let time = value(store, node, &nn(&logic_iri("time"))).map(|t| term_str(&t));

    let conf_node = value(store, node, &nn(&logic_iri("confidence")));
    let confidence = conf_node.as_ref().and_then(confidence_from_term);
    if let Some(cn) = &conf_node
        && confidence.is_none()
    {
        diagnostics.push(Diagnostic::warning(
            "INVALID_CONFIDENCE",
            format!(
                "confidence value {:?} is not a float in [0, 1]; ignored",
                term_str(cn)
            ),
            Some(subject_str(node)),
        ));
    }

    let modality = modality_from_term(value(store, node, &nn(&logic_iri("modality"))).as_ref());
    let provenance = value(store, node, &nn(&logic_iri("provenance"))).map(|t| term_str(&t));
    // `logic:inModule` — the Common Logic module (theory context) this statement is in.
    let module = value(store, node, &nn(&logic_iri("inModule"))).map(|t| term_str(&t));

    // `ContextualScope::new` only fails on out-of-range confidence, which we have
    // already filtered to `None` above, so this never errors.
    ContextualScope::new(standpoint, time, confidence, modality, provenance, module)
        .unwrap_or_default()
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
            "closureEntry" | "closureKey" | "closureValue" | "complexityClass"
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

/// Read a rule node's aggregation (reduce) spec, when present. Requires the function, the
/// aggregated variable, and the result variable; the group keys are optional (a reduce with no
/// group key aggregates the whole relation). A node carrying a partial spec is a hard skip with
/// a diagnostic, never a silent drop.
fn aggregation_from_node(
    store: &RdfDataset,
    node: &Subject,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<AggregateSpec> {
    let function = value(store, node, &nn(&logic_iri("aggregateFunction"))).map(|t| term_str(&t));
    let aggregate_var =
        value(store, node, &nn(&logic_iri("aggregateVariable"))).map(|t| term_str(&t));
    let result_var = value(store, node, &nn(&logic_iri("aggregateResult"))).map(|t| term_str(&t));
    // No aggregation surface at all → an ordinary Horn rule.
    if function.is_none() && aggregate_var.is_none() && result_var.is_none() {
        return None;
    }
    let (Some(function), Some(aggregate_var), Some(result_var)) =
        (function, aggregate_var, result_var)
    else {
        diagnostics.push(Diagnostic::warning(
            "MALFORMED_RULE_AGGREGATION",
            "logic:Rule aggregation needs logic:aggregateFunction, logic:aggregateVariable, \
             and logic:aggregateResult; partial spec skipped",
            Some(subject_str(node)),
        ));
        return None;
    };
    let group_keys: Vec<String> = objects(store, node, &nn(&logic_iri("groupKey")))
        .iter()
        .map(term_str)
        .collect();
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
fn collect_contract_config_subjects(store: &RdfDataset) -> HashSet<String> {
    let mut subjects: HashSet<String> = HashSet::new();
    for class_local in ["ReasoningContract", "ReasoningPreset", "ClosureEntry"] {
        let class_term = Node::iri(logic_iri(class_local));
        for subj in subjects_with(store, &nn(RDF_TYPE), &class_term) {
            subjects.insert(subject_str(&subj));
        }
    }
    subjects
}

/// Collect the IRIs / blank-node ids of every `logic:RecoveryCase` node reached as the
/// object of some `logic:recoveryCase` edge — i.e. every recovery case OWNED by a
/// correspondence. A `logic:RecoveryCase` node typed but never reached this way is
/// authored recovery evidence with no owner, so it must not be silently dropped.
fn collect_owned_recovery_cases(store: &RdfDataset) -> HashSet<String> {
    let recovery_case_pred = logic_iri("recoveryCase");
    default_graph_quads(store)
        .into_iter()
        .filter(|quad| quad.predicate.as_str() == recovery_case_pred)
        .map(|quad| term_str(&quad.object))
        .collect()
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
) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();
    for quad in default_graph_quads(store) {
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
        let Node::Lit { lexical, lang, .. } = &quad.object else {
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
        if lang.as_deref() != Some(X_GMEOW_ENGLISH_TAG) {
            continue;
        }
        match LogicAxiom::new(
            subject_str(&quad.subject),
            p_str,
            lexical.clone(),
            true,
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(
                ax.with_node_kind(NodeKind::Annotation)
                    .with_load_bearing(annotation_pred_is_load_bearing(p_str)),
            ),
            Err(exc) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_ANNOTATION",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }
    axioms
}

fn extract_axioms(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();

    // Meta-config subjects (contracts / presets / closure entries): facet-config
    // triples on these are contract configuration, not domain facts.
    let config_subjects = collect_contract_config_subjects(store);

    // Recovery cases owned by some correspondence (via `logic:recoveryCase`); a
    // `logic:RecoveryCase` typing not in this set is orphaned evidence (see step 2 below).
    let owned_recovery_cases = collect_owned_recovery_cases(store);

    // Class-expression restrictions authored in logic: (`C logic:subClassOf
    // [ a logic:Restriction ; logic:onProperty P ; logic:someValuesFrom D ]`) lift
    // through the SAME skolemizer the owl: adapter uses, so an owl:-authored and a
    // logic:-authored restriction normalize to identical skolem-keyed axioms (the
    // isomorphism gate).  The node set drives the skip filter below so the blank
    // restriction node's internals never leak as blank-labelled axioms — the load-
    // bearing ordering: skolemize FIRST, then skip restriction-internal triples.
    let logic_vocab = restriction::RestrictionVocab::logic();
    let mut rnodes = restriction::restriction_node_labels(store, &logic_vocab);
    rnodes.extend(restriction::enumeration_node_labels(store, &logic_vocab));
    rnodes.extend(restriction::datarange_node_labels(store, &logic_vocab));
    let mut lifted_class_exprs =
        restriction::skolemize_restrictions(store, &logic_vocab, diagnostics);
    lifted_class_exprs.extend(restriction::skolemize_enumerations(
        store,
        &logic_vocab,
        diagnostics,
    ));
    lifted_class_exprs.extend(restriction::skolemize_dataranges(
        store,
        &logic_vocab,
        diagnostics,
    ));
    for lifted in lifted_class_exprs {
        if let Ok(ax) = LogicAxiom::new(
            lifted.subject,
            lifted.predicate,
            lifted.obj,
            lifted.obj_is_literal,
            false,
            ContextualScope::default(),
        ) {
            axioms.push(ax);
        }
    }

    // 1. Triples with a logic: predicate (excluding rdf:type).
    for quad in default_graph_quads(store) {
        let p_str = quad.predicate.as_str();
        if !p_str.starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        if p_str == RDF_TYPE {
            continue; // unreachable (rdf:type is not logic:) but mirrors Python.
        }
        // Restriction internals + anchor edges are owned by the skolemizer above.
        // Skip a triple whose subject is a restriction node, and a subClassOf /
        // equivalentClass edge whose object is a restriction node (re-emitted
        // redirected to the skolem node).
        if rnodes.contains(&subject_str(&quad.subject)) {
            continue;
        }
        let p_local = &p_str[LOGIC_NAMESPACE.len()..];
        if matches!(p_local, "subClassOf" | "equivalentClass")
            && rnodes.contains(&term_str(&quad.object))
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
        match LogicAxiom::new(
            subject_str(&quad.subject),
            p_str,
            term_str(&quad.object),
            term_is_literal(&quad.object),
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(ax),
            Err(exc) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_AXIOM",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }

    // 2. rdf:type triples whose object is a logic: class.
    for quad in default_graph_quads(store) {
        if quad.predicate.as_str() != RDF_TYPE {
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
        if matches!(o_local, "Formula" | "TermCarrier") {
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
            if !owned_recovery_cases.contains(&case_iri) {
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
        if subject_str(&quad.subject).starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        match LogicAxiom::new(
            subject_str(&quad.subject),
            RDF_TYPE,
            o_str,
            false,
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(ax),
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

fn extract_scoped_axioms(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();

    // RDF 1.2 style: reifier node with rdf:reifies → triple term.
    for quad in default_graph_quads(store) {
        if quad.predicate.as_str() != RDF_REIFIES {
            continue;
        }
        let reifier = quad.subject.clone();
        let scope = scope_from_node(store, &reifier, diagnostics);
        if let Node::Triple(triple) = &quad.object {
            let t_p = triple.predicate.as_str();
            if !t_p.starts_with(LOGIC_NAMESPACE) && t_p != RDF_TYPE {
                continue;
            }
            let t_s = subject_str(&triple.subject);
            let t_o = term_str(&triple.object);
            match LogicAxiom::new(t_s, t_p, t_o, term_is_literal(&triple.object), false, scope) {
                Ok(ax) => axioms.push(ax),
                Err(exc) => diagnostics.push(Diagnostic::warning(
                    "MALFORMED_SCOPED_AXIOM",
                    exc.message().to_owned(),
                    Some(subject_str(&reifier)),
                )),
            }
        }
    }

    // Classic reification: rdf:Statement nodes with logic: scope annotations.
    let rdf_type_term = Node::iri(RDF_STATEMENT);
    for stmt in subjects_with(store, &nn(RDF_TYPE), &rdf_type_term) {
        let scope = scope_from_node(store, &stmt, diagnostics);
        if scope == ContextualScope::default() {
            continue;
        }
        let t_p = value(store, &stmt, &nn(RDF_PREDICATE));
        let Some(t_p) = t_p else {
            diagnostics.push(Diagnostic::warning(
                "MISSING_PREDICATE",
                "rdf:Statement node has no rdf:predicate; skipped",
                Some(subject_str(&stmt)),
            ));
            continue;
        };
        let p_str = term_str(&t_p);
        if !p_str.starts_with(LOGIC_NAMESPACE) && p_str != RDF_TYPE {
            continue;
        }
        let t_s = value(store, &stmt, &nn(RDF_SUBJECT));
        let t_o = value(store, &stmt, &nn(RDF_OBJECT));
        let subj = t_s.as_ref().map(term_str).unwrap_or_default();
        let obj = t_o.as_ref().map(term_str).unwrap_or_default();
        let obj_is_literal = t_o.as_ref().is_some_and(term_is_literal);
        match LogicAxiom::new(subj, p_str, obj, obj_is_literal, false, scope) {
            Ok(ax) => axioms.push(ax),
            Err(exc) => diagnostics.push(Diagnostic::warning(
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
    for ty in objects(store, &subject, &nn(RDF_TYPE)) {
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
/// canonical RDF12 projection emits facet selections as bare
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
/// correct field of `contract`. Returns `false` if the facet class is unknown
/// (caller emits the `UNKNOWN_PROFILE` diagnostic).
fn route_facet_value(contract: &mut ReasoningContract, facet_class: &str, value_local: String) {
    match facet_class {
        "FormulaFragment" => contract.formula_fragment = Some(value_local),
        "ModelSemantics" => contract.model_semantics = Some(value_local),
        "TruthAlgebra" => contract.truth_algebra = Some(value_local),
        "AdmissibleValuationPolicy" => contract.admissible_valuation = Some(value_local),
        "DesignatedValueSet" => contract.designated_values = Some(value_local),
        "EvolutionMode" => contract.evolution = Some(value_local),
        "ArgumentationSemantics" => contract.argumentation = Some(value_local),
        "RevisionPolicy" => contract.revision = Some(value_local),
        "EqualityPolicy" => contract.equality_policy = Some(value_local),
        // `ClosureValue` reached via `logic:defaultClosure` sets the map default.
        "ClosureValue" => contract.default_closure = Some(value_local),
        "NegationOperator" => {
            contract.negation_operators.insert(value_local);
        }
        "ContextAxis" => {
            contract.context_axes.insert(value_local);
        }
        "UncertaintyMeasure" => {
            contract.uncertainty_measures.insert(value_local);
        }
        "ResourcePolicy" => {
            contract.resource_policies.insert(value_local);
        }
        "ProjectionTarget" => {
            contract.projection_targets.insert(value_local);
        }
        _ => {}
    }
}

/// Whether the source graph declares a probability model: either a triple with
/// predicate `logic:probabilityModel`, or any individual typed
/// `logic:ProbabilityModel` (probabilistic inference must never
/// silently assume independence over un-modelled confidence metadata).
fn graph_declares_probability_model(store: &RdfDataset) -> bool {
    // Any triple whose predicate is logic:probabilityModel.
    if has_predicate(store, &nn(&logic_iri("probabilityModel"))) {
        return true;
    }
    // Any individual typed logic:ProbabilityModel.
    let prob_model_class = Node::iri(logic_iri("ProbabilityModel"));
    has_predicate_object(store, &nn(RDF_TYPE), &prob_model_class)
}

fn extract_contracts(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ReasoningContract> {
    let mut contracts: Vec<ReasoningContract> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Computed once: whether the graph declares any logic:ProbabilityModel; used
    // by the graph-dependent RuleProbabilisticRequiresModel below.
    let has_probability_model = graph_declares_probability_model(store);

    // Subjects typed logic:ReasoningContract OR logic:ReasoningPreset.
    let mut subjects: Vec<Subject> = Vec::new();
    for class_local in ["ReasoningContract", "ReasoningPreset"] {
        let class_term = Node::iri(logic_iri(class_local));
        subjects.extend(subjects_with(store, &nn(RDF_TYPE), &class_term));
    }

    for individual in subjects {
        let iri_str = subject_str(&individual);
        if !seen.insert(iri_str.clone()) {
            continue;
        }

        let mut contract = ReasoningContract::new();

        // If it is a recognised preset, record its preset id; an unrecognised
        // preset is a hard-skip with a diagnostic (behaviour-preserving).
        let is_preset = contains(
            store,
            &individual,
            &nn(RDF_TYPE),
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
                    continue;
                }
            }
        }

        // Direct facet properties + the expandsToFacet bundle. A DIRECT facet
        // property (everything but expandsToFacet) routes by the PROPERTY name —
        // so the rdf:type-less canonical RDF12 projection round-trips; the
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
                        route_facet_value(&mut contract, &facet_class, value_local)
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
        // logic:closureKey (string) + logic:closureValue (ClosureValue individual).
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
            let key =
                value(store, &entry_node, &nn(&logic_iri("closureKey"))).map(|t| term_str(&t));
            let val = value(store, &entry_node, &nn(&logic_iri("closureValue"))).map(|t| {
                let v = term_str(&t);
                v.strip_prefix(LOGIC_NAMESPACE).unwrap_or(&v).to_owned()
            });
            match (key, val) {
                (Some(key), Some(val)) => {
                    contract.closure_entries.insert(key, val);
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

        // Carried decidability data (reviewer B2): logic:complexityClass.
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

        // Graph-dependent RuleProbabilisticRequiresModel: a
        // probabilistic measure demands a declared logic:ProbabilityModel; absent
        // one, refuse rather than silently assume independence.
        if contract
            .uncertainty_measures
            .contains("ProbabilisticMeasure")
            && !has_probability_model
        {
            diagnostics.push(Diagnostic::error(
                "UNSUPPORTED_CONTRACT",
                format!(
                    "reasoning contract {iri_str:?} carries logic:ProbabilisticMeasure but the \
                     graph declares no logic:ProbabilityModel: a probabilistic measure requires a \
                     declared logic:ProbabilityModel (it is never inferred from confidence \
                     metadata)"
                ),
                Some(iri_str.clone()),
            ));
        }

        contracts.push(contract);
    }

    contracts
}

// --------------------------------------------------------------------------- //
// Rule extraction (forward-compatible: absent logic:Rule → empty list)
// --------------------------------------------------------------------------- //

/// Read a reified atom node (`rdf:subject` / `rdf:predicate` / `rdf:object`) into
/// a [`LogicAxiom`], returning `Err` with a message on a missing predicate or a
/// validation failure.
fn read_reified_axiom(
    store: &RdfDataset,
    node: &Subject,
    negated: bool,
) -> gmeow_errors::Result<LogicAxiom> {
    let p = value(store, node, &nn(RDF_PREDICATE));
    let Some(p) = p else {
        return Err(Diag::of_kind(crate::error::Frontend {
            detail: "__missing_predicate__".to_owned(),
        }));
    };
    let s = value(store, node, &nn(RDF_SUBJECT));
    let o = value(store, node, &nn(RDF_OBJECT));
    let subj = s.as_ref().map(term_str).unwrap_or_default();
    let obj = o.as_ref().map(term_str).unwrap_or_default();
    let obj_is_literal = o.as_ref().is_some_and(term_is_literal);
    LogicAxiom::new(
        subj,
        term_str(&p),
        obj,
        obj_is_literal,
        negated,
        ContextualScope::default(),
    )
}

fn extract_rules(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicRule> {
    let mut rules: Vec<LogicRule> = Vec::new();

    let logic_rule = Node::iri(logic_iri("Rule"));
    let logic_head = nn(&logic_iri("head"));
    let logic_body = nn(&logic_iri("body"));
    let logic_negated_body = nn(&logic_iri("negatedBody"));
    let logic_distinct_body = nn(&logic_iri("distinctBody"));

    for rule_node in subjects_with(store, &nn(RDF_TYPE), &logic_rule) {
        let scope = scope_from_node(store, &rule_node, diagnostics);

        // Head.
        let Some(head_term) = value(store, &rule_node, &logic_head) else {
            diagnostics.push(Diagnostic::warning(
                "MISSING_RULE_HEAD",
                "logic:Rule node has no logic:head; skipped",
                Some(subject_str(&rule_node)),
            ));
            continue;
        };
        let Some(head_node) = term_as_subject(&head_term) else {
            diagnostics.push(Diagnostic::warning(
                "MALFORMED_RULE_HEAD",
                "logic:head node has no rdf:predicate; skipped",
                Some(subject_str(&rule_node)),
            ));
            continue;
        };
        let head_axiom = match read_reified_axiom(store, &head_node, false) {
            Ok(ax) => ax,
            Err(msg) => {
                let message = if msg.message() == "__missing_predicate__" {
                    "logic:head node has no rdf:predicate; skipped".to_owned()
                } else {
                    msg.message().to_owned()
                };
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_RULE_HEAD",
                    message,
                    Some(subject_str(&rule_node)),
                ));
                continue;
            }
        };

        // Body (positive then negated).
        let mut body_axioms: Vec<LogicAxiom> = Vec::new();
        for (body_predicate, negated) in [(&logic_body, false), (&logic_negated_body, true)] {
            for body_term in objects(store, &rule_node, body_predicate) {
                let Some(body_node) = term_as_subject(&body_term) else {
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_RULE_BODY",
                        "logic:body node has no rdf:predicate; body atom skipped",
                        Some(subject_str(&rule_node)),
                    ));
                    continue;
                };
                match read_reified_axiom(store, &body_node, negated) {
                    Ok(ax) => body_axioms.push(ax),
                    Err(msg) => {
                        let message = if msg.message() == "__missing_predicate__" {
                            "logic:body node has no rdf:predicate; body atom skipped".to_owned()
                        } else {
                            msg.message().to_owned()
                        };
                        diagnostics.push(Diagnostic::warning(
                            "MALFORMED_RULE_BODY",
                            message,
                            Some(subject_str(&rule_node)),
                        ));
                    }
                }
            }
        }

        // Inequality guards: logic:distinctBody nodes carry rdf:subject /
        // rdf:object variable Literals and NO rdf:predicate.
        let mut distinct_pairs: Vec<(String, String)> = Vec::new();
        for distinct_term in objects(store, &rule_node, &logic_distinct_body) {
            let Some(distinct_node) = term_as_subject(&distinct_term) else {
                continue;
            };
            let d_s = value(store, &distinct_node, &nn(RDF_SUBJECT));
            let d_o = value(store, &distinct_node, &nn(RDF_OBJECT));
            let (Some(d_s), Some(d_o)) = (d_s, d_o) else {
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_RULE_BODY",
                    "logic:distinctBody node lacks rdf:subject or rdf:object; \
                     inequality guard skipped",
                    Some(subject_str(&rule_node)),
                ));
                continue;
            };
            let d_s_str = term_str(&d_s);
            let d_o_str = term_str(&d_o);
            if !d_s_str.starts_with('?') || !d_o_str.starts_with('?') {
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_RULE_BODY",
                    format!(
                        "logic:distinctBody guard terms must both be variables (?-prefixed); \
                         got {:?}; inequality guard skipped",
                        (d_s_str.clone(), d_o_str.clone())
                    ),
                    Some(subject_str(&rule_node)),
                ));
                continue;
            }
            distinct_pairs.push((d_s_str, d_o_str));
        }

        let mut rule = LogicRule::new(head_axiom, body_axioms, distinct_pairs, scope);
        if let Some(agg) = aggregation_from_node(store, &rule_node, diagnostics) {
            rule = rule.with_aggregation(agg);
        }
        rules.push(rule);
    }

    rules
}

// --------------------------------------------------------------------------- //
// Path-shape extraction (absent logic:PathShape → empty list)
// --------------------------------------------------------------------------- //

/// Parse a positive-integer literal's lexical value (`xsd:positiveInteger`),
/// returning `None` on a non-numeric, overflowing, or non-positive value.
fn parse_positive_int(lexical: &str) -> Option<u32> {
    match lexical.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Some(n),
        _ => None,
    }
}

/// Derive closed-world [`ValidationShapeIr`]s from an ontology graph's OWL restrictions (the
/// closed-world validation reading of the open-world axioms). For every `Class rdfs:subClassOf [
/// owl:onProperty P ; owl:someValuesFrom C ]` the target `Class` gets a property shape on `P`
/// carrying `sh:class C` (or `sh:datatype C` when `C` is a concrete datatype — a literal is never
/// an instance of a class, so `sh:class` on a datatype would flag every node; or
/// `sh:nodeKind sh:BlankNodeOrIRI` when `C` is `owl:Thing` — a universal-top range says "any
/// individual", and spec-conformant `sh:class owl:Thing` would demand a never-materialized
/// `rdf:type owl:Thing` edge, so the faithful closed-world projection of the open range is a
/// node-kind constraint; or `sh:nodeKind sh:Literal` when `C` is `rdfs:Literal` — a
/// universal-literal-top range says "any literal", and `sh:datatype rdfs:Literal` never matches
/// a concrete literal, so the faithful projection is again a node-kind constraint). Unqualified
/// `owl:cardinality` / `owl:minCardinality` / `owl:maxCardinality` restrictions lower to
/// `sh:minCount`/`sh:maxCount` with [`ConstraintProvenance::OwlRestriction`]. Derived
/// constraints are grouped per class and sorted + deduped for determinism.
///
/// These are closed-world readings of open-world axioms, so their ledger polarity is
/// `logic:ValidationOnly` (an under-approximation) — never claimed as an entailment
/// (Principle 17). **Public** so the pipeline can run it over the merged authored ontology
/// (where the domain restrictions live), not just the logic: front-end source.
///
/// Hard-fails (returns `Err`) if a derived constraint is malformed (e.g. a cardinality
/// restriction with `minCardinality > maxCardinality`) rather than silently dropping it — a
/// required structural element that cannot be represented is a hard error, not a fallback.
/// The per-property / per-class **validation-reading opt-out** set (R3): a `logic:closureEntry`
/// whose `logic:closureValue` is `logic:OpenWorldClosure` and whose `logic:closureKey` names a
/// property or class IRI marks that axiom as *genuinely open-world only* — it takes no
/// closed-world validation reading, so no shape is derived for it. This reuses the existing
/// closure vocabulary verbatim (no new shape DSL); it is the single authored signal the issue
/// allows. The default is to derive a shape for every eligible axiom (MAXIMAL UTILITY), so an
/// absent annotation means "derive". Read directly off the merged authored store, consistent
/// with the dataset-derive architecture. Property-GLOBAL entries only: an entry carrying
/// `logic:onClass` is class-scoped and never suppresses corpus-wide (see
/// [`closure_keys_with_value`]).
fn closure_validation_optouts(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    closure_keys_with_value(store, "OpenWorldClosure")
}

/// The `logic:closureKey` set of every PROPERTY-GLOBAL `logic:closureEntry` whose
/// `logic:closureValue` is the closure-value individual named by `value_local`. An entry that
/// carries `logic:onClass` is CLASS-SCOPED — its (class, key) pair is read by
/// [`closure_validation_closed_requirements`] alone — and contributes NOTHING here: sweeping a
/// class-scoped key into a global set would promote a per-class closure into a corpus-wide law
/// the author never asserted. The predicate nodes are built once and reused across entries
/// (not re-minted per iteration).
fn closure_keys_with_value(
    store: &RdfDataset,
    value_local: &str,
) -> std::collections::BTreeSet<String> {
    let target = Node::iri(logic_iri(value_local));
    let key_pred = nn(&logic_iri("closureKey"));
    let class_pred = nn(&logic_iri("onClass"));
    let mut set = std::collections::BTreeSet::new();
    for entry in subjects_with(store, &nn(&logic_iri("closureValue")), &target) {
        if value(store, &entry, &class_pred).is_some() {
            continue;
        }
        if let Some(key) = value(store, &entry, &key_pred) {
            set.insert(term_str(&key));
        }
    }
    set
}

/// The per-property **closed-world-reading opt-IN** set (R3, inverse polarity of
/// [`closure_validation_optouts`]): a `logic:closureEntry` whose `logic:closureValue` is
/// `logic:ClosedWorldClosure` and whose `logic:closureKey` names a property marks that property's
/// open-world `rdfs:domain`/`rdfs:range` axioms as ones that SHOULD take the closed-world
/// validation reading. Domain/range are inference axioms and open-world by default (a closed
/// reading over-claims on any graph relying on the entailment), so this is the single authored
/// signal that closes one — the exact peer of the opt-out, reusing the existing closure vocabulary
/// verbatim (no new shape DSL). An absent annotation means "open-world / derive no domain-range
/// shape". Read directly off the merged authored store, consistent with the dataset-derive
/// architecture. Property-GLOBAL entries only: an entry carrying `logic:onClass` is class-scoped
/// — it feeds [`closure_validation_closed_requirements`], never this global set, so it derives
/// no corpus-wide `sh:targetSubjectsOf`/`sh:targetObjectsOf` domain/range shape (see
/// [`closure_keys_with_value`]).
fn closure_validation_closed_optins(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    closure_keys_with_value(store, "ClosedWorldClosure")
}

/// Class/property pairs whose closure entry explicitly turns an `owl:allValuesFrom` restriction
/// into a required closed-world path. The pair is class-scoped: closing a predicate globally does
/// not imply that every class must carry it. This avoids an OWL existential in the reasoned core
/// while giving the validation projection an explicit, canonical `sh:minCount 1` authority.
fn closure_validation_closed_requirements(
    store: &RdfDataset,
) -> std::collections::BTreeSet<(String, String)> {
    let closed = Node::iri(logic_iri("ClosedWorldClosure"));
    let key_pred = nn(&logic_iri("closureKey"));
    let class_pred = nn(&logic_iri("onClass"));
    let mut set = std::collections::BTreeSet::new();
    for entry in subjects_with(store, &nn(&logic_iri("closureValue")), &closed) {
        if let (Some(key), Some(Node::Iri(class))) = (
            value(store, &entry, &key_pred),
            value(store, &entry, &class_pred),
        ) {
            set.insert((class, term_str(&key)));
        }
    }
    set
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, collecting its IRI members in
/// order. Blank / literal members are skipped (a non-resource list element has no IRI form),
/// and the walk terminates on `rdf:nil`, a cell missing `rdf:rest`, a non-resource cell, or a
/// cycle — a malformed list contributes only the members read so far rather than looping.
fn read_iri_list(store: &RdfDataset, head: &Node) -> Vec<String> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let first = nn(RDF_FIRST);
    let rest = nn(RDF_REST);
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut cursor = head.clone();
    while let Some(node) = term_as_subject(&cursor) {
        // Compute the node's subject string once and reuse for the nil-check and cycle-guard.
        let node_str = subject_str(&node);
        if node_str == RDF_NIL {
            break;
        }
        if !seen.insert(node_str) {
            break;
        }
        if let Some(Node::Iri(iri)) = value(store, &node, &first) {
            out.push(iri);
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    out
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, collecting each member as a
/// [`Subject`] (blank OR IRI) in order — the peer of [`read_iri_list`] for lists whose members
/// are themselves nodes to be read (e.g. the blank facet nodes of an `owl:withRestrictions`
/// list). Terminates on `rdf:nil`, a missing `rdf:rest`, a non-resource cell, or a cycle.
fn read_list_member_subjects(store: &RdfDataset, head: &Node) -> Vec<Subject> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let first = nn(RDF_FIRST);
    let rest = nn(RDF_REST);
    let mut out: Vec<Subject> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut cursor = head.clone();
    while let Some(node) = term_as_subject(&cursor) {
        let node_str = subject_str(&node);
        if node_str == RDF_NIL || !seen.insert(node_str) {
            break;
        }
        if let Some(member) = value(store, &node, &first)
            && let Some(member_subj) = term_as_subject(&member)
        {
            out.push(member_subj);
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    out
}

/// Derive the closed-world OWL fragment of an ontology graph into [`ValidationShapeIr`]s: the
/// per-class restriction walk (`some`/`allValuesFrom`, `hasValue`, unqualified + qualified
/// cardinality), the class-level axioms (`disjointWith`/`complementOf`/`oneOf`/
/// `AllDisjointClasses`), and the property-level axioms (`rdfs:domain`/`rdfs:range` and
/// `owl:Functional`/`InverseFunctionalProperty`). Every family accumulates by SHAPE TARGET —
/// one shape per target — so a class carrying both a restriction and a disjointness, or a
/// property carrying both a domain and a functionality axiom, folds into a single shape rather
/// than colliding. Constraints are sorted + deduped at construction so supply order never
/// affects the emitted surface (content-addressed determinism).
///
/// These are closed-world READINGS of open-world axioms — `logic:ValidationOnly` polarity,
/// never claimed as entailments (Principle 17). Derivation is scoped to the GMEOW authoring
/// namespace (only our own classes / properties own a shape; the target of an `sh:class` may
/// live in any namespace). The per-property / per-class opt-out (R3, see
/// [`closure_validation_optouts`]) suppresses any axiom whose subject a closure entry marks
/// `OpenWorldClosure`. `rdfs:domain`/`rdfs:range` are the one exception to derive-all: they are
/// open-world INFERENCE axioms, so they are OPEN-WORLD BY DEFAULT and derive a shape only for a
/// property explicitly opted IN with a `ClosedWorldClosure` closure entry (see
/// [`closure_validation_closed_optins`]).
///
/// Hard-fails (`Err`) rather than silently dropping a malformed REQUIRED constraint: a
/// cardinality with `min > max` (via [`PropertyConstraintIr::new`]), a qualified cardinality
/// with no `owl:onClass`, or an `owl:hasValue` whose fixed value is an anonymous node.
/// Conjunctively merge the property shapes that share the same `(path, inverse)` on one shape
/// target. FAMILY 1 emits one [`PropertyConstraintIr`] per restriction axiom, so a class that
/// authors a cardinality restriction AND an `owl:allValuesFrom` class restriction on ONE property
/// yields two same-path property shapes. SHACL reads several property shapes on one path
/// CONJUNCTIVELY — identical in enforcement to one `sh:property` block carrying every conjunct —
/// so a hand-authored legacy shape states them as a single merged block. Emitting them unmerged
/// keys distinctly from that block ([`PropertyConstraintIr::enforcement_key`] is per-path), which
/// would defeat the equivalence-before-deletion oracle for a shape whose covered fragment is
/// genuinely equivalent. Merging is sound: the lower bound is the tightest present (max of mins),
/// the upper bound the tightest present (min of maxes), the components the de-duplicated union, and
/// the reifier obligation the strengthened one (present shape / required OR). A same-path but
/// opposite-`inverse` pair constrains different statements (forward vs. inverse) and is never
/// merged. Deterministic: groups keep first-seen path order, and [`PropertyConstraintIr::new`]
/// sorts the merged components into canonical order. A merged `min > max` is a genuine
/// contradiction and hard-fails through the constructor, never a silent drop.
fn merge_same_path_properties(
    props: Vec<PropertyConstraintIr>,
) -> gmeow_errors::Result<Vec<PropertyConstraintIr>> {
    let mut order: Vec<(String, bool)> = Vec::new();
    let mut groups: BTreeMap<(String, bool), Vec<PropertyConstraintIr>> = BTreeMap::new();
    for p in props {
        let key = (p.path.clone(), p.inverse);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(p);
    }
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let mut group = groups
            .remove(&key)
            .expect("group was inserted for every ordered key");
        if group.len() == 1 {
            out.push(group.pop().expect("a one-element group has its element"));
            continue;
        }
        let (path, inverse) = key;
        let mut min_count: Option<u32> = None;
        let mut max_count: Option<u32> = None;
        let mut components: Vec<ConstraintComponent> = Vec::new();
        let mut reifier_shape: Option<String> = None;
        let mut reification_required = false;
        for p in group {
            if let Some(lo) = p.min_count {
                min_count = Some(min_count.map_or(lo, |cur| cur.max(lo)));
            }
            if let Some(hi) = p.max_count {
                max_count = Some(max_count.map_or(hi, |cur| cur.min(hi)));
            }
            for c in p.components {
                if !components.contains(&c) {
                    components.push(c);
                }
            }
            if p.reifier_shape.is_some() {
                reifier_shape = p.reifier_shape;
            }
            reification_required |= p.reification_required;
        }
        let provenance = (min_count.is_some() || max_count.is_some())
            .then_some(ConstraintProvenance::OwlRestriction);
        let mut pc = PropertyConstraintIr::new(path, min_count, max_count, provenance, components)?;
        if inverse {
            pc = pc.inverted();
        }
        if reifier_shape.is_some() || reification_required {
            pc = pc.with_reifier(reifier_shape, reification_required)?;
        }
        out.push(pc);
    }
    Ok(out)
}

/// THE single authoring-namespace authority: the four namespaces GMEOW dogfoods —
/// the core `gmeow:` vocabulary plus the grounding slices `math:`, `lang:`, `logic:`.
/// This is the dogfooding boundary [`derive_validation_shapes`] uses to decide which
/// classes/properties own a *derived* validation shape, and it is also the eligibility
/// test the `shape-migrate` injector (`gmeow-dev-cli`) consumes to decide which
/// hand-authored legacy shapes can be retired in favor of the derived projection. Any
/// other namespace (imported ontologies such as gUFO/FOAF, or anything external) is
/// linked, not validated/injected, by our surface. There is exactly one copy of this
/// set in the codebase; do not redeclare it elsewhere.
pub const AUTHORING_NAMESPACES: [&str; 4] = [
    "https://blackcatinformatics.ca/gmeow/",
    "https://blackcatinformatics.ca/math/",
    "https://blackcatinformatics.ca/lang/",
    "https://blackcatinformatics.ca/logic/",
];

/// Whether `iri` falls in an authoring namespace — see [`AUTHORING_NAMESPACES`].
pub fn is_authoring_namespace(iri: &str) -> bool {
    AUTHORING_NAMESPACES.iter().any(|ns| iri.starts_with(ns))
}

pub fn derive_validation_shapes(
    store: &RdfDataset,
) -> gmeow_errors::Result<Vec<ValidationShapeIr>> {
    // The per-property/per-class closed-world-reading opt-out (R3). Default is derive-all; a
    // property/class named by an `OpenWorldClosure` closure entry is suppressed below.
    let optouts = closure_validation_optouts(store);
    // The per-property closed-world-reading opt-IN (R3, inverse polarity): rdfs:domain/range are
    // open-world by default (they are inference axioms), so a domain/range shape is derived only
    // for a property a `ClosedWorldClosure` closure entry explicitly closes. See FAMILY 3.
    let closed_optins = closure_validation_closed_optins(store);
    let closed_requirements = closure_validation_closed_requirements(store);
    let owl = "http://www.w3.org/2002/07/owl#";
    let rdfs = "http://www.w3.org/2000/01/rdf-schema#";
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    let owl_class = Node::iri(format!("{owl}Class"));
    let owl_thing = format!("{owl}Thing");
    let rdfs_datatype = Node::iri(format!("{rdfs}Datatype"));
    let rdfs_literal = format!("{rdfs}Literal");
    let rdfs_resource = format!("{rdfs}Resource");
    let p_on = nn(&format!("{owl}onProperty"));
    let p_some = nn(&format!("{owl}someValuesFrom"));
    let p_all = nn(&format!("{owl}allValuesFrom"));
    let p_hasvalue = nn(&format!("{owl}hasValue"));
    let p_onclass = nn(&format!("{owl}onClass"));
    let p_ondatarange = nn(&format!("{owl}onDataRange"));
    let p_mincard = nn(&format!("{owl}minCardinality"));
    let p_maxcard = nn(&format!("{owl}maxCardinality"));
    let p_card = nn(&format!("{owl}cardinality"));
    let p_qmincard = nn(&format!("{owl}minQualifiedCardinality"));
    let p_qmaxcard = nn(&format!("{owl}maxQualifiedCardinality"));
    let p_qcard = nn(&format!("{owl}qualifiedCardinality"));
    let p_subclass = nn(&format!("{rdfs}subClassOf"));
    // Canonical `logic:` restriction spelling.  Declarative shape derivation reads the
    // merged AUTHORED dataset (before the OWL projection is materialized), so a
    // `logic:subClassOf [ a logic:Restriction ; ... ]` must be read through the same
    // semantic slots as its legacy RDFS/OWL spelling.  Keep the two spellings in one
    // view here; the ValidationShapeIr construction below remains the single lowering.
    let p_logic_on = nn(&logic_iri("onProperty"));
    let p_logic_some = nn(&logic_iri("someValuesFrom"));
    let p_logic_all = nn(&logic_iri("allValuesFrom"));
    let p_logic_hasvalue = nn(&logic_iri("hasValue"));
    let p_logic_onclass = nn(&logic_iri("onClass"));
    let p_logic_ondatarange = nn(&logic_iri("onDataRange"));
    let p_logic_mincard = nn(&logic_iri("minCardinality"));
    let p_logic_maxcard = nn(&logic_iri("maxCardinality"));
    let p_logic_card = nn(&logic_iri("cardinality"));
    let p_logic_qmincard = nn(&logic_iri("minQualifiedCardinality"));
    let p_logic_qmaxcard = nn(&logic_iri("maxQualifiedCardinality"));
    let p_logic_qcard = nn(&logic_iri("qualifiedCardinality"));
    let p_logic_subclass = nn(&logic_iri("subClassOf"));
    let p_disjoint = nn(&format!("{owl}disjointWith"));
    let p_complement = nn(&format!("{owl}complementOf"));
    let p_oneof = nn(&format!("{owl}oneOf"));
    let p_members = nn(&format!("{owl}members"));
    let p_domain = nn(&format!("{rdfs}domain"));
    let p_range = nn(&format!("{rdfs}range"));
    let owl_alldisjoint = Node::iri(format!("{owl}AllDisjointClasses"));

    let restriction_value = |subject: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| {
        value(store, subject, owl_predicate).or_else(|| value(store, subject, logic_predicate))
    };
    let restriction_objects = |subject: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| {
        let mut out = objects(store, subject, owl_predicate);
        out.extend(objects(store, subject, logic_predicate));
        out
    };

    // GMEOW's authoring ground: derive validation shapes for our own domain classes /
    // properties across every dogfooded namespace (Principle 4 / maximal dogfooding) — the
    // core `gmeow:` vocabulary plus the grounding slices (`math:`, `lang:`, `logic:`), whose
    // hand-authored shapes migrate to these derived projections (declarative-migration wave 1).
    // Imported
    // ontologies (gUFO, FOAF, …) are linked, not validated by our surface — and their
    // namespaces are not registered in the downstream JSON-Schema discriminator. The TARGET of
    // an `sh:class` may live in any namespace; only the SHAPE-owning class / property must be
    // authoring-NS. `is_authoring_ns` here is a thin local alias of the module-level
    // authority ([`is_authoring_namespace`]) kept for call-site brevity in this function.
    let is_authoring_ns = is_authoring_namespace;

    // A range target is a DATATYPE (→ sh:datatype) rather than a class (→ sh:class) when it is
    // in the XSD space, is rdfs:Literal, or is declared `a rdfs:Datatype`.
    let is_datatype = |iri: &str| -> bool {
        iri.starts_with(xsd)
            || iri == rdfs_literal
            || objects(store, &Subject::Iri(iri.to_owned()), &nn(RDF_TYPE)).contains(&rdfs_datatype)
    };

    // The single classification component for an IRI target: `None` for `rdfs:Resource`, the
    // UNIVERSAL TOP (the class of everything, literals included) — a range/domain of
    // rdfs:Resource is VACUOUS, and `sh:class rdfs:Resource` would demand a never-materialized
    // `rdf:type rdfs:Resource` edge on every object, false-positiving universally; a vacuous top
    // must emit NO node/class constraint at all. The two BOUNDED universal-tops instead project to
    // an open node-kind constraint (`owl:Thing` → sh:BlankNodeOrIRI; `rdfs:Literal` →
    // sh:Literal) — the `owl:Thing` guard sits AHEAD of `is_datatype` so a quirk axiom
    // `owl:Thing a rdfs:Datatype` can never fall through to a vacuous `sh:datatype owl:Thing`.
    // A concrete datatype → sh:datatype; anything else → sh:class.
    let classify = |iri: &str| -> Option<ConstraintComponent> {
        if iri == rdfs_resource {
            None
        } else if iri == owl_thing {
            Some(ConstraintComponent::NodeKindShacl(
                ShaclNodeKind::BlankNodeOrIri,
            ))
        } else if iri == rdfs_literal {
            Some(ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal))
        } else if is_datatype(iri) {
            Some(ConstraintComponent::Datatype(iri.to_owned()))
        } else {
            Some(ConstraintComponent::Class(iri.to_owned()))
        }
    };

    // Whether `p` is a DECLARED `owl:DatatypeProperty` (and not also an object property — a
    // corpus that declares both is contradictory OWL and gets the conservative object reading,
    // never a silently narrowed one).
    let owl_datatype_property = Node::iri(format!("{owl}DatatypeProperty"));
    let owl_object_property = Node::iri(format!("{owl}ObjectProperty"));
    let is_datatype_property = |p: &str| -> bool {
        let types = objects(store, &Subject::Iri(p.to_owned()), &nn(RDF_TYPE));
        types.contains(&owl_datatype_property) && !types.contains(&owl_object_property)
    };

    // `classify`, resolved against the property the filler is a filler FOR.
    //
    // The two bounded universal tops are NOT interchangeable across the object/data divide:
    // `owl:Thing` is the top of the INDIVIDUAL domain, `rdfs:Literal` the top of the DATA
    // domain. A declared `owl:DatatypeProperty` takes literal values only, so an authored
    // `owl:someValuesFrom owl:Thing` / `owl:allValuesFrom owl:Thing` / `rdfs:range owl:Thing`
    // on such a property means "any value" in the only domain that property has — the data
    // domain — and its faithful projection is `sh:nodeKind sh:Literal`. Projecting the
    // individual-domain reading (`sh:nodeKind sh:BlankNodeOrIRI`) there is not a lossy
    // approximation but an INVERTED constraint: it rejects every correct literal value the
    // property is declared to carry (the `gmeow:spanStart` / `gmeow:spanEnd` integer offsets on
    // `gmeow:Chunk` are the corpus witness). The redirect is keyed on the property's own
    // declaration, so it can never narrow an object-valued or undeclared path.
    let classify_on = |on: &str, iri: &str| -> Option<ConstraintComponent> {
        if iri == owl_thing && is_datatype_property(on) {
            classify(&rdfs_literal)
        } else {
            classify(iri)
        }
    };

    // The closed-world reading of a faceted datatype filler
    // (`[ a rdfs:Datatype ; owl:onDatatype xsd:string ; owl:withRestrictions ( [ xsd:pattern "…" ]
    // [ xsd:minLength "…" ] … ) ]`): the base datatype plus each XSD length / pattern facet, as
    // SHACL components (`sh:datatype` + `sh:pattern` / `sh:minLength` / `sh:maxLength`). Returns an
    // empty vector for a blank node that carries no `owl:withRestrictions` (an anonymous class
    // expression, not a datatype facet) — the caller then skips it, unchanged.
    let p_ondatatype = nn(&format!("{owl}onDatatype"));
    let p_withrestrictions = nn(&format!("{owl}withRestrictions"));
    let p_logic_ondatatype = nn(&logic_iri("onDatatype"));
    let p_logic_withrestrictions = nn(&logic_iri("withRestrictions"));
    let xsd_pattern = nn(&format!("{xsd}pattern"));
    let xsd_minlength = nn(&format!("{xsd}minLength"));
    let xsd_maxlength = nn(&format!("{xsd}maxLength"));
    let xsd_mininclusive = nn(&format!("{xsd}minInclusive"));
    let xsd_maxinclusive = nn(&format!("{xsd}maxInclusive"));
    let xsd_minexclusive = nn(&format!("{xsd}minExclusive"));
    let xsd_maxexclusive = nn(&format!("{xsd}maxExclusive"));
    let datatype_facets = |filler: &Subject| -> Vec<ConstraintComponent> {
        let Some(list_head) =
            restriction_value(filler, &p_withrestrictions, &p_logic_withrestrictions)
        else {
            return Vec::new();
        };
        let mut comps = Vec::new();
        if let Some(Node::Iri(dt)) = restriction_value(filler, &p_ondatatype, &p_logic_ondatatype) {
            comps.push(ConstraintComponent::Datatype(dt));
        }
        // Numeric-bound facets accumulate into a SINGLE `NumericRange` (a min/max pair with per-
        // endpoint inclusivity) so an interval authored as two facet nodes projects to one
        // `sh:minInclusive`/`sh:maxInclusive` (or the exclusive peers) component, matching the
        // direct-emit oracle. An inclusive bound wins over an exclusive one on the same endpoint
        // (an author who states both means the tighter closed reading).
        let (mut lo, mut hi): (Option<f64>, Option<f64>) = (None, None);
        let (mut lo_incl, mut hi_incl) = (true, true);
        for facet in read_list_member_subjects(store, &list_head) {
            if let Some(Node::Lit { lexical: regex, .. }) = value(store, &facet, &xsd_pattern) {
                comps.push(ConstraintComponent::Pattern { regex, flags: None });
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_minlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MinLength(n));
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_maxlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MaxLength(n));
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_mininclusive)
                && let Ok(n) = n.trim().parse::<f64>()
            {
                lo = Some(n);
                lo_incl = true;
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_minexclusive)
                && let Ok(n) = n.trim().parse::<f64>()
                && lo.is_none()
            {
                lo = Some(n);
                lo_incl = false;
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_maxinclusive)
                && let Ok(n) = n.trim().parse::<f64>()
            {
                hi = Some(n);
                hi_incl = true;
            }
            if let Some(Node::Lit { lexical: n, .. }) = value(store, &facet, &xsd_maxexclusive)
                && let Ok(n) = n.trim().parse::<f64>()
                && hi.is_none()
            {
                hi = Some(n);
                hi_incl = false;
            }
        }
        if lo.is_some() || hi.is_some() {
            comps.push(ConstraintComponent::NumericRange {
                min: lo,
                max: hi,
                min_inclusive: lo_incl,
                max_inclusive: hi_incl,
            });
        }
        comps
    };

    // The closed-world reading of a blank ANONYMOUS-CLASS-EXPRESSION filler (an
    // `owl:someValuesFrom` / `owl:allValuesFrom` value whose filler is not a named class): an
    // `owl:unionOf ( C1 C2 … )` → `sh:or ( [ sh:class C1 ] … )`, an `owl:disjointUnionOf ( … )` →
    // `sh:xone ( … )`, an `owl:complementOf C` → `sh:not [ sh:class C ]`, and the nested value
    // negation `owl:complementOf [ owl:hasValue v ]` → `sh:not [ sh:hasValue v ]`. Returns `None`
    // for a filler that carries no such expression (the caller then leaves it in the canon).
    let p_unionof = nn(&format!("{owl}unionOf"));
    let p_disjointunion = nn(&format!("{owl}disjointUnionOf"));
    let p_logic_unionof = nn(&logic_iri("unionOf"));
    let p_logic_disjointunion = nn(&logic_iri("disjointUnionOf"));
    let p_logic_oneof = nn(&logic_iri("oneOf"));
    let p_logic_complement = nn(&logic_iri("complementOf"));
    let classify_filler = |fs: &Subject| -> Option<ConstraintComponent> {
        if let Some(head) = restriction_value(fs, &p_unionof, &p_logic_unionof) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Or(branches));
            }
        }
        if let Some(head) = restriction_value(fs, &p_disjointunion, &p_logic_disjointunion) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Xone(branches));
            }
        }
        // An enumerated filler (`owl:oneOf ( a b … )` on an anonymous class) → `sh:in ( a b … )`:
        // every value of the path must be one of the enumerated individuals. IRI members only —
        // a literal member would make the expression a data range, which the facet arm owns.
        if let Some(head) = restriction_value(fs, &p_oneof, &p_logic_oneof) {
            let members = read_iri_list(store, &head);
            if !members.is_empty() {
                return Some(ConstraintComponent::In(
                    members.into_iter().map(ShapeValue::Iri).collect(),
                ));
            }
        }
        match restriction_value(fs, &p_complement, &p_logic_complement) {
            Some(Node::Iri(d)) => classify(&d).map(|cc| ConstraintComponent::Not(Box::new(cc))),
            Some(inner @ Node::Blank { .. }) => {
                let bs = term_as_subject(&inner)?;
                let sv = match restriction_value(&bs, &p_hasvalue, &p_logic_hasvalue)? {
                    Node::Iri(i) => ShapeValue::Iri(i),
                    Node::Lit {
                        lexical,
                        datatype,
                        lang,
                    } => ShapeValue::Literal {
                        lexical,
                        datatype,
                        lang,
                    },
                    _ => return None,
                };
                Some(ConstraintComponent::Not(Box::new(
                    ConstraintComponent::HasValue(sv),
                )))
            }
            _ => None,
        }
    };

    // A non-negative-integer cardinality literal off a restriction blank node; `None` (never a
    // hard error here) for an absent or non-integer object — a broken count contributes nothing.
    let card_of = |restr: &Subject, p: &Iri| -> Option<u32> {
        match value(store, restr, p) {
            Some(Node::Lit { lexical: lex, .. }) => lex.trim().parse::<u32>().ok(),
            _ => None,
        }
    };
    let restriction_card_of =
        |restr: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| -> Option<u32> {
            match restriction_value(restr, owl_predicate, logic_predicate) {
                Some(Node::Lit { lexical: lex, .. }) => lex.trim().parse::<u32>().ok(),
                _ => None,
            }
        };

    // Accumulate by SHAPE TARGET: shape IRI → (target, node_components, properties). One shape
    // per target, so every family merges into a single shape rather than colliding.
    type Acc = BTreeMap<
        String,
        (
            ShapeTarget,
            Vec<ConstraintComponent>,
            Vec<PropertyConstraintIr>,
        ),
    >;
    let mut acc: Acc = BTreeMap::new();

    // The stable per-target shape IRI (distinct per target family so domain / range / class
    // shapes never collide).
    fn entry_for(
        acc: &mut Acc,
        target: ShapeTarget,
    ) -> &mut (
        ShapeTarget,
        Vec<ConstraintComponent>,
        Vec<PropertyConstraintIr>,
    ) {
        let iri = match &target {
            ShapeTarget::Class(c) => format!("{c}-shape"),
            ShapeTarget::SubjectsOf(p) => format!("{p}-domain-shape"),
            ShapeTarget::ObjectsOf(p) => format!("{p}-range-shape"),
            ShapeTarget::ValueKeyed { predicate, value } => {
                format!("{predicate}-{value}-value-shape")
            }
            // The declarative deriver never mints a direct-instance / raw-sparql target (those are
            // procedural-constraint selectors only), but the match stays exhaustive.
            ShapeTarget::DirectClass(c) => format!("{c}-direct-shape"),
            ShapeTarget::Sparql(_) => "sparql-target-shape".to_owned(),
        };
        acc.entry(iri)
            .or_insert_with(|| (target, Vec::new(), Vec::new()))
    }

    // ── Unique-language sugar (logic:UniqueLangConstraint) → declarative sh:uniqueLang ────────
    // A per-property unique-language record grounds a `sh:uniqueLang true` facet DECLARATIVELY on
    // its `logic:onClass` node shape (the localizable-prose convention): the value path localizes
    // at most once per language tag. Unlike the procedural sugars, uniqueLang is a faithful SHACL
    // Core facet, so it rides the class node shape as a covered property component.
    let unique_lang_ty = Node::iri(logic_iri("UniqueLangConstraint"));
    for rec in subjects_with(store, &nn(RDF_TYPE), &unique_lang_ty) {
        let (Some(Node::Iri(class_iri)), Some(Node::Iri(path))) = (
            value(store, &rec, &nn(&logic_iri("onClass"))),
            value(store, &rec, &nn(&logic_iri("valuePath"))),
        ) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) || optouts.contains(&path) {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &path,
            None,
            None,
            None,
            vec![ConstraintComponent::UniqueLang],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── FAMILY 1 — per-class restriction walk (Class(C) target) ───────────────────────────
    let classes = subjects_with(store, &nn(RDF_TYPE), &owl_class);
    for class in &classes {
        // An anonymous class expression (blank node) is not a shape target — skip it.
        if subject_is_blank(class) {
            continue;
        }
        let class_iri = subject_str(class);
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) {
            continue;
        }
        for restr in restriction_objects(class, &p_subclass, &p_logic_subclass) {
            let Some(restr_subj) = term_as_subject(&restr) else {
                continue;
            };
            // A blank superclass carrying `owl:unionOf ( [ owl:onProperty P1 ; owl:someValuesFrom
            // owl:Thing ] … )` — an either-of-these-properties existence obligation — reads
            // closed-world as the node-level `sh:or` over required property paths
            // ([`ConstraintComponent::OrProperties`]). Only the exact all-branches-are-bare-
            // existential form is read; any other union member (a named class, a qualified
            // filler) leaves the whole expression in the canon, never a partial disjunction.
            if restriction_value(&restr_subj, &p_on, &p_logic_on).is_none()
                && let Some(head) = restriction_value(&restr_subj, &p_unionof, &p_logic_unionof)
            {
                let members = read_list_member_subjects(store, &head);
                let mut paths: Vec<String> = Vec::with_capacity(members.len());
                let mut all_bare_existentials = !members.is_empty();
                for m in &members {
                    let on_p = restriction_value(m, &p_on, &p_logic_on);
                    let filler = restriction_value(m, &p_some, &p_logic_some);
                    match (on_p, filler) {
                        (Some(Node::Iri(p)), Some(Node::Iri(f)))
                            if f == owl_thing && !optouts.contains(&p) =>
                        {
                            paths.push(p);
                        }
                        _ => {
                            all_bare_existentials = false;
                            break;
                        }
                    }
                }
                if all_bare_existentials && paths.len() >= 2 {
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .1
                        .push(ConstraintComponent::OrProperties(paths));
                }
                continue;
            }
            // A restriction constrains exactly one property; skip a malformed one with no
            // IRI-valued `owl:onProperty`.
            let Some(Node::Iri(on)) = restriction_value(&restr_subj, &p_on, &p_logic_on) else {
                continue;
            };
            // Per-property validation-reading opt-out (R3).
            if optouts.contains(&on) {
                continue;
            }

            // owl:someValuesFrom is an OPEN-WORLD existential and therefore never becomes a
            // closed-world minimum by itself. Its validation projection is the conservative
            // value-typing under-approximation. Required paths are authored separately as a
            // class-scoped `ClosedWorldClosure` entry paired with `owl:allValuesFrom`; this keeps
            // the SHACL minimum explicit without causing the native reasoner to mint existential
            // witnesses into the shipped closure.
            match restriction_value(&restr_subj, &p_some, &p_logic_some) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify_on(&on, &cv) {
                        let pc = PropertyConstraintIr::new(&on, None, None, None, vec![cc])?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
                // A blank filler that is a faceted datatype (`owl:onDatatype` + `owl:withRestrictions`)
                // reads as the length/pattern facets its values must satisfy — the same closed-world
                // condition as `allValuesFrom` over that datatype. A blank filler with no facet is an
                // anonymous class expression, carried in the canon, never a bare blank shape → skip.
                Some(filler @ Node::Blank { .. }) => {
                    if let Some(fs) = term_as_subject(&filler) {
                        let facets = datatype_facets(&fs);
                        if !facets.is_empty() {
                            let pc = PropertyConstraintIr::new(&on, None, None, None, facets)?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        } else if let Some(cc) = classify_filler(&fs) {
                            let pc = PropertyConstraintIr::new(&on, None, None, None, vec![cc])?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        }
                    }
                }
                _ => {}
            }

            // owl:allValuesFrom is UNIVERSAL: every value satisfies the inner shape → a bare
            // `sh:class` / `sh:datatype` / `sh:nodeKind` on the path, or the length/pattern facets
            // of a faceted-datatype filler. A non-faceted blank filler → skip.
            match restriction_value(&restr_subj, &p_all, &p_logic_all) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify_on(&on, &cv) {
                        let min = closed_requirements
                            .contains(&(class_iri.clone(), on.clone()))
                            .then_some(1);
                        let pc = PropertyConstraintIr::new(
                            &on,
                            min,
                            None,
                            min.map(|_| ConstraintProvenance::OwlRestriction),
                            vec![cc],
                        )?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
                Some(filler @ Node::Blank { .. }) => {
                    if let Some(fs) = term_as_subject(&filler) {
                        let facets = datatype_facets(&fs);
                        if !facets.is_empty() {
                            let min = closed_requirements
                                .contains(&(class_iri.clone(), on.clone()))
                                .then_some(1);
                            let pc = PropertyConstraintIr::new(
                                &on,
                                min,
                                None,
                                min.map(|_| ConstraintProvenance::OwlRestriction),
                                facets,
                            )?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        } else if let Some(cc) = classify_filler(&fs) {
                            let min = closed_requirements
                                .contains(&(class_iri.clone(), on.clone()))
                                .then_some(1);
                            let pc = PropertyConstraintIr::new(
                                &on,
                                min,
                                None,
                                min.map(|_| ConstraintProvenance::OwlRestriction),
                                vec![cc],
                            )?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        }
                    }
                }
                _ => {}
            }

            // owl:hasValue → `sh:hasValue` (a fixed required value). A blank / quoted-triple
            // fixed value is impossible (a fixed value cannot be an anonymous node) → hard-fail.
            match restriction_value(&restr_subj, &p_hasvalue, &p_logic_hasvalue) {
                None => {}
                Some(Node::Iri(v)) => {
                    let pc = PropertyConstraintIr::new(
                        &on,
                        None,
                        None,
                        None,
                        vec![ConstraintComponent::HasValue(ShapeValue::Iri(v))],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
                Some(Node::Lit {
                    lexical,
                    datatype,
                    lang,
                }) => {
                    // Preserve the fixed value's datatype / language tag: a typed
                    // `owl:hasValue "1"^^xsd:integer` derives a TYPED `sh:hasValue`, never a
                    // bare untyped `"1"` that would match the wrong literal.
                    let pc = PropertyConstraintIr::new(
                        &on,
                        None,
                        None,
                        None,
                        vec![ConstraintComponent::HasValue(ShapeValue::Literal {
                            lexical,
                            datatype,
                            lang,
                        })],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
                Some(Node::Blank { .. }) | Some(Node::Triple(_)) => {
                    return Err(Diag::of_kind(crate::error::Frontend {
                        detail: format!(
                            "owl:hasValue on {on} is an anonymous node; a fixed required value \
                         cannot be a blank node or quoted triple"
                        ),
                    }));
                }
            }

            // Cardinality. Qualified (`owl:onClass` + a `qualified*Cardinality`) is distinct from
            // unqualified: it counts only the values satisfying the inner shape.
            let has_qcard = restriction_value(&restr_subj, &p_qcard, &p_logic_qcard).is_some()
                || restriction_value(&restr_subj, &p_qmincard, &p_logic_qmincard).is_some()
                || restriction_value(&restr_subj, &p_qmaxcard, &p_logic_qmaxcard).is_some();
            if has_qcard {
                let q_exact = restriction_card_of(&restr_subj, &p_qcard, &p_logic_qcard);
                let (mut qlo, mut qhi) = (
                    restriction_card_of(&restr_subj, &p_qmincard, &p_logic_qmincard),
                    restriction_card_of(&restr_subj, &p_qmaxcard, &p_logic_qmaxcard),
                );
                if let Some(n) = q_exact {
                    qlo = Some(n);
                    qhi = Some(n);
                }
                match restriction_value(&restr_subj, &p_onclass, &p_logic_onclass) {
                    // A qualified cardinality qualifies over EITHER an object filler
                    // (`owl:onClass <Class>`) or a datatype filler (`owl:onDataRange <Datatype>`).
                    // With no `owl:onClass`, fall through to the datatype-qualified peer.
                    None => {
                        match restriction_value(&restr_subj, &p_ondatarange, &p_logic_ondatarange) {
                            // `owl:onDataRange <Datatype>` is the datatype-qualified peer of
                            // `owl:onClass <Class>`. It reads as the datatype every counted value must
                            // carry, degraded to a PLAIN `sh:datatype` + `sh:minCount`/`sh:maxCount`
                            // (min→minCount, max→maxCount, exact→both — the same count the onClass arm
                            // carries). A bare `sh:datatype` is what the JSON-Schema deriver reads; a
                            // `sh:qualifiedValueShape [ sh:datatype … ]` it would ignore.
                            Some(Node::Iri(dt)) => {
                                // `classify` maps a concrete datatype → `sh:datatype`, `rdfs:Literal`
                                // → the `sh:Literal` node-kind, and `rdfs:Resource` → no component
                                // (the vacuous universal top) — the datatype analogue of the onClass
                                // arm's class handling.
                                let comps: Vec<_> = classify(&dt).into_iter().collect();
                                let pc = PropertyConstraintIr::new(
                                    &on,
                                    qlo,
                                    qhi,
                                    Some(ConstraintProvenance::OwlRestriction),
                                    comps,
                                )?;
                                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                    .2
                                    .push(pc);
                            }
                            // Neither `owl:onClass` nor `owl:onDataRange` — a qualified cardinality
                            // REQUIRES a qualifying filler; absent → hard-fail. An anonymous
                            // (blank / literal / quoted-triple) data range is carried in the canon,
                            // never a bare blank shape — skip (do not emit).
                            Some(Node::Blank { .. })
                            | Some(Node::Lit { .. })
                            | Some(Node::Triple(_)) => {}
                            None => {
                                return Err(Diag::of_kind(crate::error::Frontend {
                                    detail: format!(
                                        "qualified cardinality on {on} requires owl:onClass or owl:onDataRange"
                                    ),
                                }));
                            }
                        }
                    }
                    // An anonymous qualifying class expression is carried in the canon, never a
                    // bare blank shape — skip (do not emit).
                    Some(Node::Blank { .. }) | Some(Node::Lit { .. }) | Some(Node::Triple(_)) => {}
                    Some(Node::Iri(q)) if q == owl_thing => {
                        // `owl:onClass owl:Thing` qualifies over "any individual" — the qualified
                        // count degrades to an unqualified `sh:minCount`/`sh:maxCount` rather than
                        // a vacuous inner shape.
                        let pc = PropertyConstraintIr::new(
                            &on,
                            qlo,
                            qhi,
                            Some(ConstraintProvenance::OwlRestriction),
                            vec![],
                        )?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                    Some(Node::Iri(q)) => {
                        // The faithful projection of a qualified cardinality is the
                        // `sh:qualifiedValueShape` alone — it carries the count on exactly the
                        // values that satisfy the inner shape. A BARE `sh:class q` reads as the
                        // UNIVERSAL "every value of `on` is a q", which a qualified cardinality
                        // (min/max `onClass q`) does NOT entail, so emitting it unconditionally
                        // OVER-CLAIMS (caught by the lift/derive round-trip `certify` invariant).
                        // Emit the bare `sh:class` — which the JSON-Schema deriver and the
                        // purrdf-shapes object-class node-ref path read (they ignore the class
                        // nested inside a `sh:qualifiedValueShape`) — ONLY when a genuine universal
                        // backs it: an `owl:allValuesFrom q` is already emitted by its own arm
                        // above, and here we cover a closed-world-opted-in `rdfs:range on q` (the
                        // same `ClosedWorldClosure` opt-in that gates the property-level range
                        // shape). With neither universal, only the qualified shape is emitted.
                        let range_backed = closed_optins.contains(&on)
                            && objects(store, &Subject::Iri(on.clone()), &p_range)
                                .into_iter()
                                .any(|c| matches!(c, Node::Iri(r) if r == q));
                        let mut comps = Vec::new();
                        if range_backed {
                            comps.push(ConstraintComponent::Class(q.clone()));
                        }
                        comps.push(ConstraintComponent::QualifiedValueShape {
                            shape: vec![ConstraintComponent::Class(q)],
                            min: qlo,
                            max: qhi,
                        });
                        let pc = PropertyConstraintIr::new(&on, None, None, None, comps)?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
            } else {
                // Unqualified cardinality → sh:minCount / sh:maxCount with OwlRestriction
                // provenance (the open-world axiom read closed-world).
                let exact = restriction_card_of(&restr_subj, &p_card, &p_logic_card);
                let (mut lo, mut hi) = (
                    restriction_card_of(&restr_subj, &p_mincard, &p_logic_mincard),
                    restriction_card_of(&restr_subj, &p_maxcard, &p_logic_maxcard),
                );
                if let Some(n) = exact {
                    lo = Some(n);
                    hi = Some(n);
                }
                if lo.is_some() || hi.is_some() {
                    let pc = PropertyConstraintIr::new(
                        &on,
                        lo,
                        hi,
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
            }
        }
    }

    // ── FAMILY 2 — class-level axioms (Class(C) target, node_components) ───────────────────
    for class in &classes {
        if subject_is_blank(class) {
            continue;
        }
        let class_iri = subject_str(class);
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) {
            continue;
        }
        // owl:disjointWith D → sh:not [ sh:class D ]. Blank operand → skip.
        for d in objects(store, class, &p_disjoint) {
            if let Node::Iri(d) = d {
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::Not(Box::new(
                        ConstraintComponent::Class(d),
                    )));
            }
        }
        // owl:complementOf D → sh:not [ sh:class D ]. Blank operand → skip.
        for d in objects(store, class, &p_complement) {
            if let Node::Iri(d) = d {
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::Not(Box::new(
                        ConstraintComponent::Class(d),
                    )));
            }
        }
        // owl:oneOf ( a b … ) → sh:in ( a b … ). An empty / malformed list contributes nothing.
        if let Some(head) = value(store, class, &p_oneof) {
            let members = read_iri_list(store, &head);
            if !members.is_empty() {
                let vals = members.into_iter().map(ShapeValue::Iri).collect();
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::In(vals));
            }
        }
    }

    // Named owl:AllDisjointClasses ( c1 … cn ): each ordered pair (ci, cj), i≠j, contributes
    // sh:not [ sh:class cj ] to ci's shape (when ci is GMEOW-NS and not opted out).
    for adc in subjects_with(store, &nn(RDF_TYPE), &owl_alldisjoint) {
        let Some(head) = value(store, &adc, &p_members) else {
            continue;
        };
        let members = read_iri_list(store, &head);
        for ci in &members {
            if !is_authoring_ns(ci) || optouts.contains(ci) {
                continue;
            }
            for cj in &members {
                if ci == cj {
                    continue;
                }
                entry_for(&mut acc, ShapeTarget::Class(ci.clone())).1.push(
                    ConstraintComponent::Not(Box::new(ConstraintComponent::Class(cj.clone()))),
                );
            }
        }
    }

    // ── FAMILY 3 — property-level axioms (SubjectsOf(P) / ObjectsOf(P) targets) ────────────
    // Collect every GMEOW-NS property: the OWL object/datatype property-type declarations plus any
    // subject of rdfs:domain / rdfs:range. The functional / inverse-functional characteristics are
    // NOT seeded here — they are read from the canonical `logic:` carrier below (a functional-only
    // property, e.g. gmeow:unit, is discovered straight from its characteristic-assertion record,
    // not from a property-type marker).
    let mut props: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for ty in ["ObjectProperty", "DatatypeProperty"] {
        for s in subjects_with(store, &nn(RDF_TYPE), &Node::iri(format!("{owl}{ty}"))) {
            if let Subject::Iri(iri) = &s
                && is_authoring_ns(iri)
            {
                props.insert(iri.clone());
            }
        }
    }
    const DOMAIN_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RANGE_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    for q in default_graph_quads(store) {
        if (q.predicate.as_str() == DOMAIN_IRI || q.predicate.as_str() == RANGE_IRI)
            && let Subject::Iri(iri) = &q.subject
            && is_authoring_ns(iri)
        {
            props.insert(iri.clone());
        }
    }

    for p in &props {
        if optouts.contains(p) {
            continue;
        }
        let p_subj = Subject::Iri(p.clone());
        // rdfs:domain / rdfs:range are open-world INFERENCE axioms — they ENTAIL the subject's /
        // object's type, they do not REQUIRE it to be asserted. Their closed-world `sh:class`
        // reading over-claims on any graph that (legitimately) relies on that entailment rather
        // than asserting the type standalone, so — unlike the genuinely closed-world constraints
        // (cardinality, value restrictions) which stay derive-all — domain/range are OPEN-WORLD BY
        // DEFAULT: a domain/range validation shape is derived ONLY for a property explicitly opted
        // IN via a `logic:ClosedWorldClosure` closure entry (the inverse polarity of the
        // `OpenWorldClosure` opt-out, reusing the same closure vocabulary — no new shape DSL).
        if closed_optins.contains(p) {
            // rdfs:domain C → a SubjectsOf(P) node condition (every subject of P satisfies it).
            for c in objects(store, &p_subj, &p_domain) {
                if let Node::Iri(c) = c
                    && let Some(cc) = classify(&c)
                {
                    entry_for(&mut acc, ShapeTarget::SubjectsOf(p.clone()))
                        .1
                        .push(cc);
                }
            }
            // rdfs:domain [ owl:Restriction on Q ] → a SubjectsOf(P) PROPERTY condition: every
            // subject of P belongs to the anonymous restriction class, i.e. satisfies the
            // restriction on Q (the required-companion pattern: "a node lowered through P must
            // also declare Q"). Read closed-world only under the same explicit ClosedWorldClosure
            // opt-in that gates the named-domain shape, with unqualified cardinality and
            // named-class/datatype fillers — any other anonymous domain expression stays in the
            // canon.
            for c in objects(store, &p_subj, &p_domain) {
                let Some(domain_subj) = term_as_subject(&c) else {
                    continue;
                };
                if !subject_is_blank(&domain_subj) {
                    continue;
                }
                let Some(Node::Iri(on_q)) = value(store, &domain_subj, &p_on) else {
                    continue;
                };
                if optouts.contains(&on_q) {
                    continue;
                }
                let mut comps: Vec<ConstraintComponent> = Vec::new();
                for vp in [&p_some, &p_all] {
                    if let Some(Node::Iri(cv)) = value(store, &domain_subj, vp)
                        && let Some(cc) = classify_on(&on_q, &cv)
                    {
                        comps.push(cc);
                    }
                }
                let exact = card_of(&domain_subj, &p_card);
                let (mut lo, mut hi) = (
                    card_of(&domain_subj, &p_mincard),
                    card_of(&domain_subj, &p_maxcard),
                );
                if let Some(n) = exact {
                    lo = Some(n);
                    hi = Some(n);
                }
                if comps.is_empty() && lo.is_none() && hi.is_none() {
                    continue;
                }
                let provenance =
                    (lo.is_some() || hi.is_some()).then_some(ConstraintProvenance::OwlRestriction);
                let pc = PropertyConstraintIr::new(&on_q, lo, hi, provenance, comps)?;
                entry_for(&mut acc, ShapeTarget::SubjectsOf(p.clone()))
                    .2
                    .push(pc);
            }
            // rdfs:range C → an ObjectsOf(P) node condition (every object of P satisfies it).
            for c in objects(store, &p_subj, &p_range) {
                if let Node::Iri(c) = c
                    && let Some(cc) = classify_on(p, &c)
                {
                    entry_for(&mut acc, ShapeTarget::ObjectsOf(p.clone()))
                        .1
                        .push(cc);
                }
            }
        }
    }

    // ── Functional / inverse-functional characteristics — read from the canonical logic: carrier ──
    // The `owl:FunctionalProperty` / `owl:InverseFunctionalProperty` property-type markers are a
    // DEPRECATED projection source; the single authority is the `logic:PropertyCharacteristicAssertion`
    // record that joins `logic:characterizes` (the characterized property P) with
    // `logic:characteristicSort` (the marker) — the same carrier the native coherence gate reads.
    // A functional/inverse-functional characteristic is a genuine closed-world cardinality bound (it
    // CONSTRAINS, it does not merely infer), so it stays derive-all (+ the OpenWorldClosure opt-out),
    // independent of the domain/range opt-in above. The functional cap lands on BOTH the
    // property-scoped SubjectsOf(P) domain shape AND — so the declarative class-node reader
    // (Pydantic / ShEx) narrows the field to scalar — every rdfs:domain class node shape of P. A
    // property with NO rdfs:domain (e.g. gmeow:unit) keeps ONLY the property-scoped cap; no class
    // node shape is fabricated. The merge (`merge_same_path_properties`) folds the class-scoped cap
    // cleanly into any restriction the class already authored on P.
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let inverse_functional_sort = Node::iri(logic_iri("inverseFunctionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    for rec in subjects_with(store, &nn(RDF_TYPE), &char_assertion_ty) {
        let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes) else {
            continue;
        };
        if optouts.contains(&prop) {
            continue;
        }
        let sorts = objects(store, &rec, &p_characteristic_sort);
        let prop_subj = Subject::Iri(prop.clone());
        if sorts.contains(&functional_sort) {
            // Property-scoped cap: each subject of P has ≤1 value (sh:maxCount 1 on P).
            let pc = PropertyConstraintIr::new(
                &prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?;
            entry_for(&mut acc, ShapeTarget::SubjectsOf(prop.clone()))
                .2
                .push(pc);
            // Class-scoped cap: the same maxCount-1 on each rdfs:domain class node shape, so the
            // class-node reader narrows the Python field to scalar.
            for c in objects(store, &prop_subj, &p_domain) {
                if let Node::Iri(c) = c
                    && matches!(classify(&c), Some(ConstraintComponent::Class(_)))
                {
                    let pc = PropertyConstraintIr::new(
                        &prop,
                        None,
                        Some(1),
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(c)).2.push(pc);
                }
            }
        }
        if sorts.contains(&inverse_functional_sort) {
            // Inverse-functional: each object of P has ≤1 subject via P (inverse sh:maxCount 1), on
            // both the property-scoped ObjectsOf(P) shape and each rdfs:range class node shape.
            let pc = PropertyConstraintIr::new(
                &prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?
            .inverted();
            entry_for(&mut acc, ShapeTarget::ObjectsOf(prop.clone()))
                .2
                .push(pc);
            for c in objects(store, &prop_subj, &p_range) {
                if let Node::Iri(c) = c
                    && matches!(classify(&c), Some(ConstraintComponent::Class(_)))
                {
                    let pc = PropertyConstraintIr::new(
                        &prop,
                        None,
                        Some(1),
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?
                    .inverted();
                    entry_for(&mut acc, ShapeTarget::Class(c)).2.push(pc);
                }
            }
        }
    }

    // ── FAMILY 4 — logic:KeyAssertion (single-property keys → inverse-functional reading) ──────
    // A `logic:KeyAssertion` record (`logic:keyClass C`, `logic:keyProperty P`) says the value of P
    // uniquely identifies the C instance — the canonical logic: carrier of a datatype/single-property
    // key (the DEPRECATED `owl:hasKey ( P )` OWL-DL axiom is its lossy view, no longer a projection
    // source; an `owl:InverseFunctionalProperty` on a datatype property would be OWL 2 Full). Its
    // closed-world reading is the same inverse `sh:maxCount 1` (each P-value has ≤1 subject via P) the
    // InverseFunctionalProperty arm emits. A COMPOSITE key (a record naming several `logic:keyProperty`
    // values) asserts the TUPLE is unique, not each part — it has no single-path SHACL form, so it is
    // carried in the canon and no shape is derived (never a wrong per-part uniqueness claim).
    let key_assertion_ty = Node::iri(logic_iri("KeyAssertion"));
    let p_key_class = nn(&logic_iri("keyClass"));
    let p_key_property = nn(&logic_iri("keyProperty"));
    for rec in subjects_with(store, &nn(RDF_TYPE), &key_assertion_ty) {
        // The keyed class must be GMEOW-owned (the dogfooding guard every family applies).
        let Some(Node::Iri(class_iri)) = value(store, &rec, &p_key_class) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) {
            continue;
        }
        let keys: Vec<String> = objects(store, &rec, &p_key_property)
            .into_iter()
            .filter_map(|o| match o {
                Node::Iri(i) => Some(i),
                _ => None,
            })
            .collect();
        // Single-property key only; a composite key has no single-path uniqueness shape.
        if let [key_prop] = keys.as_slice() {
            if optouts.contains(key_prop) {
                continue;
            }
            let pc = PropertyConstraintIr::new(
                key_prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?
            .inverted();
            entry_for(&mut acc, ShapeTarget::ObjectsOf(key_prop.clone()))
                .2
                .push(pc);
        }
    }

    // ── FAMILY 5 — statement-layer reifier obligations ────────────────────────────────────
    // The one closed-world condition with NO ordinary-OWL antecedent: "every K→P→value assertion
    // must be reified, and its reifier must conform to shape C." A slice declares it with the
    // classic RDF reification form (`rdf:Statement` + `rdf:subject`/`rdf:predicate`/`rdf:object`),
    // which the frontend already reads (see `extract_scoped_axioms`). The classic form is used
    // rather than the native `rdf:reifies <<( K P O )>>` term deliberately: a quoted-triple object
    // cannot ride the base-quad fold to the `gmeow.gts` terminal (the statement layer travels the
    // reifier/annotation tables), so the schema-level obligation is authored as plain base triples.
    // No parallel shape vocabulary is minted; `C` must be a constrained GMEOW class so FAMILY 1
    // derives the `{C}-shape` node the `sh:reifierShape` reference resolves to. Lowered to a
    // property shape on path `P` carrying `sh:reifierShape {C}-shape` + `sh:reificationRequired true`.
    let p_rdf_subject = nn(RDF_SUBJECT);
    let p_rdf_predicate = nn(RDF_PREDICATE);
    for r in subjects_with(store, &nn(RDF_TYPE), &Node::iri(RDF_STATEMENT)) {
        // The reified predicate `P` and subject class `K` must both be GMEOW-owned (the dogfooding
        // guard every family applies).
        let Some(Node::Iri(p)) = value(store, &r, &p_rdf_predicate) else {
            continue;
        };
        let Some(Node::Iri(k)) = value(store, &r, &p_rdf_subject) else {
            continue;
        };
        if !is_authoring_ns(&k) || !is_authoring_ns(&p) {
            continue;
        }
        // The reifier's GMEOW type `C` names the shape the reifier must conform to. An untyped
        // reifier carries no shape reference (a `sh:reifierShape` with no resolvable target would
        // dangle), so the typed-reifier form is required — an untyped one is not an obligation.
        let Some(c) = objects(store, &r, &nn(RDF_TYPE))
            .into_iter()
            .find_map(|t| match t {
                Node::Iri(i) if is_authoring_ns(&i) => Some(i),
                _ => None,
            })
        else {
            continue;
        };
        let property = PropertyConstraintIr::new(p.clone(), None, None, None, vec![])?
            .with_reifier(Some(format!("{c}-shape")), true)?;
        entry_for(&mut acc, ShapeTarget::Class(k.clone()))
            .2
            .push(property);
    }

    // ── FAMILY 6 — value-keyed general class inclusion (ValueKeyed(P,V) target) ────────────
    // A general class axiom whose SUBJECT is an anonymous value restriction
    // `[ owl:onProperty P ; owl:hasValue V ]` reads closed-world as "every focus node with P = V
    // must satisfy the superclass" — a value-keyed selection projected to an `sh:SPARQLTarget`
    // (`SELECT ?this WHERE { ?this P V }`). It is the modes-ride-one-class idiom: several kinds
    // share ONE class, discriminated by a VALUE (`inferenceModeOf gmeow:modeAbduction`), never a
    // subclass (Principle 9). The superclass restriction lowers to one property constraint exactly
    // as FAMILY 1 reads a subclass restriction. Only an IRI-valued key is value-keyable (the SPARQL
    // target binds an IRI object); a literal key or a superclass that is not a single-property
    // restriction is carried in the canon, never a wrong value-keyed shape.
    let subclassof_iri = format!("{rdfs}subClassOf");
    for q in default_graph_quads(store) {
        if q.predicate.as_str() != subclassof_iri || !subject_is_blank(&q.subject) {
            continue;
        }
        let key_subj = q.subject.clone();
        let Some(Node::Iri(key_pred)) = value(store, &key_subj, &p_on) else {
            continue;
        };
        let Some(Node::Iri(key_val)) = value(store, &key_subj, &p_hasvalue) else {
            continue;
        };
        if !is_authoring_ns(&key_pred) || optouts.contains(&key_pred) {
            continue;
        }
        let Some(super_subj) = term_as_subject(&q.object) else {
            continue;
        };
        let Some(Node::Iri(on)) = value(store, &super_subj, &p_on) else {
            continue;
        };
        if optouts.contains(&on) {
            continue;
        }
        let mut components: Vec<ConstraintComponent> = Vec::new();
        for vp in [&p_some, &p_all] {
            if let Some(Node::Iri(cv)) = value(store, &super_subj, vp)
                && let Some(cc) = classify_on(&on, &cv)
            {
                components.push(cc);
            }
        }
        if let Some(Node::Iri(v)) = value(store, &super_subj, &p_hasvalue) {
            components.push(ConstraintComponent::HasValue(ShapeValue::Iri(v)));
        }
        let exact = card_of(&super_subj, &p_card);
        let (mut lo, mut hi) = (
            card_of(&super_subj, &p_mincard),
            card_of(&super_subj, &p_maxcard),
        );
        if let Some(n) = exact {
            lo = Some(n);
            hi = Some(n);
        }
        if components.is_empty() && lo.is_none() && hi.is_none() {
            continue;
        }
        let provenance =
            (lo.is_some() || hi.is_some()).then_some(ConstraintProvenance::OwlRestriction);
        let pc = PropertyConstraintIr::new(&on, lo, hi, provenance, components)?;
        entry_for(
            &mut acc,
            ShapeTarget::ValueKeyed {
                predicate: key_pred,
                value: key_val,
            },
        )
        .2
        .push(pc);
    }

    // ── FAMILY 7 — pinned-value forbidden-pattern records (Class(C) target) ────────────────
    // An authored `logic:ForbiddenPatternConstraint` carrying `logic:onClass C`,
    // `logic:forbiddenPredicate P`, and a PINNED `logic:forbiddenValue v` is the canonical
    // validates-but-does-not-entail record for "an instance of C must never carry P = v".
    // It is the decidable authoring form of the value-complement pattern: the OWL rendering
    // (`owl:allValuesFrom [ owl:complementOf [ owl:hasValue v ] ]`) is outside the native
    // reasoner's decidable fragment, so it must never sit in the reasoning core as an axiom.
    // The record lowers declaratively to the exact SHACL-Core component the legacy shapes
    // carried — `sh:not [ sh:hasValue v ]` on path P of the `{C}-shape` — merging with the
    // class's other property conditions on that path. It is a validation descriptor, not an
    // OWL axiom being re-read, so the per-property `OpenWorldClosure` opt-out (which governs
    // the validation READING of reasoning axioms) does not apply, and its triples never enter
    // `prog.axioms` (`is_constraint_structural_predicate` / `is_constraint_sugar_class`).
    // LITERAL pins only: an IRI pin is the class-negation idiom (`logic:forbiddenPredicate
    // rdf:type`), whose declarative home is the node-level `sh:not [ sh:class … ]` the
    // disjointness family already derives — lowering it here would mint a redundant
    // `rdf:type`-path property shape that leaks into the schema projections. The IRI-pinned
    // and UNPINNED forms keep their procedural projection only.
    let forbidden_ty = Node::iri(logic_iri("ForbiddenPatternConstraint"));
    let p_sugar_onclass = nn(&logic_iri("onClass"));
    let p_forbidden_pred = nn(&logic_iri("forbiddenPredicate"));
    let p_forbidden_value = nn(&logic_iri("forbiddenValue"));
    for record in subjects_with(store, &nn(RDF_TYPE), &forbidden_ty) {
        let Some(Node::Iri(class_iri)) = value(store, &record, &p_sugar_onclass) else {
            continue;
        };
        // Only an authoring-namespace class owns a derived shape (the same dogfooding
        // boundary every other family enforces).
        if !is_authoring_ns(&class_iri) {
            continue;
        }
        let Some(Node::Iri(on)) = value(store, &record, &p_forbidden_pred) else {
            continue;
        };
        let sv = match value(store, &record, &p_forbidden_value) {
            Some(Node::Lit {
                lexical,
                datatype,
                lang,
            }) => ShapeValue::Literal {
                lexical,
                datatype,
                lang,
            },
            // IRI-pinned, unpinned, or malformed — no per-value SHACL-Core form here; the
            // record's canonical logic:Constraint expansion carries it procedurally.
            _ => continue,
        };
        let pc = PropertyConstraintIr::new(
            &on,
            None,
            None,
            None,
            vec![ConstraintComponent::Not(Box::new(
                ConstraintComponent::HasValue(sv),
            ))],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── FAMILY 8 — value-range records (Class(C) target) ───────────────────────────────────
    // An authored `logic:ValueRangeConstraint` (`logic:onClass C`, `logic:valuePath P`, and an
    // inclusive `logic:minInclusiveBound` and/or `logic:maxInclusiveBound` literal) is the
    // canonical validates-but-does-not-entail record for a bounded numeric range on a path.
    // Like FAMILY 7 it is the decidable authoring form of an out-of-fragment OWL pattern: a
    // faceted-datatype `owl:allValuesFrom` filler is undecidable for the native reasoner as
    // soon as any literal is asserted on the constrained path (no datatype value-space
    // reasoning), so the range must never sit in the reasoning core. It lowers declaratively
    // to the exact components the legacy facet carried — `sh:minInclusive`/`sh:maxInclusive`
    // on path P of the `{C}-shape` — merging with the class's other conditions on that path.
    // A validation descriptor, never a re-read OWL axiom, so the `OpenWorldClosure` opt-out
    // does not apply and its triples never enter `prog.axioms`.
    let range_ty = Node::iri(logic_iri("ValueRangeConstraint"));
    let p_valuepath = nn(&logic_iri("valuePath"));
    let p_min_bound = nn(&logic_iri("minInclusiveBound"));
    let p_max_bound = nn(&logic_iri("maxInclusiveBound"));
    // Exclusive peers (`logic:minExclusiveBound` / `logic:maxExclusiveBound`) lower to
    // `sh:minExclusive` / `sh:maxExclusive` — the faithful record for a legacy shape whose bound
    // was authored open (e.g. `sh:minExclusive 0` for a strictly-positive denominator). An
    // inclusive bound wins over an exclusive one on the same endpoint (the tighter closed reading).
    let p_min_excl = nn(&logic_iri("minExclusiveBound"));
    let p_max_excl = nn(&logic_iri("maxExclusiveBound"));
    for record in subjects_with(store, &nn(RDF_TYPE), &range_ty) {
        let Some(Node::Iri(class_iri)) = value(store, &record, &p_sugar_onclass) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) {
            continue;
        }
        let Some(Node::Iri(on)) = value(store, &record, &p_valuepath) else {
            continue;
        };
        let bound_of = |p: &Iri| -> Option<f64> {
            match value(store, &record, p) {
                Some(Node::Lit { lexical, .. }) => lexical.trim().parse::<f64>().ok(),
                _ => None,
            }
        };
        // Inclusive bound wins over the exclusive peer on the same endpoint.
        let (lo, lo_incl) = match bound_of(&p_min_bound) {
            Some(v) => (Some(v), true),
            None => (bound_of(&p_min_excl), false),
        };
        let (hi, hi_incl) = match bound_of(&p_max_bound) {
            Some(v) => (Some(v), true),
            None => (bound_of(&p_max_excl), false),
        };
        if lo.is_none() && hi.is_none() {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &on,
            None,
            None,
            None,
            vec![ConstraintComponent::NumericRange {
                min: lo,
                max: hi,
                min_inclusive: lo_incl,
                max_inclusive: hi_incl,
            }],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── Build one shape per target ────────────────────────────────────────────────────────
    // Dedup by structural (`PartialEq`) identity so a duplicate axiom never double-counts; the IR
    // constructors then sort node_components + properties into canonical content-key order, so
    // supply order never affects the emitted bytes. Order-preserving and allocation-free (the
    // per-target component/property lists are small), so no `format!("{:?}")` per element.
    fn dedup_eq<T: PartialEq>(v: Vec<T>) -> Vec<T> {
        let mut out: Vec<T> = Vec::with_capacity(v.len());
        for x in v {
            if !out.contains(&x) {
                out.push(x);
            }
        }
        out
    }

    let mut shapes = Vec::new();
    for (iri, (target, node_components, properties)) in acc {
        let node_components = dedup_eq(node_components);
        let properties = merge_same_path_properties(dedup_eq(properties))?;
        if node_components.is_empty() && properties.is_empty() {
            continue;
        }
        let mut shape = ValidationShapeIr::new(iri, target.clone(), properties, None)?
            .with_node_components(node_components)?;
        // Failure metadata belongs to the canonical target term, never to a hand-authored SHACL
        // node. For a class-targeted shape that term is the CLASS; for the domain/range shapes
        // derived from `rdfs:domain P …` / `rdfs:range P …` it is the PROPERTY — the shape's whole
        // focus set is "the subjects (objects) of P", so P is the only authored term the shape
        // belongs to, and without this a property-scoped law's findings resolve to no failure class
        // at all (`<unmapped>`), leaving a conformance cell unable to say which class it just
        // proved. Collapse repeated identical values; hard-fail distinct values.
        let failure_source = match &target {
            ShapeTarget::Class(class) => Some(("class", class)),
            ShapeTarget::SubjectsOf(predicate) | ShapeTarget::ObjectsOf(predicate) => {
                Some(("property", predicate))
            }
            ShapeTarget::ValueKeyed { .. }
            | ShapeTarget::DirectClass(_)
            | ShapeTarget::Sparql(_) => None,
        };
        if let Some((kind, term)) = failure_source {
            let failure_classes = distinct_failure_classes(store, &Subject::Iri(term.clone()))?;
            if failure_classes.len() > 1 {
                return Err(Diag::of_kind(crate::error::Frontend {
                    detail: format!("{kind} {term} has distinct gmeow:enforcesFailureClass values"),
                }));
            }
            if let Some(failure_class) = failure_classes.first() {
                shape = shape.with_failure_class(failure_class)?;
            }
        }
        shapes.push(shape);
    }
    Ok(shapes)
}

/// The authoring-completeness invariant for the functional-characteristic carrier migration.
///
/// Every `gmeow:` property still declared `owl:FunctionalProperty` MUST also carry a
/// `logic:PropertyCharacteristicAssertion` record whose `logic:characteristicSort` is
/// `logic:functionalProperty` and whose `logic:characterizes` names it — the canonical carrier the
/// SHACL/Pydantic projection now reads (the `owl:FunctionalProperty` marker is a deprecated,
/// no-longer-projected source). A property in the returned set is an AUTHORING GAP: its
/// functionality would silently vanish from the derived projection because no carrier record
/// grounds it. Returned sorted (BTreeSet) for a deterministic diagnostic; a non-empty result is a
/// HARD FAIL on the sync path, never a soft warning — the caller must stop, not degrade.
///
/// The live sync-path caller is the `stage-compile-logic` shape-derivation stage
/// (`crates/pipeline/src/stages/compile_logic.rs`), which runs this over the merged authored
/// corpus and returns `Err` if the set is non-empty. Post-removal of the `owl:FunctionalProperty`
/// slice sources it is vacuously satisfied (nothing is `declared`), so it guards RE-introduction.
pub fn functional_properties_missing_logic_carrier(
    store: &RdfDataset,
) -> std::collections::BTreeSet<String> {
    use gmeow_ns::GMEOW_NS;
    let owl_functional = Node::iri("http://www.w3.org/2002/07/owl#FunctionalProperty");
    // Every gmeow:-owned property carrying the deprecated OWL functional marker.
    let mut declared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for s in subjects_with(store, &nn(RDF_TYPE), &owl_functional) {
        if let Subject::Iri(iri) = &s
            && iri.starts_with(GMEOW_NS)
        {
            declared.insert(iri.clone());
        }
    }
    // Every property named by a functional-sort characteristic-assertion carrier record.
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    let mut carried: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for rec in subjects_with(store, &nn(RDF_TYPE), &char_assertion_ty) {
        if objects(store, &rec, &p_characteristic_sort).contains(&functional_sort)
            && let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes)
        {
            carried.insert(prop);
        }
    }
    declared.difference(&carried).cloned().collect()
}

// --------------------------------------------------------------------------- //
// Functional-carrier integrity (migration-surviving, non-vacuous)
// --------------------------------------------------------------------------- //

/// The frozen completeness ledger of every property that MUST bear a functional
/// `logic:PropertyCharacteristicAssertion` carrier — one canonical IRI per line, sorted, with `#`
/// comment lines and blank lines ignored.
///
/// This is a DELIBERATE, human-blessed committed artifact, exactly like the `dl_oracle_gold`
/// frozen verdicts and the `reasoning_session_semver` drift-pins: it is NEVER auto-updated. Any
/// property that gains or loses a functional carrier moves this file in the SAME commit that moves
/// the carrier, a conscious re-bless the author performs by hand. The
/// [`functional_carrier_integrity`] gate asserts current-set == ledger and hard-fails on any drift
/// (a silently-dropped or unexpectedly-added carrier), so a drift can never land unnoticed.
///
/// # Re-blessing (a deliberate human act)
///
/// When a functional carrier is intentionally added or removed, regenerate this file from the
/// current corpus and commit it alongside the carrier change. The set is exactly the
/// `logic:characterizes` targets of the `logic:functionalProperty`-sort
/// `logic:PropertyCharacteristicAssertion` records over the merged authored corpus (see
/// [`functional_carrier_property_iris`]); the pinning test `functional_carrier_ledger_matches_corpus`
/// (in `crates/pipeline/tests`) prints the exact expected body on a mismatch.
const FUNCTIONAL_CARRIER_LEDGER: &str = include_str!("frontend/functional_carrier_ledger.txt");

/// Parse [`FUNCTIONAL_CARRIER_LEDGER`] into its sorted IRI set (ignoring `#` comments / blanks).
fn functional_carrier_ledger() -> std::collections::BTreeSet<String> {
    FUNCTIONAL_CARRIER_LEDGER
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The RDF/OWL types whose subject "exists as a declared property" for the orphan check: an
/// `rdf:Property` or any of its OWL sub-kinds. This is the property-declaration reading the
/// validation-shape derivation uses (`owl:ObjectProperty` / `owl:DatatypeProperty`), widened to
/// the full property-declaration vocabulary so a carrier target declared with ANY property type is
/// recognised, and a merely misspelled / never-declared target is not.
const PROPERTY_DECLARATION_TYPES: [&str; 11] = [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#AnnotationProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
    "http://www.w3.org/2002/07/owl#ReflexiveProperty",
    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
];

/// Every functional carrier record's `logic:characterizes` target, as a property → carrier-count
/// multiset over the store. A property named functional by two distinct
/// `logic:PropertyCharacteristicAssertion` records appears with count 2 (the duplicate signal).
fn functional_carrier_multiset(store: &RdfDataset) -> std::collections::BTreeMap<String, usize> {
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for rec in subjects_with(store, &nn(RDF_TYPE), &char_assertion_ty) {
        if objects(store, &rec, &p_characteristic_sort).contains(&functional_sort)
            && let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes)
        {
            *counts.entry(prop).or_insert(0) += 1;
        }
    }
    counts
}

/// The set of properties bearing at least one functional carrier record — the exact set the
/// completeness ledger pins. Sorted and deterministic; this is the generator a re-bless copies
/// into `frontend/functional_carrier_ledger.txt`.
pub fn functional_carrier_property_iris(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    functional_carrier_multiset(store).into_keys().collect()
}

/// Every subject typed as an RDF/OWL property (see [`PROPERTY_DECLARATION_TYPES`]).
fn declared_property_iris(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    let mut props: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for ty in PROPERTY_DECLARATION_TYPES {
        for s in subjects_with(store, &nn(RDF_TYPE), &Node::iri(ty)) {
            if let Subject::Iri(iri) = s {
                props.insert(iri);
            }
        }
    }
    props
}

/// A single functional-carrier integrity violation, each a HARD FAIL on the sync path.
///
/// The variants make the migration-surviving completeness invariant NON-VACUOUS: unlike the bare
/// [`functional_properties_missing_logic_carrier`] RE-introduction guard (vacuous once the
/// `owl:FunctionalProperty` markers were removed), these bite against the LIVE carrier corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionalCarrierViolation {
    /// A property still typed `owl:FunctionalProperty` with no functional carrier record — the
    /// original RE-introduction regression guard, retained.
    ReintroducedOwlMarker { property: String },
    /// A functional carrier's `logic:characterizes` names an IRI that is not a declared property
    /// anywhere in the store (a misspelled / never-declared target).
    OrphanCarrier { property: String },
    /// A property is named functional by more than one carrier record.
    DuplicateCarrier { property: String, count: usize },
    /// A property is in the frozen ledger but bears NO functional carrier now — a silently-dropped
    /// carrier.
    LedgerMissing { property: String },
    /// A property bears a functional carrier but is ABSENT from the frozen ledger — an unexpected
    /// new carrier that was never blessed.
    LedgerUnexpected { property: String },
}

impl fmt::Display for FunctionalCarrierViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionalCarrierViolation::ReintroducedOwlMarker { property } => write!(
                f,
                "re-introduced owl:FunctionalProperty on {property} without a \
                 logic:PropertyCharacteristicAssertion functionalProperty carrier record"
            ),
            FunctionalCarrierViolation::OrphanCarrier { property } => write!(
                f,
                "orphan functional carrier: logic:characterizes names {property}, which is not a \
                 declared rdf:Property / owl:ObjectProperty / owl:DatatypeProperty in the corpus"
            ),
            FunctionalCarrierViolation::DuplicateCarrier { property, count } => write!(
                f,
                "duplicate functional carrier: {property} is named functional by {count} \
                 PropertyCharacteristicAssertion records (exactly one is required)"
            ),
            FunctionalCarrierViolation::LedgerMissing { property } => write!(
                f,
                "completeness drift: {property} is in the frozen functional-carrier ledger but \
                 bears no functional carrier now (a silently-dropped carrier); re-bless the \
                 ledger only if this drop is intended"
            ),
            FunctionalCarrierViolation::LedgerUnexpected { property } => write!(
                f,
                "completeness drift: {property} bears a functional carrier but is absent from the \
                 frozen functional-carrier ledger (an unexpected new carrier); re-bless the \
                 ledger only if this addition is intended"
            ),
        }
    }
}

/// The migration-surviving functional-carrier integrity gate — the NON-VACUOUS successor to
/// [`functional_properties_missing_logic_carrier`].
///
/// It runs four checks over the merged authored corpus and returns every violation, sorted
/// deterministically (each kind is drawn from a `BTree*`, so the diagnostic bytes are stable):
///
/// 1. **RE-introduction guard** (retained) — any property still typed `owl:FunctionalProperty`
///    with no functional carrier ([`FunctionalCarrierViolation::ReintroducedOwlMarker`]).
/// 2. **Orphan carrier** — a carrier `logic:characterizes` target that is not a declared property
///    ([`FunctionalCarrierViolation::OrphanCarrier`]).
/// 3. **Duplicate carrier** — a property named functional by more than one record
///    ([`FunctionalCarrierViolation::DuplicateCarrier`]).
/// 4. **Positive completeness ledger** — the set of carrier-bearing properties must equal the
///    committed frozen ledger; any drift yields
///    [`FunctionalCarrierViolation::LedgerMissing`] / [`FunctionalCarrierViolation::LedgerUnexpected`].
///
/// A non-empty result is a HARD FAIL on the sync path (see
/// `crates/pipeline/src/stages/compile_logic.rs`), never a soft warning — the caller must stop.
pub fn functional_carrier_integrity(store: &RdfDataset) -> Vec<FunctionalCarrierViolation> {
    let mut violations: Vec<FunctionalCarrierViolation> = Vec::new();

    // (1) RE-introduction guard — kept verbatim from the pre-migration completeness gate.
    for property in functional_properties_missing_logic_carrier(store) {
        violations.push(FunctionalCarrierViolation::ReintroducedOwlMarker { property });
    }

    let multiset = functional_carrier_multiset(store);
    let declared = declared_property_iris(store);

    // (2) Orphan carrier + (3) duplicate carrier — both keyed off the sorted carrier multiset.
    for (property, count) in &multiset {
        if !declared.contains(property) {
            violations.push(FunctionalCarrierViolation::OrphanCarrier {
                property: property.clone(),
            });
        }
        if *count > 1 {
            violations.push(FunctionalCarrierViolation::DuplicateCarrier {
                property: property.clone(),
                count: *count,
            });
        }
    }

    // (4) Positive completeness ledger — the exact carrier-bearing set must equal the frozen ledger.
    let carried: std::collections::BTreeSet<String> = multiset.into_keys().collect();
    let ledger = functional_carrier_ledger();
    for property in ledger.difference(&carried) {
        violations.push(FunctionalCarrierViolation::LedgerMissing {
            property: property.clone(),
        });
    }
    for property in carried.difference(&ledger) {
        violations.push(FunctionalCarrierViolation::LedgerUnexpected {
            property: property.clone(),
        });
    }

    violations
}

/// Read `logic:PathShape` individuals into [`PathShapeIr`]s.
///
/// Fail-soft, like the rest of the front-end: a malformed shape (a step that is
/// both named and wildcard, neither named nor wildcard, a non-positive-integer
/// depth, or `min > max`) emits a `MALFORMED_PATH_SHAPE` warning and is skipped —
/// never silently dropped.
fn extract_path_shapes(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathShapeIr> {
    let mut shapes: Vec<PathShapeIr> = Vec::new();

    let path_shape_ty = Node::iri(logic_iri("PathShape"));
    let p_step = nn(&logic_iri("pathStepPredicate"));
    let p_wildcard = nn(&logic_iri("pathWildcard"));
    let p_min = nn(&logic_iri("pathMinDepth"));
    let p_max = nn(&logic_iri("pathMaxDepth"));
    let p_ns = nn(&logic_iri("pathNamespaceScope"));
    let p_param = nn(&logic_iri("pathDepthParam"));

    for node in subjects_with(store, &nn(RDF_TYPE), &path_shape_ty) {
        let subj = subject_str(&node);

        // Base step: named-predicate XOR wildcard.
        let step_pred = value(store, &node, &p_step);
        // xsd:boolean lexical space: "true"/"1" = true, "false"/"0" = false.
        // Any other literal is a hard-fail (no silent coercion to false).
        let wildcard_result: Result<bool, ()> = match value(store, &node, &p_wildcard) {
            None => Ok(false),
            Some(t) => match term_str(&t).as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                other => {
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        format!(
                            "logic:pathWildcard has unrecognized boolean literal {:?}; \
                             expected \"true\", \"false\", \"1\", or \"0\"; shape skipped",
                            other
                        ),
                        Some(subj.clone()),
                    ));
                    Err(())
                }
            },
        };
        let wildcard = match wildcard_result {
            Ok(b) => b,
            Err(()) => continue,
        };

        let base = match (step_pred.as_ref(), wildcard) {
            (Some(_), true) => {
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_PATH_SHAPE",
                    "logic:PathShape declares BOTH logic:pathStepPredicate and \
                     logic:pathWildcard true (a step is a named predicate XOR a \
                     wildcard); shape skipped",
                    Some(subj.clone()),
                ));
                continue;
            }
            (Some(p), false) => match p {
                // A step predicate MUST be an IRI: a literal or blank-node object
                // would produce a malformed predicate IRI downstream (no silent
                // coercion). Skip the shape with a diagnostic, like every other
                // malformed-shape branch.
                Node::Iri(_) => PathBase::NamedPredicate(term_str(p)),
                _ => {
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        "logic:pathStepPredicate must be an IRI named node; shape skipped",
                        Some(subj.clone()),
                    ));
                    continue;
                }
            },
            (None, true) => PathBase::Wildcard,
            (None, false) => {
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_PATH_SHAPE",
                    "logic:PathShape declares neither logic:pathStepPredicate nor \
                     logic:pathWildcard true (a step needs a named predicate or a \
                     wildcard); shape skipped",
                    Some(subj.clone()),
                ));
                continue;
            }
        };

        // Depth bounds (min defaults to 1; absent max ⇒ unbounded).
        let min_depth = match value(store, &node, &p_min) {
            Some(t) => match parse_positive_int(&term_str(&t)) {
                Some(n) => n,
                None => {
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        format!(
                            "logic:pathMinDepth is not a positive integer ({:?}); shape skipped",
                            term_str(&t)
                        ),
                        Some(subj.clone()),
                    ));
                    continue;
                }
            },
            None => 1,
        };
        let max_depth = match value(store, &node, &p_max) {
            Some(t) => match parse_positive_int(&term_str(&t)) {
                Some(n) => Some(n),
                None => {
                    diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        format!(
                            "logic:pathMaxDepth is not a positive integer ({:?}); shape skipped",
                            term_str(&t)
                        ),
                        Some(subj.clone()),
                    ));
                    continue;
                }
            },
            None => None,
        };

        let namespace_scope = value(store, &node, &p_ns).map(|t| term_str(&t));
        let depth_param = value(store, &node, &p_param).map(|t| term_str(&t));

        match PathShapeIr::new(
            subj.clone(),
            base,
            min_depth,
            max_depth,
            namespace_scope,
            depth_param,
        ) {
            Ok(shape) => shapes.push(shape),
            Err(msg) => diagnostics.push(Diagnostic::warning(
                "MALFORMED_PATH_SHAPE",
                msg.message().to_owned(),
                Some(subj),
            )),
        }
    }

    shapes
}

// --------------------------------------------------------------------------- //
// Public API
// --------------------------------------------------------------------------- //

// --------------------------------------------------------------------------- //
// Full first-order formula extraction (absent logic:Formula → empty list)
// --------------------------------------------------------------------------- //

/// The sub-formula link predicates: a `logic:Formula` reached through any of these is a
/// COMPONENT of another formula, so it is not a top-level assertion (`logic:hasFormula`).
const FORMULA_SUBLINKS: [&str; 8] = [
    "not",
    "and",
    "or",
    "antecedent",
    "consequent",
    "iff",
    "forall",
    "exists",
];

/// Read top-level `logic:Formula` trees into [`Formula`]s. A malformed node emits an
/// error-grade `MALFORMED_FORMULA` finding and is skipped: accepting a partial or ambiguous
/// tree would change its logical meaning. A returned formula MAY be trivially-Horn (a reified
/// ordinary triple) — the caller ([`parse_logic_dataset`]) partitions those out to
/// [`LogicProgram::axioms`] via [`Formula::as_horn_axiom`] so the `with_formulas` invariant
/// is enforced by routing, not by assuming the projection never authors one.
struct FormulaExtraction {
    formulas: Vec<Formula>,
    malformed: BTreeSet<String>,
}

fn extract_formulas(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> FormulaExtraction {
    let formula_ty = Node::iri(logic_iri("Formula"));
    let subjects = subjects_with(store, &nn(RDF_TYPE), &formula_ty);

    // A formula reached by a sub-formula link is a component, not a top-level node.
    let mut referenced: HashSet<String> = HashSet::new();
    for subj in &subjects {
        for link in FORMULA_SUBLINKS {
            for obj in objects(store, subj, &nn(&logic_iri(link))) {
                referenced.insert(term_str(&obj));
            }
        }
    }
    // A formula reached as a `logic:Constraint`'s `logic:integrity` is that constraint's
    // integrity condition, NOT a free-standing top-level assertion: it is owned by
    // `LogicProgram::constraints`, so it must not ALSO enter `LogicProgram::formulas` (that
    // would give one authored formula two content-addressed homes). Exclude every integrity
    // root here.
    let integrity_pred = nn(&logic_iri("integrity"));
    for constraint in subjects_with(store, &nn(RDF_TYPE), &Node::iri(logic_iri("Constraint"))) {
        for obj in objects(store, &constraint, &integrity_pred) {
            referenced.insert(term_str(&obj));
        }
    }
    // A formula reached as a correspondence recovery transform is owned by that
    // first-class `logic:RecoveryCase`, not a free-standing assertion.  Keep one semantic
    // home while still validating every node in the shared formula parser below.
    let recovery_transform_pred = nn(&logic_iri("recoveryTransform"));
    for case in subjects_with(store, &nn(RDF_TYPE), &Node::iri(logic_iri("RecoveryCase"))) {
        for obj in objects(store, &case, &recovery_transform_pred) {
            referenced.insert(term_str(&obj));
        }
    }
    // A formula reached as a `logic:ReasoningProgram`'s `logic:clause` / `logic:programQuery`
    // / `logic:verdictProbe` is owned by that program, not a free-standing assertion — exactly
    // like a constraint's `logic:integrity` and a recovery case's `logic:recoveryTransform`
    // above. Excluding every clause/query/probe root here is what lets
    // `extract_reasoning_programs` reuse `parse_formula` without giving one authored formula
    // two content-addressed IR homes.
    let reasoning_programs = subjects_with(
        store,
        &nn(RDF_TYPE),
        &Node::iri(logic_iri("ReasoningProgram")),
    );
    for link in ["clause", "programQuery", "verdictProbe"] {
        let pred = nn(&logic_iri(link));
        for program in &reasoning_programs {
            for obj in objects(store, program, &pred) {
                referenced.insert(term_str(&obj));
            }
        }
    }
    // A formula reached as a `logic:AbductiveSchema`'s `logic:completenessFormula` is the
    // discipline-satisfied condition the abductive producer instantiates at a gap subject —
    // NOT a free-standing top-level assertion. Excluding every completeness root here is what
    // keeps authoring a completeness condition from asserting it as an always-true axiom (which
    // would both corrupt the reasoned core and auto-assert the very structure the advice only
    // RECOMMENDS adding).
    let completeness_pred = nn(&logic_iri("completenessFormula"));
    for schema in subjects_with(
        store,
        &nn(RDF_TYPE),
        &Node::iri(logic_iri("AbductiveSchema")),
    ) {
        for obj in objects(store, &schema, &completeness_pred) {
            referenced.insert(term_str(&obj));
        }
    }

    // Validate every declared formula, not only roots. This catches a cycle whose every node is
    // referenced (and therefore has no root), as well as malformed constraint-owned subtrees.
    let mut parsed: HashMap<String, Formula> = HashMap::new();
    let mut malformed: BTreeMap<String, String> = BTreeMap::new();
    for subj in &subjects {
        match parse_formula(store, subj) {
            Ok(f) => {
                parsed.insert(subject_str(subj), f);
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
                    .or_insert_with(|| error.message().to_owned());
            }
        }
    }

    for (focus, message) in &malformed {
        diagnostics.push(Diagnostic::error(
            "MALFORMED_FORMULA",
            message,
            Some(focus.clone()),
        ));
    }

    let formulas = subjects
        .iter()
        .filter(|subj| !referenced.contains(&subject_str(subj)))
        .filter_map(|subj| parsed.remove(&subject_str(subj)))
        .collect();
    FormulaExtraction {
        formulas,
        malformed: malformed.into_keys().collect(),
    }
}

/// Every object of a `logic:<link>` property.
fn formula_objects(store: &RdfDataset, node: &Subject, link: &str) -> Vec<Node> {
    objects(store, node, &nn(&logic_iri(link)))
}

/// The exactly-one child formula reached by `link` from `node`.
fn formula_err_for(focus: impl Into<String>, detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Frontend {
        detail: detail.into(),
    })
    .with_focus(focus)
}

fn formula_err(node: &Subject, detail: impl Into<String>) -> Diag {
    formula_err_for(subject_str(node), detail)
}

fn one_child_subject(
    store: &RdfDataset,
    node: &Subject,
    link: &str,
) -> gmeow_errors::Result<Subject> {
    let children = formula_objects(store, node, link);
    if children.len() != 1 {
        return Err(formula_err(
            node,
            format!(
                "logic:Formula {} requires exactly one logic:{link} object; found {}",
                subject_str(node),
                children.len()
            ),
        ));
    }
    term_as_subject(&children[0]).ok_or_else(|| {
        formula_err(
            node,
            format!(
                "logic:Formula {} has a non-resource logic:{link} object",
                subject_str(node)
            ),
        )
    })
}

/// Recursively reconstruct a [`Formula`] rooted at `node`.
///
/// The parser is deliberately strict: every node has exactly one constructor family; singleton
/// constructors have exactly one child; `and`/`or` have at least two children; `iff` has exactly
/// two; implication has one antecedent and one consequent; and recursive cycles are rejected.
pub(crate) fn parse_formula(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<Formula> {
    parse_formula_inner(store, node, &mut Vec::new())
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

fn parse_formula_inner(
    store: &RdfDataset,
    node: &Subject,
    active: &mut Vec<String>,
) -> gmeow_errors::Result<Formula> {
    let node_id = subject_str(node);
    if let Some(cycle_start) = active.iter().position(|member| member == &node_id) {
        let mut members = active[cycle_start..].to_vec();
        members.sort();
        members.dedup();
        let focus = members.first().cloned().unwrap_or_else(|| node_id.clone());
        return Err(formula_err_for(
            focus,
            format!(
                "logic:Formula recursive constructor cycle among {}",
                members.join(", ")
            ),
        ));
    }
    active.push(node_id.clone());

    let result = (|| {
        let relation = formula_objects(store, node, "relation");
        let not = formula_objects(store, node, "not");
        let and = formula_objects(store, node, "and");
        let or = formula_objects(store, node, "or");
        let iff = formula_objects(store, node, "iff");
        let antecedent = formula_objects(store, node, "antecedent");
        let consequent = formula_objects(store, node, "consequent");
        let forall = formula_objects(store, node, "forall");
        let exists = formula_objects(store, node, "exists");

        let families = [
            ("relation", !relation.is_empty()),
            ("not", !not.is_empty()),
            ("and", !and.is_empty()),
            ("or", !or.is_empty()),
            ("iff", !iff.is_empty()),
            (
                "antecedent/consequent",
                !antecedent.is_empty() || !consequent.is_empty(),
            ),
            ("forall", !forall.is_empty()),
            ("exists", !exists.is_empty()),
        ];
        let present: Vec<&str> = families
            .iter()
            .filter_map(|(name, is_present)| is_present.then_some(*name))
            .collect();
        if present.len() != 1 {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {node_id} requires exactly one constructor family; found {} ({})",
                    present.len(),
                    present.join(", ")
                ),
            ));
        }

        match present[0] {
            "relation" => {
                if relation.len() != 1 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} requires exactly one logic:relation; found {}",
                            relation.len()
                        ),
                    ));
                }
                let Node::Iri(relation_iri) = &relation[0] else {
                    return Err(formula_err(
                        node,
                        format!("logic:Formula {node_id} requires an IRI-valued logic:relation"),
                    ));
                };
                let relation =
                    Term::iri(relation_iri.clone()).map_err(|e| formula_err(node, e.message()))?;
                let args = parse_term_carriers(store, node, "argument", &mut Vec::new())?;
                if args.is_empty() {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} atomic predication requires at least one logic:argument"
                        ),
                    ));
                }
                Formula::atom(relation, args).map_err(|e| formula_err(node, e.message()))
            }
            "not" => {
                let child = one_child_subject(store, node, "not")?;
                Ok(Formula::Not(Box::new(parse_formula_inner(
                    store, &child, active,
                )?)))
            }
            "and" | "or" => {
                let link = present[0];
                let child_terms = if link == "and" { &and } else { &or };
                if child_terms.len() < 2 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} logic:{link} requires at least two operands; found {}",
                            child_terms.len()
                        ),
                    ));
                }
                let mut parsed = Vec::with_capacity(child_terms.len());
                for child in child_terms {
                    let child = term_as_subject(child).ok_or_else(|| {
                        formula_err(
                            node,
                            format!(
                                "logic:Formula {node_id} has a non-resource logic:{link} operand"
                            ),
                        )
                    })?;
                    parsed.push(parse_formula_inner(store, &child, active)?);
                }
                Ok(if link == "and" {
                    Formula::And(parsed)
                } else {
                    Formula::Or(parsed)
                })
            }
            "iff" => {
                if iff.len() != 2 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} logic:iff requires exactly two operands; found {}",
                            iff.len()
                        ),
                    ));
                }
                let a = term_as_subject(&iff[0]).ok_or_else(|| {
                    formula_err(
                        node,
                        format!("logic:Formula {node_id} has a non-resource logic:iff operand"),
                    )
                })?;
                let b = term_as_subject(&iff[1]).ok_or_else(|| {
                    formula_err(
                        node,
                        format!("logic:Formula {node_id} has a non-resource logic:iff operand"),
                    )
                })?;
                Ok(Formula::Iff(
                    Box::new(parse_formula_inner(store, &a, active)?),
                    Box::new(parse_formula_inner(store, &b, active)?),
                ))
            }
            "antecedent/consequent" => {
                let a = one_child_subject(store, node, "antecedent")?;
                let c = one_child_subject(store, node, "consequent")?;
                Ok(Formula::Implies(
                    Box::new(parse_formula_inner(store, &a, active)?),
                    Box::new(parse_formula_inner(store, &c, active)?),
                ))
            }
            "forall" | "exists" => {
                let link = present[0];
                let body_node = one_child_subject(store, node, link)?;
                let vars = parse_bound_vars(store, node)?;
                let body = Box::new(parse_formula_inner(store, &body_node, active)?);
                Ok(if link == "forall" {
                    Formula::Forall { vars, body }
                } else {
                    Formula::Exists { vars, body }
                })
            }
            _ => unreachable!("constructor family was selected from a closed local array"),
        }
    })();

    let popped = active.pop();
    debug_assert_eq!(popped.as_deref(), Some(node_id.as_str()));
    result
}

/// Read an ordered argument list from `node`'s `logic:<link>` term-carriers (sorted by
/// `logic:termIndex`). Duplicate or gapped ordinals are malformed: RDF order must never become a
/// hidden fallback for the IR's explicit order.
fn parse_term_carriers(
    store: &RdfDataset,
    node: &Subject,
    link: &str,
    active: &mut Vec<String>,
) -> gmeow_errors::Result<Vec<Term>> {
    let mut indexed: Vec<(usize, Term)> = Vec::new();
    for carrier_term in formula_objects(store, node, link) {
        let carrier = term_as_subject(&carrier_term).ok_or_else(|| {
            formula_err(
                node,
                format!(
                    "logic:Formula {} has a non-resource logic:{link} carrier",
                    subject_str(node)
                ),
            )
        })?;
        let idx = parse_term_index(store, node, &carrier)?;
        indexed.push((idx, parse_term(store, node, &carrier, active)?));
    }
    indexed.sort_by_key(|(i, _)| *i);
    validate_contiguous_indices(&indexed, node, link)?;
    Ok(indexed.into_iter().map(|(_, t)| t).collect())
}

/// Read a quantifier's ordered bound-variable names from its `logic:quantifiedVariable`
/// term-carriers (sorted by `logic:termIndex`).
///
/// Returns an error if any carrier is malformed (unparsable `termIndex` or missing
/// `termVariable`) or if the binder is vacuous (zero bound variables) — a malformed
/// binder must surface as `MALFORMED_FORMULA`, never silently narrow `∀{x,y}` to `∀{x}`.
fn parse_bound_vars(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<Vec<String>> {
    let mut indexed: Vec<(usize, String)> = Vec::new();
    for carrier_term in formula_objects(store, node, "quantifiedVariable") {
        let carrier = term_as_subject(&carrier_term).ok_or_else(|| {
            formula_err(
                node,
                format!(
                    "logic:Formula {} has a non-resource logic:quantifiedVariable carrier",
                    subject_str(node)
                ),
            )
        })?;
        let idx = parse_term_index(store, node, &carrier)?;
        // A bound-variable carrier must resolve to a plain variable, so it never opens a
        // function-term recursion; a fresh cycle guard suffices.
        let term = parse_term(store, node, &carrier, &mut Vec::new())?;
        let Term::Var(name) = term else {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {} bound-variable carrier {} must contain exactly one logic:termVariable",
                    subject_str(node),
                    subject_str(&carrier)
                ),
            ));
        };
        indexed.push((idx, name));
    }
    if indexed.is_empty() {
        return Err(formula_err(
            node,
            format!(
                "logic:Formula {} quantifier requires at least one logic:quantifiedVariable",
                subject_str(node)
            ),
        ));
    }
    indexed.sort_by_key(|(i, _)| *i);
    validate_contiguous_indices(&indexed, node, "quantifiedVariable")?;
    Ok(indexed.into_iter().map(|(_, n)| n).collect())
}

/// Reconstruct a [`Term`] from a term-carrier node by its single term-value property.
fn parse_term(
    store: &RdfDataset,
    formula: &Subject,
    carrier: &Subject,
    active: &mut Vec<String>,
) -> gmeow_errors::Result<Term> {
    let fields = [
        "termIri",
        "termVariable",
        "termLiteral",
        "termSequenceMarker",
        "termApplication",
    ];
    let present: Vec<(&str, Vec<Node>)> = fields
        .iter()
        .map(|field| (*field, formula_objects(store, carrier, field)))
        .filter(|(_, values)| !values.is_empty())
        .collect();
    if present.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one term-value property; found {} ({})",
                subject_str(formula),
                subject_str(carrier),
                present.len(),
                present
                    .iter()
                    .map(|(field, _)| format!("logic:{field}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let (field, values) = &present[0];
    if values.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one logic:{field} value; found {}",
                subject_str(formula),
                subject_str(carrier),
                values.len()
            ),
        ));
    }
    let datatype_values = formula_objects(store, carrier, "termLiteralDatatype");
    if *field != "termLiteral" && !datatype_values.is_empty() {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} may carry logic:termLiteralDatatype only with logic:termLiteral",
                subject_str(formula),
                subject_str(carrier)
            ),
        ));
    }

    let value = &values[0];
    match *field {
        "termIri" => {
            let Node::Iri(iri) = value else {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires an IRI-valued logic:termIri",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            };
            Term::iri(iri.clone()).map_err(|e| formula_err(formula, e.message()))
        }
        "termVariable" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termVariable name",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            Term::var(term_str(value)).map_err(|e| formula_err(formula, e.message()))
        }
        "termLiteral" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termLiteral value",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            if datatype_values.len() > 1 {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} permits at most one logic:termLiteralDatatype; found {}",
                        subject_str(formula),
                        subject_str(carrier),
                        datatype_values.len()
                    ),
                ));
            }
            let datatype = match datatype_values.first() {
                Some(Node::Iri(iri)) => Some(iri.clone()),
                Some(_) => {
                    return Err(formula_err(
                        formula,
                        format!(
                            "logic:Formula {} logic:TermCarrier {} requires an IRI-valued logic:termLiteralDatatype",
                            subject_str(formula),
                            subject_str(carrier)
                        ),
                    ));
                }
                None => None,
            };
            Term::literal(term_str(value), datatype).map_err(|e| formula_err(formula, e.message()))
        }
        "termSequenceMarker" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termSequenceMarker name",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            Term::sequence_marker(term_str(value)).map_err(|e| formula_err(formula, e.message()))
        }
        "termApplication" => {
            let function_term = term_as_subject(value).ok_or_else(|| {
                formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a resource-valued logic:termApplication (a logic:FunctionTerm node)",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                )
            })?;
            parse_function_term(store, formula, &function_term, active)
        }
        _ => unreachable!("term value property was selected from a closed local array"),
    }
}

/// Reconstruct a [`Term::App`] from the `logic:FunctionTerm` node a `logic:termApplication`
/// carrier points at: its single reified `logic:functionSymbol` (an IRI-named `logic:Type`
/// individual, never a variable — keeping the object level first-order) applied to its ordered
/// `logic:argument` term-carriers. The argument carriers are read with the same
/// [`parse_term_carriers`] machinery the atomic-predication arguments use, so an argument may
/// itself be a `logic:termApplication` and a nested term like `cons(H, cons(1, nil))`
/// round-trips. `active` is the path of function-term nodes currently being expanded: a node
/// reached from its own expansion is a cycle (`cons` whose argument is `cons`) and is rejected
/// rather than recursed into forever.
fn parse_function_term(
    store: &RdfDataset,
    formula: &Subject,
    function_term: &Subject,
    active: &mut Vec<String>,
) -> gmeow_errors::Result<Term> {
    let node_id = subject_str(function_term);
    if active.contains(&node_id) {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} is cyclic: it appears within its own logic:argument expansion",
                subject_str(formula),
                node_id
            ),
        ));
    }

    let symbols = formula_objects(store, function_term, "functionSymbol");
    if symbols.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} requires exactly one logic:functionSymbol; found {}",
                subject_str(formula),
                node_id,
                symbols.len()
            ),
        ));
    }
    let Node::Iri(symbol) = &symbols[0] else {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} requires an IRI-valued logic:functionSymbol (the reified function symbol, never a variable)",
                subject_str(formula),
                node_id
            ),
        ));
    };

    active.push(node_id.clone());
    let args = parse_term_carriers(store, function_term, "argument", active);
    let popped = active.pop();
    debug_assert_eq!(popped.as_deref(), Some(node_id.as_str()));
    let args = args?;

    // `Term::app` rejects a nullary application (a 0-ary function symbol is a constant and
    // must be a logic:termIri), so a logic:FunctionTerm with no logic:argument fails here
    // rather than minting a second spelling for a constant.
    Term::app(symbol.clone(), args).map_err(|e| formula_err(formula, e.message()))
}

fn parse_term_index(
    store: &RdfDataset,
    formula: &Subject,
    carrier: &Subject,
) -> gmeow_errors::Result<usize> {
    let values = formula_objects(store, carrier, "termIndex");
    if values.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one logic:termIndex; found {}",
                subject_str(formula),
                subject_str(carrier),
                values.len()
            ),
        ));
    }
    term_str(&values[0]).parse::<usize>().map_err(|_| {
        formula_err(formula, format!(
            "logic:Formula {} logic:TermCarrier {} has an invalid non-negative integer logic:termIndex {:?}",
            subject_str(formula),
            subject_str(carrier),
            term_str(&values[0])
        ))
    })
}

fn validate_contiguous_indices<T>(
    indexed: &[(usize, T)],
    node: &Subject,
    link: &str,
) -> gmeow_errors::Result<()> {
    for (expected, (actual, _)) in indexed.iter().enumerate() {
        if *actual != expected {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {} logic:{link} indices must be unique and contiguous from zero; expected {expected}, found {actual}",
                    subject_str(node)
                ),
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------- //
// Reasoning-program extraction (`logic:ReasoningProgram`)
// --------------------------------------------------------------------------- //

/// The `logic:` sub-formula links a `logic:variableSort` declaration may be reached
/// through, mirroring [`FORMULA_SUBLINKS`] (the SAME structural links [`parse_formula`]
/// already validates). Walking these links again to harvest sort declarations — rather
/// than re-deriving term identity — keeps the sort harvest a read-only companion pass over
/// an already-validated tree, not a second clause reader.
const SORT_WALK_SUBFORMULA_LINKS: [&str; 8] = [
    "not",
    "and",
    "or",
    "antecedent",
    "consequent",
    "iff",
    "forall",
    "exists",
];

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
    for link in SORT_WALK_SUBFORMULA_LINKS {
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
        Term::Var(_) | Term::Literal { .. } | Term::SequenceMarker(_) => {}
    }
}

/// Reconstruct one [`ReasoningProgramIr`] rooted at a `logic:ReasoningProgram` node, or
/// return the reason it is malformed (surfaced as one `MALFORMED_REASONING_PROGRAM` error
/// diagnostic by [`extract_reasoning_programs`], which then skips the program). Reuses
/// [`parse_formula`] for every clause/query/probe root — there is no second clause reader —
/// and [`collect_variable_sorts`] to harvest each clause/query's `logic:variableSort`
/// declarations.
fn read_reasoning_program(
    store: &RdfDataset,
    node: &Subject,
) -> gmeow_errors::Result<ReasoningProgramIr> {
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
        clauses.push(parse_formula(store, clause_node)?);
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
    let query = parse_formula(store, &query_node)?;

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
        verdict_probes.push(parse_formula(store, probe_node)?);
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
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ReasoningProgramIr> {
    let program_ty = Node::iri(logic_iri("ReasoningProgram"));
    let mut programs = Vec::new();
    for subj in subjects_with(store, &nn(RDF_TYPE), &program_ty) {
        match read_reasoning_program(store, &subj) {
            Ok(p) => programs.push(p),
            Err(err) => diagnostics.push(Diagnostic::error(
                "MALFORMED_REASONING_PROGRAM",
                err.message().to_owned(),
                Some(subject_str(&subj)),
            )),
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
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Correspondence> {
    let (correspondences, errors) =
        crate::projections::correspondence::extract_correspondences(store);
    for (iri, message) in errors {
        diagnostics.push(Diagnostic::warning(
            "MALFORMED_CORRESPONDENCE",
            message,
            Some(iri),
        ));
    }
    correspondences
}

/// Read every authored `logic:Constraint` individual into a [`ConstraintIr`] — the typed
/// home for a closed-world procedural integrity condition (whose violation is a finding).
/// Fail-soft like the other extractors: a malformed constraint emits a
/// `MALFORMED_CONSTRAINT` warning and is skipped, never silently dropped.
///
/// A constraint node carries its integrity condition via `logic:integrity` (a top-level
/// [`Formula`] tree, reconstructed by the shared [`parse_formula`] reader — no duplicated
/// formula parsing), its `sh:severity` via a `logic:severity "Violation"|"Warning"|"Info"`
/// literal (absent ⇒ the SHACL default `Violation`), an optional advisory `logic:message`
/// literal, and an optional `logic:formalizes` gmeow-domain back-reference. The focus
/// [`ShapeTarget`] is DERIVED from the integrity's `∀` guard by [`ConstraintIr::new`], which
/// hard-fails (⇒ a `MALFORMED_CONSTRAINT` warning) on a non-range-restricted formula.
///
/// A constraint-free source yields an empty vector, and `with_constraints(vec![])` is a no-op
/// in [`LogicProgram::canonical_key`] (the segment is append-only) — so adding this stage to
/// every parse leaves every existing artifact byte-identical.
fn extract_constraints(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
    malformed_formulas: &BTreeSet<String>,
) -> Vec<ConstraintIr> {
    let constraint_ty = Node::iri(logic_iri("Constraint"));
    let mut constraints: Vec<ConstraintIr> = Vec::new();
    for subj in subjects_with(store, &nn(RDF_TYPE), &constraint_ty) {
        match read_constraint(store, &subj) {
            Ok(c) => constraints.push(c),
            Err(err) => {
                let focus = err
                    .inner()
                    .source_ctx
                    .focus
                    .as_ref()
                    .map(|focus| focus.0.as_str());
                if focus.is_some_and(|focus| malformed_formulas.contains(focus)) {
                    continue;
                }
                diagnostics.push(Diagnostic::warning(
                    "MALFORMED_CONSTRAINT",
                    err.message().to_owned(),
                    Some(subject_str(&subj)),
                ));
            }
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
pub fn extract_all_constraints(store: &RdfDataset) -> (Vec<ConstraintIr>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let malformed_formulas = extract_formulas(store, &mut diagnostics).malformed;
    let mut constraints = extract_constraints(store, &mut diagnostics, &malformed_formulas);
    constraints.extend(extract_sugar_constraints(store, &mut diagnostics));
    (constraints, diagnostics)
}

/// Build a `MALFORMED_CONSTRAINT`-grade frontend [`Diag`] from a message. The constraint-sugar
/// readers surface a fail-soft reason (rendered as one warning) as the structured [`Diag`] that is
/// the sole first-party error type (the Phase-6 Diag substrate); a malformed record degrades to a
/// warning rather than aborting the parse.
fn sugar_err(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Frontend {
        detail: detail.into(),
    })
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

/// Reconstruct one [`ConstraintIr`] rooted at a `logic:Constraint` node, or return a
/// human-readable reason the constraint is malformed (surfaced as one `MALFORMED_CONSTRAINT`
/// warning by [`extract_constraints`]).
fn read_constraint(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ConstraintIr> {
    let iri = subject_str(node);
    // Validate constraint-owned metadata before parsing the integrity tree. If both the formula
    // and an independent constraint annotation are malformed, each receives its own authoritative
    // diagnostic instead of the formula failure masking the constraint defect.
    // Absent severity ⇒ the SHACL default `Violation`; an unrecognized token is a hard error.
    let severity = match value(store, node, &nn(&logic_iri("severity"))) {
        Some(t) => {
            let token = term_str(&t);
            ShaclSeverity::from_local(&token).ok_or_else(|| {
                sugar_err(format!(
                    "logic:severity '{token}' is not one of Violation/Warning/Info"
                ))
                .with_focus(iri.clone())
            })?
        }
        None => ShaclSeverity::Violation,
    };
    let message = value(store, node, &nn(&logic_iri("message"))).map(|t| term_str(&t));
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
    let integrity = parse_formula(store, &integrity_node)?;

    let mut constraint = ConstraintIr::new(&iri, integrity, severity, message)
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

/// The `logic:severity` of a sugar record (absent ⇒ the SHACL default `Violation`).
fn sugar_severity(store: &RdfDataset, node: &Subject) -> gmeow_errors::Result<ShaclSeverity> {
    match value(store, node, &nn(&logic_iri("severity"))) {
        Some(t) => {
            let token = term_str(&t);
            ShaclSeverity::from_local(&token).ok_or_else(|| {
                sugar_err(format!(
                    "logic:severity '{token}' is not one of Violation/Warning/Info"
                ))
            })
        }
        None => Ok(ShaclSeverity::Violation),
    }
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
    let message = value(store, node, &nn(&logic_iri("message"))).map(|t| term_str(&t));
    let formalizes_all = sugar_iri_list(store, node, "formalizes");
    let (primary, rest) = formalizes_all.split_first().ok_or_else(|| {
        sugar_err(
            "constraint-sugar record requires logic:formalizes (the gmeow term it formalizes)",
        )
    })?;
    let mut constraint = ConstraintIr::new(subject_str(node), integrity, severity, message)?
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
    let mode = value(store, node, &nn(&logic_iri("choiceMode")))
        .map(|t| term_str(&t).trim().to_ascii_lowercase())
        .ok_or_else(|| sugar_err("logic:ChoiceGroupConstraint requires logic:choiceMode"))?;
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
                Node::Lit {
                    lexical, datatype, ..
                } => Term::literal(lexical.clone(), datatype.clone())?,
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
                Some(Node::Lit {
                    lexical, datatype, ..
                }) => Term::literal(lexical, datatype)?,
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
    let mode = value(store, node, &nn(&logic_iri("crossMode")))
        .map(|t| term_str(&t).trim().to_ascii_lowercase())
        .ok_or_else(|| sugar_err("logic:CrossNodeConstraint requires logic:crossMode"))?;
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
) -> gmeow_errors::Result<ConstraintIr> {
    let class = sugar_target_class(store, node)?;
    let forbidden = value(store, node, &nn(&logic_iri("forbiddenPredicate")))
        .map(|t| term_str(&t))
        .ok_or_else(|| {
            sugar_err("logic:ForbiddenPatternConstraint requires logic:forbiddenPredicate")
        })?;
    let consequent = match value(store, node, &nn(&logic_iri("forbiddenValue"))) {
        Some(v) => {
            let obj = match &v {
                Node::Iri(i) => Term::iri(i)?,
                Node::Lit {
                    lexical, datatype, ..
                } => Term::literal(lexical.clone(), datatype.clone())?,
                _ => {
                    return Err(sugar_err(
                        "logic:forbiddenValue must be an IRI or a literal",
                    ));
                }
            };
            Formula::Not(Box::new(f_atom2(&forbidden, t_var("this"), obj)?))
        }
        None => Formula::Not(Box::new(f_pred_exists(&forbidden, "b")?)),
    };
    finalize_sugar(
        store,
        node,
        f_forall_this(f_guard_class(&class)?, consequent),
    )
}

/// P6 / aggregate — an aggregate-comparison constraint: a target class + an aggregate function
/// over a path + a comparator + a right-hand side (a compared property of the focus, or a
/// literal). Expands to a [`ConstraintIr`] carrying BOTH the honest reified FOL integrity (so the
/// Convert an authored object [`Node`] (IRI or literal) to a FOL [`Term`], preserving the
/// distinction so a set member / string pattern round-trips as the right SPARQL token.
fn node_to_term(n: &Node) -> gmeow_errors::Result<Term> {
    match n {
        Node::Iri(i) => Term::iri(i),
        Node::Lit {
            lexical, datatype, ..
        } => Term::literal(lexical.clone(), datatype.clone()),
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
        Some(Node::Lit { lexical, .. }) => lexical,
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
    let function = value(store, node, &nn(&logic_iri("aggFunction")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:AggregateConstraint requires logic:aggFunction"))?;
    let path = value(store, node, &nn(&logic_iri("aggPath")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:AggregateConstraint requires logic:aggPath"))?;
    let comparator = value(store, node, &nn(&logic_iri("aggComparator")))
        .map(|t| term_str(&t))
        .and_then(|s| AggregateComparator::from_symbol(&s))
        .ok_or_else(|| {
            sugar_err("logic:AggregateConstraint requires a logic:aggComparator in =/!=/</<=/>/>=")
        })?;
    let distinct = value(store, node, &nn(&logic_iri("aggDistinct")))
        .map(|t| term_str(&t).trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // The right-hand side: an IRI object is a compared PROPERTY of the focus; a literal is a fixed
    // comparison value.
    let compare_to = match value(store, node, &nn(&logic_iri("aggCompareTo"))) {
        Some(Node::Iri(p)) => AggregateRhs::Property(p),
        Some(Node::Lit {
            lexical, datatype, ..
        }) => AggregateRhs::Literal { lexical, datatype },
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
    let function = value(store, node, &nn(&logic_iri("aggFunction")))
        .map(|t| term_str(&t))
        .ok_or_else(|| sugar_err("logic:JoinAggregateConstraint requires logic:aggFunction"))?;
    let comparator = value(store, node, &nn(&logic_iri("aggComparator")))
        .map(|t| term_str(&t))
        .and_then(|s| AggregateComparator::from_symbol(&s))
        .ok_or_else(|| {
            sugar_err(
                "logic:JoinAggregateConstraint requires a logic:aggComparator in =/!=/</<=/>/>=",
            )
        })?;
    let (threshold_lexical, threshold_datatype) = match value(
        store,
        node,
        &nn(&logic_iri("aggThreshold")),
    ) {
        Some(Node::Lit {
            lexical, datatype, ..
        }) => (lexical, datatype),
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
    match kind.trim() {
        "IRI" | "iri" => Some("termIsIri"),
        "Literal" | "literal" => Some("termIsLiteral"),
        "BlankNodeOrIRI" | "BlankNodeOrIri" | "blankNodeOrIri" => Some("termIsBlankOrIri"),
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
            Some(Node::Lit {
                lexical, datatype, ..
            }) => Ok(Some(Term::literal(lexical, datatype)?)),
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
/// [`ConstraintIr`]s. Fail-soft like the other extractors: a malformed record emits a
/// `MALFORMED_CONSTRAINT` warning and is skipped, never silently dropped. A sugar-free source
/// yields an empty vector (append-only, so the canonical key stays byte-identical).
fn extract_sugar_constraints(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ConstraintIr> {
    type Reader = fn(&RdfDataset, &Subject) -> gmeow_errors::Result<ConstraintIr>;
    let readers: [(&str, Reader); 18] = [
        ("ChoiceGroupConstraint", read_choice_group),
        ("GuardedImplicationConstraint", read_guarded_implication),
        (
            "DisjunctiveRequirednessConstraint",
            read_disjunctive_requiredness,
        ),
        ("PathValueTypeConstraint", read_path_value_type),
        ("CrossNodeConstraint", read_cross_node),
        ("ForbiddenPatternConstraint", read_forbidden_pattern),
        ("ValueRangeConstraint", read_value_range),
        ("AggregateConstraint", read_aggregate_constraint),
        ("JoinAggregateConstraint", read_join_aggregate_constraint),
        (
            "AggregateBalanceConstraint",
            read_aggregate_balance_constraint,
        ),
        ("ComparisonConstraint", read_comparison),
        ("PathNodeKindConstraint", read_path_node_kind),
        ("SelfJoinUniquenessConstraint", read_self_join_uniqueness),
        ("InverseExistenceConstraint", read_inverse_existence),
        (
            "TransitiveReachabilityConstraint",
            read_transitive_reachability,
        ),
        ("AcyclicConstraint", read_acyclic),
        ("ValueSetMembershipConstraint", read_value_set_membership),
        ("StringPatternConstraint", read_string_pattern),
    ];
    let mut out: Vec<ConstraintIr> = Vec::new();
    for (class_local, reader) in readers {
        let class_ty = Node::iri(logic_iri(class_local));
        for subj in subjects_with(store, &nn(RDF_TYPE), &class_ty) {
            match reader(store, &subj) {
                Ok(c) => out.push(c),
                Err(err) => diagnostics.push(Diagnostic::warning(
                    "MALFORMED_CONSTRAINT",
                    err.message().to_owned(),
                    Some(subject_str(&subj)),
                )),
            }
        }
    }
    out
}

/// Parse a `logic:` RDF source already loaded into a wasm-clean [`RdfDataset`]
/// (default graph) into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_dataset(
    dataset: &RdfDataset,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if is_empty(dataset) {
        return Err(LogicParseError(
            "Source graph is empty — nothing to parse.  Pass a non-empty graph or a \
             Turtle file with logic: triples."
                .to_owned(),
        ));
    }

    // Re-label blank nodes to their RDFC-1.0 canonical ids BEFORE extraction, so
    // every projection (text back-ends included) is a deterministic function of the
    // source graph rather than the parser's random per-parse blank-node labels.
    let canon =
        canonicalize_blank_nodes(dataset).map_err(|e| LogicParseError(e.message().to_owned()))?;
    let store = canon.as_ref();

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let plain_axioms = extract_axioms(store, &mut diagnostics);
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
        formulas: extracted_formulas,
        malformed: malformed_formulas,
    } = extract_formulas(store, &mut diagnostics);
    let mut formulas: Vec<Formula> = Vec::new();
    let mut horn_axioms: Vec<LogicAxiom> = Vec::new();
    for f in extracted_formulas {
        if f.is_trivially_horn() {
            match f.as_horn_axiom() {
                Some(ax) => horn_axioms.push(ax),
                None => diagnostics.push(Diagnostic::error(
                    "MALFORMED_FORMULA",
                    "trivially-Horn logic:Formula has a non-subject term (a literal or sequence \
                     marker) in argument position 0; it is neither a formula nor a fact; skipped",
                    None,
                )),
            }
        } else {
            formulas.push(f);
        }
    }

    // Merge + dedup by full content (mirrors the Python `set(...) | set(...)`),
    // preserving first-occurrence order (plain, then scoped, then the routed Horn leaves) for
    // deterministic tie-breaking; `LogicProgram::new` then sorts canonically.
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_axioms: Vec<LogicAxiom> = Vec::new();
    for ax in plain_axioms
        .into_iter()
        .chain(scoped_axioms)
        .chain(horn_axioms)
        .chain(annotation_axioms)
    {
        if seen.insert(content_dedup_key(&ax)) {
            all_axioms.push(ax);
        }
    }

    let contracts = extract_contracts(store, &mut diagnostics);
    let rules = extract_rules(store, &mut diagnostics);
    let path_shapes = extract_path_shapes(store, &mut diagnostics);
    let correspondences = extract_correspondences(store, &mut diagnostics);
    // Resolve each leg IRI to its `gm:path` body so the round-trip gate can compose the
    // actual leg paths (not IRI strings). Leg-body-free corpora yield an empty registry and
    // leave the canonical key byte-identical (the segment is append-only).
    let transaction_programs =
        crate::projections::correspondence::extract_leg_programs(store, &correspondences);

    // Authored `logic:Constraint` individuals PLUS the compact constraint-sugar records (P1–P5,
    // P7, and the aggregate P6), each expanded to one canonical `ConstraintIr`. Both feed the same
    // `LogicProgram::constraints` home (which sorts + content-keys them canonically).
    let mut constraints = extract_constraints(store, &mut diagnostics, &malformed_formulas);
    constraints.extend(extract_sugar_constraints(store, &mut diagnostics));

    // Authored `logic:ReasoningProgram` clause-set-plus-goal individuals. A structurally
    // malformed program emits an error-grade MALFORMED_REASONING_PROGRAM diagnostic above
    // and is excluded here; every other authored program still compiles.
    let reasoning_programs = extract_reasoning_programs(store, &mut diagnostics);

    let program = LogicProgram::new(all_axioms, rules, contracts, source_iri)
        .with_path_shapes(path_shapes)
        .with_correspondences(correspondences)
        .map_err(|e| LogicParseError(e.message().to_owned()))?
        .with_transaction_programs(transaction_programs)
        .with_formulas(formulas)
        .with_constraints(constraints)
        .with_reasoning_programs(reasoning_programs);
    Ok((program, diagnostics))
}

/// Parse Turtle source text into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_str(
    turtle: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    // Native codec parse → frozen wasm-clean IR dataset, straight into the parser
    // (no oxigraph Store hop).
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None)
        .map_err(|e| LogicParseError(format!("Failed to parse Turtle source: {e}")))?;
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
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if !path.exists() {
        return Err(LogicParseError(format!(
            "Source file does not exist: {}",
            path.display()
        )));
    }
    let source_iri = source_iri.or_else(|| path_to_file_uri(path));
    let turtle = std::fs::read_to_string(path)
        .map_err(|e| LogicParseError(format!("Failed to read {}: {e}", path.display())))?;
    parse_logic_str(&turtle, source_iri)
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
