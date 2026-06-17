// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure SPARQL evaluation helpers for SHACL-AF.
//!
//! [`eval_target`] runs a `sh:SPARQLTarget` SELECT query and returns the
//! bound `?this` focus nodes.  [`eval_sparql_constraint`] runs a
//! `sh:SPARQLConstraint` SELECT query with `?this` substituted to the focus
//! node and maps each solution row to a [`ValidationResult`].

use oxigraph::model::{NamedNode, Term, Variable};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use crate::report::{Severity, ValidationResult};

// ── Public API ────────────────────────────────────────────────────────────────

/// Execute a SHACL-AF `sh:SPARQLTarget` SELECT query against `store`.
///
/// The query **must** be a SELECT that binds `?this` in every solution row;
/// any other query form or a missing `?this` binding is a hard error.
///
/// The returned [`Vec<Term>`] is deduplicated and sorted by string
/// representation so the focus-node set is deterministic across runs.
///
/// # Errors
///
/// Returns `Err(String)` if the query cannot be parsed, if execution fails,
/// if the result is not a SELECT (`Boolean` / `Graph` are rejected), or if
/// any solution row has no `?this` binding.
pub fn eval_target(store: &Store, select: &str) -> Result<Vec<Term>, String> {
    let results = SparqlEvaluator::new()
        .parse_query(select)
        .map_err(|e| format!("SPARQLTarget query parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("SPARQLTarget query evaluation error: {e}"))?;

    let solutions = match results {
        QueryResults::Solutions(s) => s,
        QueryResults::Boolean(_) => {
            return Err(
                "SPARQLTarget query must be a SELECT, got a boolean (ASK) result".to_owned(),
            );
        }
        QueryResults::Graph(_) => {
            return Err(
                "SPARQLTarget query must be a SELECT, got a graph (CONSTRUCT/DESCRIBE) result"
                    .to_owned(),
            );
        }
    };

    let mut nodes: Vec<Term> = Vec::new();
    for sol in solutions {
        let sol = sol.map_err(|e| format!("SPARQLTarget solution error: {e}"))?;
        match sol.get("this") {
            Some(t) => nodes.push(t.clone()),
            None => {
                return Err(
                    "SPARQLTarget query produced a solution row with no ?this binding".to_owned(),
                );
            }
        }
    }

    nodes.sort_by_key(|t| t.to_string());
    nodes.dedup();
    Ok(nodes)
}

/// Execute a SHACL-AF `sh:SPARQLConstraint` SELECT query for a single focus
/// node, mapping each solution row to a [`ValidationResult`].
///
/// `?this` / `$this` is substituted with `focus` before evaluation.  Each
/// solution row produces exactly one result:
///
/// | SPARQL binding | `ValidationResult` field |
/// |---|---|
/// | `?path` | `result_path` (optional) |
/// | `?value` | `value` (optional) |
///
/// `component`, `source_shape`, `severity`, and `message` are taken from the
/// caller (the shape evaluator) and are the same for every row.
///
/// Results are returned in solution order; the caller (engine) is responsible
/// for deterministic sorting of the final report.
///
/// # Errors
///
/// Returns `Err(String)` if the query cannot be parsed, if execution fails,
/// or if the result is not a SELECT.
pub fn eval_sparql_constraint(
    store: &Store,
    focus: &Term,
    select: &str,
    component: NamedNode,
    source_shape: &Term,
    severity: Severity,
    message: Option<String>,
) -> Result<Vec<ValidationResult>, String> {
    let this_var =
        Variable::new("this").map_err(|e| format!("variable 'this' parse error: {e}"))?;

    let results = SparqlEvaluator::new()
        .parse_query(select)
        .map_err(|e| format!("SPARQLConstraint query parse error: {e}"))?
        .substitute_variable(this_var, focus.clone())
        .on_store(store)
        .execute()
        .map_err(|e| format!("SPARQLConstraint query evaluation error: {e}"))?;

    let solutions = match results {
        QueryResults::Solutions(s) => s,
        QueryResults::Boolean(_) => {
            return Err(
                "SPARQLConstraint query must be a SELECT, got a boolean (ASK) result".to_owned(),
            );
        }
        QueryResults::Graph(_) => {
            return Err(
                "SPARQLConstraint query must be a SELECT, got a graph (CONSTRUCT/DESCRIBE) result"
                    .to_owned(),
            );
        }
    };

    let mut out: Vec<ValidationResult> = Vec::new();
    for sol in solutions {
        let sol = sol.map_err(|e| format!("SPARQLConstraint solution error: {e}"))?;
        out.push(ValidationResult {
            focus_node: focus.clone(),
            result_path: sol.get("path").cloned(),
            value: sol.get("value").cloned(),
            source_constraint_component: component.clone(),
            source_shape: source_shape.clone(),
            severity,
            message: message.clone(),
            source_box_roles: vec![],
            path_box_roles: vec![],
            result_box_roles: vec![],
        });
    }
    Ok(out)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use oxigraph::model::{GraphName, NamedNode, NamedOrBlankNode, Quad};
    use oxigraph::store::Store;

    use super::*;
    use crate::report::Severity;

    /// Build a tiny in-memory store from a slice of N-Triples lines.
    fn store_from_ntriples(lines: &[&str]) -> Store {
        use oxigraph::io::RdfFormat;
        let store = Store::new().expect("in-memory store");
        let ntriples = lines.join("\n");
        store
            .load_from_reader(RdfFormat::NTriples, ntriples.as_bytes())
            .expect("valid N-Triples");
        store
    }

    fn named(iri: &str) -> NamedNode {
        NamedNode::new_unchecked(iri)
    }

    fn named_term(iri: &str) -> Term {
        Term::NamedNode(named(iri))
    }

    fn dummy_shape() -> Term {
        named_term("http://example.org/Shape")
    }

    fn dummy_component() -> NamedNode {
        named("http://www.w3.org/ns/shacl#SPARQLConstraintComponent")
    }

    // ── eval_target ───────────────────────────────────────────────────────────

    /// A store with two <Foo> instances; the target query must return exactly
    /// those two nodes, deduplicated and sorted.
    #[test]
    fn eval_target_returns_foo_instances() {
        let store = store_from_ntriples(&[
            "<http://example.org/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> .",
            "<http://example.org/b> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> .",
            "<http://example.org/c> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Bar> .",
        ]);

        let select = "SELECT ?this WHERE { ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> }";
        let nodes = eval_target(&store, select).expect("eval_target must succeed");

        assert_eq!(nodes.len(), 2, "exactly two Foo instances");
        assert!(
            nodes.contains(&named_term("http://example.org/a")),
            "must include <a>"
        );
        assert!(
            nodes.contains(&named_term("http://example.org/b")),
            "must include <b>"
        );
        assert!(
            !nodes.contains(&named_term("http://example.org/c")),
            "must not include <c>"
        );
        // Verify sorted order
        let sorted = {
            let mut v = nodes.clone();
            v.sort_by_key(|t| t.to_string());
            v
        };
        assert_eq!(nodes, sorted, "result must be sorted");
    }

    /// Duplicate ?this bindings in the solution set must be deduped.
    #[test]
    fn eval_target_deduplicates() {
        // VALUES produces the same IRI twice; dedup must collapse it.
        let store = Store::new().expect("in-memory store");
        let select =
            "SELECT ?this WHERE { VALUES ?this { <http://example.org/x> <http://example.org/x> } }";
        let nodes = eval_target(&store, select).expect("eval_target must succeed");
        assert_eq!(
            nodes.len(),
            1,
            "duplicate binding must be deduped to one entry"
        );
    }

    /// A malformed query string must return Err.
    #[test]
    fn eval_target_parse_error() {
        let store = Store::new().expect("in-memory store");
        let result = eval_target(&store, "SELECT ?this WHERE {");
        assert!(result.is_err(), "malformed query must return Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("parse error") || msg.contains("Parse") || msg.contains("syntax"),
            "error message must mention parse: {msg}"
        );
    }

    // ── eval_sparql_constraint ────────────────────────────────────────────────

    /// Insert one self-referencing triple; the constraint must fire for that
    /// focus node and return zero results for a non-self-referencing focus.
    #[test]
    fn eval_sparql_constraint_self_reference() {
        // Build store manually with a single quad
        let store = Store::new().expect("in-memory store");
        let self_iri = named("http://example.org/self-node");
        let pred = named("http://example.org/self");
        store
            .insert(&Quad::new(
                NamedOrBlankNode::NamedNode(self_iri.clone()),
                pred.clone(),
                Term::NamedNode(self_iri.clone()),
                GraphName::DefaultGraph,
            ))
            .expect("insert");

        let select = "SELECT $this WHERE { $this <http://example.org/self> $this }";

        // Focus = the self-referencing node → one result
        let focus_self = Term::NamedNode(self_iri);
        let results = eval_sparql_constraint(
            &store,
            &focus_self,
            select,
            dummy_component(),
            &dummy_shape(),
            Severity::Violation,
            None,
        )
        .expect("eval must succeed for self-referencing focus");
        assert_eq!(
            results.len(),
            1,
            "self-referencing focus must yield one result"
        );
        assert_eq!(results[0].focus_node, focus_self);
        assert_eq!(results[0].severity, Severity::Violation);
        assert_eq!(results[0].result_path, None);
        assert_eq!(results[0].value, None);

        // Focus = an unrelated node → zero results (substitution filters it out)
        let focus_other = named_term("http://example.org/other");
        let results_other = eval_sparql_constraint(
            &store,
            &focus_other,
            select,
            dummy_component(),
            &dummy_shape(),
            Severity::Violation,
            None,
        )
        .expect("eval must succeed for non-matching focus");
        assert_eq!(
            results_other.len(),
            0,
            "non-matching focus must yield zero results"
        );
    }

    /// A malformed constraint query must return Err.
    #[test]
    fn eval_sparql_constraint_parse_error() {
        let store = Store::new().expect("in-memory store");
        let focus = named_term("http://example.org/x");
        let result = eval_sparql_constraint(
            &store,
            &focus,
            "SELECT $this WHERE {",
            dummy_component(),
            &dummy_shape(),
            Severity::Violation,
            None,
        );
        assert!(result.is_err(), "malformed query must return Err");
    }
}
