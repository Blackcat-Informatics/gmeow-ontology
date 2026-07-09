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

use std::collections::{BTreeMap, HashSet};
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
    AggregateSpec, ComplexityClass, ConstraintComponent, ConstraintProvenance, ContextualScope,
    Correspondence, Formula, LOGIC_NAMESPACE, LogicAxiom, LogicModality, LogicProgram, LogicRule,
    PathBase, PathShapeIr, PropertyConstraintIr, ReasoningContract, SemanticProfileId,
    ShaclNodeKind, ShapeTarget, ShapeValue, Term, ValidationShapeIr,
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
    /// A hard error (currently unused by the fail-soft parser; reserved).
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
    if let Node::Lit(lexical) = term
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

fn extract_axioms(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();

    // Meta-config subjects (contracts / presets / closure entries): facet-config
    // triples on these are contract configuration, not domain facts.
    let config_subjects = collect_contract_config_subjects(store);

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
        // Rule-aggregation triples (the reduce spec carried on a logic:Rule node) are consumed
        // by extract_rules; they are rule structure, never domain facts, and must not pollute
        // the axiom set or the canonical round-trip.
        if is_rule_aggregation_predicate(p_local) {
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
/// with the dataset-derive architecture.
fn closure_validation_optouts(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    closure_keys_with_value(store, "OpenWorldClosure")
}

/// The `logic:closureKey` set of every `logic:closureEntry` whose `logic:closureValue` is the
/// closure-value individual named by `value_local`. The `closureKey` predicate node is built
/// once and reused across entries (not re-minted per iteration).
fn closure_keys_with_value(
    store: &RdfDataset,
    value_local: &str,
) -> std::collections::BTreeSet<String> {
    let target = Node::iri(logic_iri(value_local));
    let key_pred = nn(&logic_iri("closureKey"));
    let mut set = std::collections::BTreeSet::new();
    for entry in subjects_with(store, &nn(&logic_iri("closureValue")), &target) {
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
/// architecture.
fn closure_validation_closed_optins(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    closure_keys_with_value(store, "ClosedWorldClosure")
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
    let p_mincard = nn(&format!("{owl}minCardinality"));
    let p_maxcard = nn(&format!("{owl}maxCardinality"));
    let p_card = nn(&format!("{owl}cardinality"));
    let p_qmincard = nn(&format!("{owl}minQualifiedCardinality"));
    let p_qmaxcard = nn(&format!("{owl}maxQualifiedCardinality"));
    let p_qcard = nn(&format!("{owl}qualifiedCardinality"));
    let p_subclass = nn(&format!("{rdfs}subClassOf"));
    let p_disjoint = nn(&format!("{owl}disjointWith"));
    let p_complement = nn(&format!("{owl}complementOf"));
    let p_oneof = nn(&format!("{owl}oneOf"));
    let p_members = nn(&format!("{owl}members"));
    let p_domain = nn(&format!("{rdfs}domain"));
    let p_range = nn(&format!("{rdfs}range"));
    let owl_alldisjoint = Node::iri(format!("{owl}AllDisjointClasses"));

    // GMEOW is the authoring ground: derive validation shapes only for our own domain
    // classes / properties (Principle 4 / maximal dogfooding). Imported ontologies (gUFO,
    // FOAF, …) are linked, not validated by our surface — and their namespaces are not
    // registered in the downstream JSON-Schema discriminator. The TARGET of an `sh:class`
    // may live in any namespace; only the SHAPE-owning class / property must be GMEOW-NS.
    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

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

    // The closed-world reading of a faceted datatype filler
    // (`[ a rdfs:Datatype ; owl:onDatatype xsd:string ; owl:withRestrictions ( [ xsd:pattern "…" ]
    // [ xsd:minLength "…" ] … ) ]`): the base datatype plus each XSD length / pattern facet, as
    // SHACL components (`sh:datatype` + `sh:pattern` / `sh:minLength` / `sh:maxLength`). Returns an
    // empty vector for a blank node that carries no `owl:withRestrictions` (an anonymous class
    // expression, not a datatype facet) — the caller then skips it, unchanged.
    let p_ondatatype = nn(&format!("{owl}onDatatype"));
    let p_withrestrictions = nn(&format!("{owl}withRestrictions"));
    let xsd_pattern = nn(&format!("{xsd}pattern"));
    let xsd_minlength = nn(&format!("{xsd}minLength"));
    let xsd_maxlength = nn(&format!("{xsd}maxLength"));
    let xsd_mininclusive = nn(&format!("{xsd}minInclusive"));
    let xsd_maxinclusive = nn(&format!("{xsd}maxInclusive"));
    let xsd_minexclusive = nn(&format!("{xsd}minExclusive"));
    let xsd_maxexclusive = nn(&format!("{xsd}maxExclusive"));
    let datatype_facets = |filler: &Subject| -> Vec<ConstraintComponent> {
        let Some(list_head) = value(store, filler, &p_withrestrictions) else {
            return Vec::new();
        };
        let mut comps = Vec::new();
        if let Some(Node::Iri(dt)) = value(store, filler, &p_ondatatype) {
            comps.push(ConstraintComponent::Datatype(dt));
        }
        // The numeric bound facets accumulate across the whole restriction list into ONE
        // `NumericRange` (a `[ xsd:minInclusive … ] [ xsd:maxExclusive … ]` list is one interval,
        // not two components). The inclusive facet wins over the exclusive one if a malformed list
        // carries both bounds on the same side; the last-read value wins for a duplicated facet.
        let mut range_min: Option<f64> = None;
        let mut range_max: Option<f64> = None;
        let mut min_inclusive = true;
        let mut max_inclusive = true;
        for facet in read_list_member_subjects(store, &list_head) {
            if let Some(Node::Lit(regex)) = value(store, &facet, &xsd_pattern) {
                comps.push(ConstraintComponent::Pattern { regex, flags: None });
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_minlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MinLength(n));
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_maxlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MaxLength(n));
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_mininclusive)
                && let Ok(v) = n.trim().parse::<f64>()
            {
                range_min = Some(v);
                min_inclusive = true;
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_minexclusive)
                && let Ok(v) = n.trim().parse::<f64>()
            {
                range_min = Some(v);
                min_inclusive = false;
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_maxinclusive)
                && let Ok(v) = n.trim().parse::<f64>()
            {
                range_max = Some(v);
                max_inclusive = true;
            }
            if let Some(Node::Lit(n)) = value(store, &facet, &xsd_maxexclusive)
                && let Ok(v) = n.trim().parse::<f64>()
            {
                range_max = Some(v);
                max_inclusive = false;
            }
        }
        if range_min.is_some() || range_max.is_some() {
            comps.push(ConstraintComponent::NumericRange {
                min: range_min,
                max: range_max,
                min_inclusive,
                max_inclusive,
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
    let classify_filler = |fs: &Subject| -> Option<ConstraintComponent> {
        if let Some(head) = value(store, fs, &p_unionof) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Or(branches));
            }
        }
        if let Some(head) = value(store, fs, &p_disjointunion) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Xone(branches));
            }
        }
        match value(store, fs, &p_complement) {
            Some(Node::Iri(d)) => classify(&d).map(|cc| ConstraintComponent::Not(Box::new(cc))),
            Some(inner @ Node::Blank { .. }) => {
                let bs = term_as_subject(&inner)?;
                let sv = match value(store, &bs, &p_hasvalue)? {
                    Node::Iri(i) => ShapeValue::Iri(i),
                    Node::Lit(lex) => ShapeValue::Literal {
                        lexical: lex,
                        datatype: None,
                        lang: None,
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
            Some(Node::Lit(lex)) => lex.trim().parse::<u32>().ok(),
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
        };
        acc.entry(iri)
            .or_insert_with(|| (target, Vec::new(), Vec::new()))
    }

    // ── FAMILY 1 — per-class restriction walk (Class(C) target) ───────────────────────────
    let classes = subjects_with(store, &nn(RDF_TYPE), &owl_class);
    for class in &classes {
        // An anonymous class expression (blank node) is not a shape target — skip it.
        if subject_is_blank(class) {
            continue;
        }
        let class_iri = subject_str(class);
        if !class_iri.starts_with(GMEOW_NS) || optouts.contains(&class_iri) {
            continue;
        }
        for restr in objects(store, class, &p_subclass) {
            let Some(restr_subj) = term_as_subject(&restr) else {
                continue;
            };
            // A restriction constrains exactly one property; skip a malformed one with no
            // IRI-valued `owl:onProperty`.
            let Some(Node::Iri(on)) = value(store, &restr_subj, &p_on) else {
                continue;
            };
            // Per-property validation-reading opt-out (R3).
            if optouts.contains(&on) {
                continue;
            }

            // owl:someValuesFrom is EXISTENTIAL ("K ⊑ ∃P.C"). Its FAITHFUL closed-world reading
            // (`sh:qualifiedValueShape [ <inner> ] ; sh:qualifiedMinCount 1`) would demand the
            // value EXIST — but a validation shape is a `logic:ValidationOnly` under-approximation
            // that must never over-claim (LOGIC-VALIDATION.md, "Where the loss is"), and an
            // existential over-claims: it false-positives on the ontology's own open-world
            // value-vocabulary individuals / fixtures, which are instances of a restricted class
            // yet legitimately do not populate the mediated relation. So the shape projects the
            // class-membership under-approximation ("any value present on P must be a C" — vacuously
            // true when absent, identical to the `allValuesFrom` projection); the existential
            // EXISTENCE obligation is carried in the canonical logic: layer, not the shape. A blank
            // filler is an anonymous class expression (union/intersection), carried in the canon,
            // never a bare blank shape — skip it.
            match value(store, &restr_subj, &p_some) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify(&cv) {
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
            match value(store, &restr_subj, &p_all) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify(&cv) {
                        let pc = PropertyConstraintIr::new(&on, None, None, None, vec![cc])?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
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

            // owl:hasValue → `sh:hasValue` (a fixed required value). A blank / quoted-triple
            // fixed value is impossible (a fixed value cannot be an anonymous node) → hard-fail.
            match value(store, &restr_subj, &p_hasvalue) {
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
                Some(Node::Lit(lexical)) => {
                    let pc = PropertyConstraintIr::new(
                        &on,
                        None,
                        None,
                        None,
                        vec![ConstraintComponent::HasValue(ShapeValue::Literal {
                            lexical,
                            datatype: None,
                            lang: None,
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
            let has_qcard = value(store, &restr_subj, &p_qcard).is_some()
                || value(store, &restr_subj, &p_qmincard).is_some()
                || value(store, &restr_subj, &p_qmaxcard).is_some();
            if has_qcard {
                let q_exact = card_of(&restr_subj, &p_qcard);
                let (mut qlo, mut qhi) = (
                    card_of(&restr_subj, &p_qmincard),
                    card_of(&restr_subj, &p_qmaxcard),
                );
                if let Some(n) = q_exact {
                    qlo = Some(n);
                    qhi = Some(n);
                }
                match value(store, &restr_subj, &p_onclass) {
                    // A qualified cardinality REQUIRES its qualifying class — absent → hard-fail.
                    None => {
                        return Err(Diag::of_kind(crate::error::Frontend {
                            detail: format!("qualified cardinality on {on} requires owl:onClass"),
                        }));
                    }
                    // An anonymous qualifying class expression is carried in the canon, never a
                    // bare blank shape — skip (do not emit).
                    Some(Node::Blank { .. }) | Some(Node::Lit(_)) | Some(Node::Triple(_)) => {}
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
                        let pc = PropertyConstraintIr::new(
                            &on,
                            None,
                            None,
                            None,
                            vec![ConstraintComponent::QualifiedValueShape {
                                shape: vec![ConstraintComponent::Class(q)],
                                min: qlo,
                                max: qhi,
                            }],
                        )?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
            } else {
                // Unqualified cardinality → sh:minCount / sh:maxCount with OwlRestriction
                // provenance (the open-world axiom read closed-world).
                let exact = card_of(&restr_subj, &p_card);
                let (mut lo, mut hi) = (
                    card_of(&restr_subj, &p_mincard),
                    card_of(&restr_subj, &p_maxcard),
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
        if !class_iri.starts_with(GMEOW_NS) || optouts.contains(&class_iri) {
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
            if !ci.starts_with(GMEOW_NS) || optouts.contains(ci) {
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
    // Collect every GMEOW-NS property: the four OWL property-type declarations plus any subject
    // of rdfs:domain / rdfs:range.
    let mut props: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for ty in [
        "ObjectProperty",
        "DatatypeProperty",
        "FunctionalProperty",
        "InverseFunctionalProperty",
    ] {
        for s in subjects_with(store, &nn(RDF_TYPE), &Node::iri(format!("{owl}{ty}"))) {
            if let Subject::Iri(iri) = &s
                && iri.starts_with(GMEOW_NS)
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
            && iri.starts_with(GMEOW_NS)
        {
            props.insert(iri.clone());
        }
    }

    let functional = Node::iri(format!("{owl}FunctionalProperty"));
    let inverse_functional = Node::iri(format!("{owl}InverseFunctionalProperty"));
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
            // rdfs:range C → an ObjectsOf(P) node condition (every object of P satisfies it).
            for c in objects(store, &p_subj, &p_range) {
                if let Node::Iri(c) = c
                    && let Some(cc) = classify(&c)
                {
                    entry_for(&mut acc, ShapeTarget::ObjectsOf(p.clone()))
                        .1
                        .push(cc);
                }
            }
        }
        // owl:FunctionalProperty → each subject of P has ≤1 value (sh:maxCount 1 on P). A
        // functional/inverse-functional axiom is a genuine closed-world cardinality bound (it
        // constrains, it does not merely infer), so it stays derive-all (+ the OpenWorldClosure
        // opt-out), independent of the domain/range opt-in above.
        if contains(store, &p_subj, &nn(RDF_TYPE), &functional) {
            let pc = PropertyConstraintIr::new(
                p,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?;
            entry_for(&mut acc, ShapeTarget::SubjectsOf(p.clone()))
                .2
                .push(pc);
        }
        // owl:InverseFunctionalProperty → each object of P has ≤1 subject (inverse sh:maxCount 1).
        if contains(store, &p_subj, &nn(RDF_TYPE), &inverse_functional) {
            let pc = PropertyConstraintIr::new(
                p,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?
            .inverted();
            entry_for(&mut acc, ShapeTarget::ObjectsOf(p.clone()))
                .2
                .push(pc);
        }
    }

    // ── FAMILY 4 — owl:hasKey (single-property keys → inverse-functional reading) ──────────
    // `K owl:hasKey ( P )` says the value of P uniquely identifies the K instance — the DL 2
    // way to state a datatype/single-property key (an `owl:InverseFunctionalProperty` on a
    // datatype property would be OWL 2 Full). Its closed-world reading is the same inverse
    // `sh:maxCount 1` (each P-value has ≤1 subject via P) the InverseFunctionalProperty arm
    // emits. A COMPOSITE key (`owl:hasKey ( P1 P2 … )`) asserts the TUPLE is unique, not each
    // part — it has no single-path SHACL form, so it is carried in the canon and no shape is
    // derived (never a wrong per-part uniqueness claim).
    let haskey_iri = format!("{owl}hasKey");
    let p_haskey = nn(&haskey_iri);
    let mut haskey_classes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for q in default_graph_quads(store) {
        if q.predicate.as_str() == haskey_iri
            && let Subject::Iri(iri) = &q.subject
            && iri.starts_with(GMEOW_NS)
        {
            haskey_classes.insert(iri.clone());
        }
    }
    for class_iri in &haskey_classes {
        let k = Subject::Iri(class_iri.clone());
        let Some(list_head) = value(store, &k, &p_haskey) else {
            continue;
        };
        let keys = read_iri_list(store, &list_head);
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
        if !k.starts_with(GMEOW_NS) || !p.starts_with(GMEOW_NS) {
            continue;
        }
        // The reifier's GMEOW type `C` names the shape the reifier must conform to. An untyped
        // reifier carries no shape reference (a `sh:reifierShape` with no resolvable target would
        // dangle), so the typed-reifier form is required — an untyped one is not an obligation.
        let Some(c) = objects(store, &r, &nn(RDF_TYPE))
            .into_iter()
            .find_map(|t| match t {
                Node::Iri(i) if i.starts_with(GMEOW_NS) => Some(i),
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
        if !key_pred.starts_with(GMEOW_NS) || optouts.contains(&key_pred) {
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
                && let Some(cc) = classify(&cv)
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
        let shape = ValidationShapeIr::new(iri, target, properties, None)?
            .with_node_components(node_components)?;
        shapes.push(shape);
    }
    Ok(shapes)
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

/// Read top-level `logic:Formula` trees into [`Formula`]s.  Fail-soft, like the rest of
/// the front-end: a malformed node emits a `MALFORMED_FORMULA` warning and is skipped,
/// never silently dropped.  A returned formula MAY be trivially-Horn (a reified ordinary
/// triple) — the caller ([`parse_logic_dataset`]) partitions those out to
/// [`LogicProgram::axioms`] via [`Formula::as_horn_axiom`] so the `with_formulas` invariant
/// is enforced by routing, not by assuming the projection never authors one.
fn extract_formulas(store: &RdfDataset, diagnostics: &mut Vec<Diagnostic>) -> Vec<Formula> {
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

    let mut formulas: Vec<Formula> = Vec::new();
    for subj in &subjects {
        if referenced.contains(&subject_str(subj)) {
            continue;
        }
        match parse_formula(store, subj) {
            Some(f) => formulas.push(f),
            None => diagnostics.push(Diagnostic::warning(
                "MALFORMED_FORMULA",
                "logic:Formula node could not be reconstructed; skipped",
                Some(subject_str(subj)),
            )),
        }
    }
    formulas
}

/// The single child formula reached by `link` from `node` (or `None`).
fn child_subject(store: &RdfDataset, node: &Subject, link: &str) -> Option<Subject> {
    value(store, node, &nn(&logic_iri(link))).and_then(|t| term_as_subject(&t))
}

/// Every child formula reached by `link` from `node`.
fn child_subjects(store: &RdfDataset, node: &Subject, link: &str) -> Vec<Subject> {
    objects(store, node, &nn(&logic_iri(link)))
        .iter()
        .filter_map(term_as_subject)
        .collect()
}

/// Recursively reconstruct a [`Formula`] rooted at `node`, branching on which structural
/// property family is present.  Returns `None` on a malformed node; the top-level
/// [`extract_formulas`] emits one `MALFORMED_FORMULA` warning per failed formula.
fn parse_formula(store: &RdfDataset, node: &Subject) -> Option<Formula> {
    // Atomic predication.
    if let Some(rel) = value(store, node, &nn(&logic_iri("relation"))) {
        let relation = Term::iri(term_str(&rel)).ok()?;
        let args = parse_term_carriers(store, node, "argument")?;
        return Formula::atom(relation, args).ok();
    }
    // Strong negation.
    if let Some(child) = child_subject(store, node, "not") {
        return Some(Formula::Not(Box::new(parse_formula(store, &child)?)));
    }
    // Conjunction / disjunction (commutative; operand order is immaterial to identity).
    for link in ["and", "or"] {
        let children = child_subjects(store, node, link);
        if !children.is_empty() {
            let parsed: Option<Vec<Formula>> =
                children.iter().map(|c| parse_formula(store, c)).collect();
            let parsed = parsed?;
            return Some(if link == "and" {
                Formula::And(parsed)
            } else {
                Formula::Or(parsed)
            });
        }
    }
    // Biconditional (exactly two operands).
    let iff_children = child_subjects(store, node, "iff");
    if iff_children.len() == 2 {
        let a = parse_formula(store, &iff_children[0])?;
        let b = parse_formula(store, &iff_children[1])?;
        return Some(Formula::Iff(Box::new(a), Box::new(b)));
    }
    // Material implication.
    if let (Some(a), Some(c)) = (
        child_subject(store, node, "antecedent"),
        child_subject(store, node, "consequent"),
    ) {
        let ant = parse_formula(store, &a)?;
        let con = parse_formula(store, &c)?;
        return Some(Formula::Implies(Box::new(ant), Box::new(con)));
    }
    // Quantifiers.
    for link in ["forall", "exists"] {
        if let Some(body_node) = child_subject(store, node, link) {
            let vars = parse_bound_vars(store, node)?;
            let body = Box::new(parse_formula(store, &body_node)?);
            return Some(if link == "forall" {
                Formula::Forall { vars, body }
            } else {
                Formula::Exists { vars, body }
            });
        }
    }
    None
}

/// Read an ordered argument list from `node`'s `logic:<link>` term-carriers (sorted by
/// `logic:termIndex`).  Returns `None` if any carrier is malformed.
fn parse_term_carriers(store: &RdfDataset, node: &Subject, link: &str) -> Option<Vec<Term>> {
    let mut indexed: Vec<(usize, Term)> = Vec::new();
    for carrier in child_subjects(store, node, link) {
        let idx = value(store, &carrier, &nn(&logic_iri("termIndex")))
            .and_then(|t| term_str(&t).parse::<usize>().ok())?;
        indexed.push((idx, parse_term(store, &carrier)?));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Some(indexed.into_iter().map(|(_, t)| t).collect())
}

/// Read a quantifier's ordered bound-variable names from its `logic:quantifiedVariable`
/// term-carriers (sorted by `logic:termIndex`).
///
/// Returns `None` if any carrier is malformed (unparsable `termIndex` or missing
/// `termVariable`) or if the binder is vacuous (zero bound variables) — a malformed
/// binder must surface as `MALFORMED_FORMULA`, never silently narrow `∀{x,y}` to `∀{x}`.
fn parse_bound_vars(store: &RdfDataset, node: &Subject) -> Option<Vec<String>> {
    let mut indexed: Vec<(usize, String)> = Vec::new();
    for carrier in child_subjects(store, node, "quantifiedVariable") {
        let idx = value(store, &carrier, &nn(&logic_iri("termIndex")))
            .and_then(|t| term_str(&t).parse::<usize>().ok())?;
        let name = value(store, &carrier, &nn(&logic_iri("termVariable"))).map(|t| term_str(&t))?;
        indexed.push((idx, name));
    }
    if indexed.is_empty() {
        return None;
    }
    indexed.sort_by_key(|(i, _)| *i);
    Some(indexed.into_iter().map(|(_, n)| n).collect())
}

/// Reconstruct a [`Term`] from a term-carrier node by its single term-value property.
fn parse_term(store: &RdfDataset, carrier: &Subject) -> Option<Term> {
    if let Some(t) = value(store, carrier, &nn(&logic_iri("termIri"))) {
        return Term::iri(term_str(&t)).ok();
    }
    if let Some(t) = value(store, carrier, &nn(&logic_iri("termVariable"))) {
        return Term::var(term_str(&t)).ok();
    }
    if let Some(t) = value(store, carrier, &nn(&logic_iri("termLiteral"))) {
        let datatype =
            value(store, carrier, &nn(&logic_iri("termLiteralDatatype"))).map(|d| term_str(&d));
        return Term::literal(term_str(&t), datatype).ok();
    }
    if let Some(t) = value(store, carrier, &nn(&logic_iri("termSequenceMarker"))) {
        return Term::sequence_marker(term_str(&t)).ok();
    }
    None
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
    let mut formulas: Vec<Formula> = Vec::new();
    let mut horn_axioms: Vec<LogicAxiom> = Vec::new();
    for f in extract_formulas(store, &mut diagnostics) {
        if f.is_trivially_horn() {
            match f.as_horn_axiom() {
                Some(ax) => horn_axioms.push(ax),
                None => diagnostics.push(Diagnostic::warning(
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

    let program = LogicProgram::new(all_axioms, rules, contracts, source_iri)
        .with_path_shapes(path_shapes)
        .with_correspondences(correspondences)
        .with_transaction_programs(transaction_programs)
        .with_formulas(formulas);
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
