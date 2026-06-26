// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `CONSTRUCT` evaluation, emitting the IR dataset **directly** (no
//! serialize/re-parse round trip).
//!
//! The `WHERE` algebra is evaluated to a solution multiset; the template is then
//! instantiated once per solution into a fresh [`RdfDatasetBuilder`] and frozen.
//! Three SPARQL rules govern instantiation (§16.2):
//!
//! 1. A template triple with **any unbound variable** is silently skipped.
//! 2. A template **blank node is minted fresh per solution row** — the same label
//!    co-refers within one row but is a distinct node across rows.
//! 3. An **ill-formed** instantiation (a literal in subject position, or a non-IRI
//!    predicate) is skipped.
//!
//! Each position is instantiated to a [`TermValue`] first so its term *kind* can be
//! validated before interning into the output builder. Byte-identical parity with
//! the oxigraph baseline is decided downstream at the RDFC-1.0 canonicalization
//! layer, so blank-node labels and quad ordering here need not match oxigraph's —
//! `freeze` sorts and de-duplicates, and canonicalization relabels blanks.

use std::sync::Arc;

use gmeow_rdf_core::{BlankScope, RdfDataset, RdfDatasetBuilder, TermFactory, TermId, TermValue};
use gmeow_sparql_algebra::{GraphPattern, NamedNodePattern, TermPattern, TriplePattern};

use crate::convert::{literal_to_value, named_node_to_value};
use crate::error::EvalError;
use crate::eval::{eval, EvalCtx};
use crate::solution::{Solution, VarSchema};
use crate::DetHashMap;

/// Evaluate a `CONSTRUCT` query to a frozen IR dataset.
pub(crate) fn eval_construct(
    template: &[TriplePattern],
    pattern: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<Arc<RdfDataset>, EvalError> {
    let seq = eval(pattern, ctx)?;
    let schema = seq.schema.clone();
    let mut builder = RdfDatasetBuilder::new();

    for row in &seq.rows {
        // Template blank labels are fresh per solution row; the map co-refers a
        // label within this row only.
        let mut blanks: DetHashMap<String, String> = DetHashMap::default();
        for tp in template {
            if let Some((s, p, o)) = instantiate(tp, row, &schema, &mut builder, &mut blanks, ctx) {
                builder.push_quad(s, p, o, None);
            }
        }
    }

    builder
        .freeze()
        .map_err(|d| EvalError::internal(format!("CONSTRUCT output failed to freeze: {d:?}")))
}

/// Instantiate one template triple for `row`, interning into `builder`. Returns
/// `None` if the triple is skipped (an unbound variable or an ill-formed position).
fn instantiate(
    tp: &TriplePattern,
    row: &Solution,
    schema: &VarSchema,
    builder: &mut RdfDatasetBuilder,
    blanks: &mut DetHashMap<String, String>,
    ctx: &mut EvalCtx<'_>,
) -> Option<(TermId, TermId, TermId)> {
    let s = instantiate_term(&tp.subject, row, schema, blanks, ctx)?;
    let p = instantiate_predicate(&tp.predicate, row, schema, ctx)?;
    let o = instantiate_term(&tp.object, row, schema, blanks, ctx)?;

    // Positional validity (§16.2): subject must not be a literal; predicate must be
    // an IRI. Ill-formed instantiations are skipped, not errored.
    if matches!(s, TermValue::Literal { .. }) || !matches!(p, TermValue::Iri(_)) {
        return None;
    }

    Some((
        builder.intern_value(&s),
        builder.intern_value(&p),
        builder.intern_value(&o),
    ))
}

/// Instantiate a subject/object template term. `None` = an unbound variable.
fn instantiate_term(
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
fn instantiate_predicate(
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
fn fresh_blank(
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

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::{RdfLiteral, TermRef};
    use gmeow_sparql_algebra::{NamedNode, Variable};

    const KNOWS: &str = "http://ex/knows";
    const RELATED: &str = "http://ex/related";

    fn knows_graph() -> Arc<RdfDataset> {
        // :a :knows :b ; :a :knows :c .
        let mut b = RdfDatasetBuilder::new();
        let knows = b.intern_iri(KNOWS.to_owned());
        let a = b.intern_iri("http://ex/a".to_owned());
        let bb = b.intern_iri("http://ex/b".to_owned());
        let cc = b.intern_iri("http://ex/c".to_owned());
        b.push_quad(a, knows, bb, None);
        b.push_quad(a, knows, cc, None);
        b.freeze().expect("freeze")
    }

    fn var(n: &str) -> TermPattern {
        TermPattern::Variable(Variable::new(n))
    }
    fn pred(iri: &str) -> NamedNodePattern {
        NamedNodePattern::NamedNode(NamedNode::new_unchecked(iri))
    }
    fn where_knows() -> GraphPattern {
        GraphPattern::Bgp {
            patterns: vec![TriplePattern {
                subject: var("s"),
                predicate: pred(KNOWS),
                object: var("o"),
            }],
        }
    }

    #[test]
    fn construct_rewrites_predicate() {
        let ds = knows_graph();
        let mut ctx = EvalCtx::new(&ds);
        // CONSTRUCT { ?s :related ?o } WHERE { ?s :knows ?o }
        let template = vec![TriplePattern {
            subject: var("s"),
            predicate: pred(RELATED),
            object: var("o"),
        }];
        let out = eval_construct(&template, &where_knows(), &mut ctx).expect("construct");
        assert_eq!(out.quad_count(), 2);
        // Every emitted quad uses :related, none :knows.
        for q in out.quads() {
            assert!(matches!(out.resolve(q.p), TermRef::Iri(p) if p == RELATED));
        }
    }

    #[test]
    fn unbound_template_var_skips_the_triple() {
        let ds = knows_graph();
        let mut ctx = EvalCtx::new(&ds);
        // CONSTRUCT { ?s :related ?missing } WHERE { ?s :knows ?o } — ?missing is
        // never bound, so every template triple is skipped → empty output.
        let template = vec![TriplePattern {
            subject: var("s"),
            predicate: pred(RELATED),
            object: var("missing"),
        }];
        let out = eval_construct(&template, &where_knows(), &mut ctx).expect("construct");
        assert_eq!(out.quad_count(), 0);
    }

    #[test]
    fn template_blank_is_fresh_per_solution() {
        let ds = knows_graph();
        let mut ctx = EvalCtx::new(&ds);
        // CONSTRUCT { _:b :related ?o } WHERE { ?s :knows ?o }
        // Two solutions → two distinct fresh blank subjects.
        let template = vec![TriplePattern {
            subject: TermPattern::BlankNode(gmeow_sparql_algebra::BlankNode::new("b")),
            predicate: pred(RELATED),
            object: var("o"),
        }];
        let out = eval_construct(&template, &where_knows(), &mut ctx).expect("construct");
        assert_eq!(out.quad_count(), 2);
        // Collect the distinct blank subjects.
        let mut blanks = std::collections::BTreeSet::new();
        for q in out.quads() {
            if let TermRef::Blank { label, .. } = out.resolve(q.s) {
                blanks.insert(label.to_owned());
            }
        }
        assert_eq!(blanks.len(), 2, "each solution mints a distinct blank");
    }

    #[test]
    fn ill_formed_literal_subject_is_skipped() {
        // CONSTRUCT { ?o :related ?s } where ?o binds to a literal → literal subject
        // → skipped.
        let mut b = RdfDatasetBuilder::new();
        let p = b.intern_iri("http://ex/p".to_owned());
        let s = b.intern_iri("http://ex/s".to_owned());
        let lit = b.intern_literal(RdfLiteral::simple("hello"));
        b.push_quad(s, p, lit, None); // :s :p "hello"
        let ds = b.freeze().expect("freeze");
        let mut ctx = EvalCtx::new(&ds);

        let where_pat = GraphPattern::Bgp {
            patterns: vec![TriplePattern {
                subject: var("s"),
                predicate: pred("http://ex/p"),
                object: var("o"),
            }],
        };
        // Template puts ?o (a literal) in subject position.
        let template = vec![TriplePattern {
            subject: var("o"),
            predicate: pred(RELATED),
            object: var("s"),
        }];
        let out = eval_construct(&template, &where_pat, &mut ctx).expect("construct");
        assert_eq!(out.quad_count(), 0);
    }
}
