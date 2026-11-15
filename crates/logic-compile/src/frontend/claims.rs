// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native statement behind a scoped claim.
//!
//! The compiler lowers a scoped reifier or `rdf:Statement` node to one axiom per
//! scope and keeps the carrier node as the axiom's source. This module answers the
//! reverse question for a consumer that needs the exact RDF terms: which statement
//! of the prepared dataset is that axiom. It reuses the compiler's own lowering to
//! decide, so no second reading of a claim exists, and it fails closed: an axiom
//! whose carrier is ambiguous, or whose lowering does not state the carried terms
//! themselves, has no resolved statement.

use purrdf::{RdfDataset, TermId, TermRef};

use super::{
    AxiomSource, LogicAxiom, Node, PreparedLogicSource, RDF_OBJECT, RDF_PREDICATE, RDF_REIFIES,
    RDF_STATEMENT, RDF_SUBJECT, RDF_TYPE, SourceNode, emission::CompiledSource, native_axiom,
    native_quoted_axiom, subject_str, term_str,
};
use crate::graphutil::{default_graph_pattern, node_of, subject_of};

/// The statement a scoped claim carries, as term identities of the prepared dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatement {
    /// Exactly one carried statement lowers to the axiom and is spelled by it.
    Resolved {
        subject: TermId,
        predicate: TermId,
        object: TermId,
    },
    /// No carrier, a carrier outside the default graph, or more than one carried
    /// statement that the axiom could be.
    Unresolved,
    /// The axiom states a term its carrier rewrote (a typed term carrier in a term
    /// position), so the carried RDF terms are not the terms of the axiom.
    Mismatch,
}

impl<S: std::borrow::Borrow<PreparedLogicSource>> CompiledSource<S> {
    /// The native statement of the scoped claim at `index` of `program().axioms`.
    ///
    /// Every reification source of the axiom must resolve to the same statement.
    /// An axiom with any other kind of source, or with none, is unresolved.
    #[must_use]
    pub fn claim_statement(&self, index: usize) -> ClaimStatement {
        let (Some(axiom), Some(sources)) = (
            self.program().axioms.get(index),
            self.axiom_sources().get(index),
        ) else {
            return ClaimStatement::Unresolved;
        };
        let store = self.source().dataset();
        let mut resolved = None;
        for source in sources {
            let AxiomSource::Reification(node) = source else {
                return ClaimStatement::Unresolved;
            };
            match carried_statement(store, *node, axiom) {
                statement @ ClaimStatement::Resolved { .. } => {
                    if resolved.is_some_and(|prior| prior != statement) {
                        return ClaimStatement::Unresolved;
                    }
                    resolved = Some(statement);
                }
                refused => return refused,
            }
        }
        resolved.unwrap_or(ClaimStatement::Unresolved)
    }
}

fn same_statement(left: &LogicAxiom, right: &LogicAxiom) -> bool {
    left.subject == right.subject && left.predicate == right.predicate && left.obj == right.obj
}

/// The one statement carried by `node` that the compiler lowered to `axiom`.
fn carried_statement(store: &RdfDataset, node: SourceNode, axiom: &LogicAxiom) -> ClaimStatement {
    if node.graph.is_some() {
        return ClaimStatement::Unresolved;
    }
    let carrier = subject_of(store, node.term);
    let mut matched: Vec<(TermId, TermId, TermId)> = Vec::new();
    let mut rewritten = false;
    let mut keep = |statement: (TermId, TermId, TermId)| {
        if !matched.contains(&statement) {
            matched.push(statement);
        }
    };

    // RDF 1.2 carrier: `node rdf:reifies <<( s p o )>>`.
    if let Some(reifies) = store.term_id_by_iri(RDF_REIFIES) {
        for quad in default_graph_pattern(store, Some(node.term), Some(reifies), None) {
            let TermRef::Triple { s, p, o } = store.resolve(quad.o) else {
                continue;
            };
            let Node::Triple(triple) = node_of(store, quad.o) else {
                continue;
            };
            let Ok(lowered) =
                native_quoted_axiom(store, &carrier, &triple, false, axiom.scope.clone())
            else {
                continue;
            };
            if !same_statement(&lowered, axiom) {
                continue;
            }
            // The compiler read this statement into the axiom. Keep it only when
            // the axiom also spells the carried terms themselves.
            let spelled = native_axiom(
                subject_str(&triple.subject),
                triple.predicate.as_str(),
                &triple.object,
                false,
                axiom.scope.clone(),
            );
            if spelled.is_ok_and(|spelled| same_statement(&spelled, axiom)) {
                keep((s, p, o));
            } else {
                rewritten = true;
            }
        }
    }

    // Classic carrier: an `rdf:Statement` node with one subject, predicate and object.
    let typed = store
        .term_id_by_iri(RDF_TYPE)
        .zip(store.term_id_by_iri(RDF_STATEMENT))
        .is_some_and(|(kind, statement)| {
            default_graph_pattern(store, Some(node.term), Some(kind), Some(statement))
                .next()
                .is_some()
        });
    if typed {
        let [subjects, predicates, objects] =
            [RDF_SUBJECT, RDF_PREDICATE, RDF_OBJECT].map(|part| {
                let mut values: Vec<TermId> = Vec::new();
                if let Some(part) = store.term_id_by_iri(part) {
                    for quad in default_graph_pattern(store, Some(node.term), Some(part), None) {
                        if !values.contains(&quad.o) {
                            values.push(quad.o);
                        }
                    }
                }
                values
            });
        if subjects.len() > 1 || predicates.len() > 1 || objects.len() > 1 {
            return ClaimStatement::Unresolved;
        }
        if let (&[s], &[p], &[o]) = (&subjects[..], &predicates[..], &objects[..])
            && let subject @ (Node::Iri(_) | Node::Blank { .. }) = node_of(store, s)
            && let Node::Iri(predicate) = node_of(store, p)
            && let Ok(lowered) = native_axiom(
                term_str(&subject),
                predicate,
                &node_of(store, o),
                false,
                axiom.scope.clone(),
            )
            && same_statement(&lowered, axiom)
        {
            keep((s, p, o));
        }
    }

    match matched[..] {
        [(subject, predicate, object)] => ClaimStatement::Resolved {
            subject,
            predicate,
            object,
        },
        [] if rewritten => ClaimStatement::Mismatch,
        _ => ClaimStatement::Unresolved,
    }
}
