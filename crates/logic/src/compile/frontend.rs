// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Front-end parser: a `logic:` RDF 1.2 source graph → [`LogicProgram`].
//!
//! The `logic:` front-end parser (#664); the Python duplicate
//! (`logic_frontend.py`) was retired in #727.  It parses a `logic:`-vocabulary
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
//!    `logic:negatedBody` (#502) / `logic:distinctBody` (#503) links.

use std::collections::HashSet;
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use gmeow_rdf::{parse_dataset, RdfDataset};

use super::compat;
use super::graphutil::{
    canonicalize_blank_nodes, contains, default_graph_quads, is_empty, nn, objects, subject_str,
    subjects_with, term_as_subject, term_is_literal, term_str, value, Node, Subject, RDF_OBJECT,
    RDF_PREDICATE, RDF_REIFIES, RDF_STATEMENT, RDF_SUBJECT, RDF_TYPE,
};
use super::ir::{
    ComplexityClass, ContextualScope, LogicAxiom, LogicModality, LogicProgram, LogicRule, PathBase,
    PathShapeIr, ReasoningContract, SemanticProfileId, LOGIC_NAMESPACE,
};

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

/// Project parse [`Diagnostic`]s into the canonical `gmeow-diagnostics` `Report`
/// (issue #856).
///
/// This is the RUST-FIRST seam: the `Finding`/`Report` construction the `logic:`
/// compile surface used to do in Python now happens here, in the Rust core, and
/// `gmeow_logic.compile_logic` hands Python a live, normalized `Report` instead of
/// a `list[dict]` of raw diagnostics.
///
/// The tool/code namespace is `logic-compile`: the report tool is `logic-compile`,
/// every finding carries `with_tool("logic-compile")`, and each code is prefixed
/// `logic-compile.<code>`. The diagnostic `subject` (an IRI / blank-node id) becomes
/// the finding's logical location; an absent **or empty** subject yields no location
/// (mirroring the prior `(subject or None)` Python behavior).
pub fn diagnostics_report(diagnostics: &[Diagnostic]) -> gmeow_diagnostics::Report {
    use gmeow_diagnostics::{Finding, Location, Report, Severity as DSeverity};

    let mut report = Report::new("logic-compile");
    for diag in diagnostics {
        let severity = match diag.severity {
            Severity::Error => DSeverity::Error,
            Severity::Warning => DSeverity::Warning,
            Severity::Info => DSeverity::Info,
        };
        let mut finding = Finding::new(
            severity,
            format!("logic-compile.{}", diag.code),
            diag.message.clone(),
        )
        .with_tool("logic-compile");
        if let Some(subject) = diag.subject.as_deref().filter(|s| !s.is_empty()) {
            finding.add_location(Location::new(None, None, None, Some(subject.to_owned())));
        }
        report.add_finding(finding);
    }
    report
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
    if let Node::Lit(lexical) = term {
        if let Ok(val) = lexical.parse::<f64>() {
            if (0.0..=1.0).contains(&val) {
                return Some(val);
            }
        }
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
    if let Some(cn) = &conf_node {
        if confidence.is_none() {
            diagnostics.push(Diagnostic::warning(
                "INVALID_CONFIDENCE",
                format!(
                    "confidence value {:?} is not a float in [0, 1]; ignored",
                    term_str(cn)
                ),
                Some(subject_str(node)),
            ));
        }
    }

    let modality = modality_from_term(value(store, node, &nn(&logic_iri("modality"))).as_ref());
    let provenance = value(store, node, &nn(&logic_iri("provenance"))).map(|t| term_str(&t));

    // `ContextualScope::new` only fails on out-of-range confidence, which we have
    // already filtered to `None` above, so this never errors.
    ContextualScope::new(standpoint, time, confidence, modality, provenance).unwrap_or_default()
}

// --------------------------------------------------------------------------- //
// Axiom extraction
// --------------------------------------------------------------------------- //

/// The set of `logic:` predicate-local names that carry reasoning-contract /
/// preset / closure *meta-configuration* (#767, Gap 1).  When such a predicate's
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

/// Collect the IRIs / blank-node ids of every subject typed
/// `logic:ReasoningContract`, `logic:ReasoningPreset`, OR `logic:ClosureEntry`.
/// These are the meta-configuration nodes whose facet-config triples must be kept
/// out of the domain axiom set (#767, Gap 1).
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
    // triples on these are contract configuration, not domain facts (#767, Gap 1).
    let config_subjects = collect_contract_config_subjects(store);

    // 1. Triples with a logic: predicate (excluding rdf:type).
    for quad in default_graph_quads(store) {
        let p_str = quad.predicate.as_str();
        if !p_str.starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        if p_str == RDF_TYPE {
            continue; // unreachable (rdf:type is not logic:) but mirrors Python.
        }
        // Skip contract/preset/closure facet-config triples: they are consumed by
        // extract_contracts and must not pollute the domain axiom set.
        let p_local = &p_str[LOGIC_NAMESPACE.len()..];
        if is_facet_config_predicate(p_local)
            && config_subjects.contains(&subject_str(&quad.subject))
        {
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
                exc,
                Some(subject_str(&quad.subject)),
            )),
        }
    }

    // 2. rdf:type triples whose object is a logic: class.
    for quad in default_graph_quads(store) {
        if quad.predicate.as_str() != RDF_TYPE {
            continue;
        }
        let o_str = term_str(&quad.object);
        if !o_str.starts_with(LOGIC_NAMESPACE) {
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
                exc,
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
                    exc,
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
                exc,
                Some(subject_str(&stmt)),
            )),
        }
    }

    axioms
}

// --------------------------------------------------------------------------- //
// Reasoning-contract extraction (#767)
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
        if let Some(local) = ty_str.strip_prefix(LOGIC_NAMESPACE) {
            if is_facet_class(local) {
                return Some(local.to_owned());
            }
        }
    }
    None
}

/// The facet value-class a DIRECT facet property routes to, independent of the
/// value individual's `rdf:type`.  This is the round-trip path (#767, Task 6): the
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
/// `logic:ProbabilityModel` (reviewer C4 — probabilistic inference must never
/// silently assume independence over un-modelled confidence metadata).
fn graph_declares_probability_model(store: &RdfDataset) -> bool {
    // Any triple whose predicate is logic:probabilityModel.
    let prob_model_pred = logic_iri("probabilityModel");
    if default_graph_quads(store)
        .iter()
        .any(|q| q.predicate.as_str() == prob_model_pred)
    {
        return true;
    }
    // Any individual typed logic:ProbabilityModel.
    let prob_model_class = Node::iri(logic_iri("ProbabilityModel"));
    !subjects_with(store, &nn(RDF_TYPE), &prob_model_class).is_empty()
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
                    // Greenfield (reviewer C3): an unrecognised preset reference is
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
            // HARD verdict (#767, Gap 4): a malformed closure entry — a non-node
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

        // ── Compatibility feature model (#767, Task 3) ──────────────────────
        // HARD verdict (reviewer C3): an unsupported contract is a Severity::Error
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

        // Graph-dependent RuleProbabilisticRequiresModel (reviewer C4): a
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
) -> Result<LogicAxiom, String> {
    let p = value(store, node, &nn(RDF_PREDICATE));
    let Some(p) = p else {
        return Err("__missing_predicate__".to_owned());
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
                let message = if msg == "__missing_predicate__" {
                    "logic:head node has no rdf:predicate; skipped".to_owned()
                } else {
                    msg
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
                        let message = if msg == "__missing_predicate__" {
                            "logic:body node has no rdf:predicate; body atom skipped".to_owned()
                        } else {
                            msg
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

        // Inequality guards (#503): logic:distinctBody nodes carry rdf:subject /
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

        rules.push(LogicRule::new(
            head_axiom,
            body_axioms,
            distinct_pairs,
            scope,
        ));
    }

    rules
}

// --------------------------------------------------------------------------- //
// Path-shape extraction (#1010; absent logic:PathShape → empty list)
// --------------------------------------------------------------------------- //

/// Parse a positive-integer literal's lexical value (`xsd:positiveInteger`),
/// returning `None` on a non-numeric, overflowing, or non-positive value.
fn parse_positive_int(lexical: &str) -> Option<u32> {
    match lexical.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Some(n),
        _ => None,
    }
}

/// Read `logic:PathShape` individuals (#1010) into [`PathShapeIr`]s.
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
            Err(msg) => {
                diagnostics.push(Diagnostic::warning("MALFORMED_PATH_SHAPE", msg, Some(subj)))
            }
        }
    }

    shapes
}

// --------------------------------------------------------------------------- //
// Public API
// --------------------------------------------------------------------------- //

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
    let canon = canonicalize_blank_nodes(dataset).map_err(LogicParseError)?;
    let store = canon.as_ref();

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let plain_axioms = extract_axioms(store, &mut diagnostics);
    let scoped_axioms = extract_scoped_axioms(store, &mut diagnostics);

    // Merge + dedup by full content (mirrors the Python `set(...) | set(...)`),
    // preserving first-occurrence order (plain then scoped) for deterministic
    // tie-breaking; `LogicProgram::new` then sorts canonically.
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_axioms: Vec<LogicAxiom> = Vec::new();
    for ax in plain_axioms.into_iter().chain(scoped_axioms) {
        if seen.insert(content_dedup_key(&ax)) {
            all_axioms.push(ax);
        }
    }

    let contracts = extract_contracts(store, &mut diagnostics);
    let rules = extract_rules(store, &mut diagnostics);
    let path_shapes = extract_path_shapes(store, &mut diagnostics);

    let program =
        LogicProgram::new(all_axioms, rules, contracts, source_iri).with_path_shapes(path_shapes);
    Ok((program, diagnostics))
}

/// Parse Turtle source text into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_str(
    turtle: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    // Native codec parse → frozen wasm-clean IR dataset, straight into the parser
    // (no oxigraph Store hop, #909/#732).
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
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        ax.sort_key(),
        ax.scope.standpoint.as_deref().unwrap_or(""),
        ax.scope.time.as_deref().unwrap_or(""),
        conf,
        ax.scope.modality.as_str(),
        ax.scope.provenance.as_deref().unwrap_or(""),
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
