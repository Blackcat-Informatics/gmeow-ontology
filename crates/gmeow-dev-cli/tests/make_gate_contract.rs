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

fn manifest(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path)).expect("read Cargo manifest")
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
///   - `rust-build`, `produce-test-fixtures`, `nextest`: `rust-prebuild` compiles
///     producer-independent units, the explicit producer-independent and producer-bound
///     CI steps publish the exact fixture receipts before archive construction,
///     `rust-archive` completes and authenticates that one build lineage, and `rust`
///     runs its exact slice union. No test target invokes a fixture producer.
///   - `doctests`: the parallel `rust-static` job runs `cargo test --doc
///     --workspace` directly because nextest archives do not carry doctests.
///   - `clippy`: the parallel `rust-static` job runs the identical all-target
///     clippy command once on its existing Rust/generated runner.
///   - `check-lint`: the source-only `lint` job runs `make lint`, which is exactly
///     the same fast pre-commit hygiene inventory without a compiled Rust hook.
///   - `console`: it is the declared Make PREREQUISITE of `console-smoke`,
///     which ci.yml does run — so `make console-smoke` assembles the tree
///     before it drives it, and a receipt naming `console` names something CI
///     genuinely ran. A separate `run: make console` step would assemble the
///     same tree a second time for no additional coverage, since the browser
///     lane cannot run at all without it.
///
/// This list must stay CLOSED and TIGHT: `every_check_dag_target_is_exercised_by_ci`
/// asserts every entry here is both a real CHECK_DAG target and genuinely
/// absent from ci.yml's `make <target>` steps, so a stale exemption (e.g. one
/// left behind after ci.yml grows a real `make check-lint` step) fails loudly
/// instead of silently rotting. The `console` entry carries one further
/// assertion of its own below: the Makefile prerequisite the exemption rests on
/// must still be declared, so dropping that edge fails here rather than
/// silently leaving the console assembled by nothing.
const CI_JOB_COVERED: &[&str] = &[
    "rust-build",
    "produce-test-fixtures",
    "nextest",
    "doctests",
    "clippy",
    "check-lint",
    "console",
];

/// The tasks lifted off `make check` onto the CI-only `make heavy` lane. Moving a
/// task there is a SCHEDULING decision, so it must still run on every PR:
/// `the_heavy_lane_still_runs_on_every_pr` proves ci.yml expands this exact set as
/// parallel branches and that the Makefile aggregate has the same membership.
const HEAVY_TASKS: &[&str] = &[
    "wasm-parity",
    "console-smoke",
    "acceptance",
    "bench-soak",
    "medium-consumer-surface",
];

#[test]
fn evidence_binaries_have_one_dependency_light_owner() {
    let pipeline = manifest("crates/pipeline/Cargo.toml");
    let validate = manifest("crates/validate/Cargo.toml");
    let evidence = manifest("crates/perf-evidence/Cargo.toml");
    assert!(
        pipeline.contains("autobins = false") && validate.contains("autobins = false"),
        "the heavyweight owner directories must not auto-discover the evidence binaries"
    );
    for retained_pipeline_binary in [
        "bench-compare",
        "gmn-dialect-paths",
        "medium-sweep",
        "perf_gate_merge",
    ] {
        assert!(
            pipeline.contains(&format!("name = \"{retained_pipeline_binary}\"")),
            "pipeline manifest dropped required binary {retained_pipeline_binary}"
        );
    }
    for evidence_binary in ["perf_sample", "perf_accept", "junit_inventory"] {
        assert!(
            evidence.contains(&format!("name = \"{evidence_binary}\"")),
            "evidence leaf does not own {evidence_binary}"
        );
        assert!(
            !pipeline.contains(&format!("name = \"{evidence_binary}\""))
                && !validate.contains(&format!("name = \"{evidence_binary}\"")),
            "{evidence_binary} has a duplicate heavyweight Cargo target"
        );
    }
}

/// Heavy tasks that need only the producer artifact. The medium consumer proof is
/// scheduled separately because it also consumes the authenticated Rust archive.
const CI_HEAVY_MATRIX_TASKS: &[&str] =
    &["wasm-parity", "console-smoke", "acceptance", "bench-soak"];

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
        "rust-gate is the aggregate ALIAS; the DAG must schedule its three shared-inventory parts"
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

    // `console` is exempt because `console-smoke` DECLARES it as a prerequisite. That is
    // the whole of its coverage, so it is read out of the Makefile rather than trusted:
    // drop the edge and the browser lane would drive whatever tree happened to be lying
    // around, while a receipt still claimed `console` ran.
    let make_source = makefile();
    let smoke_header = target_header(&make_source, "console-smoke");
    let prerequisites = smoke_header
        .split_once(':')
        .map(|(_, rest)| rest.split('#').next().unwrap_or_default())
        .unwrap_or_default()
        .split_whitespace()
        .collect::<BTreeSet<_>>();
    assert!(
        prerequisites.contains("console"),
        "`console` is exempt from a direct ci.yml step only because `console-smoke` \
         declares it as a prerequisite, and that edge is gone: `{smoke_header}`. Restore it, \
         or give `console` a real `run: make console` step in ci.yml."
    );
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
        "test-fixtures",
        "clippy",
        "nextest",
        "doctests",
        "validate",
        "medium-gate",
        "constitution-check",
        "audit",
        "wikidata",
        "coverage",
        "reason-verify",
        "console-test",
        "console",
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
        // The aggregate alias must never be scheduled alongside its three parts.
        "rust-gate",
        // The CI-only breadth lane.
        "wasm-parity",
        "acceptance",
        "bench-soak",
        "medium-consumer-surface",
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

/// Corpus-independent Rust lanes are siblings under `rust-build`; nextest waits for the
/// explicit fixture producer node and owns carrier/coherence without invoking a producer.
#[test]
fn the_rust_gate_is_split_into_independent_dag_nodes() {
    let source = xtask();
    for task in ["clippy", "doctests"] {
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

    let nextest = source
        .split("name: \"nextest\"")
        .nth(1)
        .expect("CHECK_DAG declares nextest");
    let nextest_dependencies = nextest
        .split("dependencies: ")
        .nth(1)
        .and_then(|tail| tail.split(',').next())
        .expect("nextest declares dependencies");
    assert_eq!(
        nextest_dependencies.trim(),
        "AFTER_TEST_FIXTURES",
        "nextest must start only after the explicit corpus-producer DAG node"
    );

    let makefile = makefile();
    for target in ["nextest", "doctests", "clippy"] {
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
    for part in ["clippy", "nextest", "doctests"] {
        assert!(
            alias.contains(part),
            "the rust-gate alias must still compose {part}"
        );
    }
}

/// Fixture production is a distinct DAG node before nextest. Both sides use the
/// already-built maintenance binary, but test-facing targets select only read-only verify.
#[test]
fn fixture_production_and_test_consumption_are_structurally_separate() {
    let makefile = makefile();
    let pipeline_build = std::fs::read_to_string(repo_root().join("crates/pipeline/build.rs"))
        .expect("read pipeline build identity");
    let producer = target_recipe(&makefile, "produce-test-fixtures");
    let verifier = target_recipe(&makefile, "verify-test-fixtures");

    assert!(
        target_header(&makefile, "produce-test-fixtures").contains("rust-build"),
        "fixture executables must be owned by the shared Rust build"
    );
    assert!(
        makefile.contains("cargo nextest run --no-run --profile ci $(RUST_TEST_WORKSPACE_ARGS)"),
        "the shared build must precompile the exact CI-profile nextest inventory"
    );
    assert!(
        makefile.contains("TEST_FIXTURE_TOOL := $(CARGO_TARGET_DIR)/debug/gmeow-dev")
            && producer.contains("$(TEST_FIXTURE_TOOL) test-fixtures produce --scope all")
            && verifier.contains("$(TEST_FIXTURE_TOOL) test-fixtures verify --scope all")
            && verifier.contains("$(TEST_FIXTURE_ENV)"),
        "one already-built maintenance binary must expose separate producer and verifier modes"
    );
    assert!(
        makefile.contains(
            "TEST_FIXTURE_MANIFEST ?= $(abspath .cache/gmeow-sync/test-fixture-manifest-v2.json)"
        ) && makefile.contains("GMEOW_TEST_FIXTURE_MANIFEST_SHA256")
            && target_recipe(&makefile, "nextest").contains("$(TEST_FIXTURE_ENV)"),
        "every corpus consumer must receive the producer-written selector and its exact digest"
    );
    assert!(
        pipeline_build.contains("build_inputs::crate_input_paths")
            && pipeline_build.contains("is_library_implementation_input")
            && pipeline_build.contains("src/fixture.rs\" | \"src/tests.rs")
            && !std::fs::read_to_string(
                repo_root().join("build-support/path_dependency_inputs.rs")
            )
            .expect("read exact path-dependency input walker")
            .contains("\"tests\", \"examples\", \"benches\""),
        "external Rust test/example/bench sources must not invalidate production-stage actions"
    );
    // Filtering is nextest-side: retaining `--workspace` keeps Cargo's feature graph
    // identical to the shared archive/prebuild lineage.
    assert!(
        makefile.contains("NEXTEST_FILTER_ARG :=")
            && target_recipe(&makefile, "nextest").contains("$(NEXTEST_FILTER_ARG)")
            && target_recipe(&makefile, "nextest").contains("$(RUST_TEST_WORKSPACE_ARGS)"),
        "focused tests must filter the already-built workspace inventory instead of creating a package-scoped Cargo lineage"
    );
    assert!(
        !makefile.contains("FIXTURE_TOOL_BUILD_ARGS")
            && !makefile.contains("--example prime-")
            && target_recipe(&makefile, "rust-prebuild").contains("test -x $(TEST_FIXTURE_TOOL)"),
        "fixture coordination must reuse nextest's gmeow-dev binary without a second Cargo build lineage"
    );
    assert!(
        !producer
            .lines()
            .any(|line| line.trim().starts_with("cargo ")),
        "fixture production must not invoke Cargo after the Rust DAG fans out: {producer}"
    );
    assert!(
        producer.contains(
            "$(BUNDLE_IMPORT_CACHE_ENV) $(TEST_FIXTURE_TOOL) test-fixtures produce --scope all"
        ),
        "the explicit producer must publish the exact shipped-bundle import before fanout"
    );
    let independent = target_recipe(&makefile, "produce-producer-independent-test-fixtures");
    let producer_bound = target_recipe(&makefile, "produce-producer-bound-test-fixtures");
    assert!(
        independent
            .contains("$(TEST_FIXTURE_TOOL) test-fixtures produce --scope producer-independent")
            && !independent.contains("BUNDLE_IMPORT_CACHE_ENV"),
        "the parallel fixture producer must contain only actions whose complete inputs are producer-independent"
    );
    assert!(
        target_header(&makefile, "produce-producer-bound-test-fixtures").contains("rust-build"),
        "a direct producer-bound invocation must establish the current shared Rust build before using gmeow-dev"
    );
    assert!(
        producer_bound.contains(
            "$(BUNDLE_IMPORT_CACHE_ENV) $(TEST_FIXTURE_TOOL) test-fixtures produce --scope producer-bound"
        ),
        "the joined fixture profile must produce generated-dependent docs and bundle actions in one process"
    );
    for target in ["nextest", "nextest-archive", "maint-rust-heavy"] {
        let recipe = target_recipe(&makefile, target);
        assert!(
            !recipe.contains("test-fixtures produce")
                && !recipe.contains("produce-test-fixtures")
                && !recipe.contains("produce-producer-"),
            "test-facing target {target} may authenticate fixtures but may never invoke a corpus producer: {recipe}"
        );
        assert!(
            target_header(&makefile, target).contains("verify-test-fixtures"),
            "test-facing target {target} must fail closed through the read-only verifier"
        );
    }
}

#[test]
fn corpus_producer_purity_is_a_pre_test_and_pre_commit_gate() {
    let makefile = makefile();
    let hook = std::fs::read_to_string(repo_root().join(".pre-commit-config.yaml"))
        .expect("read pre-commit config");
    let lint = std::fs::read_to_string(repo_root().join("scripts/lint-test-corpus-producers.sh"))
        .expect("read test-corpus purity linter");

    assert!(
        target_recipe(&makefile, "test-corpus-purity")
            .contains("./scripts/lint-test-corpus-producers.sh"),
        "Make must expose the static test-corpus purity gate"
    );
    for target in ["rust-build", "rust-prebuild"] {
        assert!(
            target_header(&makefile, target).contains("test-corpus-purity"),
            "{target} must reject producer-reachable tests before compiling or running them"
        );
    }
    assert!(
        hook.contains("id: test-corpus-purity")
            && hook.contains("entry: ./scripts/lint-test-corpus-producers.sh")
            && hook.contains("files: ^(crates/.*\\.rs|crates/.*/README\\.md|Makefile|scripts/"),
        "pre-commit must reject corpus-producing changes without running on unrelated commits"
    );
    for seal in [
        "run_full",
        "run_import",
        "run_acceptance",
        "prime_stage_fixture",
        "snapshot_dataset",
        "serialize_carrier_snapshot",
        "compile_mappings",
        "load_authored[A-Za-z0-9_]*",
        "examples_graph",
        "build_[A-Za-z0-9_]*corpus",
        "produce-test-fixtures",
    ] {
        assert!(
            lint.contains(seal),
            "test-corpus purity linter lost the {seal:?} producer seal"
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
    let matrix = ci
        .lines()
        .find_map(|line| line.trim().strip_prefix("task: ["))
        .and_then(|tail| tail.strip_suffix(']'))
        .expect("ci.yml declares the explicit heavy task matrix")
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    assert_eq!(
        matrix, CI_HEAVY_MATRIX_TASKS,
        "the producer-only heavy matrix changed"
    );
    assert!(
        ci.contains("run: make ${{ matrix.task }}"),
        "each heavy matrix branch must invoke its selected task"
    );
    assert!(
        !ci_make_targets(&ci).contains("heavy"),
        "CI expands the independent heavy DAG; invoking the serial aggregate too would \
         execute every breadth proof twice"
    );
    for task in CI_HEAVY_MATRIX_TASKS {
        assert!(
            !ci_make_targets(&ci).contains(*task),
            "ci.yml still runs `make {task}` directly; the heavy lane now owns it, so the \
             duplicate step must go"
        );
    }
    let medium_job = ci
        .split_once("\n  medium-consumer-surface:\n")
        .expect("ci.yml carries the standalone medium consumer job")
        .1
        .split_once("\n  rust-static:\n")
        .expect("rust-static follows the standalone medium consumer job")
        .0;
    assert!(
        medium_job.contains("needs: [producer, rust-archive]")
            && medium_job.contains(
                "./scripts/producer-receipt.sh verify generated dist/producer-receipt.json"
            )
            && medium_job.contains(
                "make medium-consumer-surface NEXTEST_ARCHIVE_INPUT=dist/nextest/ci.tar.zst"
            ),
        "the archive-dependent heavy task must run from the same authenticated archive on every PR"
    );
    let medium_recipe = target_recipe(&makefile, "medium-consumer-surface");
    assert!(
        medium_recipe.contains("$(NEXTEST_ARCHIVE_REPLAY_ARGS)")
            && medium_recipe.contains("$(TEST_FIXTURE_ENV)")
            && medium_recipe.contains("$(BUNDLE_IMPORT_CACHE_ENV)")
            && !medium_recipe.contains("check-sync"),
        "the medium tests must consume exact producer identities without invoking a corpus producer"
    );
    assert!(
        ci.contains("needs: [producer, lint, rust-archive, rust, rust-static, heavy, medium-consumer-surface, ontology-validate, ontology-generated, ontology-reason, ontology-misc]"),
        "the aggregate quality gate must require both heavy scheduling branches"
    );
}

/// CI compiles the test inventory once, authenticates it, and runs a disjoint/exact
/// slice partition from that archive. Static Rust surfaces remain parallel siblings.
#[test]
fn ci_reuses_one_authenticated_nextest_archive_without_coverage_loss() {
    let ci = ci_workflow();
    let makefile = makefile();
    let receipt_script =
        std::fs::read_to_string(repo_root().join("scripts/nextest-archive-receipt.sh"))
            .expect("read nextest archive receipt verifier");
    let producer_receipt_script =
        std::fs::read_to_string(repo_root().join("scripts/producer-receipt.sh"))
            .expect("read producer receipt verifier");
    let ci_receipt_script = std::fs::read_to_string(repo_root().join("scripts/ci-run-receipt.sh"))
        .expect("read hosted critical-path receipt collector");
    let console_producer_spec = std::fs::read_to_string(
        repo_root().join("crates/docs/assets/console/smoke/specs/14-producer.spec.mjs"),
    )
    .expect("read browser producer contract");

    assert!(
        ci.contains("  rust-archive:"),
        "CI needs one archive authority job"
    );
    let prebuild_job = ci
        .split_once("\n  rust-prebuild:\n")
        .and_then(|(_, tail)| tail.split_once("\n  # The receipt binds the\n"))
        .map(|(job, _)| job)
        .expect("producer-independent Rust prebuild job is bounded by the archive comment");
    assert!(
        prebuild_job.contains("run: make rust-prebuild")
            && prebuild_job.contains("make produce-producer-independent-test-fixtures")
            && prebuild_job.contains("rust-prebuild-evidence-${{ github.sha }}")
            && !prebuild_job.contains("generated-tree-${{ github.sha }}"),
        "the Rust build and explicit producer-independent fixture stage must overlap generation and must not consume its output"
    );
    let archive_job = ci
        .split_once("\n  rust-archive:\n")
        .and_then(|(_, tail)| tail.split_once("\n  # Shards execute"))
        .map(|(job, _)| job)
        .expect("Rust archive job is bounded by the shard comment");
    assert!(
        archive_job.contains("needs: [producer, rust-prebuild]")
            && archive_job.contains("Restore same-run producer-independent Rust build products")
            && archive_job.contains("Verify every transferred test-profile pipeline fixture read-only")
            && archive_job.contains(
                "Produce generated-bound docs and bundle-import fixtures before archive construction"
            )
            && archive_job.contains("Build dependency-light archive evidence tools")
            && archive_job.contains("target/debug/perf_sample")
            && archive_job.contains("archive-build-sample.json")
            && archive_job.contains("rust-archive-evidence-${{ github.sha }}")
            && archive_job
                .contains("--work-telemetry dist/nextest/fixture-timings-producer-bound.json")
            && archive_job.contains("--output-root archive=dist/nextest")
            && archive_job.contains("make nextest-archive")
            && !archive_job.contains("NEXTEST_FIXTURE_PRIME_TARGET")
            && !target_recipe(&makefile, "nextest-archive").contains("test-fixtures produce"),
        "the archive authority must join the producer artifact with the parallel Rust prebuild"
    );
    assert_eq!(
        ci.matches("key: rust-prebuild-v1-").count(),
        3,
        "prebuild producer, archive consumer, and static Rust consumer must name the same exact cache lineage"
    );
    assert!(
        ci.contains("rust-prebuild-v1-${{ runner.os }}-${{ runner.arch }}-${{ steps.rust-prebuild-identity.outputs.rustc }}-")
            && !ci.contains("steps.rust-prebuild-identity.outputs.runner"),
        "same-OS/architecture jobs must share the prebuild lineage across rolling image patch revisions while retaining exact rustc identity"
    );
    let nextest_install = archive_job
        .find("Install cargo-nextest (pinned archive format and partition semantics)")
        .expect("archive job provisions its pinned nextest runner");
    let transfer_fallback = archive_job
        .find("Rebuild producer-independent products on transfer miss")
        .expect("archive job carries a complete cache-miss fallback");
    assert!(
        nextest_install < transfer_fallback,
        "the archive cache-miss fallback must provision nextest before invoking make rust-prebuild"
    );
    assert!(
        ci.matches("${{ github.run_id }}-${{ github.run_attempt }}")
            .count()
            >= 2
            && ci.contains("cache-targets: false"),
        "the handoff key must advance each run while the registry cache stays separate"
    );
    assert_eq!(
        ci.lines()
            .filter(|line| {
                let mut words = line.split_whitespace();
                words.next() == Some("make") && words.next() == Some("nextest-archive")
            })
            .count(),
        1,
        "CI must build exactly one workspace test archive"
    );
    assert!(
        ci.contains("needs: [producer, rust-archive]"),
        "every test slice must wait for the authenticated archive"
    );
    let medium_job = ci
        .split_once("\n  medium-consumer-surface:\n")
        .and_then(|(_, tail)| tail.split_once("\n  rust-static:\n"))
        .map(|(job, _)| job)
        .expect("standalone medium consumer job is bounded by rust-static");
    assert!(
        medium_job.contains("rust-test-archive-${{ github.sha }}")
            && medium_job.contains("run: make nextest-archive-verify")
            && medium_job.contains(
                "make medium-consumer-surface NEXTEST_ARCHIVE_INPUT=dist/nextest/ci.tar.zst"
            ),
        "the medium consumer proof must verify and replay the same-run authenticated archive"
    );
    let rust_job = ci
        .split_once("\n  rust:\n")
        .expect("ci.yml carries the rust shard job")
        .1
        .split_once("\n  medium-consumer-surface:\n")
        .expect("the medium consumer follows the broad rust shard job")
        .0;
    assert!(
        rust_job.contains("fetch-depth: 0"),
        "archive shards must fetch origin/main so merge-base invariance tests grade a real comparand"
    );
    let rust_static_job = ci
        .split_once("\n  rust-static:\n")
        .expect("ci.yml carries the static Rust job")
        .1
        .split_once("\n  # === ONTOLOGY GATE LANES ===")
        .expect("ontology gate lanes follow the static Rust job")
        .0;
    assert!(
        rust_static_job.contains("cargo clippy --all-targets -- -D warnings")
            && !rust_static_job.contains("cargo-nextest")
            && !rust_static_job.contains("make carrier-purity"),
        "rust-static must own clippy without compiling or replaying tests from the authenticated archive"
    );
    let lint_job = ci
        .split_once("\n  lint:\n")
        .expect("ci.yml carries the source-only lint job")
        .1
        .split_once("\n  # === ONE RUST TEST BUILD LINEAGE ===")
        .expect("Rust build lineage follows lint")
        .0;
    assert!(
        !lint_job.contains("cargo clippy")
            && !lint_job.contains("Swatinem/rust-cache")
            && !lint_job.contains("generated-tree-${{ github.sha }}"),
        "source-only lint must not own a Cargo cache, generated transfer, or workspace compile"
    );
    assert!(
        ci.contains("--archive-file dist/nextest/ci.tar.zst")
            && ci.contains("--workspace-remap \"$PWD\"")
            && ci.contains("--profile ci")
            && ci.contains("--partition slice:${{ matrix.shard }}/3"),
        "all shards must reuse the same archive, profile, config, and stable slice scheme"
    );
    assert!(
        !ci.contains("--partition count:"),
        "deprecated timing-sensitive count partitioning must not return"
    );
    assert!(
        ci.matches("cargo-nextest@0.9.137").count() >= 4,
        "archive, shard, medium, and static nextest consumers must pin one reviewed release"
    );
    assert!(
        target_recipe(&makefile, "nextest-archive")
            .contains("cargo nextest archive --profile ci --workspace")
            && target_recipe(&makefile, "nextest-archive")
                .contains("nextest-archive-receipt.sh write"),
        "the archive target must build and receipt the canonical CI inventory"
    );
    assert!(
        target_recipe(&makefile, "nextest-archive-verify")
            .contains("nextest-archive-receipt.sh verify"),
        "every consumer needs the same receipt verifier"
    );
    for field in [
        "source_sha",
        "source_tree_sha256",
        "rustc_identity_sha256",
        "nextest_identity_sha256",
        "generated_tree_sha256",
        "build_config_sha256",
        "inventory_sha256",
        "junit_inventory_sha256",
        "perf_sample_sha256",
        "perf_accept_sha256",
        "test_fixture_manifest_sha256",
    ] {
        assert!(
            receipt_script.contains(field),
            "archive receipt must bind {field}"
        );
    }
    assert!(
        receipt_script.contains("uniq -d \"$union\"")
            && receipt_script.contains("cmp -s \"$canonical\" \"$union\""),
        "receipt verification must prove partition disjointness and exact union"
    );
    for script in [&receipt_script, &producer_receipt_script] {
        assert!(
            script.contains("git ls-files --cached --others --exclude-standard -z")
                && script.contains("stat -c '%a' -- \"$path\"")
                && script.contains("done < \"$paths\""),
            "receipt source hashing must materialize a fail-closed tracked/untracked path, type, mode, and content inventory"
        );
        assert!(
            !script.contains("git ls-files --stage"),
            "receipt source identity must bind working-tree bytes and modes, not irrelevant staging state"
        );
        assert!(
            !script.contains("done < <(git ls-files"),
            "receipt source hashing must not hide git/hash failures behind process substitution"
        );
    }
    assert!(
        ci.matches("key: test-actions-v3-").count() == 2
            && ci.matches("key: bundle-import-v1-").count() == 1
            && ci
                .matches("name: bundle-import-cache-${{ github.sha }}")
                .count()
                == 3
            && ci
                .matches(".cache/gmeow-sync/test-fixture-manifest-v2.json")
                .count()
                >= 3
            && ci.contains("fixture-timings-producer-bound.json"),
        "the bounded shared action store and exact bundle import need distinct cache, artifact, and evidence authorities in archive, shard, and medium consumers"
    );
    assert!(
        ci.contains("dist/nextest/perf_sample")
            && ci.contains("dist/nextest/perf_accept")
            && ci.contains("dist/nextest/junit_inventory")
            && ci.contains("--identity-receipt producer=dist/producer-receipt.json")
            && ci.contains("junit-shard-${{ matrix.shard }}.json")
            && ci.contains("shard-${{ matrix.shard }}-sample.json"),
        "each archive shard must bind the producer identity and emit authenticated inventory/duration and resource evidence"
    );
    assert_eq!(
        ci.matches(
            "chmod +x dist/nextest/junit_inventory dist/nextest/perf_sample dist/nextest/perf_accept"
        )
        .count(),
        2,
        "artifact downloads strip executable modes, so both archive replay jobs must restore every authenticated evidence tool"
    );
    for bundle_cache_contract in [
        "name: bundle-import-cache-${{ github.sha }}",
        "--bundle-import-cache-state warm",
        "--cache-root bundle-import=.cache/gmeow-bundle-import",
    ] {
        assert!(
            medium_job.contains(bundle_cache_contract),
            "the reserved medium consumer must transfer and census the exact bundle import: \
             missing {bundle_cache_contract:?}"
        );
    }
    assert!(
        rust_job.contains("--bundle-import-cache-state warm")
            && rust_job.contains("--cache-root actions=.cache/gmeow-sync/actions")
            && rust_job.contains("--cache-root bundle-import=.cache/gmeow-bundle-import")
            && rust_job.contains("GMEOW_BUNDLE_IMPORT_CACHE:")
            && rust_job.contains("GMEOW_BUNDLE_IMPORT_SOURCE_SHA256:")
            && rust_job.contains("GMEOW_TEST_FIXTURE_MANIFEST_SHA256=$(jq -er"),
        "archive shards must select the authenticated whole-bundle product read-only instead of rebuilding it per process"
    );
    assert!(
        target_recipe(&makefile, "medium-consumer-surface")
            .contains("$(BUNDLE_IMPORT_CACHE_ENV) cargo nextest run"),
        "the host-reserved producer-bound consumer lane must still select the exact cache"
    );
    assert_eq!(
        ci.matches("      GMEOW_DEV: ./dist/bin/gmeow-dev").count(),
        5,
        "all four ontology lanes and every heavy branch must use the authenticated producer binary"
    );
    assert!(
        console_producer_spec
            .contains("const PRODUCER = [\"--no-print-directory\", \"console-assemble\"]")
            && console_producer_spec.contains("execFileSync(\"make\"")
            && console_producer_spec.contains("`CONSOLE_OUT=${out}`")
            && !console_producer_spec.contains("execFileSync(\"cargo\""),
        "the browser reproducibility proof must reuse the authenticated producer through the canonical Make target instead of creating a second Cargo build authority"
    );
    assert!(
        target_recipe(&makefile, "nextest-evidence-tools")
            .contains("-p gmeow-perf-evidence --bins")
            && !target_recipe(&makefile, "nextest-evidence-tools")
                .contains("-p gmeow-pipeline --bin perf_sample")
            && !target_recipe(&makefile, "nextest-evidence-tools")
                .contains("-p gmeow-validate --bin junit_inventory"),
        "archive evidence tools must build through the dependency-light leaf package"
    );
    assert!(
        target_recipe(&makefile, "perf-accept")
            .contains("-p gmeow-perf-evidence --bin perf_accept")
            && !target_recipe(&makefile, "perf-accept").contains("-p gmeow-pipeline"),
        "paired outcome grading must use the dependency-light evidence package"
    );
    for contract in [
        "schema_version 3",
        "proof_inventory",
        "source_producer_fallback_job_groups",
        "actual_pipeline_stage_executions",
        "fixture_builder_executions",
        "gts_import_builds",
        "closure_constructions",
        "indexed_rdf_rows",
        "cargo_test_build_authorities",
        "rustc_identity_sha256",
        "critical_path_execution_ms",
        "--cache-state",
        "--partial-change",
    ] {
        assert!(
            ci_receipt_script.contains(contract),
            "hosted critical-path receipts must carry the gradable acceptance contract: missing {contract:?}"
        );
    }

    let rust_job = ci
        .split_once("  rust:\n")
        .and_then(|(_, tail)| tail.split_once("\n  medium-consumer-surface:"))
        .map(|(job, _)| job)
        .expect("rust shard job is bounded by the medium consumer");
    for duplicate in ["cargo fmt", "cargo clippy", "cargo test --doc", "cargo doc"] {
        assert!(
            !rust_job.contains(duplicate),
            "test shards must not serialize the static lane `{duplicate}`"
        );
    }
    assert!(
        ci.contains("  rust-static:") && ci.contains("run: cargo test --doc --workspace"),
        "doctests and rustdoc must remain required parallel static surfaces"
    );
    let nextest = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read nextest config");
    assert!(
        !nextest
            .split("default-filter = '''")
            .nth(1)
            .and_then(|tail| tail.split("'''").next())
            .expect("default filter")
            .contains("binary(carrier_purity)")
            && !ci.contains("run: make carrier-purity"),
        "carrier purity must execute once in the authenticated nextest archive"
    );
    assert!(
        !nextest
            .split("default-filter = '''")
            .nth(1)
            .and_then(|tail| tail.split("'''").next())
            .expect("default filter")
            .contains("whole_bundle_.*gate")
            && !ci.contains("run: make coherence-gate-teeth"),
        "coherence teeth must execute once in the authenticated nextest archive"
    );
}

#[test]
fn standalone_targets_remain_complete_while_check_uses_scoped_composition() {
    let source = makefile();

    assert!(
        target_recipe(&source, "check").contains("SYNC_MODE=update cargo xtask check"),
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
    // ONE PRODUCER. `check-sync` is the only Make target that runs the regeneration
    // pipeline: `make check`'s DAG, CI's cold materialization, and every non-gate lane
    // (install/commit/release/release-publish/Pages) all drive it. A second invocable
    // pipeline target does not merely duplicate the work — `gmeow-dev sync` takes the
    // HOST-GLOBAL gate lock, so the second run blocks on the first.
    assert!(
        target_recipe(&source, "check-sync")
            .contains("$(GMEOW_DEV) sync --mode $(SYNC_MODE) --outputs $(SYNC_OUTPUTS)"),
        "the single producer must select its mode and output scope explicitly"
    );
    assert_eq!(
        source
            .lines()
            .filter(|line| line.contains("$(GMEOW_DEV) sync --mode"))
            .count(),
        1,
        "exactly one Make recipe line may run the pipeline; a second entry point queues \
         the whole host behind itself on the gate lock"
    );
    assert!(
        !source.lines().any(|line| line.starts_with("sync:")),
        "the standalone `make sync` target must stay removed (it duplicated the producer)"
    );
    assert!(
        source.contains("SYNC_MODE ?= check"),
        "direct and CI producer invocations must remain read-only by default"
    );
    assert!(
        !source.contains("CHECK_SYNC_MODE"),
        "the producer's mode has ONE variable name (SYNC_MODE); the CHECK_SYNC_MODE alias \
         was a second name for the same selector"
    );

    // `make regen` is POISONED, not merely removed: it stays present so the habit fails
    // LOUDLY and names its replacement, and it must never run the pipeline again.
    let regen = target_recipe(&source, "regen");
    assert!(
        !regen.contains("$(GMEOW_DEV) sync"),
        "make regen must not run the pipeline: it was the second producer entry point"
    );
    assert!(
        regen
            .lines()
            .any(|line| line.trim_start_matches('\t').trim() == "@exit 1"),
        "make regen must hard-fail unconditionally, not warn — a banner left the double \
         run reachable"
    );
    for alternative in [
        "make check",
        "make check-sync SYNC_MODE=update",
        "make install",
    ] {
        assert!(
            regen.contains(alternative),
            "the refusal must name the correct alternative {alternative:?}"
        );
    }
    assert!(
        !regen
            .lines()
            .any(|line| line.trim_start_matches('\t').starts_with("@if")
                || line.trim_start_matches('\t').starts_with("if ")),
        "the refusal must be unconditional: any escape hatch restores the double run"
    );
    assert!(
        !source.lines().any(|line| line.contains("$(MAKE) regen")),
        "no Make target may still invoke the poisoned regen lane"
    );

    assert_eq!(
        target_header(&source, "lint"),
        "lint: ## Run fast pre-commit hygiene (Rust fmt, spelling, YAML/actions, secrets, and source-policy seals)."
    );
    let lint = target_recipe(&source, "lint");
    assert!(lint.contains("pre-commit run --all-files --show-diff-on-failure"));
    assert!(
        !lint.contains("SKIP="),
        "standalone lint must remain complete"
    );

    let check_lint = target_recipe(&source, "check-lint");
    assert!(
        check_lint.contains("pre-commit run --all-files --show-diff-on-failure")
            && !check_lint.contains("SKIP=")
    );

    for target in [
        "lint-issue-refs",
        "reason-verify",
        "mappings",
        "bench-golden-gate",
        "bench-soak",
        "gts-frame-profile-gate",
        "medium-gate",
    ] {
        let recipe = target_recipe(&source, target);
        assert!(!recipe.trim().is_empty(), "{target} must remain runnable");
    }

    assert!(
        target_recipe(&source, "reason-verify")
            .contains("$(GMEOW_DEV) reason-verify $(REASON_VERIFY_TIMINGS_ARG)")
    );
    assert!(target_recipe(&source, "bench-soak").contains("--soak 3"));
    // The target's ARGUMENT is unchanged — the gate still audits the one shipped
    // bundle. What moved is the rule it states: the universal Rule 6 codec check now
    // runs beside the DECLARED-MEDIA audit, which holds each frame to the dictionary
    // its rep's registered gmeow:PayloadSchema names.
    assert_eq!(
        target_header(&source, "gts-frame-profile-gate"),
        "gts-frame-profile-gate: ## Enforce zstd-rsyncable level 12 on every materialized GTS payload frame, and the declared medium each frame is primed with."
    );
    assert!(
        target_recipe(&source, "gts-frame-profile-gate")
            .contains("$(GMEOW_DEV) gts-frame-profile generated/dist/gmeow.gts"),
        "the frame-profile gate must audit through the already-built producer binary"
    );
    // The MEDIUM gate is the frame-profile gate's sibling: same artifact, same producer
    // binary, a strictly stronger rule (every frame decoded, every envelope re-derived,
    // every dictionary priced). Its header is pinned for the same reason the sibling's
    // is — the header is what `make help` publishes as the gate's claim, so a reworded
    // one is a reworded promise.
    assert_eq!(
        target_header(&source, "medium-gate"),
        "medium-gate: ## Audit the whole medium axis of the materialized bundle: every frame decoded, every envelope re-derived, every dictionary paid for, and the declared reader contract matched."
    );
    assert!(
        target_recipe(&source, "medium-gate")
            .contains("$(GMEOW_DEV) medium-gate generated/dist/gmeow.gts"),
        "the medium gate must audit the materialized bundle through the already-built \
         producer binary"
    );
    // …and unlike the frame-profile gate it IS an aggregate-DAG task: the wire clauses it
    // owns are read by no other `make check` task, so leaving it to CI alone would mean a
    // local gate that cannot see a medium regression at all.
    assert!(
        xtask().contains("target: \"medium-gate\""),
        "medium-gate must be wired into the aggregate check DAG"
    );
    assert!(!target_header(&source, "coherence-gate-teeth").contains("reason-verify"));
    let xtask_source = xtask();
    assert!(xtask_source.contains("const AFTER_RUST_BUILD: &[&str] = &[\"rust-build\"]"));
    // The whole-bundle gate-teeth proofs run their OWN reasoning inside nextest; they
    // never consume `reason-verify` output or receive a separate aggregate task.
    assert!(
        !xtask_source.contains("AFTER_REASON")
            && !xtask_source.contains("target: \"coherence-gate-teeth\""),
        "coherence-gate-teeth must be part of nextest, not chained or compiled separately"
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
    let producer = source
        .split_once("\n  producer:\n")
        .and_then(|(_, tail)| tail.split_once("\n  lint:\n"))
        .map(|(job, _)| job)
        .expect("producer job is bounded by the lint job");

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
            .matches("make check-sync GMEOW_DEV=./dist/bin/gmeow-dev SYNC_MODE=update")
            .count(),
        1,
        "one matrix step must define both cold generations through the prebuilt producer, \
         driving the SINGLE producer target"
    );
    assert!(
        !source.contains("make regen"),
        "CI must never invoke the poisoned regen lane"
    );
    assert!(
        producer.contains("diff --recursive --brief --no-dereference")
            && producer.contains("\"${RUNNER_TEMP}/generated-a\" \"${RUNNER_TEMP}/generated-b\""),
        "the authority job must compare the complete independent trees byte-for-byte"
    );
    for staging_dir in ["generated-a", "generated-b", "evidence-a", "evidence-b"] {
        let temp_path = format!("path: ${{{{ runner.temp }}}}/{staging_dir}");
        let checkout_path = format!("\n          path: {staging_dir}\n");
        assert!(
            producer.contains(&temp_path),
            "producer download {staging_dir} must be staged outside the checkout"
        );
        assert!(
            !producer.contains(&checkout_path),
            "producer download {staging_dir} must not enter the authenticated source census"
        );
    }
    assert!(
        producer.contains("\"${RUNNER_TEMP}/generated-a/dist/gmeow.gts\"")
            && producer.contains("\"${RUNNER_TEMP}/evidence-a/manifest.json\"")
            && producer.contains("\"${RUNNER_TEMP}/evidence-b/manifest.json\"")
            && producer.contains("path: ${{ runner.temp }}/generated-a"),
        "the producer must compare, receipt, digest, and republish only the isolated authority tree"
    );
    assert!(
        source.contains("SYNC_TIMINGS_JSON=dist/sync/update-timings.json")
            && source.contains("SYNC_TIMINGS_JSON=dist/sync/check-timings.json"),
        "both cold execution and fixed-point reuse must emit versioned work telemetry"
    );
    assert!(
        source.contains("generation-evidence-${{ github.sha }}-${{ matrix.generation }}")
            && source.contains("./scripts/producer-receipt.sh write")
            && source.contains("producer-receipt-${{ github.sha }}")
            && source.contains("./scripts/producer-receipt.sh verify"),
        "the authority artifact must carry and downstream-verify the full producer receipt"
    );
    let ontology_reason = source
        .split_once("  ontology-reason:\n")
        .and_then(|(_, tail)| tail.split_once("\n  ontology-misc:"))
        .map(|(job, _)| job)
        .expect("ontology-reason is bounded by ontology-misc");
    assert!(
        ontology_reason.contains("REASON_VERIFY_TIMINGS_JSON=dist/reason/reason-verify.json")
            && ontology_reason.contains("reason-evidence-${{ github.sha }}"),
        "the native reasoning lane must publish its deterministic work separately from job time"
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
    let ontology_validate = source
        .split_once("  ontology-validate:\n")
        .and_then(|(_, tail)| tail.split_once("\n  ontology-generated:"))
        .map(|(job, _)| job)
        .expect("ontology-validate is bounded by ontology-generated");
    assert_eq!(
        ontology_validate.matches("uses: actions/checkout@").count(),
        1,
        "ontology-validate must not repeat checkout"
    );
    assert!(
        source.contains(
            "needs: [producer, lint, rust-archive, rust, rust-static, heavy, medium-consumer-surface, ontology-validate, ontology-generated, ontology-reason, ontology-misc]"
        ),
        "the aggregate quality gate must require the retained quality jobs"
    );
}

/// The `validate` target's help string must stay in lock-step with the
/// declarative coverage registry `gmeow_validate::dsl_coverage::VALIDATE_PHASE_COVERAGE`:
/// it must NAME every DSL surface the live gate actually runs
/// (`home == OnValidate`), and it must ATTRIBUTE every deliberately-excluded
/// phase (`home == OnRustTest`) to where it does run. This is the machine-checked
/// half of the fix — the original defect was help that advertised "DSL SHACL"
/// while the live entrypoint ran none of it, indistinguishable from the outside
/// from a gate that ran and found nothing. A future edit that drops a DSL
/// surface from the wiring (or adds a phase without updating the help) fails here.
#[test]
fn validate_help_matches_the_phase_coverage_registry() {
    use gmeow_validate::dsl_coverage::{PhaseHome, VALIDATE_PHASE_COVERAGE};

    let makefile = makefile();
    let header = target_header(&makefile, "validate");
    let help = header
        .split_once("## ")
        .map(|(_, help)| help)
        .expect("the validate target carries a `## ` help string");

    assert!(
        help.contains("DSL SHACL"),
        "validate help must state that DSL SHACL runs: {help:?}"
    );

    // Every DSL surface the gate runs live must be named in the help. The token
    // is derived from the phase label (`<kind>-dsl-shacl` -> `<kind>`), so a new
    // OnValidate DSL surface forces its kind into the help or this fails.
    let onvalidate_dsl_tokens = VALIDATE_PHASE_COVERAGE
        .iter()
        .filter(|phase| phase.home == PhaseHome::OnValidate)
        .filter_map(|phase| phase.phase.strip_suffix("-dsl-shacl"))
        .collect::<Vec<_>>();
    assert!(
        !onvalidate_dsl_tokens.is_empty(),
        "the registry must declare at least one OnValidate DSL surface"
    );
    for token in &onvalidate_dsl_tokens {
        assert!(
            help.contains(token),
            "validate help omits the OnValidate DSL surface {token:?}: it advertises DSL SHACL \
             but does not name every surface the gate runs — the help has drifted from \
             VALIDATE_PHASE_COVERAGE: {help:?}"
        );
    }

    // Corpus-building validation phases may not be delegated to Rust tests.
    for phase in VALIDATE_PHASE_COVERAGE {
        if let PhaseHome::OnRustTest(owner) = phase.home {
            panic!(
                "validation phase {:?} delegates corpus work to Rust test owner {owner:?}",
                phase.phase
            );
        }
    }
    assert!(
        !help.contains("Rust test")
            && !help.contains("per-example")
            && !help.contains("slice-test"),
        "validate help must not claim corpus validation is delegated to tests: {help:?}"
    );
}
