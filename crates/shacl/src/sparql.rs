// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure SPARQL evaluation helpers for SHACL-AF.
//!
//! [`eval_target`] runs a `sh:SPARQLTarget` SELECT query and returns the
//! bound `?this` focus nodes.  [`eval_sparql_constraint`] runs a
//! `sh:SPARQLConstraint` SELECT query with `?this` substituted to the focus
//! node and maps each solution row to a [`ValidationResult`].

use oxigraph::model::{NamedNode, Term, Variable};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
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
/// `prepared` is a pre-parsed query (parsed once at shape-load time). It is
/// cheaply cloned here so `.on_store()` can consume it.
///
/// # Errors
///
/// Returns `Err(String)` if execution fails, if the result is not a SELECT
/// (`Boolean` / `Graph` are rejected), or if any solution row has no `?this`
/// binding.
pub fn eval_target(store: &Store, prepared: &PreparedSparqlQuery) -> Result<Vec<Term>, String> {
    let results = prepared
        .clone()
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
/// `prepared` is a pre-parsed query (parsed once at shape-load time). It is
/// cloned here so `substitute_variable` can consume it per focus-node call.
///
/// Results are returned in solution order; the caller (engine) is responsible
/// for deterministic sorting of the final report.
///
/// # Errors
///
/// Returns `Err(String)` if execution fails or if the result is not a SELECT.
pub fn eval_sparql_constraint(
    store: &Store,
    focus: &Term,
    prepared: &PreparedSparqlQuery,
    component: NamedNode,
    source_shape: &Term,
    severity: Severity,
    message: Option<String>,
) -> Result<Vec<ValidationResult>, String> {
    let this_var =
        Variable::new("this").map_err(|e| format!("variable 'this' parse error: {e}"))?;

    let results = prepared
        .clone()
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
    use oxigraph::sparql::SparqlEvaluator;
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

    /// Helper: parse a SPARQL query string into a `PreparedSparqlQuery`.
    fn parse(select: &str) -> PreparedSparqlQuery {
        SparqlEvaluator::new()
            .parse_query(select)
            .expect("test query must be valid SPARQL")
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

        let prepared = parse("SELECT ?this WHERE { ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> }");
        let nodes = eval_target(&store, &prepared).expect("eval_target must succeed");

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
        let prepared = parse(
            "SELECT ?this WHERE { VALUES ?this { <http://example.org/x> <http://example.org/x> } }",
        );
        let nodes = eval_target(&store, &prepared).expect("eval_target must succeed");
        assert_eq!(
            nodes.len(),
            1,
            "duplicate binding must be deduped to one entry"
        );
    }

    /// Querying with a malformed SPARQL string must fail at parse time (before
    /// `eval_target` is called).  This test verifies the parse step itself.
    #[test]
    fn sparql_parse_error_before_eval_target() {
        let result = SparqlEvaluator::new().parse_query("SELECT ?this WHERE {");
        assert!(result.is_err(), "malformed query must fail to parse");
        // Verify the error is non-empty; the exact format is owned by spargebra.
        let msg = result.err().unwrap().to_string();
        assert!(!msg.is_empty(), "error message must be non-empty");
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

        let prepared = parse("SELECT $this WHERE { $this <http://example.org/self> $this }");

        // Focus = the self-referencing node → one result
        let focus_self = Term::NamedNode(self_iri);
        let results = eval_sparql_constraint(
            &store,
            &focus_self,
            &prepared,
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
            &prepared,
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

    /// Querying with a malformed SPARQL string must fail at parse time (before
    /// `eval_sparql_constraint` is called).  This test verifies the parse step itself.
    #[test]
    fn sparql_parse_error_before_eval_constraint() {
        let result = SparqlEvaluator::new().parse_query("SELECT $this WHERE {");
        assert!(result.is_err(), "malformed query must fail to parse");
    }
}
