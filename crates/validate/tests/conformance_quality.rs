// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_quality.py
//!
//! The whole-ontology Principle-9 sweep: no gmeow: vocabulary term is a
//! preferred/primary selector. A dynamic sweep over every gmeow:-namespaced
//! subject of the merged graph (`GraphStore::ontology()`) — not the quality module
//! alone — so a module-scoped cell would silently narrow it.

mod conformance_support;
use conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// Twin of `test_no_preferred_or_primary_term_is_declared` (Principle 9): no gmeow:
/// term in the merged ontology whose local name (containing no `/`) case-insensitively
/// starts with `primary`/`preferred`.
#[test]
fn no_preferred_or_primary_term_is_declared() {
    let g = GraphStore::ontology();
    let (_vars, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let mut offenders = Vec::new();
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(iri) = term.as_iri() else {
            continue;
        };
        if let Some(local) = iri.strip_prefix(GMEOW) {
            let lower = local.to_lowercase();
            if !local.contains('/')
                && (lower.starts_with("primary") || lower.starts_with("preferred"))
            {
                offenders.push(iri.to_owned());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "preferred/primary quality term leaked: {offenders:?}"
    );
}
