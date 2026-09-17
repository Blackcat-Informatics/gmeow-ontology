// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn rq(graph: &str) -> RunnerQuad {
    RunnerQuad {
        modal_evaluation: None,
        graph: graph.to_string(),
        subject: "s".to_string(),
        predicate: "p".to_string(),
        obj: "<o>".to_string(),
        derivation_id: String::new(),
        rule_iri: String::new(),
        source_quad_ids: Vec::new(),
        budget_status: "ok".to_string(),
    }
}

#[test]
fn verdict_status_wire_strings() {
    assert_eq!(VerdictStatus::Consistent.as_str(), "consistent");
    assert_eq!(VerdictStatus::Inconsistent.as_str(), "inconsistent");
    assert_eq!(VerdictStatus::Incomplete.as_str(), "incomplete");
}

#[test]
fn quad_status_json_sorts_and_carries_per_quad_stamp() {
    // Two quads with DIFFERENT per-quad stamps (the frontier-aware verdict): the
    // saturated-stratum quad is `ok`, the cut-stratum quad `exhausted`. The output is
    // sorted by the quad key and carries each stamp verbatim.
    let mut ok_quad = rq("https://example.org/w");
    ok_quad.subject = "https://example.org/b".to_string();
    ok_quad.predicate = "https://example.org/reachable".to_string();
    ok_quad.obj = "<https://example.org/b>".to_string();
    ok_quad.budget_status = "ok".to_string();
    let mut cut_quad = rq("https://example.org/w");
    cut_quad.subject = "https://example.org/c".to_string();
    cut_quad.predicate = "https://example.org/unreachable".to_string();
    cut_quad.obj = "<https://example.org/c>".to_string();
    cut_quad.budget_status = "exhausted".to_string();

    // Pass cut before ok to prove deterministic re-sort by the quad key.
    let json = quad_status_to_json(&[cut_quad, ok_quad]);
    assert_eq!(
        json,
        serde_json::json!([
            {
                "quad": "<https://example.org/b> <https://example.org/reachable> <https://example.org/b> <https://example.org/w> .",
                "status": "ok"
            },
            {
                "quad": "<https://example.org/c> <https://example.org/unreachable> <https://example.org/c> <https://example.org/w> .",
                "status": "exhausted"
            }
        ])
    );
}

#[test]
fn build_verdicts_consistent_reproduces_pre_753_shape() {
    // Two quads in world `w`, one in `v` → sparse per-world counts, all
    // `consistent` (the byte-for-byte golden shape).
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
