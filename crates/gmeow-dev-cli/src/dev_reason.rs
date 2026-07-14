// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native reasoning commands: `reason`, `verify`, `reason-verify`, `explain`,
//! and the `certify` static profile check.
//!
//! All run over the folded `gmeow.gts` snapshot, whose default graph ALREADY
//! carries the pipeline's reasoned closure and whose `graph/reasoning` carries the
//! typed verdict-and-provenance result. The lanes therefore REUSE that shipped
//! result by default (contract-hash-checked) instead of recomputing the closure
//! the pipeline just shipped; `--fresh` forces a full re-reasoning pass with the
//! Java/Docker-free Rust EL/DL engine (`gmeow_logic`). `certify` statically
//! certifies a `.logic` program against its declared semantic profile.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gmeow_logic::entail_crosscheck::{CrosscheckOutcome, run_entail_crosscheck};
use gmeow_logic::reason::ledger::DivergenceKind;
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

/// Re-derive the SHIPPED typed reasoning result from the snapshot's
/// `graph/reasoning` projection — the SAME reverse parse the pipeline cache uses —
/// and refuse a verdict minted under a different reasoning contract than this
/// binary's engine: a stale bundle must be regenerated (or re-reasoned with
/// `--fresh`), never re-reported as current.
fn shipped_reasoning_result(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<ReasoningResult> {
    let graph = dataset.project_named_graph(gmeow_logic::result_rdf::GRAPH_REASONING);
    if graph.quad_count() == 0 {
        return Err(error::reasoning(
            "the snapshot carries no graph/reasoning verdict; run `make regenerate` \
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
             {shipped} but this binary implements {current}; run `make regenerate` to \
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
        match reason_all(dataset.as_ref()) {
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

/// Discover the `(name, sparql)` verify SELECT queries in the working tree:
/// `queries/verify/*.rq` plus each slice's `queries/verify/*.rq`.
fn discover_verify_queries(root: &Path) -> Vec<(String, String)> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rq(&root.join("queries").join("verify"), &mut files);
    collect_slice_verify(&root.join("slices"), &mut files);
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let name = p.to_string_lossy().into_owned();
            std::fs::read_to_string(&p).ok().map(|text| (name, text))
        })
        .collect()
}

/// Collect `*.rq` directly under `dir`.
fn collect_rq(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rq") {
                out.push(path);
            }
        }
    }
}

/// Collect `queries/verify/*.rq` under every slice directory below `slices`.
fn collect_slice_verify(slices: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(slices) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let verify = path.join("queries").join("verify");
            if verify.is_dir() {
                collect_rq(&verify, out);
            }
            collect_slice_verify(&path, out);
        }
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
    let queries = discover_verify_queries(&root);
    let report = if fresh {
        match verify_reasoned(dataset.as_ref(), &queries) {
            Ok(r) => r,
            Err(e) => return fail(format!("native verify failed: {e}")),
        }
    } else {
        let result = match shipped_reasoning_result(dataset.as_ref()) {
            Ok(r) => r,
            Err(e) => return fail(format!("cannot reuse the shipped verdict: {e}")),
        };
        match verify_with_reasoning_result(dataset.as_ref(), &result, &queries) {
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
    F: FnOnce(&purrdf::RdfDataset) -> gmeow_errors::Result<ReasoningResult>,
{
    let result_started = Instant::now();
    let result = produce_result(dataset)?;
    let result_ms = elapsed_ms(result_started);

    if !result.is_decided_consistent() {
        return if result.is_consistent() {
            Err(error::reasoning(format!(
                "cannot decide consistency: {n} out-of-fragment construct(s) the native \
                 path does not decide; refusing to verify",
                n = result.preservation.unsupported_constructs.len(),
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
    let snapshot_ms = elapsed_ms(snapshot_started);
    let queries = discover_verify_queries(&root);
    let (evaluation, result_phase) = if fresh {
        (
            evaluate_reason_verify_once(dataset.as_ref(), &queries, |edb| {
                reason_all(edb)
                    .map_err(|e| error::reasoning(format!("native reason+verify failed: {e}")))
            }),
            "reason-native",
        )
    } else {
        (
            evaluate_reason_verify_once(dataset.as_ref(), &queries, |edb| {
                shipped_reasoning_result(edb)
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
        return fail(format!(
            "verify: {} violation(s) on the reasoned graph",
            evaluation.report.error_count()
        ));
    }
    println!("native EL/DL reasoning + reasoned-graph verify (Docker-free)");
    0
}

/// `gmeow-dev reason-crosscheck` — the native ↔ entail-oracle subsumption
/// divergence cross-check (the Docker-free replacement for the retired external
/// subsumption lane).
///
/// Loads the committed bundle EDB (the same snapshot import `reason-verify` uses),
/// drives gmeow's own native reasoner against the independent, conformance-tested
/// `purrdf::entail` OWL-RL subsumption oracle, prints the structured divergence
/// ledger, and returns exit `0` iff the strict `enforce()` **superset** verdict
/// passes: `native ⊇ oracle`, i.e. no OracleOnly / DlGap / CorpusOnly row.
/// NativeOnly rows are gmeow's richer world-scoped closure and are expected, not a
/// failure. Any real divergence is printed with its classification so a red is a
/// diagnosable disagreement, never an opaque failure.
pub fn reason_crosscheck(timings_json: Option<&Path>) -> i32 {
    let root = project_root();
    let started = Instant::now();
    let snapshot_started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let snapshot_ms = elapsed_ms(snapshot_started);
    // Reason FRESH: the cross-check needs the COMPLETE native subsumption closure.
    // The shipped `graph/reasoning` projection persists a reduced subsumption set
    // (its transitive surface omits ~70 links the OWL-RL oracle still derives), so
    // reusing it would under-report native and false-flag those as OracleOnly. A
    // fresh `reason_all` is the full world-partitioned closure the comparison needs.
    let reason_started = Instant::now();
    let result = match reason_all(dataset.as_ref()) {
        Ok(r) => r,
        Err(e) => return fail(format!("reason-crosscheck: native reasoning failed: {e}")),
    };
    let reason_ms = elapsed_ms(reason_started);
    let oracle_started = Instant::now();
    let outcome = match run_entail_crosscheck(&result, dataset.as_ref()) {
        Ok(o) => o,
        Err(e) => return fail(format!("reason-crosscheck failed: {e}")),
    };
    let oracle_ms = elapsed_ms(oracle_started);
    let elapsed = elapsed_ms(started);

    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "reason-crosscheck",
            "mode": "native",
            "ok": outcome.verdict.passed,
            "metrics": {
                "inferred_axioms": result.inferred().len(),
                "source_worlds": outcome.source_worlds,
                "native_subsumptions": outcome.native_subsumptions,
                "oracle_subsumptions": outcome.oracle_subsumptions,
                "agree": outcome.ledger.agree,
                "native_only": outcome.ledger.native_only,
                "oracle_only": outcome.ledger.oracle_only,
                "dl_gap": outcome.ledger.dl_gap,
            },
            "timings": [
                { "phase": "snapshot-import", "elapsed_ms": snapshot_ms, "metadata": null },
                { "phase": "reason-native", "elapsed_ms": reason_ms, "metadata": null },
                { "phase": "entail-oracle", "elapsed_ms": oracle_ms, "metadata": format!("worlds={}", outcome.source_worlds) },
                { "phase": "reason-crosscheck-total", "elapsed_ms": elapsed, "metadata": null },
            ],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }

    emit_crosscheck_outcome(&outcome)
}

fn emit_crosscheck_outcome(outcome: &CrosscheckOutcome) -> i32 {
    let ledger = &outcome.ledger;
    println!(
        "native ↔ entail-oracle subsumption cross-check (Docker-free): {worlds} source world(s); \
         {native} native vs {oracle} oracle subsumption(s)",
        worlds = outcome.source_worlds,
        native = outcome.native_subsumptions,
        oracle = outcome.oracle_subsumptions,
    );
    println!(
        "  ledger: {agree} agree, {native_only} native-only (expected richness), \
         {oracle_only} oracle-only, {dl_gap} dl-gap",
        agree = ledger.agree,
        native_only = ledger.native_only,
        oracle_only = ledger.oracle_only,
        dl_gap = ledger.dl_gap,
    );

    // Print every FAILING row so a divergence is diagnosable, not opaque. NativeOnly
    // rows are NOT failures — they are gmeow's richer world-scoped closure (`native ⊇
    // oracle`) — so they are summarized by count above, not enumerated as noise.
    for row in &ledger.rows {
        let tag = match row.kind {
            DivergenceKind::OracleOnly => "oracle-only",
            DivergenceKind::DlGap => "dl-gap",
            DivergenceKind::CorpusOnly => "corpus-only",
            DivergenceKind::Agree | DivergenceKind::NativeOnly => continue,
        };
        note(
            "gmeow-dev.reason-crosscheck.divergence",
            format!("  [{tag}] ({}) {}", row.category, row.detail),
        );
    }

    if outcome.verdict.passed {
        println!(
            "reason-crosscheck: native ⊇ oracle — no divergence (native, Docker-free); \
             {} native-only enrichment(s)",
            ledger.native_only
        );
        0
    } else {
        for reason in &outcome.verdict.reasons {
            note(
                "gmeow-dev.reason-crosscheck.reason",
                format!("reason-crosscheck: {reason}"),
            );
        }
        fail("reason-crosscheck: native↔oracle divergence")
    }
}

/// `gmeow-dev reason-gate` — one snapshot import and one complete native closure
/// feeding both reasoned-graph verification and the independent purrdf oracle.
pub fn reason_gate(timings_json: Option<&Path>) -> i32 {
    let root = project_root();
    let started = Instant::now();
    let snapshot_started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let snapshot_ms = elapsed_ms(snapshot_started);
    let queries = discover_verify_queries(&root);
    let evaluation = match evaluate_reason_verify_once(dataset.as_ref(), &queries, |edb| {
        reason_all(edb).map_err(|e| error::reasoning(format!("native reason gate failed: {e}")))
    }) {
        Ok(evaluation) => evaluation,
        Err(message) => return fail(message),
    };

    let oracle_started = Instant::now();
    let outcome = match run_entail_crosscheck(&evaluation.result, dataset.as_ref()) {
        Ok(outcome) => outcome,
        Err(e) => return fail(format!("reason gate cross-check failed: {e}")),
    };
    let oracle_ms = elapsed_ms(oracle_started);
    let elapsed = elapsed_ms(started);
    let ok = evaluation.report.ok() && outcome.verdict.passed;

    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "reason-gate",
            "mode": "native",
            "ok": ok,
            "metrics": {
                "inferred_axioms": evaluation.result.inferred().len(),
                "verify_errors": evaluation.report.error_count(),
                "source_worlds": outcome.source_worlds,
                "native_subsumptions": outcome.native_subsumptions,
                "oracle_subsumptions": outcome.oracle_subsumptions,
                "agree": outcome.ledger.agree,
                "native_only": outcome.ledger.native_only,
                "oracle_only": outcome.ledger.oracle_only,
                "dl_gap": outcome.ledger.dl_gap,
            },
            "timings": [
                { "phase": "snapshot-import", "elapsed_ms": snapshot_ms, "metadata": null },
                { "phase": "reason-native", "elapsed_ms": evaluation.result_ms, "metadata": null },
                { "phase": "verify-native", "elapsed_ms": evaluation.verify_ms, "metadata": null },
                { "phase": "entail-oracle", "elapsed_ms": oracle_ms, "metadata": format!("worlds={}", outcome.source_worlds) },
                { "phase": "reason-gate-total", "elapsed_ms": elapsed, "metadata": null },
            ],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }

    if evaluation.report.ok() {
        println!("native EL/DL reasoning + reasoned-graph verify (Docker-free)");
    }
    let crosscheck_code = emit_crosscheck_outcome(&outcome);
    if !evaluation.report.ok() {
        return fail(format!(
            "verify: {} violation(s) on the reasoned graph",
            evaluation.report.error_count()
        ));
    }
    if crosscheck_code != 0 {
        return crosscheck_code;
    }
    println!("reason gate: one native closure passed verify + entail-oracle cross-check");
    0
}

/// `gmeow-dev explain` — explain unsatisfiable classes / inconsistency.
pub fn explain() -> i32 {
    let root = project_root();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let result = match reason_all(dataset.as_ref()) {
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
        let evaluation = evaluate_reason_verify_once(&dataset, &[], |edb| {
            calls.set(calls.get() + 1);
            reason_all(edb)
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
