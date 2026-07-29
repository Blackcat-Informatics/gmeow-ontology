// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot contract for the per-test budgets in `.config/nextest.toml`.
//!
//! Every `[[profile.default.overrides]]` entry there exists to give a genuinely heavy
//! test the room it needs — an extended `slow-timeout`, or `threads-required` so it does
//! not compete with the rest of the gate. Each one names its target by `package(...)`
//! plus a `test(...)` / `binary(...)` pattern.
//!
//! That naming is a hand-maintained reference into the source tree, and it fails
//! SILENTLY: when a module moves to another crate the filter simply stops matching, the
//! test loses its budget, and the only symptom is a timeout somewhere else entirely,
//! attributed to host load. Splitting the MCP consumer surface out of `gmeow-pipeline`
//! did exactly that to the whole-bundle overlay tests.
//!
//! So this gate pins the reference: every test name an override claims to cover must
//! actually exist in the source of a package that override names.
//!
//! It deliberately reads the SOURCE rather than shelling out to `cargo nextest list` —
//! a gate that re-invokes the test runner it is part of would be both recursive and far
//! slower than the thing it protects. Reading the tree catches the failure mode that
//! actually happens (a test moving between crates) at no cost.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

/// Map every workspace package name to its crate directory, read from the manifests
/// themselves so a rename or a new crate needs no edit here.
fn package_dirs() -> BTreeMap<String, PathBuf> {
    let mut map = BTreeMap::new();
    let crates = repo_root().join("crates");
    for entry in std::fs::read_dir(&crates).expect("read crates/") {
        let dir = entry.expect("read crates/ entry").path();
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest).expect("read a crate manifest");
        // The FIRST `name = "..."` after `[package]` is the package name; a later one
        // could belong to a `[[bin]]` or a dependency table.
        let Some(pkg) = text.split_once("[package]").map(|(_, rest)| rest) else {
            continue;
        };
        if let Some(name) = pkg
            .lines()
            .filter_map(|l| l.trim().strip_prefix("name"))
            .filter_map(|l| l.trim_start().strip_prefix('='))
            .filter_map(|l| {
                let l = l.trim();
                l.strip_prefix('"').and_then(|l| l.split('"').next())
            })
            .next()
        {
            map.insert(name.to_string(), dir);
        }
    }
    assert!(
        map.len() > 20,
        "expected the workspace to have many crates; found {} — the manifest scan is broken",
        map.len()
    );
    map
}

/// Every `filter = '...'` value under `[[profile.default.overrides]]`.
fn override_filters(config: &str) -> Vec<String> {
    config
        .lines()
        .filter_map(|l| l.trim().strip_prefix("filter = "))
        .filter_map(|l| l.trim().strip_prefix('\''))
        .filter_map(|l| l.strip_suffix('\''))
        .map(str::to_string)
        .collect()
}

/// The argument text of every `kind(...)` call in a filter expression, handling the
/// nesting that appears inside `test(/.../)` alternations.
fn calls_of(filter: &str, kind: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("{kind}(");
    let mut rest = filter;
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let mut depth = 1usize;
        let mut end = after.len();
        for (i, c) in after.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(after[..end].to_string());
        rest = &after[end..];
    }
    out
}

/// Pull the plausible Rust identifiers out of a `test(...)` payload.
///
/// The payload may be a bare substring or a `/regex/`, and may contain alternations and
/// character classes. Splitting on every regex metacharacter and keeping only long
/// snake_case tokens is deliberately CONSERVATIVE: a fragment too short to be a whole
/// test name yields no assertion rather than a false alarm. A real test name — which is
/// what rots when a module moves — is always long enough to survive this filter.
fn candidate_test_names(payload: &str) -> Vec<String> {
    payload
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|tok| tok.len() >= 12 && tok.contains('_') && !tok.starts_with('_'))
        .map(str::to_string)
        .collect()
}

#[test]
fn nextest_override_filters_all_match_a_live_test() {
    let config = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let filters = override_filters(&config);
    assert!(
        filters.len() >= 15,
        "expected the config to carry many override filters; found {} — the parse is broken",
        filters.len()
    );

    let dirs = package_dirs();
    let mut failures = Vec::new();
    let mut checked = 0usize;

    for filter in &filters {
        let packages = calls_of(filter, "package");
        assert!(
            !packages.is_empty(),
            "every override filter should scope itself to a package, but this one does not: {filter}"
        );

        // The union of the sources of every package this filter names.
        let mut sources = String::new();
        for pkg in &packages {
            let dir = dirs.get(pkg.trim()).unwrap_or_else(|| {
                panic!("override filter names package `{pkg}`, which is not a workspace crate: {filter}")
            });
            for sub in ["src", "tests"] {
                collect_rust_sources(&dir.join(sub), &mut sources);
            }
        }

        for payload in calls_of(filter, "test") {
            for name in candidate_test_names(&payload) {
                checked += 1;
                if !sources.contains(&name) {
                    failures.push(format!(
                        "  - `{name}` is named by an override filter scoped to {packages:?}, \
                         but no source under those crates mentions it\n      filter: {filter}"
                    ));
                }
            }
        }
    }

    assert!(
        checked >= 20,
        "expected to check many test names; only checked {checked} — the extraction is too narrow \
         to protect anything"
    );
    assert!(
        failures.is_empty(),
        "{} nextest override filter(s) name a test that has moved or been renamed, so the budget \
         they grant silently applies to NOTHING (the symptom is a timeout elsewhere, blamed on \
         host load):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Append every `.rs` file's text under `dir` (recursively) to `sink`.
fn collect_rust_sources(dir: &Path, sink: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, sink);
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            sink.push_str(&text);
            sink.push('\n');
        }
    }
}

#[test]
fn the_contract_catches_a_filter_whose_test_moved_crates() {
    // Non-vacuity: the exact regression this gate exists for. A filter scoped to
    // `gmeow-pipeline` naming a test that now lives in `gmeow-mcp` must be reported.
    let dirs = package_dirs();
    let moved = "verify_graph_accepts_a_normal_small_overlay_over_the_whole_bundle";

    let mut pipeline = String::new();
    for sub in ["src", "tests"] {
        collect_rust_sources(&dirs["gmeow-pipeline"].join(sub), &mut pipeline);
    }
    let mut mcp = String::new();
    for sub in ["src", "tests"] {
        collect_rust_sources(&dirs["gmeow-mcp"].join(sub), &mut mcp);
    }

    assert!(
        mcp.contains(moved),
        "the overlay test should live in gmeow-mcp — if it moved again, this gate needs its \
         witness updated"
    );
    assert!(
        !pipeline.contains(moved),
        "the overlay test should NO LONGER be in gmeow-pipeline; if it is, the crate split \
         regressed and the gate above can no longer distinguish the two"
    );

    // And the extraction really does surface that name from a realistic filter payload.
    let filter = format!("package(gmeow-pipeline) & test(/mcp::tests::{moved}/)");
    let names: Vec<String> = calls_of(&filter, "test")
        .iter()
        .flat_map(|p| candidate_test_names(p))
        .collect();
    assert!(
        names.iter().any(|n| n == moved),
        "the identifier extraction must surface `{moved}` from {filter}; got {names:?}"
    );
}
