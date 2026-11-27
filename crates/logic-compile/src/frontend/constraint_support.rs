// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact source carriers for successful authored integrity constraints.

use std::collections::BTreeSet;

use purrdf::{QuadIds, RdfDataset, TermRef};

use super::{GMEOW_ENFORCES_FAILURE_CLASS, SourceCarrier, SourceNode, SourceStatementBinding};
use crate::graphutil::{RDF_REIFIES, is_structural_type_predicate};

const CONSTRAINT: &str = "https://blackcatinformatics.ca/logic/Constraint";

/// Record only the owner's consumed fields and the successful formula read trace.
/// The formula reader collapses equal values across physical storage tables.
/// Recover exact carriers from the same immutable graph without retaining other
/// outgoing fields of an owned formula node. This grants no proof authority.
pub(super) fn authored_constraint(
    dataset: &RdfDataset,
    owner: SourceNode,
    formula_rows: impl Iterator<Item = QuadIds>,
) -> BTreeSet<SourceStatementBinding> {
    let mut support: BTreeSet<_> = super::source_fields(dataset, owner, None)
        .filter(|binding| owner_field(dataset, binding.canonical))
        .collect();
    let reifies = dataset.term_id_by_iri(RDF_REIFIES);
    // Read each subject's carrier rows once. Wide formula bodies may consume
    // many objects of the same predicate from the same physical subject run.
    let mut rows: Vec<_> = formula_rows.collect();
    rows.sort_unstable_by_key(|row| (row.s, row.g, row.p, row.o));
    rows.dedup();
    for group in rows.chunk_by(|a, b| (a.s, a.g) == (b.s, b.g)) {
        let [first, ..] = group else { continue };
        let node = SourceNode {
            term: first.s,
            graph: first.g,
        };
        let consumed = |quad: QuadIds| {
            group
                .binary_search_by_key(&(quad.p, quad.o), |row| (row.p, row.o))
                .is_ok()
        };
        support.extend(
            super::source_fields(dataset, node, None).filter(|binding| consumed(binding.canonical)),
        );
        if group.iter().any(|row| Some(row.p) == reifies) {
            support.extend(
                dataset
                    .reifier_quads_of(node.term)
                    .filter(|row| row.g == node.graph && consumed(*row))
                    .map(|canonical| SourceStatementBinding {
                        canonical,
                        carrier: SourceCarrier::Reifier,
                    }),
            );
        }
    }
    support
}

fn owner_field(dataset: &RdfDataset, quad: QuadIds) -> bool {
    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
        return false;
    };
    predicate == GMEOW_ENFORCES_FAILURE_CLASS
        || matches!(
            predicate.strip_prefix(crate::ir::LOGIC_NAMESPACE),
            Some("integrity" | "formalizes" | "message" | "severity")
        )
        || (is_structural_type_predicate(predicate, CONSTRAINT)
            && matches!(dataset.resolve(quad.o), TermRef::Iri(CONSTRAINT)))
}
