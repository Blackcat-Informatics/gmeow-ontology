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
    elapsed_ms, fail, fail_code, project_root, snapshot_bytes, write_timings_json,
};

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
fn shipped_reasoning_result(dataset: &purrdf::RdfDataset) -> Result<ReasoningResult, String> {
    let graph = dataset.project_named_graph(gmeow_logic::result_rdf::GRAPH_REASONING);
    if graph.quad_count() == 0 {
        return Err(
            "the snapshot carries no graph/reasoning verdict; run `make regenerate` \
             (or re-reason with --fresh)"
                .to_string(),
        );
    }
    // The projected sub-dataset is default-graph only, so its canonical N-Quads
    // lines are `s p o .` — exactly the N-Triples shape the reverse parser reads.
    let nt = purrdf::serialize_dataset(
        &graph,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| format!("serialize graph/reasoning: {e}"))?;
    let nt = String::from_utf8(nt).map_err(|e| format!("graph/reasoning is not UTF-8: {e}"))?;
    let result = gmeow_logic::result_rdf::parse_reasoning_graph(&nt)?;
    let current = native_contract_hash();
    if result.provenance.contract_hash != current {
        return Err(format!(
            "the shipped graph/reasoning verdict was minted under reasoning contract \
             {shipped} but this binary implements {current}; run `make regenerate` to \
             re-mint the bundle (or re-reason with --fresh)",
            shipped = result.provenance.contract_hash,
        ));
    }
    Ok(result)
}

/// `gmeow-dev reason [--mode --fresh --merge …]` — native EL/DL reasoning. By
/// default the shipped `graph/reasoning` verdict is reused (contract-hash-checked);
/// `--fresh` recomputes the closure with the native engine.
pub fn reason(mode: &str, fresh: bool, timings_json: Option<&Path>) -> i32 {
    if mode == "docker" {
        return fail_code(
            "reason --mode docker needs the classic ELK/HermiT container stack, which the \
             native binary does not embed; use --mode native (the Docker-free authority)",
            2,
        );
    }
    if mode != "native" {
        return fail(format!(
            "unknown reasoning mode: {mode:?} (expected native or docker)"
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
    let ok = result.is_consistent();
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
    if mode == "docker" {
        return fail_code(
            "verify --mode docker needs the classic ROBOT container stack, which the native \
             binary does not embed; use --mode native (the Docker-free authority)",
            2,
        );
    }
    if mode != "native" {
        return fail(format!(
            "unknown verify mode: {mode:?} (expected native or docker)"
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
    let text = gmeow_errors::render::to_text(&report.normalized());
    if !text.trim().is_empty() {
        eprintln!("{text}");
    }
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
    if !result.is_consistent() {
        return fail("inconsistent ontology");
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
    // recorded for the glut. The Docker HermiT oracle is only for extra-detailed
    // derivations (`make maint-reason-hermit`); the native verdict stands here.
    let witnesses = &result.provenance.contradiction_witnesses;
    eprintln!(
        "ontology is inconsistent: {} contradiction witness(es)",
        witnesses.len()
    );
    for w in witnesses {
        eprintln!("  {w:?}");
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
    let arts = match gmeow_logic_compile::projections::compile_program(&program) {
        Ok(a) => a,
        Err(e) => {
            return fail(format!(
                "certify: cannot compile {}: {e}",
                input_path.display()
            ));
        }
    };
    let verdict = match gmeow_logic::certify::certify(&arts.nemo_rules, &profile_str, None) {
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
        eprintln!(
            "certify: {} violation(s) for {profile_str}",
            verdict.violations.len()
        );
        for v in &verdict.violations {
            eprintln!("  {v}");
        }
        1
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
