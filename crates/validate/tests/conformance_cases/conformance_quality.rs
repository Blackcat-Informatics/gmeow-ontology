// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_quality.py
//!
//! The whole-ontology Principle-9 sweep: no gmeow: vocabulary term is a
//! preferred/primary selector. A dynamic sweep over every gmeow:-namespaced
//! subject of the merged graph (`GraphStore::ontology()`) — not the quality module
//! alone — so a module-scoped cell would silently narrow it.

use crate::conformance_support::*;

/// Twin of `test_no_preferred_or_primary_term_is_declared` (Principle 9): no gmeow:
/// term in the merged ontology whose local name (containing no `/`) case-insensitively
/// starts with `primary`/`preferred`.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_term_is_declared() {
    let g = GraphStore::ontology();
    let offenders = g.primary_or_preferred_terms();
    assert!(
        offenders.is_empty(),
        "preferred/primary quality term leaked: {offenders:?}"
    );
}
