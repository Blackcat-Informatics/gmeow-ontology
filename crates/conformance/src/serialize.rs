// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Serialization of the produced artifacts into the shapes the goldens compare.
//!
//! Comparison is canonical-JSON / graph-isomorphism, so these produce
//! *semantically* equal (not byte-identical) output to the retired Python
//! runner's serializations.

use std::collections::BTreeMap;

use gmeow_logic::certify::CertificationVerdict;
use gmeow_logic::compile::projections::LedgerEntry;

use crate::run::RunnerQuad;

/// Serialize the materialized quads to a deterministic N-Quads document.
///
/// One line per quad, `<s> <p> {obj} <g> .`, sorted lexicographically. The
/// object is already in N3 form (`<iri>` or a literal). Empty input yields the
/// empty string (no trailing newline).
pub fn materialized_to_nquads(quads: &[RunnerQuad]) -> String {
    let mut lines: Vec<String> = quads
        .iter()
        .map(|q| {
            format!(
                "<{}> <{}> {} <{}> .",
                q.subject, q.predicate, q.obj, q.graph
            )
        })
        .collect();
    lines.sort();
    if lines.is_empty() {
        String::new()
    } else {
        let mut doc = lines.join("\n");
        doc.push('\n');
        doc
    }
}

/// Build the minimal world-indexed verdicts JSON.
///
/// Every world that materializes is `consistent`; each carries its quad count.
/// Worlds with no quads do not appear (the world set is exactly the set of quad
/// graphs).
pub fn build_verdicts(quads: &[RunnerQuad]) -> serde_json::Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for q in quads {
        *counts.entry(q.graph.clone()).or_insert(0) += 1;
    }
    let mut obj = serde_json::Map::new();
    for (world, n) in counts {
        obj.insert(
            world,
            serde_json::json!({ "quads": n, "status": "consistent" }),
        );
    }
    serde_json::Value::Object(obj)
}

/// Build the preservation-ledger JSON from the compiler's ledger entries.
///
/// `{ target: { preservation, complexity, lossy_drops } }` — the shape the
/// `expected/projections/preservation-ledger.json` golden compares.
pub fn ledger_to_json(entries: &[LedgerEntry]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for e in entries {
        obj.insert(
            e.target.clone(),
            serde_json::json!({
                "preservation": e.preservation,
                "complexity": e.complexity,
                "lossy_drops": e.lossy_drops,
            }),
        );
    }
    serde_json::Value::Object(obj)
}

/// Build the certification JSON from the native verdict.
///
/// `{ certified, decidability_class, profile_id, violations }` — identical shape
/// and (sorted) values to `gmeow_logic.certify`.
pub fn certification_to_json(verdict: &CertificationVerdict) -> serde_json::Value {
    let (certified, decidability_class, profile_id, violations) = verdict.to_json_pairs();
    serde_json::json!({
        "certified": certified,
        "decidability_class": decidability_class,
        "profile_id": profile_id,
        "violations": violations,
    })
}

/// Build the budget-governor JSON marker (`{ budget_status, incomplete }`).
pub fn budget_to_json(budget_status: &str, incomplete: bool) -> serde_json::Value {
    serde_json::json!({ "budget_status": budget_status, "incomplete": incomplete })
}
