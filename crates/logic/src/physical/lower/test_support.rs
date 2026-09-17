// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

/// Fold a `TermDag` node's content key into the published digest — TEST SCAFFOLDING ONLY.
///
/// Production never calls this: the shipped `math:structuralKey` is computed by
/// [`arena_structural_key`], through the arena facade, and both routes end at
/// [`fold_content_key`] over the same bytes ([`gmeow_term_arena::Arena::key`] returns
/// `TermDag::key` verbatim). It exists because several invariant tests below intern SEVERAL
/// nodes into ONE shared `TermDag` to check hash-consing, which the graph-and-root production
/// entry point cannot express. `#[cfg(test)]` so it can never become a second production
/// surface — the duplicate-entry-point condition the arena facade was deleted for.
pub(crate) fn structural_digest(dag: &TermDag, id: NodeId) -> String {
    fold_content_key(dag.key(id))
}
