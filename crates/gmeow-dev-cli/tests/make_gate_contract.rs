// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Structural contract for the aggregate Make gate.
//!
//! The standalone targets remain complete developer entry points. `make check`
//! composes their non-overlapping equivalents so one invocation does not repeat
//! clippy, issue-reference linting, native reasoning, mapping compilation, or the
//! first iteration of the deterministic engine soak.

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

fn logical_assignment<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
    let prefix = format!("{name} :=");
    let mut lines = source.lines();
    let first = lines
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing {name} assignment"));
    let mut words = Vec::new();
    let mut line = first[prefix.len()..].trim();
    loop {
        let continued = line.ends_with('\\');
        words.extend(line.trim_end_matches('\\').split_whitespace());
        if !continued {
            break;
        }
        line = lines
            .next()
            .expect("continued Make assignment has a line")
            .trim();
    }
    words
}

fn target_header<'a>(source: &'a str, target: &str) -> &'a str {
    let prefix = format!("{target}:");
    source
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing Make target {target}"))
}

fn target_recipe(source: &str, target: &str) -> String {
    let header = target_header(source, target);
    let start = source.find(header).expect("header is in source") + header.len();
    source[start..]
        .lines()
        .skip(1)
        .take_while(|line| line.starts_with('\t') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn aggregate_gate_has_one_owner_for_each_expensive_equivalence_class() {
    let source = makefile();
    let targets = logical_assignment(&source, "CHECK_TARGETS");
    let expected = vec![
        "check-lint",
        "rust-gate",
        "gts-frame-profile-gate",
        "validate",
        "check-generated",
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
    ];
    assert_eq!(targets, expected, "aggregate gate inventory changed");

    let unique: BTreeSet<_> = targets.iter().copied().collect();
    assert_eq!(unique.len(), targets.len(), "aggregate repeats a target");

    for redundant in [
        "lint",
        "reason-verify",
        "reason-crosscheck",
        "mappings",
        "bench-golden-gate",
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
    ] {
        let recipe = target_recipe(&source, target);
        assert!(!recipe.trim().is_empty(), "{target} must remain runnable");
    }

    assert!(target_recipe(&source, "reason-gate").contains("$(GMEOW_DEV) reason-gate"));
    assert!(target_recipe(&source, "bench-soak").contains("--soak 3"));
    assert!(
        target_header(&source, "coherence-gate-teeth").contains("reason-gate"),
        "standalone teeth proof must retain the clean-bundle prerequisite"
    );
}
