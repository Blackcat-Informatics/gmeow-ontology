// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Front-end parser: a `logic:` RDF 1.2 source graph → [`LogicProgram`].
//!
//! A faithful Rust port of `src/gmeow_tools/logic_frontend.py`, parsing a
//! `logic:`-vocabulary RDF graph (Turtle text or a parsed oxigraph [`Store`])
//! into a typed [`LogicProgram`] plus a list of [`Diagnostic`] messages.
//!
//! # Parse contract (identical to the Python ancestor)
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
//! 3. **Profiles** — `rdf:type logic:SemanticProfile` declarations.
//! 4. **Rules** — `logic:Rule` nodes with `logic:head` / `logic:body` /
//!    `logic:negatedBody` (#502) / `logic:distinctBody` (#503) links.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use oxigraph::io::RdfFormat;
use oxigraph::model::{
    GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Quad, Term,
};
use oxigraph::store::Store;

use super::ir::{
    ComplexityClass, ContextualScope, LogicAxiom, LogicModality, LogicProfile, LogicProgram,
    LogicRule, SemanticProfileId, LOGIC_NAMESPACE,
};

// Well-known IRIs (string constants — avoids per-call `NamedNode::new`).
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";

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
// Term stringification (matches rdflib `str(node)`)
// --------------------------------------------------------------------------- //

/// `str(node)` for an object term — matches rdflib: IRI → the IRI; blank node →
/// the bare id; literal → its lexical value (no datatype/quotes).
fn term_str(term: &Term) -> String {
    match term {
        Term::NamedNode(nn) => nn.as_str().to_owned(),
        Term::BlankNode(bn) => bn.as_str().to_owned(),
        Term::Literal(lit) => lit.value().to_owned(),
        Term::Triple(_) => String::new(),
    }
}

/// `str(node)` for a subject node.
fn subject_str(s: &NamedOrBlankNode) -> String {
    match s {
        NamedOrBlankNode::NamedNode(nn) => nn.as_str().to_owned(),
        NamedOrBlankNode::BlankNode(bn) => bn.as_str().to_owned(),
    }
}

fn term_is_literal(term: &Term) -> bool {
    matches!(term, Term::Literal(_))
}

/// View a term as a subject node (for `graph.value(term, ...)` lookups), if it
/// is an IRI or blank node.
fn term_as_subject(term: &Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(nn) => Some(NamedOrBlankNode::NamedNode(nn.clone())),
        Term::BlankNode(bn) => Some(NamedOrBlankNode::BlankNode(bn.clone())),
        _ => None,
    }
}

// --------------------------------------------------------------------------- //
// Graph access helpers
// --------------------------------------------------------------------------- //

/// All triples in the default graph, materialized for repeated iteration.
fn default_graph_quads(store: &Store) -> Vec<Quad> {
    store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .collect()
}

/// `graph.value(subject, predicate)` — the first object of `(subject, predicate, *)`
/// in the default graph, or `None`.
fn value(store: &Store, subject: &NamedOrBlankNode, predicate: &NamedNode) -> Option<Term> {
    let s: NamedOrBlankNodeRef<'_> = subject.as_ref();
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(Some(s), Some(p), None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .next()
        .map(|q| q.object)
}

/// All objects of `(subject, predicate, *)` in the default graph.
fn objects(store: &Store, subject: &NamedOrBlankNode, predicate: &NamedNode) -> Vec<Term> {
    let s: NamedOrBlankNodeRef<'_> = subject.as_ref();
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(Some(s), Some(p), None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .map(|q| q.object)
        .collect()
}

/// All subjects of `(*, predicate, object)` in the default graph.
fn subjects_with(store: &Store, predicate: &NamedNode, object: &Term) -> Vec<NamedOrBlankNode> {
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(
            None,
            Some(p),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .map(|q| q.subject)
        .collect()
}

// --------------------------------------------------------------------------- //
// Scope extraction
// --------------------------------------------------------------------------- //

fn confidence_from_term(term: &Term) -> Option<f64> {
    if let Term::Literal(lit) = term {
        if let Ok(val) = lit.value().parse::<f64>() {
            if (0.0..=1.0).contains(&val) {
                return Some(val);
            }
        }
    }
    None
}

fn modality_from_term(term: Option<&Term>) -> LogicModality {
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
    store: &Store,
    node: &NamedOrBlankNode,
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

/// Construct a [`NamedNode`] from a known-valid IRI string.
fn nn(iri: &str) -> NamedNode {
    NamedNode::new(iri).unwrap_or_else(|e| panic!("invalid built-in IRI {iri:?}: {e}"))
}

// --------------------------------------------------------------------------- //
// Axiom extraction
// --------------------------------------------------------------------------- //

fn extract_axioms(store: &Store, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();

    // 1. Triples with a logic: predicate (excluding rdf:type).
    for quad in default_graph_quads(store) {
        let p_str = quad.predicate.as_str();
        if !p_str.starts_with(LOGIC_NAMESPACE) {
            continue;
        }
        if p_str == RDF_TYPE {
            continue; // unreachable (rdf:type is not logic:) but mirrors Python.
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
    let rdf_type = nn(RDF_TYPE);
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf_type.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
    {
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

fn extract_scoped_axioms(store: &Store, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();

    // RDF 1.2 style: reifier node with rdf:reifies → triple term.
    let rdf_reifies = nn(RDF_REIFIES);
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf_reifies.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
    {
        let reifier = quad.subject.clone();
        let scope = scope_from_node(store, &reifier, diagnostics);
        if let Term::Triple(triple) = &quad.object {
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
    let rdf_type_term = Term::NamedNode(nn(RDF_STATEMENT));
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
// Profile extraction
// --------------------------------------------------------------------------- //

fn extract_profiles(store: &Store, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicProfile> {
    let mut profiles: Vec<LogicProfile> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let profile_class = Term::NamedNode(nn(&logic_iri("SemanticProfile")));
    for individual in subjects_with(store, &nn(RDF_TYPE), &profile_class) {
        let iri_str = subject_str(&individual);
        let local = iri_str.strip_prefix(LOGIC_NAMESPACE);
        let profile_id = local.and_then(SemanticProfileId::from_local);
        let Some(profile_id) = profile_id else {
            diagnostics.push(Diagnostic::warning(
                "UNKNOWN_PROFILE",
                format!(
                    "{iri_str:?} is declared as logic:SemanticProfile but is not a \
                     recognised named individual; skipped"
                ),
                Some(iri_str.clone()),
            ));
            continue;
        };
        if !seen.insert(iri_str.clone()) {
            continue;
        }

        let complexity_node = value(store, &individual, &nn(&logic_iri("complexityClass")));
        let mut complexity: Option<ComplexityClass> = None;
        if let Some(cn) = complexity_node {
            let label = term_str(&cn).trim().to_owned();
            match ComplexityClass::new(label.clone()) {
                Ok(cc) => complexity = Some(cc),
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

        profiles.push(LogicProfile::new(profile_id, complexity));
    }

    profiles
}

// --------------------------------------------------------------------------- //
// Rule extraction (forward-compatible: absent logic:Rule → empty list)
// --------------------------------------------------------------------------- //

/// Read a reified atom node (`rdf:subject` / `rdf:predicate` / `rdf:object`) into
/// a [`LogicAxiom`], returning `Err` with a message on a missing predicate or a
/// validation failure.
fn read_reified_axiom(
    store: &Store,
    node: &NamedOrBlankNode,
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

fn extract_rules(store: &Store, diagnostics: &mut Vec<Diagnostic>) -> Vec<LogicRule> {
    let mut rules: Vec<LogicRule> = Vec::new();

    let logic_rule = Term::NamedNode(nn(&logic_iri("Rule")));
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
// Public API
// --------------------------------------------------------------------------- //

/// Parse a `logic:` RDF source already loaded into an oxigraph [`Store`]
/// (default graph) into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_store(
    store: &Store,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .next()
        .is_none()
    {
        return Err(LogicParseError(
            "Source graph is empty — nothing to parse.  Pass a non-empty graph or a \
             Turtle file with logic: triples."
                .to_owned(),
        ));
    }

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

    let profiles = extract_profiles(store, &mut diagnostics);
    let rules = extract_rules(store, &mut diagnostics);

    let program = LogicProgram::new(all_axioms, rules, profiles, source_iri);
    Ok((program, diagnostics))
}

/// Parse Turtle source text into a [`LogicProgram`] + diagnostics.
pub fn parse_logic_str(
    turtle: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    let store =
        Store::new().map_err(|e| LogicParseError(format!("in-memory store init failed: {e}")))?;
    store
        .load_from_reader(RdfFormat::Turtle, turtle.as_bytes())
        .map_err(|e| LogicParseError(format!("Failed to parse Turtle source: {e}")))?;
    parse_logic_store(&store, source_iri)
}

/// Parse a Turtle file into a [`LogicProgram`] + diagnostics.  When `source_iri`
/// is `None`, the file URI is recorded as the program's provenance source.
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
        .map(|c| c.to_string())
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
fn path_to_file_uri(path: &Path) -> Option<String> {
    let abs = std::fs::canonicalize(path).ok()?;
    Some(format!("file://{}", abs.display()))
}

#[cfg(test)]
mod tests;
