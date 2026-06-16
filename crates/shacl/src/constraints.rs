// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
            focus,
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
            focus,
            &value_nodes,
            constraint,
            Some(&ps.path),
            &ps_as_shape,
        );
        // Stamp the property-shape path and focus onto every result, but PRESERVE
        // a path the constraint itself bound — a `sh:sparql` query may project
        // `?path` (→ result_path, SHACL-AF §3.4.2.2), which is more specific than
        // the shape's declared path and must not be clobbered.
        for r in &mut rs {
            if r.result_path.is_none() {
                r.result_path = Some(path_term.clone());
            }
            r.focus_node = focus.clone();
        }
        results.extend(rs);
    }
    results
}

// ── Per-constraint evaluator ───────────────────────────────────────────────────

/// Evaluate a single constraint against the provided value node set.
///
/// `focus_node` is the SHACL focus node (subject) — always the real focus, never
/// a path value.  For node-level constraints `focus_node == value_nodes[0]`; for
/// property shapes `focus_node` is the subject while `value_nodes` are the path
/// objects.  `sh:sparql`'s `$this` must bind to `focus_node` in both contexts
/// (SHACL-AF spec: `$this` = focus node, not value node).
///
/// `path` is `None` for node-level constraints, `Some` for property shapes.
fn eval_constraint(
    store: &Store,
    focus_node: &Term,
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

        // ── Class (per value node; honors asserted rdfs:subClassOf, §4.2.5) ────
        Constraint::Class(class_iri) => {
            // Hoist the BFS closure computation once, outside the per-value loop.
            // Previously called inside the loop: O(N×M) → now O(M) + O(N).
            let closure = crate::engine::subclass_closure(store, class_iri);
            let mut results = Vec::new();
            let focus = value_nodes
                .first()
                .cloned()
                .unwrap_or_else(|| source_shape.clone());
            for value in value_nodes {
                let violates = match value {
                    Term::Literal(_) => true,
                    _ => !is_shacl_instance(store, value, &closure),
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

        // ── Sparql (SHACL-AF — $this always binds to the focus node, never to a
        //           path value node.  SHACL-AF spec §3.4: for sh:sparql on a
        //           property shape, $this is still the focus subject; the path
        //           objects are NOT auto-bound.)
        //
        // The constraint blank node may carry its own sh:message / sh:severity;
        // those override the shape-level defaults at eval time.
        // Query parseability is guaranteed at shapes-parse time, so .expect() is correct.
        Constraint::Sparql {
            select,
            message: cmsg,
            severity: csev,
        } => {
            let sev = csev.unwrap_or(severity);
            let msg = cmsg.clone().or_else(|| message.clone());
            crate::sparql::eval_sparql_constraint(
                store,
                focus_node,
                select,
                NamedNode::from(sh::SPARQL_CONSTRAINT_COMPONENT),
                &source_shape,
                sev,
                msg,
            )
            .expect("sh:sparql query execution failed (parseability checked at parse time)")
        }
    }
}

// ── Helper functions ───────────────────────────────────────────────────────────

/// Whether `value` is a SHACL instance of a class, given a precomputed subclass
/// closure (SHACL §4.2.5).
///
/// `closure` must contain the class IRI itself plus every transitive subclass
/// derived from asserted `rdfs:subClassOf` edges (as returned by
/// [`crate::engine::subclass_closure`]).  The caller hoists the closure
/// computation once before the per-value-node loop to avoid O(N×M) BFS cost.
fn is_shacl_instance(
    store: &Store,
    value: &Term,
    closure: &std::collections::HashSet<Term>,
) -> bool {
    let Some(subj_ref) = term_as_subject_ref(value) else {
        return false;
    };
    store
        .quads_for_pattern(Some(subj_ref), Some(rdf::TYPE), None, None)
        .flatten()
        .any(|q| closure.contains(&q.object))
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
    if matches!(s, "INF" | "-INF" | "NaN") {
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
/// - Must be a `Literal` whose datatype matches `dt_iri`.
/// - On an exact datatype-IRI match, additionally validates the lexical form for
///   common XSD types (xsd:integer unbounded, xsd:decimal no scientific notation,
///   xsd:double/float, xsd:boolean).
/// - Oxigraph canonicalizes XSD derived integer types to `xsd:integer` in the
///   store at load time (e.g. `"1"^^xsd:nonNegativeInteger` becomes
///   `"1"^^xsd:integer`), which would break the exact-IRI match. When the shape
///   requires such a derived type and the stored literal carries the canonical
///   base, accept iff the lexical value lies in the derived type's value space.
///   This matches pySHACL's spec-correct result. See #598.
fn check_datatype(value: &Term, dt_iri: &NamedNode) -> bool {
    let Term::Literal(lit) = value else {
        return false;
    };
    let stored_dt = lit.datatype();
    let lex = lit.value();
    if stored_dt.as_str() == dt_iri.as_str() {
        return xsd_lexical_valid(dt_iri.as_str(), lex);
    }
    derived_integer_matches(stored_dt.as_str(), dt_iri.as_str(), lex)
}

/// Lexical-form validity for an exact datatype-IRI match. Unknown datatypes are
/// accepted (no lexical facet enforced).
fn xsd_lexical_valid(dt: &str, lex: &str) -> bool {
    match dt {
        "http://www.w3.org/2001/XMLSchema#integer" => is_xsd_integer_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#decimal" => is_xsd_decimal_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#double" => is_xsd_double_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#float" => is_xsd_double_lexical(lex),
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            matches!(lex.trim(), "true" | "false" | "1" | "0")
        }
        _ => true,
    }
}

/// Whether a literal that oxigraph stored as the canonical base type satisfies a
/// shape's required XSD *derived* integer type, by validating the lexical value
/// against the derived type's value space. Every XSD integer-derived type
/// canonicalizes to `xsd:integer` in oxigraph; only that base is considered here.
/// See #598.
fn derived_integer_matches(stored_dt: &str, required_dt: &str, lex: &str) -> bool {
    const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
    if stored_dt != XSD_INTEGER || !is_xsd_integer_lexical(lex) {
        return false;
    }
    let trimmed = lex.trim();
    // For sign-constrained but unbounded types, fall back to a lexical sign check
    // when the magnitude exceeds i128 (astronomically large; never in practice).
    let value = trimmed.parse::<i128>().ok();
    let is_negative = || value.map_or(trimmed.starts_with('-'), |n| n < 0);
    let is_positive = || value.map_or(!trimmed.starts_with('-'), |n| n > 0);
    let is_zero = || value == Some(0);
    match required_dt {
        "http://www.w3.org/2001/XMLSchema#nonNegativeInteger" => !is_negative(),
        "http://www.w3.org/2001/XMLSchema#positiveInteger" => is_positive(),
        "http://www.w3.org/2001/XMLSchema#nonPositiveInteger" => is_negative() || is_zero(),
        "http://www.w3.org/2001/XMLSchema#negativeInteger" => is_negative(),
        "http://www.w3.org/2001/XMLSchema#long" => trimmed.parse::<i64>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#int" => trimmed.parse::<i32>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#short" => trimmed.parse::<i16>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#byte" => trimmed.parse::<i8>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#unsignedLong" => trimmed.parse::<u64>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#unsignedInt" => trimmed.parse::<u32>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#unsignedShort" => trimmed.parse::<u16>().is_ok(),
        "http://www.w3.org/2001/XMLSchema#unsignedByte" => trimmed.parse::<u8>().is_ok(),
        _ => false,
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

/// Build a compiled `Regex` from a pattern string and optional `sh:flags` string.
///
/// Supported flags (XPath 2.0 subset with Rust `regex` semantics):
/// - `i` — case-insensitive
/// - `s` — dot-all (`.` matches newlines)
/// - `m` — multi-line (`^`/`$` match line boundaries)
/// - `x` — ignore unescaped whitespace in pattern
///
/// **Hard-fail discipline**: any flag character outside `{i, s, m, x}` — including
/// `q` (the XPath literal-match flag) — is a hard error. Silently ignoring `q`
/// would change matching semantics in ways the caller cannot detect. Consistent
/// with this crate's policy of hard-failing on any unmodelled SHACL feature, an
/// unsupported flag returns `Err` immediately.
///
/// **Deviation from XPath 2.0**: patterns are evaluated with Rust `regex` crate
/// semantics, not XPath 2.0 regex semantics. Behaviour diverges on features such
/// as Unicode category escapes (`\p{…}`) and backreferences.
fn build_regex(pattern: &str, flags: Option<&str>) -> Result<regex::Regex, String> {
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
                _ => {
                    return Err(format!(
                        "unsupported sh:flags character {c:?} in sh:pattern \
                         (supported: i, s, m, x)"
                    ));
                }
            }
        }
    }
    builder.build().map_err(|e| e.to_string())
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
        // ex:b is typed ex:SubFoo, and there is NO asserted ex:SubFoo
        // rdfs:subClassOf ex:Foo triple in the data — so b is not a SHACL
        // instance of ex:Foo and the constraint fails. (We honor asserted
        // subClassOf, but invent none: no reasoner runs.)
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

    #[test]
    fn class_pass_asserted_subclass() {
        // ex:b is typed ex:SubFoo and the data ASSERTS ex:SubFoo rdfs:subClassOf
        // ex:Foo, so b is a SHACL instance of ex:Foo (SHACL §4.2.5) and the
        // sh:class ex:Foo constraint conforms — matching pySHACL. See #599.
        let store = load_store(
            "@prefix ex: <http://example.org/ns#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . ex:a ex:p ex:b . ex:b rdf:type ex:SubFoo . ex:SubFoo rdfs:subClassOf ex:Foo .",
        );
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}Foo"
            )))],
        );
        assert!(
            validate_shape(&store, &ex("a"), &shape).is_empty(),
            "asserted subClassOf must make ex:b a SHACL instance of ex:Foo"
        );
    }

    #[test]
    fn class_pass_transitive_subclass() {
        // Transitive: ex:b a ex:C, ex:C ⊑ ex:B, ex:B ⊑ ex:A → b is an A-instance.
        let store = load_store(
            "@prefix ex: <http://example.org/ns#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . ex:a ex:p ex:b . ex:b rdf:type ex:C . ex:C rdfs:subClassOf ex:B . ex:B rdfs:subClassOf ex:A .",
        );
        let shape = prop_shape(
            "S",
            &format!("{EX}p"),
            vec![Constraint::Class(NamedNode::new_unchecked(format!(
                "{EX}A"
            )))],
        );
        assert!(
            validate_shape(&store, &ex("a"), &shape).is_empty(),
            "transitive asserted subClassOf must be honored"
        );
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

    // ── datatype derived-integer (oxigraph canonicalization, #598) ──────────────

    #[test]
    fn datatype_derived_nonneg_integer_pass() {
        // Oxigraph stores "5"^^xsd:nonNegativeInteger as "5"^^xsd:integer, but a
        // shape requiring xsd:nonNegativeInteger must still accept it (value 5 is
        // in range) — matching pySHACL. Pre-fix this produced a false violation.
        let store = load_store(&format!(
            "@prefix ex: <{EX}> . ex:a ex:n \"5\"^^<{XSD}nonNegativeInteger> ."
        ));
        let shape = prop_shape(
            "S",
            &format!("{EX}n"),
            vec![Constraint::Datatype(NamedNode::new_unchecked(format!(
                "{XSD}nonNegativeInteger"
            )))],
        );
        assert!(
            validate_shape(&store, &ex("a"), &shape).is_empty(),
            "in-range derived-integer value must conform under canonicalization"
        );
    }

    #[test]
    fn derived_integer_value_space() {
        let int = "http://www.w3.org/2001/XMLSchema#integer";
        let nn = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
        let pos = "http://www.w3.org/2001/XMLSchema#positiveInteger";
        let neg = "http://www.w3.org/2001/XMLSchema#negativeInteger";
        let byte = "http://www.w3.org/2001/XMLSchema#byte";
        // nonNegativeInteger: >= 0
        assert!(derived_integer_matches(int, nn, "5"));
        assert!(derived_integer_matches(int, nn, "0"));
        assert!(!derived_integer_matches(int, nn, "-3"));
        // positiveInteger: > 0 (zero excluded)
        assert!(derived_integer_matches(int, pos, "1"));
        assert!(!derived_integer_matches(int, pos, "0"));
        // negativeInteger: < 0
        assert!(derived_integer_matches(int, neg, "-2"));
        assert!(!derived_integer_matches(int, neg, "0"));
        // byte: -128..=127
        assert!(derived_integer_matches(int, byte, "127"));
        assert!(!derived_integer_matches(int, byte, "128"));
        // only the xsd:integer base is the canonical fold target; a non-integer
        // stored type or a non-numeric lexical form never matches a derived type.
        assert!(!derived_integer_matches(
            "http://www.w3.org/2001/XMLSchema#string",
            nn,
            "5"
        ));
        assert!(!derived_integer_matches(int, nn, "x"));
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

    // ── build_regex ────────────────────────────────────────────────────────────

    #[test]
    fn build_regex_rejects_unknown_flag() {
        // 'q' (XPath literal-match flag) is unsupported — must hard-fail.
        assert!(
            build_regex("foo", Some("q")).is_err(),
            "build_regex should reject unknown flag 'q'"
        );
        // Verify the error message identifies the offending character.
        let err = build_regex("foo", Some("q")).unwrap_err();
        assert!(
            err.contains('q'),
            "error message should mention the rejected flag character"
        );
    }

    #[test]
    fn build_regex_accepts_supported_flags() {
        // All four supported flags must compile without error.
        assert!(
            build_regex("foo", Some("i")).is_ok(),
            "flag 'i' should be accepted"
        );
        assert!(
            build_regex("foo", Some("s")).is_ok(),
            "flag 's' should be accepted"
        );
        assert!(
            build_regex("foo", Some("m")).is_ok(),
            "flag 'm' should be accepted"
        );
        assert!(
            build_regex("foo", Some("x")).is_ok(),
            "flag 'x' should be accepted"
        );
        assert!(
            build_regex("foo", Some("ismx")).is_ok(),
            "combined flags should be accepted"
        );
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
    fn xsd_double_rejects_plus_inf() {
        // "+INF" is NOT in the xsd:double/float lexical space (only INF, -INF, NaN).
        let dt_iri = NamedNode::new_unchecked(format!("{XSD}double"));
        let value = Term::Literal(Literal::new_typed_literal("+INF", dt_iri.clone()));
        assert!(
            !check_datatype(&value, &dt_iri),
            "+INF must not conform for xsd:double"
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
