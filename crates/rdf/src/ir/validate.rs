// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pre-freeze structural validation for the immutable `RdfDataset` (#819 C1).
//!
//! [`validate`] is invoked by [`RdfDatasetBuilder::freeze`] BEFORE any dataset is
//! materialized; on any failure it returns a precise [`RdfDiagnostic`] and freeze
//! HARD-fails (no degraded fallback, per the no-optionality doctrine). It enforces,
//! per the normative C0 contract (`docs/design/819-rdf-ir-dataflow.md`):
//!
//! - **Positional constraints (C0 / RDF 1.2):** a predicate MUST be an IRI; a graph
//!   name MUST be an IRI or a blank node (never a literal or a triple term); a
//!   subject MUST NOT be a literal. Triple terms may appear only in object position
//!   (and recursively as the components of another triple term).
//! - **ID-reference validity:** every `TermId` referenced by any quad / reifier /
//!   annotation is `< term_count()`.
//! - **Triple-term acyclicity (C0.3):** the `Triple{s,p,o}` nesting graph is acyclic
//!   and bounded by [`MAX_TERM_NESTING_DEPTH`]; a triple term MUST NOT (transitively)
//!   contain itself.
//!
//! [`RdfDatasetBuilder::freeze`]: super::builder::RdfDatasetBuilder::freeze

use crate::RdfDiagnostic;

use super::builder::RdfDatasetBuilder;
use super::term::{InternedTerm, TermId};

/// Maximum triple-term nesting depth. Reuses the GTS importer's nesting bound so the
/// IR and the transport agree on the acyclicity cliff.
pub(crate) const MAX_TERM_NESTING_DEPTH: usize = 16;

/// Validate the builder's accumulated structure. Returns `Ok(())` when the dataset
/// is structurally sound, or a precise [`RdfDiagnostic`] on the first violation.
pub(crate) fn validate(builder: &RdfDatasetBuilder) -> Result<(), RdfDiagnostic> {
    let term_count = builder.term_count();

    // 1. Every interned triple term references in-range ids, has an IRI predicate
    //    and a non-literal subject, and the whole nesting forest is acyclic and
    //    depth-bounded.
    validate_triple_terms(builder, term_count)?;

    // 2. Quad positional + id-reference validity.
    for (i, q) in builder.quad_rows().iter().enumerate() {
        check_id_in_range(q.s, term_count, || quad_ref_ctx(i, "subject"))?;
        check_id_in_range(q.p, term_count, || quad_ref_ctx(i, "predicate"))?;
        check_id_in_range(q.o, term_count, || quad_ref_ctx(i, "object"))?;
        if let Some(g) = q.g {
            check_id_in_range(g, term_count, || quad_ref_ctx(i, "graph"))?;
        }

        require_non_literal_subject(builder, q.s, || quad_ref_ctx(i, "subject"))?;
        require_iri_predicate(builder, q.p, || quad_ref_ctx(i, "predicate"))?;
        if let Some(g) = q.g {
            require_graph_name(builder, g, || quad_ref_ctx(i, "graph"))?;
        }
    }

    // 3. Reifier id-reference validity; the reified target MUST be a triple term.
    for (i, (reifier, triple)) in builder.reifier_rows().iter().enumerate() {
        check_id_in_range(*reifier, term_count, || format!("reifier #{i} resource"))?;
        check_id_in_range(*triple, term_count, || format!("reifier #{i} target"))?;
        require_non_literal_subject(builder, *reifier, || format!("reifier #{i} resource"))?;
        if !matches!(builder.term(*triple), InternedTerm::Triple { .. }) {
            return Err(diag(
                "rdf-ir-reifier-not-triple",
                format!(
                    "reifier #{i} must bind a triple term, but its target resolves to {}",
                    kind_str(builder.term(*triple))
                ),
            ));
        }
    }

    // 4. Annotation id-reference validity + predicate-is-IRI.
    for (i, (reifier, p, o)) in builder.annotation_rows().iter().enumerate() {
        check_id_in_range(*reifier, term_count, || format!("annotation #{i} reifier"))?;
        check_id_in_range(*p, term_count, || format!("annotation #{i} predicate"))?;
        check_id_in_range(*o, term_count, || format!("annotation #{i} object"))?;
        require_iri_predicate(builder, *p, || format!("annotation #{i} predicate"))?;
        require_non_literal_subject(builder, *reifier, || format!("annotation #{i} reifier"))?;
    }

    Ok(())
}

/// Validate every interned triple term: in-range components, IRI predicate,
/// non-literal subject, and global acyclicity (C0.3) bounded by depth.
fn validate_triple_terms(
    builder: &RdfDatasetBuilder,
    term_count: usize,
) -> Result<(), RdfDiagnostic> {
    // First pass: per-triple positional + id-range checks. A triple term cannot have
    // a literal subject nor a non-IRI predicate, exactly like an asserted statement.
    for raw in 0..term_count {
        let id = TermId::from_index(raw as u32);
        if let InternedTerm::Triple { s, p, o } = *builder.term(id) {
            check_id_in_range(s, term_count, || format!("triple term #{raw} subject"))?;
            check_id_in_range(p, term_count, || format!("triple term #{raw} predicate"))?;
            check_id_in_range(o, term_count, || format!("triple term #{raw} object"))?;
            require_non_literal_subject(builder, s, || format!("triple term #{raw} subject"))?;
            require_iri_predicate(builder, p, || format!("triple term #{raw} predicate"))?;
        }
    }

    // Second pass: acyclicity. DFS each triple term following only the components
    // that are themselves triple terms. With ids in range guaranteed above, a cycle
    // is the only way to exceed MAX_TERM_NESTING_DEPTH, so the depth bound doubles as
    // the cycle guard.
    let mut state = vec![VisitState::Unvisited; term_count];
    for raw in 0..term_count {
        let id = TermId::from_index(raw as u32);
        if matches!(builder.term(id), InternedTerm::Triple { .. }) {
            check_acyclic(builder, id, 0, &mut state)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    /// On the current DFS stack — re-encountering it is a back edge (cycle).
    OnStack,
    /// Fully explored and proven acyclic — never re-walked.
    Done,
}

/// DFS over triple-term nesting from `id`. Detects back edges (cycles) and enforces
/// the depth bound. Non-triple components are leaves.
fn check_acyclic(
    builder: &RdfDatasetBuilder,
    id: TermId,
    depth: usize,
    state: &mut [VisitState],
) -> Result<(), RdfDiagnostic> {
    if depth > MAX_TERM_NESTING_DEPTH {
        return Err(diag(
            "rdf-ir-triple-nesting-limit",
            format!("triple-term nesting depth exceeds the limit of {MAX_TERM_NESTING_DEPTH}",),
        ));
    }
    match state[id.index()] {
        VisitState::Done => return Ok(()),
        VisitState::OnStack => {
            return Err(diag(
                "rdf-ir-triple-cycle",
                "triple term participates in a reference cycle (a triple cannot contain itself)",
            ));
        }
        VisitState::Unvisited => {}
    }

    let InternedTerm::Triple { s, p, o } = *builder.term(id) else {
        // A non-triple leaf is trivially acyclic.
        state[id.index()] = VisitState::Done;
        return Ok(());
    };

    state[id.index()] = VisitState::OnStack;
    for component in [s, p, o] {
        if matches!(builder.term(component), InternedTerm::Triple { .. }) {
            check_acyclic(builder, component, depth + 1, state)?;
        }
    }
    state[id.index()] = VisitState::Done;
    Ok(())
}

/// Reject an out-of-range term id (orphan reference) with a precise diagnostic.
fn check_id_in_range(
    id: TermId,
    term_count: usize,
    ctx: impl FnOnce() -> String,
) -> Result<(), RdfDiagnostic> {
    if id.index() >= term_count {
        return Err(diag(
            "rdf-ir-term-out-of-range",
            format!(
                "{} references term #{} but only {term_count} terms are interned",
                ctx(),
                id.index(),
            ),
        ));
    }
    Ok(())
}

/// A subject (and any reifier resource) MUST NOT be a literal.
fn require_non_literal_subject(
    builder: &RdfDatasetBuilder,
    id: TermId,
    ctx: impl FnOnce() -> String,
) -> Result<(), RdfDiagnostic> {
    if matches!(builder.term(id), InternedTerm::Literal(_)) {
        return Err(diag(
            "rdf-ir-literal-subject",
            format!("{} must not be a literal", ctx()),
        ));
    }
    Ok(())
}

/// A predicate MUST be an IRI.
fn require_iri_predicate(
    builder: &RdfDatasetBuilder,
    id: TermId,
    ctx: impl FnOnce() -> String,
) -> Result<(), RdfDiagnostic> {
    if !matches!(builder.term(id), InternedTerm::Iri(_)) {
        return Err(diag(
            "rdf-ir-predicate-not-iri",
            format!(
                "{} must be an IRI, but resolves to {}",
                ctx(),
                kind_str(builder.term(id))
            ),
        ));
    }
    Ok(())
}

/// A graph name MUST be an IRI or a blank node (never a literal or triple term).
fn require_graph_name(
    builder: &RdfDatasetBuilder,
    id: TermId,
    ctx: impl FnOnce() -> String,
) -> Result<(), RdfDiagnostic> {
    match builder.term(id) {
        InternedTerm::Iri(_) | InternedTerm::Blank { .. } => Ok(()),
        other => Err(diag(
            "rdf-ir-graph-name-invalid",
            format!(
                "{} must be an IRI or blank node, but resolves to {}",
                ctx(),
                kind_str(other)
            ),
        )),
    }
}

fn quad_ref_ctx(index: usize, position: &str) -> String {
    format!("quad #{index} {position}")
}

fn kind_str(term: &InternedTerm) -> &'static str {
    match term {
        InternedTerm::Iri(_) => "an IRI",
        InternedTerm::Blank { .. } => "a blank node",
        InternedTerm::Literal(_) => "a literal",
        InternedTerm::Triple { .. } => "a triple term",
    }
}

/// Construct a structural-validation error diagnostic.
fn diag(code: &str, message: impl Into<String>) -> RdfDiagnostic {
    RdfDiagnostic::error(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;
    use crate::RdfLiteral;

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    #[test]
    fn freeze_ok_on_well_formed_quad() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None);
        assert!(b.freeze().is_ok());
    }

    /// Gate 3: a quad referencing an out-of-range term id hard-fails.
    #[test]
    fn freeze_err_on_out_of_range_term_id() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p) = (iri(&mut b, "s"), iri(&mut b, "p"));
        // Forge an id past the interned count: never minted by this builder.
        let bogus = TermId::from_index((b.term_count() + 5) as u32);
        b.push_quad(s, p, bogus, None);
        let err = b.freeze().expect_err("out-of-range object must fail");
        assert_eq!(err.code, "rdf-ir-term-out-of-range");
    }

    /// Gate 3: a literal in predicate position hard-fails.
    #[test]
    fn freeze_err_on_literal_predicate() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p_lit = b.intern_literal(RdfLiteral::simple("not-a-predicate"));
        let o = iri(&mut b, "o");
        b.push_quad(s, p_lit, o, None);
        let err = b.freeze().expect_err("literal predicate must fail");
        assert_eq!(err.code, "rdf-ir-predicate-not-iri");
    }

    /// Gate 3: a literal in subject position hard-fails.
    #[test]
    fn freeze_err_on_literal_subject() {
        let mut b = RdfDatasetBuilder::new();
        let s_lit = b.intern_literal(RdfLiteral::simple("not-a-subject"));
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        b.push_quad(s_lit, p, o, None);
        let err = b.freeze().expect_err("literal subject must fail");
        assert_eq!(err.code, "rdf-ir-literal-subject");
    }

    /// Gate 3: a cyclic triple term hard-fails. We build a self-referential triple by
    /// forging a triple term whose object id equals the triple term's own id.
    #[test]
    fn freeze_err_on_cyclic_triple_term() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        // The next interned term takes this id; make its object point back at itself.
        let self_id = TermId::from_index(b.term_count() as u32);
        let _ = o; // keep `o` named for clarity though unused in the cyclic triple
        let cyclic = b.intern_triple(s, p, self_id);
        assert_eq!(cyclic, self_id, "the forged self-id is the new triple's id");
        b.push_quad(s, p, cyclic, None);
        let err = b.freeze().expect_err("cyclic triple term must fail");
        assert_eq!(err.code, "rdf-ir-triple-cycle");
    }

    #[test]
    fn freeze_err_on_literal_graph_name() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let g_lit = b.intern_literal(RdfLiteral::simple("graph"));
        b.push_quad(s, p, o, Some(g_lit));
        let err = b.freeze().expect_err("literal graph name must fail");
        assert_eq!(err.code, "rdf-ir-graph-name-invalid");
    }

    #[test]
    fn freeze_err_on_reifier_target_not_triple() {
        let mut b = RdfDatasetBuilder::new();
        let r = iri(&mut b, "r");
        let not_triple = iri(&mut b, "x");
        b.push_reifier(r, not_triple);
        let err = b
            .freeze()
            .expect_err("reifier target must be a triple term");
        assert_eq!(err.code, "rdf-ir-reifier-not-triple");
    }

    /// A nested-but-acyclic triple term freezes fine even several levels deep.
    #[test]
    fn freeze_ok_on_deep_acyclic_nesting() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let mut o = iri(&mut b, "o");
        for _ in 0..MAX_TERM_NESTING_DEPTH {
            o = b.intern_triple(s, p, o);
        }
        b.push_quad(s, p, o, None);
        assert!(
            b.freeze().is_ok(),
            "acyclic nesting within the bound is valid"
        );
    }
}
