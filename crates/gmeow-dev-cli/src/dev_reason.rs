// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native reasoning commands: `reason`, `verify`, `reason-verify`, `explain`,
//! and the `certify` static profile check.
//!
//! All read the folded `gmeow.gts` snapshot. The default lane reuses its shipped
//! `graph/reasoning` typed verdict (contract-hash-checked); fresh lanes first project
//! the snapshot back to the exact object-level EDB the pipeline authority used, so
//! shipped mapping/correspondence/report graphs remain meta-level and never enter
//! closure. `certify` statically certifies a `.logic` program against its declared
//! semantic profile.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gmeow_logic::reason::{native_contract_hash, reason_all};
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::verify::{verify as verify_reasoned, verify_with_reasoning_result};

use crate::dev_common::{
    elapsed_ms, emit_report, fail, note, project_root, snapshot_bytes, write_timings_json,
};
use crate::error;

/// Import the committed snapshot into a reasoning dataset.
fn snapshot_dataset(root: &Path) -> Result<std::sync::Arc<purrdf::RdfDataset>, i32> {
    let bytes = snapshot_bytes(root)?;
    let bundle = purrdf::import_gts_events(&bytes)
        .map_err(|e| fail(format!("cannot read snapshot: {e}")))?;
    Ok(bundle.dataset)
}

/// Recover the exact object-level reasoning EDB from a full shipped snapshot. The
/// shared pipeline projector is the authority for the graph boundary, keeping CLI
/// fresh reasoning byte-for-byte aligned with `stage-reason` rather than reasoning
/// over every ontology-resident meta/report graph.
fn snapshot_reasoning_dataset(
    snapshot: &purrdf::RdfDataset,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, i32> {
    gmeow_pipeline::stages::carrier::snapshot_reasoning_edb(snapshot)
        .map_err(|e| fail(format!("cannot project snapshot reasoning EDB: {e}")))
}

/// Re-derive the SHIPPED typed reasoning result from the snapshot's
/// `graph/reasoning` projection — the SAME reverse parse the pipeline cache uses —
/// and refuse a verdict minted under a different reasoning contract than this
/// binary's engine: a stale bundle must be regenerated (or re-reasoned with
/// `--fresh`), never re-reported as current.
fn shipped_reasoning_result(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<ReasoningResult> {
    let graph = dataset.project_named_graph(gmeow_logic::result_rdf::GRAPH_REASONING);
    if graph.quad_count() == 0 {
        return Err(error::reasoning(
            "the snapshot carries no graph/reasoning verdict; run `make check` \
             (or re-reason with --fresh)",
        ));
    }
    // The projected sub-dataset is default-graph only, so its canonical N-Quads
    // lines are `s p o .` — exactly the N-Triples shape the reverse parser reads.
    let nt = purrdf::serialize_dataset(
        &graph,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| error::rdf(format!("serialize graph/reasoning: {e}")))?;
    let nt = String::from_utf8(nt)
        .map_err(|e| error::encoding(format!("graph/reasoning is not UTF-8: {e}")))?;
    let result = gmeow_logic::result_rdf::parse_reasoning_graph(&nt).map_err(error::reasoning)?;
    let current = native_contract_hash();
    if result.provenance.contract_hash != current {
        return Err(error::reasoning(format!(
            "the shipped graph/reasoning verdict was minted under reasoning contract \
             {shipped} but this binary implements {current}; run `make check` to \
             re-mint the bundle (or re-reason with --fresh)",
            shipped = result.provenance.contract_hash,
        )));
    }
    Ok(result)
}

/// `gmeow-dev reason [--mode --fresh --merge …]` — native EL/DL reasoning. By
/// default the shipped `graph/reasoning` verdict is reused (contract-hash-checked);
/// `--fresh` recomputes the closure with the native engine.
pub fn reason(mode: &str, fresh: bool, timings_json: Option<&Path>) -> i32 {
    if mode != "native" {
        return fail(format!(
            "unknown reasoning mode: {mode:?} (only the native Docker-free reasoner exists)"
        ));
    }
    let root = project_root();
    let started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let (result, phase) = if fresh {
        let edb = match snapshot_reasoning_dataset(dataset.as_ref()) {
            Ok(edb) => edb,
            Err(code) => return code,
        };
        match reason_all(edb.as_ref()) {
            Ok(r) => (r, "reason-native"),
            Err(e) => return fail(format!("native reasoning failed: {e}")),
        }
    } else {
        match shipped_reasoning_result(dataset.as_ref()) {
            Ok(r) => (r, "reason-shipped"),
            Err(e) => return fail(format!("cannot reuse the shipped verdict: {e}")),
        }
    };
    let elapsed = elapsed_ms(started);
    // A positive headline requires a DECIDED consistency proof, not merely the
    // absence of a glut: an out-of-fragment bundle is honestly cannot-decide, and
    // reporting it as "consistent" would silently ignore the undecided axioms.
    let ok = result.is_decided_consistent();
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "reason",
            "mode": "native",
            "ok": ok,
            "timings": [{ "phase": phase, "elapsed_ms": elapsed, "metadata": null }],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    if ok {
        println!(
            "native EL/DL reasoning (Docker-free, {source}): consistent, {n} inferred axiom(s)",
            source = if fresh {
                "fresh closure"
            } else {
                "shipped verdict"
            },
            n = result.inferred().len()
        );
        0
    } else if result.is_consistent() {
        // Undetermined: no contradiction was derived, but the native path did not
        // decide every construct — an honest cannot-decide, never a wrong
        // "consistent".
        fail(format!(
            "cannot decide consistency: {n} out-of-fragment construct(s) the native \
             path does not decide ({constructs})",
            n = result.preservation.unsupported_constructs.len(),
            constructs = result
                .preservation
                .unsupported_constructs
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ))
    } else {
        fail("inconsistent ontology")
    }
}

/// `gmeow-dev verify [--mode --fresh …]` — reasoned-graph negative tests. By
/// default the queries run against the shipped closure + verdict already folded
/// into the snapshot (contract-hash-checked, no second chase); `--fresh`
/// recomputes the closure with the native engine first.
pub fn verify(mode: &str, fresh: bool, timings_json: Option<&Path>) -> i32 {
    if mode != "native" {
        return fail(format!(
            "unknown verify mode: {mode:?} (only the native Docker-free verifier exists)"
        ));
    }
    let root = project_root();
    let started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let edb = match snapshot_reasoning_dataset(dataset.as_ref()) {
        Ok(edb) => edb,
        Err(code) => return code,
    };
    let queries = gmeow_logic::verify::embedded_verify_queries();
    let report = if fresh {
        match verify_reasoned(edb.as_ref(), &queries) {
            Ok(r) => r,
            Err(e) => return fail(format!("native verify failed: {e}")),
        }
    } else {
        let result = match shipped_reasoning_result(dataset.as_ref()) {
            Ok(r) => r,
            Err(e) => return fail(format!("cannot reuse the shipped verdict: {e}")),
        };
        match verify_with_reasoning_result(edb.as_ref(), &result, &queries) {
            Ok(r) => r,
            Err(e) => return fail(format!("native verify failed: {e}")),
        }
    };
    let elapsed = elapsed_ms(started);
    emit_report(&report);
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "verify",
            "mode": "native",
            "ok": report.ok(),
            "timings": [{ "phase": "verify-native", "elapsed_ms": elapsed, "metadata": null }],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    if report.ok() {
        println!("verify: no violations on the reasoned graph (native, Docker-free)");
        0
    } else {
        fail(format!(
            "verify: {} violation(s) on the reasoned graph",
            report.error_count()
        ))
    }
}

struct ReasonVerifyEvaluation {
    result: ReasoningResult,
    report: gmeow_errors::Report,
    result_ms: u128,
    verify_ms: u128,
}

/// Obtain one complete reasoning result and verify that exact value.
///
/// The producer is `FnOnce` deliberately: the type prevents this orchestration
/// seam from invoking a fresh reasoner twice. Both the focused fresh command and
/// the aggregate reason gate use this path.
fn evaluate_reason_verify_once<F>(
    dataset: &purrdf::RdfDataset,
    queries: &[(String, String)],
    produce_result: F,
) -> gmeow_errors::Result<ReasonVerifyEvaluation>
where
    F: FnOnce() -> gmeow_errors::Result<ReasoningResult>,
{
    let result_started = Instant::now();
    let result = produce_result()?;
    let result_ms = elapsed_ms(result_started);

    if !result.is_decided_consistent() {
        return if result.is_consistent() {
            // Name them. The sibling refusal in `run_reason` lists the constructs, and a
            // refusal that reports only a COUNT cannot be acted on: it says an axiom took
            // the ontology out of the decided fragment without saying which one, which is
            // the whole content of the diagnosis.
            Err(error::reasoning(format!(
                "cannot decide consistency: {n} out-of-fragment construct(s) the native \
                 path does not decide; refusing to verify ({constructs})",
                n = result.preservation.unsupported_constructs.len(),
                constructs = result
                    .preservation
                    .unsupported_constructs
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            )))
        } else {
            Err(error::reasoning("inconsistent ontology"))
        };
    }

    let verify_started = Instant::now();
    let report = verify_with_reasoning_result(dataset, &result, queries)
        .map_err(|e| error::reasoning(format!("native reason+verify failed: {e}")))?;
    let verify_ms = elapsed_ms(verify_started);

    Ok(ReasonVerifyEvaluation {
        result,
        report,
        result_ms,
        verify_ms,
    })
}

/// `gmeow-dev reason-verify [--fresh --merge --timings-json]` — reason + verify in
/// one pass. By default both halves reuse the shipped closure + verdict
/// (contract-hash-checked, ONE snapshot import and no chase); `--fresh` computes
/// one complete native closure and verifies that same value.
pub fn reason_verify(fresh: bool, timings_json: Option<&Path>) -> i32 {
    let root = project_root();
    let started = Instant::now();
    let snapshot_started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let edb = match snapshot_reasoning_dataset(dataset.as_ref()) {
        Ok(edb) => edb,
        Err(code) => return code,
    };
    let snapshot_ms = elapsed_ms(snapshot_started);
    let queries = gmeow_logic::verify::embedded_verify_queries();
    let (evaluation, result_phase) = if fresh {
        (
            evaluate_reason_verify_once(edb.as_ref(), &queries, || {
                reason_all(edb.as_ref())
                    .map_err(|e| error::reasoning(format!("native reason+verify failed: {e}")))
            }),
            "reason-native",
        )
    } else {
        (
            evaluate_reason_verify_once(edb.as_ref(), &queries, || {
                shipped_reasoning_result(dataset.as_ref())
                    .map_err(|e| error::reasoning(format!("cannot reuse the shipped verdict: {e}")))
            }),
            "reason-result-shipped",
        )
    };
    let evaluation = match evaluation {
        Ok(evaluation) => evaluation,
        Err(message) => return fail(message),
    };
    let elapsed = elapsed_ms(started);
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "reason-verify",
            "mode": "native",
            "ok": evaluation.report.ok(),
            "metrics": {
                "inferred_axioms": evaluation.result.inferred().len(),
                "verify_errors": evaluation.report.error_count(),
            },
            "timings": [
                { "phase": "snapshot-import", "elapsed_ms": snapshot_ms, "metadata": null },
                { "phase": result_phase, "elapsed_ms": evaluation.result_ms, "metadata": null },
                { "phase": "verify-native", "elapsed_ms": evaluation.verify_ms, "metadata": null },
                { "phase": "reason-verify-total", "elapsed_ms": elapsed, "metadata": null },
            ],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    if !evaluation.report.ok() {
        // Emit the findings, exactly as the standalone `verify` command does. A
        // refusal that reports only a COUNT cannot be acted on: it says the reasoned
        // graph carries a violation without saying WHICH, and the whole content of
        // the diagnosis is which obligation, query, or gate produced it.
        emit_report(&evaluation.report);
        return fail(format!(
            "verify: {} violation(s) on the reasoned graph",
            evaluation.report.error_count()
        ));
    }
    println!("native EL/DL reasoning + reasoned-graph verify (Docker-free)");
    0
}

/// `gmeow-dev explain` — explain unsatisfiable classes / inconsistency.
pub fn explain() -> i32 {
    let root = project_root();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let edb = match snapshot_reasoning_dataset(dataset.as_ref()) {
        Ok(edb) => edb,
        Err(code) => return code,
    };
    let result = match reason_all(edb.as_ref()) {
        Ok(r) => r,
        Err(e) => return fail(format!("explain failed: {e}")),
    };
    if result.is_consistent() {
        println!("no unsatisfiable classes");
        return 0;
    }
    // The native explanation: enumerate the contradiction witnesses the reasoner
    // recorded for the glut. The native verdict stands here.
    let witnesses = &result.provenance.contradiction_witnesses;
    note(
        "gmeow-dev.explain.inconsistent",
        format!(
            "ontology is inconsistent: {} contradiction witness(es)",
            witnesses.len()
        ),
    );
    for w in witnesses {
        note("gmeow-dev.explain.witness", format!("  {w:?}"));
    }
    fail("inconsistent ontology")
}

/// The six `logic:ReasoningPreset` local names.
const VALID_PROFILES: &[&str] = &[
    "PositiveHornProfile",
    "StratifiedNAFProfile",
    "WellFoundedProfile",
    "StableModelProfile",
    "ProceduralPrologProfile",
    "ProbabilisticProfile",
];

/// `gmeow-dev certify INPUT --profile P` — statically certify a `.logic` program.
pub fn certify(input_path: &Path, profile: Option<&str>) -> i32 {
    if !input_path.is_file() {
        return fail(format!(
            "certify: input not found: {}",
            input_path.display()
        ));
    }
    let profile_str = match resolve_profile(input_path, profile) {
        Ok(p) => p,
        Err(code) => return code,
    };
    if !VALID_PROFILES.contains(&profile_str.as_str()) {
        return fail(format!(
            "certify: unknown profile {profile_str:?}; must be one of {VALID_PROFILES:?}"
        ));
    }
    let source_ttl = match std::fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            return fail(format!(
                "certify: cannot read {}: {e}",
                input_path.display()
            ));
        }
    };
    let (program, _diag) = match gmeow_logic_compile::frontend::parse_logic_str(&source_ttl, None) {
        Ok(p) => p,
        Err(e) => {
            return fail(format!(
                "certify: cannot compile {}: {}",
                input_path.display(),
                e.0
            ));
        }
    };
    let verdict = match gmeow_logic::certify::certify_program(&program, &profile_str) {
        Ok(v) => v,
        Err(e) => return fail(format!("certify: native certifier failed: {e}")),
    };
    if verdict.violations.is_empty() {
        let name = input_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("input");
        println!("certify: {name} is certified for {profile_str}");
        0
    } else {
        for v in &verdict.violations {
            note("gmeow-dev.certify.violation", format!("  {v}"));
        }
        fail(format!(
            "certify: {} violation(s) for {profile_str}",
            verdict.violations.len()
        ))
    }
}

/// Resolve the declared profile: `--profile` > sibling `profile.json` > default.
fn resolve_profile(input_path: &Path, profile: Option<&str>) -> Result<String, i32> {
    if let Some(p) = profile {
        return Ok(p.to_owned());
    }
    let sibling = input_path
        .parent()
        .map(|p| p.join("profile.json"))
        .unwrap_or_else(|| PathBuf::from("profile.json"));
    if sibling.is_file() {
        let text = std::fs::read_to_string(&sibling)
            .map_err(|e| fail(format!("certify: cannot read {}: {e}", sibling.display())))?;
        let data: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| fail(format!("certify: cannot read {}: {e}", sibling.display())))?;
        if let Some(contract) = data.get("reasoning_contract") {
            let preset = contract.get("preset").and_then(|v| v.as_str());
            match preset {
                Some(p) => return Ok(p.to_owned()),
                None => {
                    return Err(fail(format!(
                        "certify: malformed reasoning_contract in {}: expected an object with a string 'preset'",
                        sibling.display()
                    )));
                }
            }
        }
    }
    Ok("PositiveHornProfile".to_owned())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn reason_verify_orchestration_invokes_the_result_producer_once() {
        let dataset = purrdf::RdfDataset::union(&[]);
        let calls = Cell::new(0usize);
        let evaluation = evaluate_reason_verify_once(&dataset, &[], || {
            calls.set(calls.get() + 1);
            reason_all(&dataset)
        })
        .expect("empty dataset reasons and verifies");

        assert_eq!(
            calls.get(),
            1,
            "the complete closure is produced exactly once"
        );
        assert!(evaluation.result.is_decided_consistent());
        assert!(evaluation.report.ok());
    }
}
