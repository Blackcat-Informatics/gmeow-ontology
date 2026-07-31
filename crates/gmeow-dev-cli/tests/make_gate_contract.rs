// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Structural contract for the aggregate task DAG and its thin Make entrypoints.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

fn makefile() -> String {
    std::fs::read_to_string(repo_root().join("Makefile")).expect("read repo Makefile")
}

fn xtask() -> String {
    std::fs::read_to_string(repo_root().join("crates/xtask/src/main.rs")).expect("read xtask DAG")
}

fn ci_workflow() -> String {
    std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml")
}

/// Extract the `.target` string literals of every `Task` in the `CHECK_DAG`
/// definition, in declaration order, by slicing the xtask source between
/// `const CHECK_DAG` and its closing `];`. This is intentionally an
/// extraction (not a hardcoded list) so the aggregate DAG stays the single
/// source of truth: adding, renaming, or removing a `Task` changes what this
/// function returns without editing this test.
fn check_dag_targets(xtask_source: &str) -> Vec<String> {
    let start = xtask_source
        .find("const CHECK_DAG")
        .expect("xtask defines CHECK_DAG");
    let block = &xtask_source[start..];
    let end = block
        .find("\n];")
        .expect("CHECK_DAG array literal is closed with `];`");
    let block = &block[..end];
    block
        .split("target: \"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
        .map(str::to_string)
        .collect()
}

/// Extract the set of Make targets CI invokes as a literal `make <target>`
/// step (`run: make <target>`, ignoring any trailing arguments and ignoring
/// non-target invocations like `make -C ...`).
fn ci_make_targets(ci_source: &str) -> BTreeSet<String> {
    ci_source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("run: make "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|target| !target.starts_with('-'))
        .map(str::to_string)
        .collect()
}

/// CHECK_DAG targets that are legitimately NOT invoked as a `make <target>`
/// step in ci.yml because they are exercised by a dedicated CI job through a
/// different surface instead of the ontology-lane `make` steps:
///   - `rust-build`, `nextest`, `doctests`, `clippy`, `carrier-purity`: the
///     `rust` job runs `cargo nextest run --profile ci --workspace`,
///     `cargo test --doc --workspace`, and `cargo clippy --all-targets -- -D
///     warnings` directly (sharded 3×; the whole-workspace gates on shard 1).
///   - `check-lint`: the `lint` job runs `cargo clippy --all-targets -- -D
///     warnings` directly and `make lint` (the standalone, non-scoped
///     target) covers the rest of the pre-commit hygiene suite.
///
/// This list must stay CLOSED and TIGHT: `every_check_dag_target_is_exercised_by_ci`
/// asserts every entry here is both a real CHECK_DAG target and genuinely
/// absent from ci.yml's `make <target>` steps, so a stale exemption (e.g. one
/// left behind after ci.yml grows a real `make check-lint` step) fails loudly
/// instead of silently rotting.
const CI_JOB_COVERED: &[&str] = &[
    "rust-build",
    "nextest",
    "doctests",
    "clippy",
    "carrier-purity",
    "check-lint",
];

/// The tasks lifted off `make check` onto the CI-only `make heavy` lane. Moving a
/// task there is a SCHEDULING decision, so it must still run on every PR:
/// `the_heavy_lane_still_runs_on_every_pr` proves ci.yml invokes `make heavy` and
/// that the Makefile's `HEAVY_TASKS` is exactly this set.
const HEAVY_TASKS: &[&str] = &["wasm-parity", "acceptance", "bench-soak"];

#[test]
fn every_check_dag_target_is_exercised_by_ci() {
    let xtask_source = xtask();
    let dag_targets = check_dag_targets(&xtask_source);
    assert!(
        dag_targets.len() >= 19,
        "CHECK_DAG target extraction looks broken: found {dag_targets:?}"
    );
    assert!(
        !dag_targets.iter().any(|target| target == "rust-gate"),
        "rust-gate is the aggregate ALIAS; the DAG must schedule its four parts"
    );

    let ci_source = ci_workflow();
    let ci_targets = ci_make_targets(&ci_source);
    assert!(
        !ci_targets.is_empty(),
        "ci.yml `make <target>` extraction looks broken: found none"
    );

    let uncovered: Vec<&str> = dag_targets
        .iter()
        .map(String::as_str)
        .filter(|target| !ci_targets.contains(*target) && !CI_JOB_COVERED.contains(target))
        .collect();
    assert!(
        uncovered.is_empty(),
        "CHECK_DAG target(s) {uncovered:?} are not invoked as `make <target>` by any ci.yml \
         step and are not in CI_JOB_COVERED. A receipt attesting these CHECK_DAG tasks ran \
         would not correspond to what CI actually runs. Either add a `run: make <target>` step \
         to ci.yml, or add and justify a new CI_JOB_COVERED entry naming the dedicated CI job \
         that exercises it through a different surface."
    );

    let dag_set: BTreeSet<&str> = dag_targets.iter().map(String::as_str).collect();
    for exempt in CI_JOB_COVERED {
        assert!(
            dag_set.contains(exempt),
            "CI_JOB_COVERED lists {exempt:?}, which is not (or no longer) a CHECK_DAG target; \
             remove the stale exemption"
        );
        assert!(
            !ci_targets.contains(*exempt),
            "CI_JOB_COVERED lists {exempt:?} as exempt, but ci.yml now runs `make {exempt}` \
             directly; remove the stale exemption now that real coverage exists"
        );
    }
}

fn target_header_index(source: &str, target: &str) -> usize {
    let prefix = format!("{target}:");
    source
        .lines()
        .position(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing Make target {target}"))
}

fn target_header<'a>(source: &'a str, target: &str) -> &'a str {
    source
        .lines()
        .nth(target_header_index(source, target))
        .expect("target header index is in bounds")
}

fn target_recipe(source: &str, target: &str) -> String {
    source
        .lines()
        .skip(target_header_index(source, target) + 1)
        .take_while(|line| line.starts_with('\t') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn aggregate_gate_has_one_owner_for_each_expensive_equivalence_class() {
    let source = xtask();
    let expected = vec![
        "sync",
        "check-lint",
        "crate-check",
        "i18n-lint",
        "rust-build",
        "carrier-purity",
        "clippy",
        "nextest",
        "doctests",
        "coherence-gate-teeth",
        "validate",
        "constitution-check",
        "audit",
        "wikidata",
        "coverage",
        "reason-verify",
        "lint-alignment",
        "doc-lint",
        "slice-quality-gate",
        "compliance-report",
    ];
    let targets = source
        .lines()
        .filter_map(|line| line.split("name: \"").nth(1))
        .filter_map(|tail| tail.split('"').next())
        .collect::<Vec<_>>();
    assert_eq!(targets, expected, "aggregate DAG inventory changed");

    let unique: BTreeSet<_> = targets.iter().copied().collect();
    assert_eq!(unique.len(), targets.len(), "aggregate repeats a target");

    for redundant in [
        "lint",
        "mappings",
        "bench-golden-gate",
        "gts-frame-profile-gate",
        // The aggregate alias must never be scheduled alongside its four parts.
        "rust-gate",
        // The CI-only breadth lane.
        "wasm-parity",
        "acceptance",
        "bench-soak",
    ] {
        assert!(
            !targets.contains(&redundant),
            "aggregate reintroduced subsumed target {redundant}"
        );
    }
}

/// `sync` is the gate's longest single stage. A task may depend on it ONLY if it
/// reads a `generated/` artifact; the tasks below were each verified to read
/// authored sources only, so they must sit at the DAG root and run CONCURRENTLY
/// with synchronization rather than behind it.
#[test]
fn sync_is_not_a_blanket_prerequisite_of_the_gate() {
    let source = xtask();
    // The blanket edge is gone: a strict MINORITY of the DAG waits on sync, and the
    // root wave is non-empty beyond sync itself.
    let after_sync = source.matches("dependencies: AFTER_SYNC").count();
    let root = source.matches("dependencies: ROOT").count();
    assert!(
        root >= 4,
        "at least sync plus the three authored-source gates must start immediately \
         (found {root} ROOT tasks)"
    );
    assert!(
        after_sync < check_dag_targets(&source).len() - 1,
        "AFTER_SYNC is a blanket prerequisite again ({after_sync} tasks wait on sync)"
    );
    for task in ["check-lint", "crate-check", "i18n-lint"] {
        let declaration = source
            .split(&format!("name: \"{task}\""))
            .nth(1)
            .unwrap_or_else(|| panic!("CHECK_DAG declares {task}"));
        let dependencies = declaration
            .split("dependencies: ")
            .nth(1)
            .and_then(|tail| tail.split(',').next())
            .unwrap_or_else(|| panic!("{task} declares dependencies"));
        assert_eq!(
            dependencies.trim(),
            "ROOT",
            "{task} reads no generated/ artifact, so it must not wait for sync"
        );
    }
}

/// The four Rust lanes are siblings under `rust-build`, never chained to each other.
#[test]
fn the_rust_gate_is_split_into_independent_dag_nodes() {
    let source = xtask();
    for task in ["carrier-purity", "clippy", "nextest", "doctests"] {
        let declaration = source
            .split(&format!("name: \"{task}\""))
            .nth(1)
            .unwrap_or_else(|| panic!("CHECK_DAG declares {task}"));
        let dependencies = declaration
            .split("dependencies: ")
            .nth(1)
            .and_then(|tail| tail.split(',').next())
            .unwrap_or_else(|| panic!("{task} declares dependencies"));
        assert_eq!(
            dependencies.trim(),
            "AFTER_RUST_BUILD",
            "{task} must be a sibling under rust-build, not chained behind another lane"
        );
    }

    let makefile = makefile();
    for target in ["nextest", "doctests", "clippy", "carrier-purity"] {
        assert!(
            !target_recipe(&makefile, target).trim().is_empty(),
            "{target} must be a real, independently runnable Make target"
        );
        assert!(
            target_header(&makefile, target).contains("rust-build"),
            "{target} must declare rust-build as its prerequisite"
        );
    }
    assert!(
        target_recipe(&makefile, "nextest").contains("cargo nextest run --profile ci"),
        "the nextest lane owns the workspace suite"
    );
    assert!(
        target_recipe(&makefile, "doctests").contains("cargo test --doc"),
        "the doctests lane owns the doctests"
    );
    // The alias remains, so a human can still ask for the whole Rust surface.
    let alias = target_header(&makefile, "rust-gate");
    for part in ["carrier-purity", "clippy", "nextest", "doctests"] {
        assert!(
            alias.contains(part),
            "the rust-gate alias must still compose {part}"
        );
    }
}

/// Moving a task to `make heavy` is a SCHEDULING decision, never a coverage cut:
/// the lane must still run on every PR, and it must refuse to run outside CI.
#[test]
fn the_heavy_lane_still_runs_on_every_pr() {
    let makefile = makefile();
    let declared = makefile
        .lines()
        .find_map(|line| line.strip_prefix("HEAVY_TASKS := "))
        .expect("the Makefile declares HEAVY_TASKS")
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(
        declared, HEAVY_TASKS,
        "the heavy lane's membership changed without updating this contract"
    );
    for task in HEAVY_TASKS {
        assert!(
            !target_recipe(&makefile, task).trim().is_empty(),
            "{task} must remain individually runnable by name"
        );
    }

    let heavy = target_recipe(&makefile, "heavy");
    assert!(
        heavy.contains("\"$${CI:-}\" != \"true\""),
        "the heavy lane must hard-fail when CI is unset or false"
    );
    assert!(
        heavy.contains("GITHUB_ACTIONS"),
        "CI alone is a variable a developer may already export; the refusal must also \
         require a CI-vendor marker"
    );
    assert!(
        heavy.contains("$(MAKE) $$t"),
        "the heavy lane must actually run every HEAVY_TASKS entry"
    );

    let ci = ci_workflow();
    assert!(
        ci_make_targets(&ci).contains("heavy"),
        "ci.yml must invoke `make heavy` so nothing moved off `make check` stops running \
         on PRs"
    );
    for task in HEAVY_TASKS {
        assert!(
            !ci_make_targets(&ci).contains(*task),
            "ci.yml still runs `make {task}` directly; the heavy lane now owns it, so the \
             duplicate step must go"
        );
    }
    assert!(
        ci.contains("needs: [producer, lint, rust, heavy, ontology-validate, ontology-generated, ontology-reason, ontology-misc]"),
        "the aggregate quality gate must require the heavy job"
    );
}

#[test]
fn standalone_targets_remain_complete_while_check_uses_scoped_composition() {
    let source = makefile();

    assert!(
        target_recipe(&source, "check").contains("CHECK_SYNC_MODE=update cargo xtask check"),
        "local check owns update-mode synchronization"
    );
    // There is ONE local gate. The receipt-backed impact profile and its `check-full`
    // escape hatch are gone: `make check` physically runs every CHECK_DAG task.
    assert!(
        !target_recipe(&source, "check").contains("--profile"),
        "the local gate has a single profile; --profile must not return"
    );
    assert!(
        !source.lines().any(|line| line.starts_with("check-full:")),
        "check-full existed only to force past the impact profile, which is removed"
    );
    // `make sync` is removed: the standalone regenerate lane is `make regen`; `make check`
    // owns its own sync pass, so agents run ONLY `make check` for the gate.
    assert!(target_recipe(&source, "regen").contains("$(GMEOW_DEV) sync"));
    assert!(
        !source.lines().any(|line| line.starts_with("sync:")),
        "the standalone `make sync` target must be removed (it duplicated `make check`'s sync pass)"
    );
    assert!(
        target_recipe(&source, "check-sync")
            .contains("sync --mode $(CHECK_SYNC_MODE) --outputs generated"),
        "aggregate sync must select its explicit update/check operation"
    );
    assert!(
        source.contains("CHECK_SYNC_MODE ?= check"),
        "direct and CI check-sync invocations must remain read-only by default"
    );
    assert_eq!(
        source
            .lines()
            .filter(|line| line.starts_with("regen:"))
            .count(),
        1,
        "standalone regenerate lane (`make regen`) has one Make authority"
    );

    assert_eq!(
        target_header(&source, "lint"),
        "lint: ## Run issue-ref lint and the full pre-commit hygiene suite (Rust fmt/clippy, spelling, YAML, actions, secrets)."
    );
    let lint = target_recipe(&source, "lint");
    assert!(lint.contains("pre-commit run --all-files --show-diff-on-failure"));
    assert!(
        !lint.contains("SKIP="),
        "standalone lint must remain complete"
    );

    let check_lint = target_recipe(&source, "check-lint");
    assert!(
        check_lint.contains("SKIP=cargo-clippy pre-commit run --all-files --show-diff-on-failure")
    );

    for target in [
        "lint-issue-refs",
        "reason-verify",
        "mappings",
        "bench-golden-gate",
        "bench-soak",
        "gts-frame-profile-gate",
    ] {
        let recipe = target_recipe(&source, target);
        assert!(!recipe.trim().is_empty(), "{target} must remain runnable");
    }

    assert!(target_recipe(&source, "reason-verify").contains("$(GMEOW_DEV) reason-verify"));
    assert!(target_recipe(&source, "bench-soak").contains("--soak 3"));
    assert_eq!(
        target_header(&source, "gts-frame-profile-gate"),
        "gts-frame-profile-gate: ## Enforce zstd-rsyncable level 12 on every materialized GTS payload frame."
    );
    assert!(
        target_recipe(&source, "gts-frame-profile-gate")
            .contains("$(GMEOW_DEV) gts-frame-profile generated/dist/gmeow.gts"),
        "the frame-profile gate must audit through the already-built producer binary"
    );
    assert!(!target_header(&source, "coherence-gate-teeth").contains("reason-verify"));
    let xtask_source = xtask();
    assert!(xtask_source.contains("const AFTER_RUST_BUILD: &[&str] = &[\"rust-build\"]"));
    // The whole-bundle gate-teeth proofs run their OWN reasoning; they never consumed
    // `reason-verify`'s output, so that serial edge is removed rather than preserved.
    assert!(
        !xtask_source.contains("AFTER_REASON"),
        "coherence-gate-teeth must not be chained behind reason-verify"
    );
}

#[test]
fn ci_parallelizes_cold_generation_without_weakening_the_authority_gate() {
    let source = ci_workflow();
    let generation = source
        .split_once("  generation:")
        .and_then(|(_, tail)| tail.split_once("\n  producer:"))
        .map(|(job, _)| job)
        .expect("generation job is bounded by the producer job");

    assert!(
        source.contains("generation: [a, b]"),
        "CI must run two independent generations as a matrix"
    );
    assert!(
        source.contains("needs: [producer-build]") && source.contains("needs: [generation]"),
        "the source-built producer, parallel generations, and authority job must remain ordered"
    );
    assert_eq!(
        source
            .matches("run: make regen GMEOW_DEV=./dist/bin/gmeow-dev")
            .count(),
        1,
        "one matrix step must define both cold generations through the prebuilt producer"
    );
    assert!(
        source.contains("diff --recursive --brief --no-dereference generated-a generated-b"),
        "the authority job must compare the complete independent trees byte-for-byte"
    );
    assert!(
        !source.contains("two_cold_generations_are_deterministic"),
        "CI must not append two more serial cold generations after the matrix proof"
    );
    assert!(
        !source.contains("Cache strict-sync manifest"),
        "independent generation jobs must start cold rather than restore a proof manifest"
    );
    assert!(
        !generation.contains("validate-gts"),
        "semantic validation must not block publication of the byte-proven authority"
    );
    assert!(
        source.contains(
            "needs: [producer, lint, rust, heavy, ontology-validate, ontology-generated, ontology-reason, ontology-misc]"
        ),
        "the aggregate quality gate must require the retained quality jobs"
    );
}
