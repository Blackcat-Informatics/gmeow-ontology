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
///   - `rust-build`, `rust-gate`: the `rust` job runs
///     `cargo nextest run --profile ci --workspace` and
///     `cargo test --doc --workspace` directly.
///   - `check-lint`: the `lint` job runs `cargo clippy --all-targets -- -D
///     warnings` directly and `make lint` (the standalone, non-scoped
///     target) covers the rest of the pre-commit hygiene suite.
///
/// This list must stay CLOSED and TIGHT: `every_check_dag_target_is_exercised_by_ci`
/// asserts every entry here is both a real CHECK_DAG target and genuinely
/// absent from ci.yml's `make <target>` steps, so a stale exemption (e.g. one
/// left behind after ci.yml grows a real `make check-lint` step) fails loudly
/// instead of silently rotting.
const CI_JOB_COVERED: &[&str] = &["rust-build", "rust-gate", "check-lint"];

#[test]
fn every_check_dag_target_is_exercised_by_ci() {
    let xtask_source = xtask();
    let dag_targets = check_dag_targets(&xtask_source);
    assert!(
        dag_targets.len() >= 19,
        "CHECK_DAG target extraction looks broken: found {dag_targets:?}"
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
        "rust-build",
        "rust-gate",
        "validate",
        "constitution-check",
        "crate-check",
        "audit",
        "wikidata",
        "coverage",
        "acceptance",
        "reason-verify",
        "lint-alignment",
        "i18n-lint",
        "doc-lint",
        "coherence-gate-teeth",
        "slice-quality-gate",
        "bench-soak",
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
    ] {
        assert!(
            !targets.contains(&redundant),
            "aggregate reintroduced subsumed target {redundant}"
        );
    }
}

#[test]
fn standalone_targets_remain_complete_while_check_uses_scoped_composition() {
    let source = makefile();

    assert!(
        target_recipe(&source, "check").contains("CHECK_SYNC_MODE=update cargo xtask check"),
        "local check owns update-mode synchronization"
    );
    assert!(
        target_recipe(&source, "check-full").contains("CHECK_SYNC_MODE=update cargo xtask check"),
        "forced-full local check owns update-mode synchronization"
    );
    assert!(target_recipe(&source, "sync").contains("$(GMEOW_DEV) sync"));
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
            .filter(|line| line.starts_with("sync:"))
            .count(),
        1,
        "standalone update-mode sync has one Make authority"
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
    assert!(xtask().contains("const AFTER_REASON: &[&str] = &[\"reason-verify\"]"));
    assert!(xtask().contains("const AFTER_RUST_BUILD: &[&str] = &[\"rust-build\"]"));
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
            .matches("run: make sync GMEOW_DEV=./dist/bin/gmeow-dev")
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
        source.contains("  bundle-validate:\n    needs: [producer]")
            && source.contains("run: make validate-gts GMEOW_DEV=./dist/bin/gmeow-dev"),
        "the authoritative bundle must still receive mandatory semantic validation"
    );
    assert!(
        source.contains(
            "needs: [producer, bundle-validate, lint, rust, wasm, ontology-validate, ontology-generated, ontology-reason, ontology-misc]"
        ),
        "the aggregate quality gate must require authoritative bundle validation"
    );
}
