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

/// `gmeow-dev reason-verify [--fresh --merge --timings-json]` — reason + verify in
/// one pass. By default both halves reuse the shipped closure + verdict
/// (contract-hash-checked, ONE snapshot import and no chase); `--fresh` recomputes.
pub fn reason_verify(fresh: bool, timings_json: Option<&Path>) -> i32 {
    let root = project_root();
    let started = Instant::now();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let (result, phase) = if fresh {
        match reason_all(dataset.as_ref()) {
            Ok(r) => (r, "reason-verify-native"),
            Err(e) => return fail(format!("native reason+verify failed: {e}")),
        }
    } else {
        match shipped_reasoning_result(dataset.as_ref()) {
            Ok(r) => (r, "reason-verify-shipped"),
            Err(e) => return fail(format!("cannot reuse the shipped verdict: {e}")),
        }
    };
    if !result.is_decided_consistent() {
        // Refuse to verify unless consistency is DECIDED: a glut is inconsistent,
        // and an undetermined (out-of-fragment) bundle cannot soundly carry a
        // verification claim — honest cannot-decide, never a wrong pass.
        return if result.is_consistent() {
            fail(format!(
                "cannot decide consistency: {n} out-of-fragment construct(s) the native \
                 path does not decide; refusing to verify",
                n = result.preservation.unsupported_constructs.len(),
            ))
        } else {
            fail("inconsistent ontology")
        };
    }
    let queries = discover_verify_queries(&root);
    let report = if fresh {
        match verify_reasoned(dataset.as_ref(), &queries) {
            Ok(r) => r,
            Err(e) => return fail(format!("native reason+verify failed: {e}")),
        }
    } else {
        match verify_with_reasoning_result(dataset.as_ref(), &result, &queries) {
            Ok(r) => r,
            Err(e) => return fail(format!("native reason+verify failed: {e}")),
        }
    };
    let elapsed = elapsed_ms(started);
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "reason-verify",
            "mode": "native",
            "ok": report.ok(),
            "timings": [{ "phase": phase, "elapsed_ms": elapsed, "metadata": null }],
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    if !report.ok() {
        return fail(format!(
            "verify: {} violation(s) on the reasoned graph",
            report.error_count()
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
pub fn reason_crosscheck() -> i32 {
    use gmeow_logic::entail_crosscheck::run_entail_crosscheck;
    use gmeow_logic::reason::ledger::DivergenceKind;

    let root = project_root();
    let dataset = match snapshot_dataset(&root) {
        Ok(d) => d,
        Err(code) => return code,
    };
    // Reason FRESH: the cross-check needs the COMPLETE native subsumption closure.
    // The shipped `graph/reasoning` projection persists a reduced subsumption set
    // (its transitive surface omits ~70 links the OWL-RL oracle still derives), so
    // reusing it would under-report native and false-flag those as OracleOnly. A
    // fresh `reason_all` is the full world-partitioned closure the comparison needs.
    let result = match reason_all(dataset.as_ref()) {
        Ok(r) => r,
        Err(e) => return fail(format!("reason-crosscheck: native reasoning failed: {e}")),
    };
    let outcome = match run_entail_crosscheck(&result, dataset.as_ref()) {
        Ok(o) => o,
        Err(e) => return fail(format!("reason-crosscheck failed: {e}")),
    };

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
