// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev logic {query,compile}` — the logic-stack developer surface.
//!
//! `logic query` resolves a backward `.logic` goal over a materialized world
//! through the native dispatcher (`gmeow_logic::dispatch`); `logic compile` emits
//! or drift-checks the ten generated logic-projection artifacts via the
//! Native `gmeow_logic_compile` compiler and the whole-pipeline drift gate.

use std::path::Path;

use gmeow_logic::counterfactual;
use gmeow_logic::dispatch::dispatch_query;
use gmeow_logic::probabilistic;
use gmeow_logic::profile_gate;
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::seam::WorldFactSnapshot;
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::projections::compile_program;
use gmeow_pipeline::run::{RunMode, run_full};

use crate::dev_common::{LOGIC_DRIFT_PREFIXES, fail, note, project_root};
use crate::error;

/// One resolved answer binding, `var → canonical-value`, plus an optional weight.
struct Answer {
    vars: Vec<(String, String)>,
    probability: Option<f64>,
}

/// `gmeow-dev logic query WORLD QUERY_FILE …` — resolve a backward goal over a
/// materialized world, routing through the native dispatcher.
#[allow(clippy::too_many_arguments)]
pub fn query(
    world: &Path,
    query_file: &Path,
    profile: &str,
    world_iri: Option<&str>,
    max_answers: Option<usize>,
    max_steps: Option<u64>,
    as_json: bool,
) -> i32 {
    if !world.is_file() {
        return fail(format!("world N-Quads file not found: {}", world.display()));
    }
    if !query_file.is_file() {
        return fail(format!("query file not found: {}", query_file.display()));
    }
    let nquads = match std::fs::read_to_string(world) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot read {}: {e}", world.display())),
    };
    let program_src = match std::fs::read_to_string(query_file) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot read {}: {e}", query_file.display())),
    };

    match resolve_query(
        &nquads,
        &program_src,
        profile,
        world_iri,
        max_answers,
        max_steps,
    ) {
        Ok((answers, status)) => render_query(&answers, &status, as_json),
        Err(e) => fail(format!("query failed: {e}")),
    }
}

/// The native resolution path — a faithful Rust twin of the `gmeow_logic.query`
/// PyO3 surface (probabilistic / counterfactual / plain backward-goal routing).
fn resolve_query(
    nquads: &str,
    program_src: &str,
    profile: &str,
    world_iri: Option<&str>,
    max_answers: Option<usize>,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(Vec<Answer>, String)> {
    let store = WorldStore::new();
    store.load_nquads(nquads).map_err(error::logic)?;

    let world = match world_iri {
        Some(w) => w.to_owned(),
        None => {
            let worlds = store.worlds();
            if worlds.len() != 1 {
                return Err(error::logic(format!(
                    "world_iri not given and the store has {} named graphs (need exactly 1): {worlds:?}",
                    worlds.len()
                )));
            }
            worlds.into_iter().next().expect("len == 1")
        }
    };

    let program = parse_query_program(program_src).map_err(error::logic)?;

    // Probabilistic marginal inference (the only path that emits a per-binding weight).
    if profile_gate::is_probabilistic_profile(profile) {
        let answer = probabilistic::evaluate(&store, &world, &program, profile, None)
            .map_err(error::logic)?;
        let answers = answer
            .bindings
            .iter()
            .map(|b| Answer {
                vars: b.vars.clone().into_iter().collect(),
                probability: Some(b.probability),
            })
            .collect();
        return Ok((answers, answer.status_str().to_owned()));
    }

    let budget = Budget {
        max_answers,
        max_steps,
    };

    let (bindings, status): (Vec<gmeow_logic::query_ir::Binding>, String) =
        if counterfactual::is_counterfactual(&program) {
            let depth = program
                .counterfactual
                .as_ref()
                .and_then(|c| c.depth_budget)
                .unwrap_or(counterfactual::DEFAULT_DEPTH_BUDGET);
            let cf = counterfactual::construct_and_resolve(
                &store, &program, profile, &budget, depth, None,
            )
            .map_err(error::logic)?;
            let status = cf.status_str().to_owned();
            (cf.bindings, status)
        } else {
            let foreign =
                WorldFactSnapshot::from_world(&store, &world, profile).map_err(error::logic)?;
            let answer = dispatch_query(&foreign, &world, &program, profile, &budget)
                .map_err(error::logic)?;
            (answer.bindings, answer.status.as_str().to_owned())
        };

    let answers = bindings
        .into_iter()
        .map(|vars| Answer {
            vars: vars.into_iter().collect(),
            probability: None,
        })
        .collect();
    Ok((answers, status))
}

/// Product results → stdout: a JSON `{bindings,status}` blob, or a per-answer table.
fn render_query(answers: &[Answer], status: &str, as_json: bool) -> i32 {
    if as_json {
        let bindings: Vec<serde_json::Value> = answers
            .iter()
            .map(|a| {
                let mut row = serde_json::Map::new();
                for (k, v) in &a.vars {
                    row.insert(k.clone(), serde_json::Value::String(v.clone()));
                }
                if let Some(p) = a.probability
                    && let Some(n) = serde_json::Number::from_f64(p)
                {
                    row.insert("probability".to_owned(), serde_json::Value::Number(n));
                }
                serde_json::Value::Object(row)
            })
            .collect();
        let out = serde_json::json!({ "bindings": bindings, "status": status });
        match serde_json::to_string(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(format!("cannot serialize JSON: {e}")),
        }
        return 0;
    }

    if answers.is_empty() {
        println!("no answers");
    } else {
        for a in answers {
            let mut vars = a.vars.clone();
            vars.sort();
            let rendered: Vec<String> = vars.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            if rendered.is_empty() {
                println!("(true)");
            } else {
                println!("{}", rendered.join(", "));
            }
        }
    }
    note(
        "gmeow-dev.logic-query.status",
        format!("{} answer(s); status={status}", answers.len()),
    );
    0
}

/// The ten `logic compile --mode` back-ends, in the Python `_LOGIC_MODES` order.
pub const LOGIC_MODES: &[&str] = &[
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "clif",
    "cgif",
    "xcl",
    "report",
];

/// `gmeow-dev logic compile [--check] [--mode M]` — emit or drift-check the
/// generated logic artifacts.
pub fn compile(check: bool, mode: Option<&str>) -> i32 {
    if let Some(m) = mode
        && !LOGIC_MODES.contains(&m)
    {
        return fail(format!(
            "unknown --mode {m:?} (valid: {})",
            LOGIC_MODES.join(", ")
        ));
    }
    let root = project_root();

    // --check with no --mode: whole-pipeline drift gate, filtered to logic prefixes.
    if check && mode.is_none() {
        let jobs = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let report = match run_full(&root, jobs, RunMode::Check) {
            Ok(r) => r,
            Err(e) => return fail(format!("pipeline check failed: {e}")),
        };
        let drift: Vec<&String> = report
            .drifted
            .iter()
            .filter(|d| LOGIC_DRIFT_PREFIXES.iter().any(|p| d.contains(p)))
            .collect();
        if !drift.is_empty() {
            let mut sorted = drift.clone();
            sorted.sort();
            for rel in sorted {
                note("gmeow-dev.logic-compile.drift", format!("drift {rel}"));
            }
            return fail(format!(
                "{} logic artifact(s) out of date — run `gmeow-dev logic compile`",
                drift.len()
            ));
        }
        println!("logic: committed artifacts match source (no drift)");
        return 0;
    }

    // --mode M (with or without --check): compile in-process and emit/inspect one back-end.
    if let Some(mode) = mode {
        return compile_one_mode(&root, mode, check);
    }

    // Default full render: the whole pipeline reproduces every committed artifact.
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    match run_full(&root, jobs, RunMode::Update) {
        Ok(_) => {
            println!("logic: artifacts compiled");
            0
        }
        Err(e) => fail(format!("logic compile failed: {e}")),
    }
}

/// The committed target path (relative to root) + compiled-artifact selector for a mode.
fn mode_target(
    mode: &str,
) -> (
    &'static str,
    fn(&gmeow_logic_compile::projections::CompiledArtifacts) -> &String,
) {
    match mode {
        "owl-dl" => ("generated/owl/gmeow-dl.ttl", |a| &a.owl_dl),
        "owl-el" => ("generated/owl/gmeow-el.ttl", |a| &a.owl_el),
        "datalog" => ("generated/datalog/gmeow.dl", |a| &a.datalog),
        "n3" => ("generated/n3/gmeow.n3", |a| &a.n3),
        "gufo" => ("generated/foundation/gufo.ttl", |a| &a.gufo),
        "canonical-rdf12" => ("generated/logic/gmeow.logic.rdf12.ttl", |a| {
            &a.canonical_rdf12
        }),
        "clif" => ("generated/cl/gmeow.clif", |a| &a.clif),
        "cgif" => ("generated/cl/gmeow.cgif", |a| &a.cgif),
        "xcl" => ("generated/cl/gmeow.xcl", |a| &a.xcl),
        _ => ("generated/logic/projection-report.ttl", |a| &a.report),
    }
}

/// Compile the `logic:` source and emit (or drift-check) exactly one back-end.
fn compile_one_mode(root: &Path, mode: &str, check: bool) -> i32 {
    let source = root.join("slices/grounding/logic/module.ttl");
    let source_ttl = match std::fs::read_to_string(&source) {
        Ok(s) => s,
        Err(e) => {
            return fail(format!(
                "logic: source not found: {} ({e})",
                source.display()
            ));
        }
    };
    let (program, diagnostics) = match parse_logic_str(&source_ttl, None) {
        Ok(p) => p,
        Err(e) => return fail(format!("logic: parse failed: {}", e.0)),
    };
    for d in &diagnostics {
        note(
            "gmeow-dev.logic-compile.diagnostic",
            format!("{} [{}] {}", d.severity.as_str(), d.code, d.message),
        );
    }
    // Discharge every authored correspondence's lens law by EXECUTION so the five
    // correspondence gates inside `compile_program` read a real per-correspondence verdict.
    // A correspondence-free source yields an empty map (the gates never run); a source that
    // declares `logic:Correspondence` cells supplies a verdict for each, so the gates never
    // reach their missing-verdict hard-fail. A malformed leg registry is a clean error.
    let verdicts = match gmeow_logic::correspondence_exec::logic_program_verdicts(&program) {
        Ok(v) => v,
        Err(e) => {
            return fail(format!(
                "logic: discharge correspondence lens laws failed: {e}"
            ));
        }
    };
    let arts = match compile_program(&program, &verdicts) {
        Ok(a) => a,
        Err(e) => return fail(format!("logic: compile failed: {e}")),
    };

    let (rel, select) = mode_target(mode);
    let content = select(&arts);
    let target = root.join(rel);

    if check {
        let committed = std::fs::read_to_string(&target).unwrap_or_default();
        if &committed != content {
            note("gmeow-dev.logic-compile.drift", format!("drift {rel}"));
            return fail(format!("--mode {mode}: committed artifact drifted"));
        }
        println!("--mode {mode}: no drift");
        return 0;
    }
    if let Some(parent) = target.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(format!("cannot create {}: {e}", parent.display()));
    }
    if let Err(e) = std::fs::write(&target, content) {
        return fail(format!("cannot write {}: {e}", target.display()));
    }
    println!("{rel}");
    0
}

#[cfg(test)]
mod query_tests {
    use super::resolve_query;

    const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

    #[test]
    fn counterfactual_depth_refusal_does_not_build_an_unused_base_snapshot() {
        // The quoted-triple object is deliberately outside the snapshot reifier
        // contract. A depth-zero counterfactual returns before it needs any base
        // snapshot; the plain-query preparation path must not run speculatively.
        let nquads = "<https://ex/s> <https://ex/meta> \
                      <<( <https://ex/qs> <https://ex/qp> <https://ex/qo> )>> \
                      <http://world/base> .\n";
        let program = ":- prefix(ex, 'https://ex/').\n\
                       :- counterfactual('http://world/cf', 'http://world/base').\n\
                       :- depth_budget(0).\n\
                       :- assume(ex:p2(ex:s, ex:o2)).\n\
                       ?- ex:p(ex:s, Z).\n";

        let (answers, status) =
            resolve_query(nquads, program, HORN_PROFILE, None, None, None).unwrap();
        assert!(answers.is_empty());
        assert_eq!(status, "incomplete");
    }
}
