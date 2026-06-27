// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Template instantiation shared by `CONSTRUCT` and SPARQL `UPDATE`.
//!
//! Both the `CONSTRUCT` output template ([`construct`](crate::construct)) and the
//! `DELETE`/`INSERT` quad templates of an UPDATE operation ([`update`](crate::update))
//! turn a triple/quad *pattern* into concrete [`TermValue`]s, once per solution row,
//! under the same three SPARQL §16.2 rules:
//!
//! 1. A template position holding an **unbound variable** makes the whole quad be
//!    skipped (`None`).
//! 2. A template **blank node is minted fresh per solution row** — the same label
//!    co-refers within one row (the `blanks` map), distinct across rows (the
//!    cross-row monotonic [`EvalCtx::bnode_counter`]).
//! 3. **Positional validity** (a literal in subject position / a non-IRI predicate)
//!    is decided by the *caller* after instantiation, because the two callers intern
//!    into different sinks (a builder vs. a [`MutableDataset`](gmeow_rdf_core::MutableDataset)).
//!
//! These helpers stop at the dataset-independent [`TermValue`]: a bound variable is
//! resolved via `ctx.scratch.value_of(ctx.dataset, term)`, so the value is valid
//! across a snapshot→mutable boundary (the UPDATE round-trip).

use gmeow_rdf_core::{BlankScope, TermValue};
use gmeow_sparql_algebra::{NamedNodePattern, TermPattern};

use crate::convert::{literal_to_value, named_node_to_value};
use crate::eval::EvalCtx;
use crate::solution::{Solution, VarSchema};
use crate::DetHashMap;

/// Instantiate a subject/object template term. `None` = an unbound variable.
pub(crate) fn instantiate_term(
    term: &TermPattern,
    row: &Solution,
    schema: &VarSchema,
    blanks: &mut DetHashMap<String, String>,
    ctx: &mut EvalCtx<'_>,
) -> Option<TermValue> {
    match term {
        TermPattern::NamedNode(n) => Some(named_node_to_value(n)),
        TermPattern::Literal(l) => Some(literal_to_value(l)),
        TermPattern::Variable(v) => {
            let term = schema.index_of(v).and_then(|c| row[c])?;
            Some(ctx.scratch.value_of(ctx.dataset, term))
        }
        TermPattern::BlankNode(b) => Some(fresh_blank(b.as_str(), blanks, ctx)),
        TermPattern::Triple(t) => {
            // RDF 1.2 quoted-triple term in the template: instantiate recursively.
            let s = instantiate_term(&t.subject, row, schema, blanks, ctx)?;
            let p = instantiate_predicate(&t.predicate, row, schema, ctx)?;
            let o = instantiate_term(&t.object, row, schema, blanks, ctx)?;
            Some(TermValue::Triple {
                s: Box::new(s),
                p: Box::new(p),
                o: Box::new(o),
            })
        }
    }
}

/// Instantiate a predicate template position. `None` = an unbound variable.
pub(crate) fn instantiate_predicate(
    predicate: &NamedNodePattern,
    row: &Solution,
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Option<TermValue> {
    match predicate {
        NamedNodePattern::NamedNode(n) => Some(named_node_to_value(n)),
        NamedNodePattern::Variable(v) => {
            let term = schema.index_of(v).and_then(|c| row[c])?;
            Some(ctx.scratch.value_of(ctx.dataset, term))
        }
    }
}

/// The fresh blank value for a template label within the current solution row: the
/// first occurrence mints a globally-unique label from the **cross-row** monotonic
/// `bnode_counter`, later occurrences in the same row reuse it (the `blanks` map
/// resets per row, so the counter — not the map — is what makes two rows' blanks
/// distinct).
pub(crate) fn fresh_blank(
    template_label: &str,
    blanks: &mut DetHashMap<String, String>,
    ctx: &mut EvalCtx<'_>,
) -> TermValue {
    if let Some(existing) = blanks.get(template_label) {
        return TermValue::Blank {
            label: existing.clone(),
            scope: BlankScope::DEFAULT,
        };
    }
    ctx.bnode_counter += 1;
    let fresh = format!("c{}", ctx.bnode_counter);
    blanks.insert(template_label.to_owned(), fresh.clone());
    TermValue::Blank {
        label: fresh,
        scope: BlankScope::DEFAULT,
    }
}
