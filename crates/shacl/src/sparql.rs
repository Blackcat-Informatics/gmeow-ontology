// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure SPARQL evaluation helpers for SHACL-AF, over the native engine (EPIC #906).
//!
//! [`eval_target`] runs a `sh:SPARQLTarget` SELECT query and returns the bound
//! `?this` focus nodes. [`eval_sparql_constraint`] runs a `sh:SPARQLConstraint`
//! SELECT query with `$this`/`?this` pre-bound to the focus node and maps each
//! solution row to a [`ValidationResult`].
//!
//! Both run the [`NativeSparqlEngine`] over the borrowed `Arc<RdfDataset>` — there is
//! no oxigraph SPARQL engine and no materialized `Store`. Focus-node substitution
//! uses [`SparqlRequest::substitutions`] (the native replacement for oxigraph's
//! `PreparedSparqlQuery::substitute_variable`, EPIC #906 GAP-A).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gmeow_rdf::ir::RdfDataset;
use gmeow_rdf::{SparqlEngine, SparqlRequest, SparqlResult, TermValue};
use gmeow_sparql_eval::NativeSparqlEngine;

use crate::report::{Severity, ValidationResult};
use crate::term::{term_value_to_native, NamedNode, Term};

// ── Public API ────────────────────────────────────────────────────────────────

/// Execute a SHACL-AF `sh:SPARQLTarget` SELECT query against `dataset`.
///
/// The query **must** be a SELECT that binds `?this` in every solution row; any
/// other query form or a missing `?this` binding is a hard error.
///
/// The returned [`Vec<Term>`] is deduplicated and sorted by string representation
/// so the focus-node set is deterministic across runs.
///
/// # Errors
///
/// Returns `Err(String)` if execution fails, if the result is not a SELECT
/// (`Boolean` / `Graph` are rejected), or if any solution row has no `?this`
/// binding.
pub fn eval_target(dataset: &Arc<RdfDataset>, select: &str) -> Result<Vec<Term>, String> {
    let solutions = run_select(dataset, select, &[]).map_err(|e| format!("SPARQLTarget {e}"))?;

    let this_index = column_index(&solutions.0, "this");

    let mut nodes: Vec<Term> = Vec::new();
    for row in &solutions.1 {
        match this_index.and_then(|i| row.get(i)).and_then(Option::as_ref) {
            Some(value) => nodes.push(term_value_to_native(value)),
            None => {
                return Err(
                    "SPARQLTarget query produced a solution row with no ?this binding".to_owned(),
                );
            }
        }
    }

    nodes.sort_by_key(Term::to_string);
    nodes.dedup();
    Ok(nodes)
}

/// Execute a SHACL-AF `sh:SPARQLConstraint` SELECT query for a single focus node,
/// mapping each solution row to a [`ValidationResult`].
///
/// `?this` / `$this` is pre-bound to `focus` before evaluation. Each solution row
/// produces exactly one result:
///
/// | SPARQL binding | `ValidationResult` field |
/// |---|---|
/// | `?path` | `result_path` (optional) |
/// | `?value` | `value` (optional) |
///
/// `component`, `source_shape`, `severity`, and `message` are taken from the caller
/// and are the same for every row.
///
/// Results are returned in solution order; the caller (engine) sorts the final
/// report deterministically.
///
/// # Errors
///
/// Returns `Err(String)` if execution fails or if the result is not a SELECT.
pub fn eval_sparql_constraint(
    dataset: &Arc<RdfDataset>,
    focus: &Term,
    select: &str,
    component: NamedNode,
    source_shape: &Term,
    severity: Severity,
    message: Option<String>,
) -> Result<Vec<ValidationResult>, String> {
    // Evaluate the constraint query ONCE per validation (unsubstituted, so `$this`
    // is a free projected variable bound to every matching node), cache the
    // `$this → rows` grouping, then serve each focus node from the cache. This is
    // behavior-equivalent to per-focus `$this`-substitution — substitution merely
    // pre-binds `$this`, and a query that projects `$this` yields the SAME (path,
    // value) cells per node — but evaluates the query once instead of once per focus
    // node, the per-focus blowup the oxigraph path avoided with a prepared query.
    let focus_key = render_term_value(&focus.to_term_value());
    let rows = with_constraint_cache(dataset, select, |grouped| {
        grouped.get(&focus_key).cloned().unwrap_or_default()
    })?;

    let mut out: Vec<ValidationResult> = Vec::with_capacity(rows.len());
    for (result_path, value) in rows {
        out.push(ValidationResult {
            focus_node: focus.clone(),
            result_path: result_path.as_ref().map(term_value_to_native),
            value: value.as_ref().map(term_value_to_native),
            source_constraint_component: component.clone(),
            source_shape: source_shape.clone(),
            severity,
            message: message.clone(),
            source_box_roles: vec![],
            path_box_roles: vec![],
            result_box_roles: vec![],
            attributions: vec![],
        });
    }
    Ok(out)
}

/// One cached constraint solution: the optional `?path` and `?value` cells.
type ConstraintRow = (Option<TermValue>, Option<TermValue>);

thread_local! {
    /// Per-validation cache of unsubstituted `sh:sparql` constraint results, keyed by
    /// query text, grouping rows by the rendered `$this` binding. Cleared at the end
    /// of each validation by [`clear_sparql_cache`] so a later validation over a
    /// DIFFERENT dataset never reads stale rows.
    static CONSTRAINT_CACHE: RefCell<HashMap<String, HashMap<String, Vec<ConstraintRow>>>> =
        RefCell::new(HashMap::new());
}

/// Clear the per-validation SHACL-SPARQL constraint result cache.
pub fn clear_sparql_cache() {
    CONSTRAINT_CACHE.with(|c| c.borrow_mut().clear());
}

/// Evaluate `select` unsubstituted (once, memoized) and run `f` against the
/// `$this`-grouped result rows.
///
/// The cache key combines the dataset's identity (`Arc::as_ptr`) with the query text
/// so a different data graph never serves stale rows even before
/// [`clear_sparql_cache`] runs.
fn with_constraint_cache<R>(
    dataset: &Arc<RdfDataset>,
    select: &str,
    f: impl FnOnce(&HashMap<String, Vec<ConstraintRow>>) -> R,
) -> Result<R, String> {
    let key = format!("{:p}\u{0}{select}", Arc::as_ptr(dataset));
    // Populate the cache for this query if absent. The borrow is dropped before the
    // (potentially re-entrant) evaluation to avoid a double mutable borrow.
    let needs_eval = CONSTRAINT_CACHE.with(|c| !c.borrow().contains_key(&key));
    if needs_eval {
        let (variables, rows) =
            run_select(dataset, select, &[]).map_err(|e| format!("SPARQLConstraint {e}"))?;
        let this_index = column_index(&variables, "this");
        let path_index = column_index(&variables, "path");
        let value_index = column_index(&variables, "value");

        let mut grouped: HashMap<String, Vec<ConstraintRow>> = HashMap::new();
        for row in &rows {
            // A constraint row with no `$this` binding cannot be attributed to a focus
            // node; SHACL-SPARQL requires `$this`, so skip a (malformed) free row.
            let Some(this) = this_index.and_then(|i| row.get(i)).and_then(Option::as_ref) else {
                continue;
            };
            let group_key = render_term_value(this);
            let path = path_index
                .and_then(|i| row.get(i))
                .and_then(Option::as_ref)
                .cloned();
            let value = value_index
                .and_then(|i| row.get(i))
                .and_then(Option::as_ref)
                .cloned();
            grouped.entry(group_key).or_default().push((path, value));
        }
        CONSTRAINT_CACHE.with(|c| {
            c.borrow_mut().insert(key.clone(), grouped);
        });
    }
    Ok(CONSTRAINT_CACHE.with(|c| f(&c.borrow()[&key])))
}

/// A stable string key for a [`TermValue`], used to group constraint rows by their
/// `$this` binding (matches the engine's focus-node identity).
fn render_term_value(value: &TermValue) -> String {
    term_value_to_native(value).to_string()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// A materialized SELECT result: the projected variable names and the rows of
/// optional term-value cells.
type SelectRows = (Vec<String>, Vec<Vec<Option<TermValue>>>);

thread_local! {
    /// A per-thread [`NativeSparqlEngine`] reused across SHACL-AF evaluations so its
    /// query plan cache memoizes each `sh:select`/`sh:SPARQLTarget` parse across the
    /// many focus-node calls of one validation (the oxigraph path kept a pre-parsed
    /// `PreparedSparqlQuery`; a fresh engine per call would re-parse every time — a
    /// per-focus blowup on the whole-ontology conformance shapes). Validation is
    /// serial, and the cache is keyed on `(base, query text)`, so reuse is sound.
    static SPARQL_ENGINE: NativeSparqlEngine = NativeSparqlEngine::new();
}

/// Run a SELECT query over the dataset, returning `(variables, rows)`. Rejects
/// non-SELECT result forms.
fn run_select(
    dataset: &Arc<RdfDataset>,
    select: &str,
    substitutions: &[(String, TermValue)],
) -> Result<SelectRows, String> {
    let result = SPARQL_ENGINE
        .with(|engine| {
            engine.query(
                dataset,
                SparqlRequest {
                    query: select,
                    base_iri: None,
                    substitutions,
                },
            )
        })
        .map_err(|e| format!("query evaluation error: {e}"))?;
    match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Ok((variables, rows)),
        SparqlResult::Boolean(_) => {
            Err("query must be a SELECT, got a boolean (ASK) result".to_owned())
        }
        SparqlResult::Graph(_) => {
            Err("query must be a SELECT, got a graph (CONSTRUCT/DESCRIBE) result".to_owned())
        }
    }
}

/// The column index of variable `name` in a result header.
fn column_index(variables: &[String], name: &str) -> Option<usize> {
    variables.iter().position(|v| v == name)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gmeow_rdf::ir::RdfDataset;

    use super::*;
    use crate::report::Severity;
    use crate::term::{NamedNode, Term};

    /// Build a tiny frozen dataset from a slice of N-Triples lines.
    fn dataset_from_ntriples(lines: &[&str]) -> Arc<RdfDataset> {
        let ntriples = lines.join("\n");
        if ntriples.is_empty() {
            return crate::text_ingest::parse_ntriples_to_dataset("").expect("empty dataset");
        }
        crate::text_ingest::parse_ntriples_to_dataset(&ntriples).expect("valid N-Triples")
    }

    fn named_term(iri: &str) -> Term {
        Term::NamedNode(NamedNode::new_unchecked(iri))
    }

    fn dummy_shape() -> Term {
        named_term("http://example.org/Shape")
    }

    fn dummy_component() -> NamedNode {
        NamedNode::new_unchecked("http://www.w3.org/ns/shacl#SPARQLConstraintComponent")
    }

    // ── eval_target ───────────────────────────────────────────────────────────

    #[test]
    fn eval_target_returns_foo_instances() {
        let dataset = dataset_from_ntriples(&[
            "<http://example.org/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> .",
            "<http://example.org/b> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> .",
            "<http://example.org/c> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Bar> .",
        ]);

        let select = "SELECT ?this WHERE { ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Foo> }";
        let nodes = eval_target(&dataset, select).expect("eval_target must succeed");

        assert_eq!(nodes.len(), 2, "exactly two Foo instances");
        assert!(nodes.contains(&named_term("http://example.org/a")));
        assert!(nodes.contains(&named_term("http://example.org/b")));
        assert!(!nodes.contains(&named_term("http://example.org/c")));
        // Verify sorted order.
        let sorted = {
            let mut v = nodes.clone();
            v.sort_by_key(Term::to_string);
            v
        };
        assert_eq!(nodes, sorted, "result must be sorted");
    }

    #[test]
    fn eval_target_deduplicates() {
        let dataset = dataset_from_ntriples(&[]);
        let select =
            "SELECT ?this WHERE { VALUES ?this { <http://example.org/x> <http://example.org/x> } }";
        let nodes = eval_target(&dataset, select).expect("eval_target must succeed");
        assert_eq!(nodes.len(), 1, "duplicate binding must be deduped");
    }

    // ── eval_sparql_constraint ────────────────────────────────────────────────

    #[test]
    fn eval_sparql_constraint_self_reference() {
        let dataset = dataset_from_ntriples(&[
            "<http://example.org/self-node> <http://example.org/self> <http://example.org/self-node> .",
        ]);

        let select = "SELECT $this WHERE { $this <http://example.org/self> $this }";

        // Focus = the self-referencing node → one result.
        let focus_self = named_term("http://example.org/self-node");
        let results = eval_sparql_constraint(
            &dataset,
            &focus_self,
            select,
            dummy_component(),
            &dummy_shape(),
            Severity::Violation,
            None,
        )
        .expect("eval must succeed for self-referencing focus");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].focus_node, focus_self);
        assert_eq!(results[0].severity, Severity::Violation);
        assert_eq!(results[0].result_path, None);
        assert_eq!(results[0].value, None);

        // Focus = an unrelated node → zero results.
        let focus_other = named_term("http://example.org/other");
        let results_other = eval_sparql_constraint(
            &dataset,
            &focus_other,
            select,
            dummy_component(),
            &dummy_shape(),
            Severity::Violation,
            None,
        )
        .expect("eval must succeed for non-matching focus");
        assert_eq!(results_other.len(), 0);
    }
}
