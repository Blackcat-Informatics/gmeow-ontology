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
        "reason-gate",
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
        "reason-verify",
        "reason-crosscheck",
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
        "reason-crosscheck",
        "reason-gate",
        "mappings",
        "bench-golden-gate",
        "bench-soak",
        "gts-frame-profile-gate",
    ] {
        let recipe = target_recipe(&source, target);
        assert!(!recipe.trim().is_empty(), "{target} must remain runnable");
    }

    assert!(target_recipe(&source, "reason-gate").contains("$(GMEOW_DEV) reason-gate"));
    assert!(target_recipe(&source, "bench-soak").contains("--soak 3"));
    assert!(!target_header(&source, "coherence-gate-teeth").contains("reason-gate"));
    assert!(xtask().contains("const AFTER_REASON: &[&str] = &[\"reason-gate\"]"));
    assert!(xtask().contains("const AFTER_RUST_BUILD: &[&str] = &[\"rust-build\"]"));
}
