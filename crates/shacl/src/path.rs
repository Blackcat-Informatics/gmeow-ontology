// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL property path evaluation.
//!
//! Evaluates a [`Path`] against an oxigraph [`Store`], returning the set of
//! value nodes reachable from a given focus node.

use oxigraph::model::{GraphNameRef, NamedOrBlankNodeRef, Term};
use oxigraph::store::Store;

use crate::shapes::Path;

/// Evaluate a SHACL property path from `focus`, returning all reachable value
/// nodes in the default graph.
///
/// The result set is deduplicated (preserving first occurrence order) as SHACL
/// specifies value nodes as a set.  If `focus` is a `Literal` or cannot serve
/// as a subject, returns an empty `Vec`.
pub fn eval(store: &Store, focus: &Term, path: &Path) -> Vec<Term> {
    let mut nodes = eval_inner(store, focus, path);
    // Dedup preserving first-occurrence order.
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|t| seen.insert(t.to_string()));
    nodes
}

/// Convert a [`Path`] to its term representation for use in `result_path`.
///
/// - `Predicate(p)` → `Term::NamedNode(p)`
/// - `Inverse(inner)` → the predicate IRI of the innermost predicate (SHACL
///   path serialisation as a full blank-node structure is out-of-scope for
///   #576; the predicate IRI is a faithful approximation for the corpus).
pub fn path_to_term(path: &Path) -> Term {
    match path {
        Path::Predicate(p) => Term::NamedNode(p.clone()),
        Path::Inverse(inner) => path_to_term(inner),
    }
}

// ── Internal recursive evaluator ───────────────────────────────────────────────

fn eval_inner(store: &Store, focus: &Term, path: &Path) -> Vec<Term> {
    match path {
        Path::Predicate(p) => {
            let Some(subj_ref) = term_as_subject_ref(focus) else {
                return vec![];
            };
            store
                .quads_for_pattern(
                    Some(subj_ref),
                    Some(p.as_ref()),
                    None,
                    Some(GraphNameRef::DefaultGraph),
                )
                .filter_map(|q| q.ok().map(|q| q.object))
                .collect()
        }
        Path::Inverse(inner) => match inner.as_ref() {
            // Inverse of a predicate: collect subjects of (?, p, focus).
            Path::Predicate(p) => {
                let focus_term_ref = focus.as_ref();
                store
                    .quads_for_pattern(
                        None,
                        Some(p.as_ref()),
                        Some(focus_term_ref),
                        Some(GraphNameRef::DefaultGraph),
                    )
                    .filter_map(|q| q.ok().map(|q| Term::from(q.subject)))
                    .collect()
            }
            // General inverse: eval inner with focus as "target", swap roles.
            // For any inner path, find all nodes `n` such that focus ∈ eval(n, inner).
            // This requires scanning every subject in the store — only Predicate inner
            // is needed for the corpus, but we keep it total.
            inner_path => {
                // Collect all distinct subjects from the default graph.
                let all_subjects: Vec<Term> = {
                    let mut subjects: Vec<Term> = store
                        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
                        .filter_map(|q| q.ok().map(|q| Term::from(q.subject)))
                        .collect();
                    let mut seen = std::collections::HashSet::new();
                    subjects.retain(|t| seen.insert(t.to_string()));
                    subjects
                };
                let focus_str = focus.to_string();
                all_subjects
                    .into_iter()
                    .filter(|candidate| {
                        eval_inner(store, candidate, inner_path)
                            .iter()
                            .any(|v| v.to_string() == focus_str)
                    })
                    .collect()
            }
        },
    }
}

/// Convert a `Term` to a `NamedOrBlankNodeRef` (subject position), or `None`
/// if the term is a `Literal` or `Triple`.
fn term_as_subject_ref(term: &Term) -> Option<NamedOrBlankNodeRef<'_>> {
    match term {
        Term::NamedNode(n) => Some(NamedOrBlankNodeRef::NamedNode(n.as_ref())),
        Term::BlankNode(b) => Some(NamedOrBlankNodeRef::BlankNode(b.as_ref())),
        _ => None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::RdfFormat;
    use oxigraph::model::NamedNode;

    fn load_store(ttl: &str) -> Store {
        let store = Store::new().expect("in-memory store");
        store
            .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
            .expect("turtle parse");
        store
    }

    const DATA: &str = r#"
        @prefix ex: <http://example.org/ns#> .
        ex:a ex:p ex:b .
        ex:a ex:p ex:c .
        ex:d ex:q ex:a .
    "#;

    fn nn(iri: &str) -> Term {
        Term::NamedNode(NamedNode::new_unchecked(iri))
    }

    #[test]
    fn predicate_path_returns_objects() {
        let store = load_store(DATA);
        let focus = nn("http://example.org/ns#a");
        let path = Path::Predicate(NamedNode::new_unchecked("http://example.org/ns#p"));
        let mut result = eval(&store, &focus, &path);
        result.sort_by_key(|a| a.to_string());
        assert_eq!(result.len(), 2);
        assert!(result.contains(&nn("http://example.org/ns#b")));
        assert!(result.contains(&nn("http://example.org/ns#c")));
    }

    #[test]
    fn inverse_path_returns_subjects() {
        let store = load_store(DATA);
        let focus = nn("http://example.org/ns#a");
        let path = Path::Inverse(Box::new(Path::Predicate(NamedNode::new_unchecked(
            "http://example.org/ns#q",
        ))));
        let result = eval(&store, &focus, &path);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], nn("http://example.org/ns#d"));
    }

    #[test]
    fn literal_focus_returns_empty() {
        use oxigraph::model::Literal;
        let store = load_store(DATA);
        let focus = Term::Literal(Literal::new_simple_literal("hello"));
        let path = Path::Predicate(NamedNode::new_unchecked("http://example.org/ns#p"));
        assert!(eval(&store, &focus, &path).is_empty());
    }

    #[test]
    fn predicate_path_deduplicates() {
        // If the store somehow has two identical quads (it won't, but test dedup logic)
        // by making two different predicates both point to the same object: only one path
        // but we can verify dedup doesn't break normal output either.
        let store = load_store(DATA);
        let focus = nn("http://example.org/ns#a");
        let path = Path::Predicate(NamedNode::new_unchecked("http://example.org/ns#p"));
        let result = eval(&store, &focus, &path);
        // Should be exactly 2 distinct values
        assert_eq!(result.len(), 2);
    }
}
