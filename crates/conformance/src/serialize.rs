// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Serialization of the produced artifacts into the shapes the goldens compare.
//!
//! Comparison is canonical-JSON / graph-isomorphism, so these produce
//! *semantically* equal (not byte-identical) output to the retired Python
//! runner's serializations.

use std::collections::BTreeMap;

use gmeow_logic::certify::CertificationVerdict;
use gmeow_logic_compile::projections::LedgerEntry;

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

/// The consistency status of a world in the verdicts JSON (#753).
///
/// Serializes to the historical lowercase wire strings. `Consistent` is the
/// materialization default; `Inconsistent` is emitted when the native DL
/// consistency path finds a populated `owl:Nothing` clash in that world (the
/// external `Theorem`/`Unsatisfiable`/`PositiveEntailment` branch); `Incomplete`
/// is emitted when the budget governor exhausts the chase (the external
/// `Unknown`/budget-tripped branch). Before #753 only `Consistent` was ever
/// produced — `build_verdicts` hard-coded the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictStatus {
    Consistent,
    Inconsistent,
    Incomplete,
}

impl VerdictStatus {
    /// The lowercase wire string the `verdicts.json` golden compares.
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictStatus::Consistent => "consistent",
            VerdictStatus::Inconsistent => "inconsistent",
            VerdictStatus::Incomplete => "incomplete",
        }
    }
}

/// Count materialized quads per world (named graph).
///
/// The world set is exactly the set of quad graphs — worlds with no quads do not
/// appear (the sparse representation the verdicts golden expects).
pub fn count_worlds(quads: &[RunnerQuad]) -> BTreeMap<String, u64> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for q in quads {
        *counts.entry(q.graph.clone()).or_insert(0) += 1;
    }
    counts
}

/// Build the world-indexed verdicts JSON from per-world quad counts and a
/// per-world status resolver.
///
/// `{ world: { quads, status } }`. The status is resolved per world so the
/// consistency path can mark only the world bearing an `owl:Nothing` clash
/// `inconsistent` while the materialization path applies one aggregate status
/// (the materializing-worlds-are-`consistent` default, or `incomplete` on a
/// budget trip). Keeping `Consistent` for every materializing world reproduces
/// the pre-#753 golden byte-for-byte.
pub fn build_verdicts(
    world_quad_counts: &BTreeMap<String, u64>,
    status_for_world: impl Fn(&str) -> VerdictStatus,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (world, n) in world_quad_counts {
        obj.insert(
            world.clone(),
            serde_json::json!({ "quads": n, "status": status_for_world(world).as_str() }),
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

/// Build the **runtime** preservation-disclosure JSON from a result's
/// [`PreservationClaim`] (#773): `{ polarities: [...], unsupported_constructs: [...] }`,
/// both sorted for determinism. This is the runtime judgment a result carries —
/// distinct from the compile-time projection ledger ([`ledger_to_json`]), which
/// describes per-target lowering classes rather than what a given evaluation
/// dropped. The shape mirrors the `gmeow_logic` `preservation` PyO3 dict key.
pub fn preservation_to_json(claim: &gmeow_logic::result::PreservationClaim) -> serde_json::Value {
    let polarities: Vec<&str> = claim.polarities.iter().map(|k| k.as_str()).collect();
    let unsupported: Vec<&str> = claim
        .unsupported_constructs
        .iter()
        .map(String::as_str)
        .collect();
    serde_json::json!({
        "polarities": polarities,
        "unsupported_constructs": unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rq(graph: &str) -> RunnerQuad {
        RunnerQuad {
            graph: graph.to_string(),
            subject: "s".to_string(),
            predicate: "p".to_string(),
            obj: "<o>".to_string(),
            derivation_id: String::new(),
            rule_iri: String::new(),
            source_quad_ids: Vec::new(),
        }
    }

    #[test]
    fn verdict_status_wire_strings() {
        assert_eq!(VerdictStatus::Consistent.as_str(), "consistent");
        assert_eq!(VerdictStatus::Inconsistent.as_str(), "inconsistent");
        assert_eq!(VerdictStatus::Incomplete.as_str(), "incomplete");
    }

    #[test]
    fn build_verdicts_consistent_reproduces_pre_753_shape() {
        // Two quads in world `w`, one in `v` → sparse per-world counts, all
        // `consistent` (the byte-for-byte pre-#753 golden shape).
        let quads = vec![rq("w"), rq("w"), rq("v")];
        let counts = count_worlds(&quads);
        let v = build_verdicts(&counts, |_| VerdictStatus::Consistent);
        assert_eq!(
            v,
            serde_json::json!({
                "v": { "quads": 1, "status": "consistent" },
                "w": { "quads": 2, "status": "consistent" },
            })
        );
    }

    #[test]
    fn build_verdicts_threads_incomplete_and_per_world_inconsistent() {
        let quads = vec![rq("w"), rq("v")];
        let counts = count_worlds(&quads);
        // Aggregate incomplete (materialization budget trip).
        let inc = build_verdicts(&counts, |_| VerdictStatus::Incomplete);
        assert_eq!(inc["w"]["status"], "incomplete");
        assert_eq!(inc["v"]["status"], "incomplete");
        // Per-world inconsistent (consistency-mode clash isolated to one world).
        let mixed = build_verdicts(&counts, |world| {
            if world == "w" {
                VerdictStatus::Inconsistent
            } else {
                VerdictStatus::Consistent
            }
        });
        assert_eq!(mixed["w"]["status"], "inconsistent");
        assert_eq!(mixed["v"]["status"], "consistent");
    }
}
