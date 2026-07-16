// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev logic {query,compile}` — the logic-stack developer surface.
//!
//! `logic query` resolves a backward `.logic` goal over a materialized world
//! through the native dispatcher (`gmeow_logic::dispatch`); `logic compile` emits
//! or drift-checks the generated logic-projection artifacts by running the
//! REAL whole-pipeline render/drift-check (`gmeow_pipeline::run::run_full`).
//! `--mode M` is a filter over that single pipeline output — the committed
//! path for the requested back-end — never a second, in-process compile, so
//! its bytes (and its `report` mode) can never diverge from what the
//! pipeline itself commits.

use std::path::Path;

use gmeow_logic::counterfactual;
use gmeow_logic::dispatch::dispatch_query;
use gmeow_logic::probabilistic;
use gmeow_logic::profile_gate;
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::seam::WorldFactSnapshot;
use gmeow_logic::store::WorldStore;
use gmeow_pipeline::run::{RunMode, run_full};
use gmeow_pipeline::stages::compile_logic::{
    CANONICAL_RDF12_PATH, CGIF_PATH, CLIF_PATH, DATALOG_PATH, GUFO_PATH, N3_PATH, OWL_DL_PATH,
    OWL_EL_PATH, PROJECTION_REPORT_PATH, XCL_PATH,
};

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

    // --mode M (with or without --check): run the REAL pipeline once and narrow
    // to the single committed artifact for the requested back-end. There is no
    // second, in-process compile — the whole-pipeline render is the single
    // producer of every committed logic artifact, `report` included.
    if let Some(mode) = mode {
        let rel = mode_path(mode);
        let jobs = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        if check {
            let report = match run_full(&root, jobs, RunMode::Check) {
                Ok(r) => r,
                Err(e) => return fail(format!("pipeline check failed: {e}")),
            };
            if report.drifted.iter().any(|d| d == rel) {
                note("gmeow-dev.logic-compile.drift", format!("drift {rel}"));
                return fail(format!("--mode {mode}: committed artifact drifted"));
            }
            println!("--mode {mode}: no drift");
            return 0;
        }
        return match run_full(&root, jobs, RunMode::Update) {
            Ok(_) => {
                println!("{rel}");
                0
            }
            Err(e) => fail(format!("logic compile failed: {e}")),
        };
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

/// The single committed path (relative to root) that `--mode M` narrows the
/// whole-pipeline output to. This `&str` constant, imported from the pipeline
/// stage that actually produces it, is the SINGLE authority for where each
/// back-end lands — there is no second path table and no in-process compile.
fn mode_path(mode: &str) -> &'static str {
    match mode {
        "owl-dl" => OWL_DL_PATH,
        "owl-el" => OWL_EL_PATH,
        "datalog" => DATALOG_PATH,
        "n3" => N3_PATH,
        "gufo" => GUFO_PATH,
        "canonical-rdf12" => CANONICAL_RDF12_PATH,
        "clif" => CLIF_PATH,
        "cgif" => CGIF_PATH,
        "xcl" => XCL_PATH,
        _ => PROJECTION_REPORT_PATH,
    }
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

#[cfg(test)]
mod compile_tests {
    use std::sync::Mutex;

    use super::compile;

    // These drive the REAL `compile()` entry point (which runs the whole
    // pipeline via `run_full`) over the committed repository tree, so
    // `project_root()`'s CARGO_MANIFEST_DIR fallback resolves the workspace
    // root regardless of the test harness's current working directory.
    // They are the discriminating proof that `--mode M` narrows the single
    // committed pipeline output rather than re-running a private, thinner
    // in-process compile whose bytes (and whose `report`) could diverge.
    //
    // `run_full` is a whole-repo, single-writer pipeline pass; two instances
    // racing inside the same test binary (cargo's default parallel test
    // runner) observably corrupt each other's transient state, so this
    // module-local lock serializes them regardless of `--test-threads`.
    static PIPELINE_LOCK: Mutex<()> = Mutex::new(());

    // The three drift tests below run the whole pipeline (~800s each) and so
    // exceed the on-gate 25s per-test budget; they are `#[ignore]`d off the
    // default/ci profile and run in the off-gate `make maint-dev-cli-heavy`
    // lane (`GMEOW_DEV_CLI_HEAVY=1 cargo nextest run -p gmeow-dev-cli
    // --run-ignored ignored-only`), matching the convention in
    // `crates/gmeow-dev-cli/tests/cli_parity.rs`. The fast `mode_path` mapping
    // test stays on-gate as the wiring proof.
    #[test]
    #[ignore = "off-gate: runs the whole pipeline; exceeds the 25s budget"]
    fn mode_owl_dl_matches_the_committed_pipeline_artifact() {
        let _guard = PIPELINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(compile(true, Some("owl-dl")), 0);
    }

    #[test]
    #[ignore = "off-gate: runs the whole pipeline; exceeds the 25s budget"]
    fn mode_datalog_matches_the_committed_pipeline_artifact() {
        let _guard = PIPELINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(compile(true, Some("datalog")), 0);
    }

    /// The discriminating test: the OLD `compile_one_mode` returned the
    /// compiler's private, base-only `a.report`, which would DRIFT against
    /// the committed union report (base + audit) that the `stage-mappings`
    /// pipeline stage assembles. If `--mode report` were ever rewired back
    /// to an in-process compile, this test fails.
    #[test]
    #[ignore = "off-gate: runs the whole pipeline; exceeds the 25s budget"]
    fn mode_report_matches_the_committed_union_report() {
        let _guard = PIPELINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(compile(true, Some("report")), 0);
    }

    /// On-gate wiring proof (instant, no pipeline run): every `--mode` name
    /// maps to the committed pipeline artifact path — in particular `report`
    /// maps to the committed UNION report `PROJECTION_REPORT_PATH`
    /// (`stage-mappings`' output), never a compiler-private path. Together
    /// with the deletion of the in-process `compile_one_mode`, this pins that
    /// `--mode M` can only ever narrow the real pipeline output. The heavy
    /// tests above prove the committed bytes actually match end-to-end.
    #[test]
    fn every_mode_maps_to_its_committed_pipeline_path() {
        use super::{LOGIC_MODES, mode_path};
        use gmeow_pipeline::stages::compile_logic::{
            CANONICAL_RDF12_PATH, CGIF_PATH, CLIF_PATH, DATALOG_PATH, GUFO_PATH, N3_PATH,
            OWL_DL_PATH, OWL_EL_PATH, PROJECTION_REPORT_PATH, XCL_PATH,
        };

        assert_eq!(mode_path("owl-dl"), OWL_DL_PATH);
        assert_eq!(mode_path("owl-el"), OWL_EL_PATH);
        assert_eq!(mode_path("datalog"), DATALOG_PATH);
        assert_eq!(mode_path("n3"), N3_PATH);
        assert_eq!(mode_path("gufo"), GUFO_PATH);
        assert_eq!(mode_path("canonical-rdf12"), CANONICAL_RDF12_PATH);
        assert_eq!(mode_path("clif"), CLIF_PATH);
        assert_eq!(mode_path("cgif"), CGIF_PATH);
        assert_eq!(mode_path("xcl"), XCL_PATH);
        // The discriminator: `report` narrows to the committed UNION report.
        assert_eq!(mode_path("report"), PROJECTION_REPORT_PATH);
        // Every validated mode has a mapping (no mode falls through unhandled
        // to the wrong artifact).
        for m in LOGIC_MODES {
            assert!(!mode_path(m).is_empty(), "mode {m} has no committed path");
        }
    }
}
