// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL Core constraint implementations.
//!
//! Evaluates all non-SPARQL SHACL Core constraint components plus the
//! recursive shape evaluator.  PyO3-free.

use oxigraph::model::{NamedNode, NamedOrBlankNodeRef, Term};
use oxigraph::store::Store;

use crate::model::{rdf, sh};
use crate::path;
use crate::report::ValidationResult;
use crate::shapes::{Constraint, NodeKindValue, Path, PropertyShape, Shape};

// ── Public surface ─────────────────────────────────────────────────────────────

/// Validate a single focus node against a shape, returning all `ValidationResult`s.
///
/// Any result ⇒ non-conformance (regardless of severity).  Recurses for
/// `sh:and`, `sh:or`, `sh:xone`, and `sh:node` constraints.
///
/// A `deactivated` shape produces no results.
pub fn validate_shape(store: &Store, focus: &Term, shape: &Shape) -> Vec<ValidationResult> {
    if shape.deactivated {
        return vec![];
    }

    let mut results: Vec<ValidationResult> = Vec::new();

    // --- Node-level constraints (value nodes = [focus], no path) ---
    let node_value_nodes = std::slice::from_ref(focus);
    for constraint in &shape.constraints {
        results.extend(eval_constraint(
            store,
            node_value_nodes,
            constraint,
            None,
            shape,
        ));
    }

    // --- Property shapes ---
    for ps in &shape.property_shapes {
        results.extend(eval_property_shape(store, focus, ps, shape));
    }

    results
}

/// Returns `true` iff the focus node produces zero validation results against
/// the shape (i.e., it fully conforms).
pub fn conforms(store: &Store, focus: &Term, shape: &Shape) -> bool {
    validate_shape(store, focus, shape).is_empty()
}

// ── Property shape evaluator ───────────────────────────────────────────────────

fn eval_property_shape(
    store: &Store,
    focus: &Term,
    ps: &PropertyShape,
    parent_shape: &Shape,
) -> Vec<ValidationResult> {
    let value_nodes = path::eval(store, focus, &ps.path);
    let path_term = path::path_to_term(&ps.path);

    // Build a synthetic shape wrapping the property shape so result
    // metadata (source_shape, severity, message) can come from the PS.
    let ps_as_shape = Shape {
        id: parent_shape.id.clone(),
        targets: vec![],
        constraints: ps.constraints.clone(),
        property_shapes: vec![],
        severity: ps.severity,
        message: ps.message.clone(),
        deactivated: false,
    };

    let mut results = Vec::new();
    for constraint in &ps.constraints {
        let mut rs = eval_constraint(
            store,
            &value_nodes,
            constraint,
            Some(&ps.path),
            &ps_as_shape,
        );
        // Override result_path for every result to match the property shape path.
        for r in &mut rs {
            r.result_path = Some(path_term.clone());
            r.focus_node = focus.clone();
        }
        results.extend(rs);
    }
    results
}

// ── Per-constraint evaluator ───────────────────────────────────────────────────

/// Evaluate a single constraint against the provided value node set.
///
/// `path` is `None` for node-level constraints, `Some` for property shapes.
fn eval_constraint(
    store: &Store,
    value_nodes: &[Term],
    constraint: &Constraint,
    path: Option<&Path>,
    shape: &Shape,
) -> Vec<ValidationResult> {
    let result_path = path.map(path::path_to_term);
    let severity = shape.severity;
    let message = shape.message.clone();
    let source_shape = shape.id.clone();

    macro_rules! result {
        ($component:expr, $value:expr) => {
            ValidationResult {
                focus_node: value_nodes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| source_shape.clone()),
                result_path: result_path.clone(),
                value: $value,
                source_constraint_component: NamedNode::from($component),
                source_shape: source_shape.clone(),
                severity,
                message: message.clone(),
            }
        };
        ($component:expr, $focus:expr, $value:expr) => {
            ValidationResult {
                focus_node: $focus,
                result_path: result_path.clone(),
                value: $value,
                source_constraint_component: NamedNode::from($component),
                source_shape: source_shape.clone(),
                severity,
                message: message.clone(),
            }
        };
    }

    match constraint {
        // ── Count constraints (operate on the SET) ─────────────────────────────
        Constraint::MinCount(n) => {
            let count = value_nodes.len() as u64;
            if count < *n {
                vec![result!(sh::MIN_COUNT_CONSTRAINT_COMPONENT, None)]
            } else {
                vec![]
            }
        }
        Constraint::MaxCount(n) => {
            let count = value_nodes.len() as u64;
            if count > *n {
                vec![result!(sh::MAX_COUNT_CONSTRAINT_COMPONENT, None)]
            } else {
                vec![]
            }
        }

        // ── Class (per value node, NO subclass inference) ──────────────────────
        Constraint::Class(class_iri) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let violates = match value {
                    Term::Literal(_) => true,
                    _ => !has_direct_type(store, value, class_iri),
                };
                if violates {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::CLASS_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── Datatype (per value node) ──────────────────────────────────────────
        Constraint::Datatype(dt_iri) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                if !check_datatype(value, dt_iri) {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::DATATYPE_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── NodeKind (per value node) ──────────────────────────────────────────
        Constraint::NodeKind(kind) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                if !check_node_kind(value, kind) {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::NODE_KIND_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── In (per value node) ────────────────────────────────────────────────
        Constraint::In(allowed) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                if !allowed.iter().any(|a| terms_equal(a, value)) {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(sh::IN_CONSTRAINT_COMPONENT),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── HasValue (on the SET, one result if missing) ───────────────────────
        Constraint::HasValue(required) => {
            let found = value_nodes.iter().any(|v| terms_equal(v, required));
            if !found {
                let focus = value_nodes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| source_shape.clone());
                vec![ValidationResult {
                    focus_node: focus,
                    result_path: result_path.clone(),
                    value: None,
                    source_constraint_component: NamedNode::from(
                        sh::HAS_VALUE_CONSTRAINT_COMPONENT,
                    ),
                    source_shape: source_shape.clone(),
                    severity,
                    message: message.clone(),
                }]
            } else {
                vec![]
            }
        }

        // ── Pattern (per value node) ───────────────────────────────────────────
        Constraint::Pattern { regex, flags } => {
            let compiled = build_regex(regex, flags.as_deref());
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let lexical = match value {
                    Term::Literal(lit) => Some(lit.value().to_owned()),
                    Term::NamedNode(nn) => Some(nn.as_str().to_owned()),
                    _ => None,
                };
                let violates = match (&compiled, &lexical) {
                    (Err(_), _) => true,   // bad regex → violation on every value node
                    (Ok(_), None) => true, // blank node → violation
                    (Ok(re), Some(lex)) => !re.is_match(lex),
                };
                if violates {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::PATTERN_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── MinLength (per value node) ─────────────────────────────────────────
        Constraint::MinLength(n) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let len_opt = lexical_length(value);
                let violates = match len_opt {
                    None => true, // blank node
                    Some(len) => (len as u64) < *n,
                };
                if violates {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::MIN_LENGTH_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── UniqueLang (on the SET) ────────────────────────────────────────────
        Constraint::UniqueLang(true) => {
            let mut seen_langs: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for value in value_nodes {
                if let Term::Literal(lit) = value {
                    if let Some(lang) = lit.language() {
                        *seen_langs.entry(lang.to_lowercase()).or_insert(0) += 1;
                    }
                }
            }
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            let mut results = Vec::new();
            for (lang, count) in &seen_langs {
                if *count > 1 {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: None,
                        source_constraint_component: NamedNode::from(
                            sh::UNIQUE_LANG_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message
                            .clone()
                            .or_else(|| Some(format!("duplicate language tag: {lang}"))),
                    });
                }
            }
            results
        }
        Constraint::UniqueLang(false) => vec![],

        // ── MinInclusive / MaxInclusive (per value node) ───────────────────────
        Constraint::MinInclusive(bound) => {
            let bound_num = numeric_value(bound);
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let violates = match (numeric_value(value), &bound_num) {
                    (Some(v), Some(b)) => v < *b,
                    _ => true,
                };
                if violates {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::MIN_INCLUSIVE_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }
        Constraint::MaxInclusive(bound) => {
            let bound_num = numeric_value(bound);
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let violates = match (numeric_value(value), &bound_num) {
                    (Some(v), Some(b)) => v > *b,
                    _ => true,
                };
                if violates {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(
                            sh::MAX_INCLUSIVE_CONSTRAINT_COMPONENT,
                        ),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── And (per value node, recursive) ───────────────────────────────────
        Constraint::And(members) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let all_conform = members.iter().all(|m| conforms(store, value, m));
                if !all_conform {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(sh::AND_CONSTRAINT_COMPONENT),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── Or (per value node, recursive) ────────────────────────────────────
        Constraint::Or(members) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let any_conforms = members.iter().any(|m| conforms(store, value, m));
                if !any_conforms {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(sh::OR_CONSTRAINT_COMPONENT),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── Xone (per value node, recursive) ──────────────────────────────────
        Constraint::Xone(members) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let count = members.iter().filter(|m| conforms(store, value, m)).count();
                if count != 1 {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(sh::XONE_CONSTRAINT_COMPONENT),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }

        // ── Node (per value node, recursive) ──────────────────────────────────
        Constraint::Node(inner_shape) => {
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                if !conforms(store, value, inner_shape) {
                    results.push(ValidationResult {
                        focus_node: focus.clone(),
                        result_path: result_path.clone(),
                        value: Some(value.clone()),
                        source_constraint_component: NamedNode::from(sh::NODE_CONSTRAINT_COMPONENT),
                        source_shape: source_shape.clone(),
                        severity,
                        message: message.clone(),
                    });
                }
            }
            results
        }
    }
}

// ── Helper functions ───────────────────────────────────────────────────────────

/// Check if `value` has a direct `rdf:type` triple to `class_iri` in the
/// default graph (NO subclass inference).
fn has_direct_type(store: &Store, value: &Term, class_iri: &NamedNode) -> bool {
    let Some(subj_ref) = term_as_subject_ref(value) else {
        return false;
    };
    let class_term = Term::NamedNode(class_iri.clone());
    store
        .quads_for_pattern(
            Some(subj_ref),
            Some(rdf::TYPE),
            Some(class_term.as_ref()),
            None,
        )
        .any(|q| q.is_ok())
}

/// Convert a `Term` to a subject ref, or `None` for literals.
fn term_as_subject_ref(term: &Term) -> Option<NamedOrBlankNodeRef<'_>> {
    match term {
        Term::NamedNode(n) => Some(NamedOrBlankNodeRef::NamedNode(n.as_ref())),
        Term::BlankNode(b) => Some(NamedOrBlankNodeRef::BlankNode(b.as_ref())),
        _ => None,
    }
}

/// `xsd:integer` lexical space: optional sign then one-or-more ASCII digits.
/// Unbounded — no native-int overflow.
fn is_xsd_integer_lexical(s: &str) -> bool {
    let s = s.trim();
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `xsd:decimal` lexical space: optional sign then digits with an optional
/// single '.' — NO exponent. At least one digit must be present.
fn is_xsd_decimal_lexical(s: &str) -> bool {
    let s = s.trim();
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for b in body.bytes() {
        match b {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            _ => return false, // rejects 'e'/'E' (scientific notation) and any other char
        }
    }
    seen_digit
}

/// `xsd:double`/`xsd:float` lexical space: the three special values exactly
/// (INF, -INF, NaN — case-sensitive per XSD), or a mantissa (decimal lexical)
/// with an optional [eE][+-]?digits exponent.
fn is_xsd_double_lexical(s: &str) -> bool {
    let s = s.trim();
    if matches!(s, "INF" | "-INF" | "+INF" | "NaN") {
        return true;
    }
    // Split optional exponent.
    let (mantissa, exponent) = match s.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (s, None),
    };
    if !is_xsd_decimal_lexical(mantissa) {
        return false;
    }
    match exponent {
        None => true,
        Some(exp) => {
            let digits = exp.strip_prefix(['+', '-']).unwrap_or(exp);
            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
        }
    }
}

/// Check that a `Term` satisfies `sh:datatype` requirements.
///
/// - Must be a `Literal` whose `.datatype()` IRI equals `dt_iri`.
/// - Additionally validates the lexical form (not native-parse) for common XSD
///   numeric/boolean datatypes: xsd:integer (unbounded, no overflow),
///   xsd:decimal (no scientific notation), xsd:double, xsd:float, xsd:boolean.
fn check_datatype(value: &Term, dt_iri: &NamedNode) -> bool {
    let Term::Literal(lit) = value else {
        return false;
    };
    if lit.datatype().as_str() != dt_iri.as_str() {
        return false;
    }
    // Lexical validity check for common XSD types.
    let lex = lit.value();
    match dt_iri.as_str() {
        "http://www.w3.org/2001/XMLSchema#integer" => is_xsd_integer_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#decimal" => is_xsd_decimal_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#double" => is_xsd_double_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#float" => is_xsd_double_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            matches!(lex.trim(), "true" | "false" | "1" | "0")
        }
        _ => true, // no lexical validation for other datatypes
    }
}

/// Check that a `Term` satisfies `sh:nodeKind`.
fn check_node_kind(value: &Term, kind: &NodeKindValue) -> bool {
    matches!(
        (value, kind),
        (Term::NamedNode(_), NodeKindValue::Iri)
            | (Term::NamedNode(_), NodeKindValue::BlankNodeOrIri)
            | (Term::NamedNode(_), NodeKindValue::IriOrLiteral)
            | (Term::BlankNode(_), NodeKindValue::BlankNode)
            | (Term::BlankNode(_), NodeKindValue::BlankNodeOrIri)
            | (Term::BlankNode(_), NodeKindValue::BlankNodeOrLiteral)
            | (Term::Literal(_), NodeKindValue::Literal)
            | (Term::Literal(_), NodeKindValue::BlankNodeOrLiteral)
            | (Term::Literal(_), NodeKindValue::IriOrLiteral)
    )
}

/// Return the character count of the lexical form of `value`, or `None` for
/// blank nodes (which violate `sh:minLength`).
fn lexical_length(value: &Term) -> Option<usize> {
    match value {
        Term::Literal(lit) => Some(lit.value().chars().count()),
        Term::NamedNode(nn) => Some(nn.as_str().chars().count()),
        _ => None,
    }
}

/// Parse a numeric value (xsd:integer, xsd:decimal, xsd:double) as `f64`.
fn numeric_value(term: &Term) -> Option<f64> {
    let Term::Literal(lit) = term else {
        return None;
    };
    let dt = lit.datatype().as_str();
    if matches!(
        dt,
        "http://www.w3.org/2001/XMLSchema#integer"
            | "http://www.w3.org/2001/XMLSchema#decimal"
            | "http://www.w3.org/2001/XMLSchema#double"
            | "http://www.w3.org/2001/XMLSchema#float"
            | "http://www.w3.org/2001/XMLSchema#long"
            | "http://www.w3.org/2001/XMLSchema#int"
            | "http://www.w3.org/2001/XMLSchema#short"
            | "http://www.w3.org/2001/XMLSchema#byte"
    ) {
        lit.value().trim().parse::<f64>().ok()
    } else {
        None
    }
}

/// Term equality: two terms are equal iff their string representations match
/// (oxigraph's `PartialEq` does the right thing for typed literals).
fn terms_equal(a: &Term, b: &Term) -> bool {
    a == b
}

/// Build a compiled `Regex` from a pattern string and optional flags.
///
/// Supported flags: `i` (case-insensitive), `s` (dot-all), `m` (multi-line),
/// `x` (ignore whitespace in pattern). Flag `q` is silently ignored.
fn build_regex(pattern: &str, flags: Option<&str>) -> Result<regex::Regex, regex::Error> {
    let mut builder = regex::RegexBuilder::new(pattern);
    if let Some(f) = flags {
        for c in f.chars() {
            match c {
                'i' => {
                    builder.case_insensitive(true);
                }
                's' => {
                    builder.dot_matches_new_line(true);
                }
                'm' => {
                    builder.multi_line(true);
                }
                'x' => {
                    builder.ignore_whitespace(true);
                }
                _ => {} // includes 'q' (literal flag) and any unknown flags; skip silently
            }
        }
    }
    builder.build()
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Severity;
    use oxigraph::io::RdfFormat;
    use oxigraph::model::{Literal, NamedNode};

    const EX: &str = "http://example.org/ns#";
    const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

    fn nn(iri: &str) -> Term {
        Term::NamedNode(NamedNode::new_unchecked(iri))
    }

    fn ex(local: &str) -> Term {
        nn(&format!("{EX}{local}"))
    }

    fn xsd_lit(value: &str, dt: &str) -> Term {
        Term::Literal(Literal::new_typed_literal(
            value,
            NamedNode::new_unchecked(format!("{XSD}{dt}")),
        ))
    }

    fn shape_with(id: &str, constraints: Vec<Constraint>) -> Shape {
        Shape {
            id: ex(id),
            targets: vec![],
            constraints,
            property_shapes: vec![],
            severity: Severity::Violation,
            message: None,
            deactivated: false,
        }
    }

    fn prop_shape(id: &str, path_iri: &str, constraints: Vec<Constraint>) -> Shape {
        use crate::shapes::Path;
        Shape {
            id: ex(id),
            targets: vec![],
            constraints: vec![],
            property_shapes: vec![PropertyShape {
                path: Path::Predicate(NamedNode::new_unchecked(path_iri)),
                constraints,
                severity: Severity::Violation,
                message: None,
            }],
            severity: Severity::Violation,
            message: None,
            deactivated: false,
        }
    }

    fn load_store(ttl: &str) -> Store {
        let store = Store::new().unwrap();
        store
            .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
            .unwrap();
        store
    }

    fn component_iri(results: &[ValidationResult]) -> Vec<String> {
        results
            .iter()
            .map(|r| r.source_constraint_component.as_str().to_owned())
            .collect()
    }

    // ── minCount ───────────────────────────────────────────────────────────────

    #[test]
    fn min_count_pass() {
        let store = load_store("@prefix ex: <http://example.org/ns#> . ex:a ex:p ex:b .");
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::MinCount(1)]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert!(results.is_empty(), "should pass with 1 value");
    }

    #[test]
    fn min_count_fail() {
        let store = load_store("@prefix ex: <http://example.org/ns#> . ex:a a ex:Thing .");
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::MinCount(1)]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MinCount"));
    }

    // ── maxCount ───────────────────────────────────────────────────────────────

    #[test]
    fn max_count_pass() {
        let store = load_store("@prefix ex: <http://example.org/ns#> . ex:a ex:p ex:b .");
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::MaxCount(1)]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert!(results.is_empty());
    }

    #[test]
    fn max_count_fail() {
        let store = load_store("@prefix ex: <http://example.org/ns#> . ex:a ex:p ex:b, ex:c .");
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::MaxCount(1)]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MaxCount"));
    }

    // ── class ──────────────────────────────────────────────────────────────────

    #[test]
    fn class_pass() {
        let store = load_store(
            "@prefix ex: <http://example.org/ns#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . ex:a ex:p ex:b . ex:b rdf:type ex:Foo .",
        );
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}Foo"
            )))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert!(results.is_empty());
    }

    #[test]
    fn class_fail_no_direct_type() {
        // ex:b is typed as ex:SubFoo (a subclass of ex:Foo in "real" world),
        // but NO inference means Class(ex:Foo) fails.
        let store = load_store(
            "@prefix ex: <http://example.org/ns#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . ex:a ex:p ex:b . ex:b rdf:type ex:SubFoo .",
        );
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}Foo"
            )))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Class"));
    }

    // ── datatype ───────────────────────────────────────────────────────────────

    #[test]
    fn datatype_pass() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:age \"42\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}age"),
            vec![Constraint::Datatype(NamedNode::new_unchecked(format!(
                "{XSD}integer"
            )))],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn datatype_fail_wrong_type() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:age \"hello\"^^<{XSD}string> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}age"),
            vec![Constraint::Datatype(NamedNode::new_unchecked(format!(
                "{XSD}integer"
            )))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Datatype"));
    }

    #[test]
    fn datatype_fail_lexically_invalid() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:n \"notanumber\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}n"),
            vec![Constraint::Datatype(NamedNode::new_unchecked(format!(
                "{XSD}integer"
            )))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Datatype"));
    }

    // ── nodeKind ───────────────────────────────────────────────────────────────

    #[test]
    fn node_kind_iri_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::NodeKind(NodeKindValue::Iri)],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn node_kind_iri_fail_literal() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p \"hello\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::NodeKind(NodeKindValue::Iri)],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("NodeKind"));
    }

    // ── in ─────────────────────────────────────────────────────────────────────

    #[test]
    fn in_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:color \"red\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}color"),
            vec![Constraint::In(vec![
                Term::Literal(Literal::new_simple_literal("red")),
                Term::Literal(Literal::new_simple_literal("green")),
            ])],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn in_fail() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:color \"blue\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}color"),
            vec![Constraint::In(vec![
                Term::Literal(Literal::new_simple_literal("red")),
                Term::Literal(Literal::new_simple_literal("green")),
            ])],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("In"));
    }

    // ── hasValue ───────────────────────────────────────────────────────────────

    #[test]
    fn has_value_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b, ex:c ."));
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::HasValue(ex("b"))]);
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn has_value_fail() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:c ."));
        let shape = prop_shape("S", &format!("{EX}p"), vec![Constraint::HasValue(ex("b"))]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("HasValue"));
    }

    // ── pattern ────────────────────────────────────────────────────────────────

    #[test]
    fn pattern_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:code \"ABC\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}code"),
            vec![Constraint::Pattern {
                regex: "^[A-Z]+$".to_owned(),
                flags: None,
            }],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn pattern_fail() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:code \"abc\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}code"),
            vec![Constraint::Pattern {
                regex: "^[A-Z]+$".to_owned(),
                flags: None,
            }],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Pattern"));
    }

    #[test]
    fn pattern_with_flags_case_insensitive() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:code \"abc\" ."));
        let shape = prop_shape(
            "S",
            &format!("{EX}code"),
            vec![Constraint::Pattern {
                regex: "^[A-Z]+$".to_owned(),
                flags: Some("i".to_owned()),
            }],
        );
        // With flag "i", lowercase should now pass.
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    // ── minLength ──────────────────────────────────────────────────────────────

    #[test]
    fn min_length_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:name \"Alice\" ."));
        let shape = prop_shape("S", &format!("{EX}name"), vec![Constraint::MinLength(3)]);
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn min_length_fail() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:name \"Al\" ."));
        let shape = prop_shape("S", &format!("{EX}name"), vec![Constraint::MinLength(3)]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MinLength"));
    }

    // ── uniqueLang ─────────────────────────────────────────────────────────────

    #[test]
    fn unique_lang_pass() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:label \"Hello\"@en, \"Bonjour\"@fr ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}label"),
            vec![Constraint::UniqueLang(true)],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn unique_lang_fail() {
        // Load two English-tagged literals via N-Triples (Turtle deduplicates in the store).
        let nt = format!("<{EX}a> <{EX}label> \"Hello\"@en .\n<{EX}a> <{EX}label> \"Hi\"@en .\n");
        let store = Store::new().unwrap();
        store
            .load_from_reader(RdfFormat::NTriples, nt.as_bytes())
            .unwrap();
        let shape = prop_shape(
            "S",
            &format!("{EX}label"),
            vec![Constraint::UniqueLang(true)],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert!(!results.is_empty());
        assert!(component_iri(&results)[0].contains("UniqueLang"));
    }

    // ── minInclusive ───────────────────────────────────────────────────────────

    #[test]
    fn min_inclusive_pass() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:age \"18\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}age"),
            vec![Constraint::MinInclusive(xsd_lit("18", "integer"))],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn min_inclusive_fail() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:age \"17\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}age"),
            vec![Constraint::MinInclusive(xsd_lit("18", "integer"))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MinInclusive"));
    }

    // ── maxInclusive ───────────────────────────────────────────────────────────

    #[test]
    fn max_inclusive_pass() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:score \"100\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}score"),
            vec![Constraint::MaxInclusive(xsd_lit("100", "integer"))],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn max_inclusive_fail() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:score \"101\"^^<{XSD}integer> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}score"),
            vec![Constraint::MaxInclusive(xsd_lit("100", "integer"))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MaxInclusive"));
    }

    // ── and ────────────────────────────────────────────────────────────────────

    #[test]
    fn and_pass() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . @prefix rdf: <{RDF}> . ex:a rdf:type ex:Foo ."
        ));
        // sh:and ([ sh:nodeKind sh:IRI ] [ sh:class ex:Foo ]) on focus node directly.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with(
            "M2",
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}Foo"
            )))],
        );
        let shape = shape_with("S", vec![Constraint::And(vec![member1, member2])]);
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn and_fail_second_member() {
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . @prefix rdf: <{RDF}> . ex:a rdf:type ex:Bar ."
        ));
        // ex:a is IRI (passes M1) but type is ex:Bar not ex:Foo (fails M2).
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with(
            "M2",
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}Foo"
            )))],
        );
        let shape = shape_with("S", vec![Constraint::And(vec![member1, member2])]);
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("And"));
    }

    // ── or ─────────────────────────────────────────────────────────────────────

    #[test]
    fn or_pass_first_member() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        // ex:b is an IRI, passes M1.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with("M2", vec![Constraint::NodeKind(NodeKindValue::Literal)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Or(vec![member1, member2])],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn or_fail_no_member() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        // Both members require Literal; ex:b is IRI → fails both.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Literal)]);
        let member2 = shape_with(
            "M2",
            vec![Constraint::MinLength(999)], // impossible length
        );
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Or(vec![member1, member2])],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Or"));
    }

    // ── xone ───────────────────────────────────────────────────────────────────

    #[test]
    fn xone_pass_exactly_one() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        // ex:b is IRI: M1 (IRI) passes, M2 (Literal) fails → exactly 1.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with("M2", vec![Constraint::NodeKind(NodeKindValue::Literal)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Xone(vec![member1, member2])],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn xone_fail_zero() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p \"hello\" ."));
        // Both require IRI; literal fails both → 0 conforming → violation.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with("M2", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Xone(vec![member1, member2])],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Xone"));
    }

    #[test]
    fn xone_fail_two() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        // Both members allow IRI → 2 conforming → violation.
        let member1 = shape_with("M1", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let member2 = shape_with("M2", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Xone(vec![member1, member2])],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("Xone"));
    }

    // ── node ───────────────────────────────────────────────────────────────────

    #[test]
    fn node_pass() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p ex:b ."));
        // sh:node targets ex:b; inner shape requires IRI.
        let inner = shape_with("Inner", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Node(Box::new(inner))],
        );
        assert!(validate_shape(&store, &ex("a"), &shape).is_empty());
    }

    #[test]
    fn node_fail() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p \"notAnIRI\" ."));
        let inner = shape_with("Inner", vec![Constraint::NodeKind(NodeKindValue::Iri)]);
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Node(Box::new(inner))],
        );
        let results = validate_shape(&store, &ex("a"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("NodeConstraintComponent"));
    }

    // ── inverse path property shape ────────────────────────────────────────────

    #[test]
    fn inverse_path_property_shape() {
        use crate::shapes::Path;
        // ex:child ex:parent ex:parent_node .
        // Shape on ex:parent_node checks inverse(ex:parent) has minCount 1.
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:child ex:parent ex:parent_node ."
        ));
        let shape = Shape {
            id: ex("S"),
            targets: vec![],
            constraints: vec![],
            property_shapes: vec![PropertyShape {
                path: Path::Inverse(Box::new(Path::Predicate(NamedNode::new_unchecked(
                    format!("{EX}parent"),
                )))),
                constraints: vec![Constraint::MinCount(1)],
                severity: Severity::Violation,
                message: None,
            }],
            severity: Severity::Violation,
            message: None,
            deactivated: false,
        };
        // ex:parent_node has 1 inverse-parent (ex:child) → passes minCount(1).
        let results = validate_shape(&store, &ex("parent_node"), &shape);
        assert!(results.is_empty(), "expected pass, got: {results:?}");
    }

    #[test]
    fn inverse_path_property_shape_fail() {
        use crate::shapes::Path;
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:unrelated ex:something ex:other ."
        ));
        let shape = Shape {
            id: ex("S"),
            targets: vec![],
            constraints: vec![],
            property_shapes: vec![PropertyShape {
                path: Path::Inverse(Box::new(Path::Predicate(NamedNode::new_unchecked(
                    format!("{EX}parent"),
                )))),
                constraints: vec![Constraint::MinCount(1)],
                severity: Severity::Violation,
                message: None,
            }],
            severity: Severity::Violation,
            message: None,
            deactivated: false,
        };
        // ex:orphan has no inverse-parent triples → fails minCount(1).
        let results = validate_shape(&store, &ex("orphan"), &shape);
        assert_eq!(results.len(), 1);
        assert!(component_iri(&results)[0].contains("MinCount"));
    }

    // ── xsd lexical validators (Gap D fix) ────────────────────────────────────

    #[test]
    fn xsd_integer_accepts_large_value() {
        // A valid xsd:integer beyond i64::MAX must PASS (no overflow rejection).
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}integer"));
        let value = Term::Literal(Literal::new_typed_literal(
            "99999999999999999999999",
            dt_iri.clone(),
        ));
        assert!(
            check_datatype(&value, &dt_iri),
            "large integer should conform"
        );
    }

    #[test]
    fn xsd_integer_rejects_decimal_point() {
        // "3.5"^^xsd:integer is lexically invalid.
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}integer"));
        let value = Term::Literal(Literal::new_typed_literal("3.5", dt_iri.clone()));
        assert!(
            !check_datatype(&value, &dt_iri),
            "decimal point in integer should violate"
        );
    }

    #[test]
    fn xsd_decimal_rejects_scientific_notation() {
        // "1e3"^^xsd:decimal is NOT a valid xsd:decimal lexical form.
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}decimal"));
        let value = Term::Literal(Literal::new_typed_literal("1e3", dt_iri.clone()));
        assert!(
            !check_datatype(&value, &dt_iri),
            "scientific notation should violate xsd:decimal"
        );
    }

    #[test]
    fn xsd_decimal_accepts_plain() {
        // "3.14"^^xsd:decimal is valid.
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}decimal"));
        let value = Term::Literal(Literal::new_typed_literal("3.14", dt_iri.clone()));
        assert!(
            check_datatype(&value, &dt_iri),
            "plain decimal should conform"
        );
    }

    #[test]
    fn xsd_double_accepts_scientific() {
        // "1e3"^^xsd:double is valid (scientific notation is allowed for double).
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}double"));
        let value = Term::Literal(Literal::new_typed_literal("1e3", dt_iri.clone()));
        assert!(
            check_datatype(&value, &dt_iri),
            "scientific notation should conform for xsd:double"
        );
    }

    #[test]
    fn xsd_double_accepts_inf() {
        // "INF"^^xsd:double is a valid XSD special value.
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}double"));
        let value = Term::Literal(Literal::new_typed_literal("INF", dt_iri.clone()));
        assert!(
            check_datatype(&value, &dt_iri),
            "INF should conform for xsd:double"
        );
    }

    #[test]
    fn xsd_float_accepts_scientific() {
        // "1e3"^^xsd:float is valid — same lexical space as double.
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}float"));
        let value = Term::Literal(Literal::new_typed_literal("1e3", dt_iri.clone()));
        assert!(
            check_datatype(&value, &dt_iri),
            "scientific notation should conform for xsd:float"
        );
    }

    // ── deactivated shape ──────────────────────────────────────────────────────

    #[test]
    fn deactivated_shape_produces_no_results() {
        let store = load_store(&format!("@prefix ex: <{EX}> . ex:a ex:p \"hello\" ."));
        // Would fail NodeKind(Iri) if active.
        let shape = Shape {
            id: ex("S"),
            targets: vec![],
            constraints: vec![Constraint::NodeKind(NodeKindValue::Iri)],
            property_shapes: vec![],
            severity: Severity::Violation,
            message: None,
            deactivated: true,
        };
        // Focus node is a literal — would fail, but shape is deactivated.
        let literal_focus = Term::Literal(Literal::new_simple_literal("anything"));
        assert!(validate_shape(&store, &literal_focus, &shape).is_empty());
    }
}
