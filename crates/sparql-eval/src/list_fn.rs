// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW `rdf:List` SPARQL extension functions.
//!
//! These bind the FnO list primitives — `listLength`, `listGet`, `listIndexOf`,
//! `listSlice`, `listConcat`, `listContains` — to executable SPARQL custom
//! functions, so a query can write `gmeow:listLength(?list)` directly. They are
//! dispatched from the `Function::Custom(iri)` arm of [`crate::expr`].
//!
//! Two shapes:
//!
//! * **Scalar readers** (`listLength`/`listGet`/`listIndexOf`/`listContains`) walk
//!   the `rdf:first`/`rdf:rest` chain in the dataset and return a single term. These
//!   mirror the reasoning-layer recursion (conformance case
//!   `goal-rdf-list-functions`) — parity is the contract.
//! * **Constructors** (`listSlice`/`listConcat`) invent a fresh `rdf:List`. Because
//!   a SPARQL expression returns one term, the new cells are emitted into the
//!   per-query constructed-quads buffer on [`EvalCtx`] and surface at the result
//!   boundary (CONSTRUCT output and the SELECT auxiliary graph). See
//!   [`materialize_list`].
//!
//! The walk is cycle-guarded: a cyclic or torn `rdf:List` is malformed input and
//! hard-fails ([`EvalError::Data`]) rather than looping forever.

use gmeow_rdf_core::{TermId, TermValue};
use gmeow_xsd::XsdValue;

use crate::error::EvalError;
use crate::eval::EvalCtx;
use crate::expr::xsd_of;
use crate::scratch::{term_id_to_value, SolutionTerm};
use crate::DetHashSet;

/// The GMEOW vocabulary namespace; function IRIs are `GMEOW_NS + local`.
pub(crate) const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

// FnO local names (the IRI is `GMEOW_NS + name`).
const LIST_LENGTH: &str = "listLength";
const LIST_GET: &str = "listGet";
const LIST_INDEX_OF: &str = "listIndexOf";
const LIST_SLICE: &str = "listSlice";
const LIST_CONCAT: &str = "listConcat";
const LIST_CONTAINS: &str = "listContains";

/// Dispatch a GMEOW list function by its full IRI.
///
/// Returns `None` when `iri` is not a GMEOW list function (the caller then falls
/// through to the generic `unsupported` error), and `Some(result)` otherwise, where
/// the inner result follows the usual expression contract: `Ok(Some)` is a value,
/// `Ok(None)` is a SPARQL error/unbound, and `Err` is a hard failure.
pub(crate) fn dispatch(
    iri: &str,
    vals: &[Option<TermValue>],
    ctx: &mut EvalCtx<'_>,
) -> Option<Result<Option<SolutionTerm>, EvalError>> {
    let local = iri.strip_prefix(GMEOW_NS)?;
    let result = match local {
        LIST_LENGTH => list_length(ctx, vals),
        LIST_GET => list_get(ctx, vals),
        LIST_INDEX_OF => list_index_of(ctx, vals),
        LIST_CONTAINS => list_contains(ctx, vals),
        LIST_SLICE => list_slice(ctx, vals),
        LIST_CONCAT => list_concat(ctx, vals),
        _ => return None,
    };
    Some(result)
}

/// `gmeow:listLength(list)` → the number of members, as `xsd:integer`. A non-list
/// argument yields a SPARQL error (`Ok(None)`).
fn list_length(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some(head) = arg(vals, 0) else {
        return Ok(None);
    };
    match walk(ctx, head)? {
        Some(members) => Ok(Some(integer_term(ctx, members.len() as i64))),
        None => Ok(None),
    }
}

/// `gmeow:listGet(list, index)` → the zero-based member, or a SPARQL error when the
/// index is out of range / not an integer.
fn list_get(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some(head), Some(index)) = (arg(vals, 0), arg(vals, 1)) else {
        return Ok(None);
    };
    let Some(idx) = as_index(index) else {
        return Ok(None);
    };
    let Some(members) = walk(ctx, head)? else {
        return Ok(None);
    };
    if idx < 0 {
        return Ok(None);
    }
    match members.into_iter().nth(idx as usize) {
        Some(value) => Ok(Some(intern(ctx, value))),
        None => Ok(None),
    }
}

/// `gmeow:listIndexOf(list, value)` → the zero-based index of the first occurrence,
/// or a SPARQL error when the value is absent.
fn list_index_of(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some(head), Some(value)) = (arg(vals, 0), arg(vals, 1)) else {
        return Ok(None);
    };
    let Some(members) = walk(ctx, head)? else {
        return Ok(None);
    };
    match members.iter().position(|m| m == value) {
        Some(pos) => Ok(Some(integer_term(ctx, pos as i64))),
        None => Ok(None),
    }
}

/// `gmeow:listContains(list, value)` → `xsd:boolean`. A non-list argument yields a
/// SPARQL error (`Ok(None)`); membership over a valid (possibly empty) list is total.
fn list_contains(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some(head), Some(value)) = (arg(vals, 0), arg(vals, 1)) else {
        return Ok(None);
    };
    let Some(members) = walk(ctx, head)? else {
        return Ok(None);
    };
    Ok(Some(bool_term(ctx, members.iter().any(|m| m == value))))
}

/// `gmeow:listSlice(list, start, end)` → a fresh `rdf:List` of the members in the
/// half-open index range `[start, end)` (clamped to the list bounds; an empty range
/// yields `rdf:nil`). Implemented in Task 2.
fn list_slice(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let _ = (ctx, vals);
    Err(EvalError::unsupported("gmeow:listSlice (pending Task 2)"))
}

/// `gmeow:listConcat(listA, listB)` → a fresh `rdf:List` of A's members followed by
/// B's. Implemented in Task 2.
fn list_concat(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let _ = (ctx, vals);
    Err(EvalError::unsupported("gmeow:listConcat (pending Task 2)"))
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

/// The argument value at index `i`, if bound (not unbound/error).
fn arg(vals: &[Option<TermValue>], i: usize) -> Option<&TermValue> {
    vals.get(i).and_then(|v| v.as_ref())
}

/// Extract a zero-based index from an `xsd:integer`-derived literal.
fn as_index(value: &TermValue) -> Option<i64> {
    match xsd_of(value)? {
        XsdValue::Integer { value, .. } => i64::try_from(value).ok(),
        _ => None,
    }
}

/// Intern a value to a solution term (promoting to an existing dataset id).
fn intern(ctx: &mut EvalCtx<'_>, value: TermValue) -> SolutionTerm {
    ctx.scratch.intern(ctx.dataset, value)
}

/// Intern an `xsd:integer` literal.
fn integer_term(ctx: &mut EvalCtx<'_>, value: i64) -> SolutionTerm {
    intern(ctx, typed(&value.to_string(), XSD_INTEGER))
}

/// Intern an `xsd:boolean` literal.
fn bool_term(ctx: &mut EvalCtx<'_>, b: bool) -> SolutionTerm {
    intern(ctx, typed(if b { "true" } else { "false" }, XSD_BOOLEAN))
}

const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// Build a typed (no-language) literal value.
fn typed(lexical: &str, datatype: &str) -> TermValue {
    TermValue::Literal {
        lexical_form: lexical.to_owned(),
        datatype: datatype.to_owned(),
        language: None,
        direction: None,
    }
}

/// Walk an `rdf:List` from `head`, returning its member values in order.
///
/// * `Ok(Some(members))` — a well-formed list (an empty list, i.e. `rdf:nil`, gives
///   `[]`).
/// * `Ok(None)` — `head` is not a list node we can read: it is `rdf:nil`-free, not
///   interned in the active dataset, or has no `rdf:first` (a SPARQL error — the
///   function yields unbound).
/// * `Err(EvalError::Data)` — a cyclic or torn list (a cell revisited, or an
///   interior cell missing `rdf:first`/`rdf:rest`): malformed input, a hard fail.
fn walk(ctx: &EvalCtx<'_>, head: &TermValue) -> Result<Option<Vec<TermValue>>, EvalError> {
    // The empty list is `rdf:nil`, whether or not it happens to be interned.
    if is_nil(head) {
        return Ok(Some(Vec::new()));
    }
    // The list nodes and edges must exist in the active dataset to be walkable.
    let (Some(first_id), Some(rest_id), Some(nil_id)) = (
        ctx.dataset.term_id_by_value(&iri(RDF_FIRST)),
        ctx.dataset.term_id_by_value(&iri(RDF_REST)),
        ctx.dataset.term_id_by_value(&iri(RDF_NIL)),
    ) else {
        // No list vocabulary in the dataset at all — `head` is not a readable list.
        return Ok(None);
    };
    let Some(head_id) = ctx.dataset.term_id_by_value(head) else {
        return Ok(None);
    };

    let scope = ctx.active_dataset.scope_for(ctx.active_graph);
    let mut members: Vec<TermValue> = Vec::new();
    let mut seen: DetHashSet<TermId> = DetHashSet::default();
    let mut cur = head_id;
    loop {
        if cur == nil_id {
            return Ok(Some(members));
        }
        if !seen.insert(cur) {
            return Err(EvalError::data(
                "cyclic rdf:List (a cell is reachable from itself)",
            ));
        }

        let mut first_obj: Option<TermId> = None;
        scope.for_each_quad(ctx.dataset, Some(cur), Some(first_id), None, |q| {
            first_obj = Some(q.o);
        });
        let Some(fo) = first_obj else {
            // No `rdf:first`: the head is simply not a list (SPARQL error); an
            // interior cell without `rdf:first` is a torn list (hard fail).
            if members.is_empty() {
                return Ok(None);
            }
            return Err(EvalError::data("rdf:List cell missing rdf:first"));
        };
        members.push(term_id_to_value(ctx.dataset, fo));

        let mut rest_obj: Option<TermId> = None;
        scope.for_each_quad(ctx.dataset, Some(cur), Some(rest_id), None, |q| {
            rest_obj = Some(q.o);
        });
        let Some(ro) = rest_obj else {
            return Err(EvalError::data("rdf:List cell missing rdf:rest"));
        };
        cur = ro;
    }
}

/// An IRI value.
fn iri(s: &str) -> TermValue {
    TermValue::Iri(s.to_owned())
}

/// Whether a term is `rdf:nil`.
fn is_nil(value: &TermValue) -> bool {
    matches!(value, TermValue::Iri(i) if i == RDF_NIL)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder, TermValue};

    use crate::error::EvalError;
    use crate::eval::{evaluate_query, EvalCtx, Outcome};

    /// The three-element list `(x y z)` rooted at `ex:l0`, plus an anchor triple
    /// `ex:q ex:list ex:l0` so a BGP can bind the head.
    fn list_ds() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let first = b.intern_iri(super::RDF_FIRST.to_owned());
        let rest = b.intern_iri(super::RDF_REST.to_owned());
        let nil = b.intern_iri(super::RDF_NIL.to_owned());
        let l0 = b.intern_iri("http://ex/l0".to_owned());
        let l1 = b.intern_iri("http://ex/l1".to_owned());
        let l2 = b.intern_iri("http://ex/l2".to_owned());
        let x = b.intern_iri("http://ex/x".to_owned());
        let y = b.intern_iri("http://ex/y".to_owned());
        let z = b.intern_iri("http://ex/z".to_owned());
        let q = b.intern_iri("http://ex/q".to_owned());
        let list = b.intern_iri("http://ex/list".to_owned());
        b.push_quad(l0, first, x, None);
        b.push_quad(l0, rest, l1, None);
        b.push_quad(l1, first, y, None);
        b.push_quad(l1, rest, l2, None);
        b.push_quad(l2, first, z, None);
        b.push_quad(l2, rest, nil, None);
        b.push_quad(q, list, l0, None);
        b.freeze().expect("freeze")
    }

    /// Run `query` and return sorted stringified rows (a multiset comparison).
    fn rows(ds: &RdfDataset, query: &str) -> Vec<Vec<String>> {
        use gmeow_sparql_algebra::SparqlParser;
        let parsed = SparqlParser::new().parse_query(query).expect("parse");
        let mut ctx = EvalCtx::new(ds);
        match evaluate_query(&parsed, &mut ctx).expect("eval") {
            Outcome::Solutions(seq) => {
                let mut out: Vec<Vec<String>> = seq
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|c| match c {
                                None => "UNBOUND".to_owned(),
                                Some(t) => match ctx.scratch.value_of(ctx.dataset, *t) {
                                    TermValue::Iri(i) => format!("<{i}>"),
                                    TermValue::Literal { lexical_form, .. } => lexical_form,
                                    TermValue::Blank { label, .. } => format!("_:{label}"),
                                    TermValue::Triple { .. } => "<<triple>>".to_owned(),
                                },
                            })
                            .collect()
                    })
                    .collect();
                out.sort();
                out
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    /// Evaluate a query expected to hard-fail, returning the error.
    fn eval_err(ds: &RdfDataset, query: &str) -> EvalError {
        use gmeow_sparql_algebra::SparqlParser;
        let parsed = SparqlParser::new().parse_query(query).expect("parse");
        let mut ctx = EvalCtx::new(ds);
        evaluate_query(&parsed, &mut ctx).expect_err("expected a hard failure")
    }

    const PREFIX: &str = "PREFIX g: <https://blackcatinformatics.ca/gmeow/> ";

    #[test]
    fn list_length_counts_members() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?n WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listLength(?l) AS ?n) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["3".to_owned()]]);
    }

    #[test]
    fn list_length_of_nil_is_zero() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?n WHERE {{ \
             BIND(g:listLength(<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>) AS ?n) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["0".to_owned()]]);
    }

    #[test]
    fn list_get_returns_indexed_member() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?x WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listGet(?l, 1) AS ?x) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["<http://ex/y>".to_owned()]]);
    }

    #[test]
    fn list_get_out_of_range_is_unbound() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?x WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listGet(?l, 5) AS ?x) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["UNBOUND".to_owned()]]);
    }

    #[test]
    fn list_index_of_finds_value() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?n WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listIndexOf(?l, <http://ex/z>) AS ?n) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["2".to_owned()]]);
    }

    #[test]
    fn list_index_of_absent_is_unbound() {
        let ds = list_ds();
        let q = format!(
            "{PREFIX} SELECT ?n WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listIndexOf(?l, <http://ex/absent>) AS ?n) }}"
        );
        assert_eq!(rows(&ds, &q), vec![vec!["UNBOUND".to_owned()]]);
    }

    #[test]
    fn list_contains_true_and_false() {
        let ds = list_ds();
        let q_true = format!(
            "{PREFIX} SELECT ?b WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listContains(?l, <http://ex/y>) AS ?b) }}"
        );
        assert_eq!(rows(&ds, &q_true), vec![vec!["true".to_owned()]]);
        let q_false = format!(
            "{PREFIX} SELECT ?b WHERE {{ ?q <http://ex/list> ?l . \
             BIND(g:listContains(?l, <http://ex/absent>) AS ?b) }}"
        );
        assert_eq!(rows(&ds, &q_false), vec![vec!["false".to_owned()]]);
    }

    #[test]
    fn unknown_custom_function_still_hard_fails() {
        let ds = list_ds();
        let q = format!("{PREFIX} SELECT ?x WHERE {{ BIND(g:notAListFunction(1) AS ?x) }}");
        let err = eval_err(&ds, &q);
        assert!(matches!(err, EvalError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn cyclic_list_is_a_hard_data_error() {
        // l0 -> first x, rest l1 ; l1 -> first y, rest l0  (a cycle, no rdf:nil).
        let mut b = RdfDatasetBuilder::new();
        let first = b.intern_iri(super::RDF_FIRST.to_owned());
        let rest = b.intern_iri(super::RDF_REST.to_owned());
        let nil = b.intern_iri(super::RDF_NIL.to_owned()); // present so the walk starts
        let l0 = b.intern_iri("http://ex/l0".to_owned());
        let l1 = b.intern_iri("http://ex/l1".to_owned());
        let x = b.intern_iri("http://ex/x".to_owned());
        let y = b.intern_iri("http://ex/y".to_owned());
        let z = b.intern_iri("http://ex/z".to_owned());
        b.push_quad(l0, first, x, None);
        b.push_quad(l0, rest, l1, None);
        b.push_quad(l1, first, y, None);
        b.push_quad(l1, rest, l0, None);
        // A well-formed terminator elsewhere so rdf:nil is interned.
        b.push_quad(z, rest, nil, None);
        let ds = b.freeze().expect("freeze");

        let q = format!("{PREFIX} SELECT ?n WHERE {{ BIND(g:listLength(<http://ex/l0>) AS ?n) }}");
        let err = eval_err(&ds, &q);
        assert!(matches!(err, EvalError::Data(_)), "got {err:?}");
        assert!(err.to_string().contains("cyclic"));
    }
}
