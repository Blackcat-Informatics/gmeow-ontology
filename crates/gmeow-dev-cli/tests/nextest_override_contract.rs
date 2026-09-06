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

/// Every `filter = '...'` value under `[[profile.default.overrides]]`, PLUS every
/// clause of the `default-filter` exclusion list.
///
/// The exclusion list rots exactly like an override filter does, and worse: an override
/// that stops matching silently withdraws a BUDGET, while an exclusion that stops
/// matching silently READMITS a test the profile deliberately keeps off the gate. Moving
/// the `describe` module into the model crate did that to the whole-repository
/// grounding-namespace proof — it ran on-gate, saturated the host, and thirteen unrelated
/// tests timed out in its wake with not one assertion failure among them. The cause is
/// indistinguishable from load until you check which clause stopped matching, so the
/// check belongs here rather than in anyone's memory.
fn override_filters(config: &str) -> Vec<String> {
    let mut out: Vec<String> = config
        .lines()
        .filter_map(|l| l.trim().strip_prefix("filter = "))
        .filter_map(|l| l.trim().strip_prefix('\''))
        .filter_map(|l| l.strip_suffix('\''))
        .map(str::to_string)
        .collect();
    out.extend(default_filter_clauses(config));
    out
}

/// The parenthesized clauses of EVERY `default-filter = '''…'''` block.
///
/// Each clause is a standalone filter expression joined by `|`, so each can be validated
/// on its own exactly as an override filter is.
///
/// Every such block is read, not just the first. `[profile.maint-heavy]` carries its own
/// filter naming the tests that even the breadth lane must not schedule, and those names
/// rot exactly like the ones in `[profile.default]` — with the same silent consequence
/// described above, one step further out: a clause that stops matching readmits a test the
/// maintainer lane deliberately excludes, and there is no lane beyond that one to catch it.
fn default_filter_clauses(config: &str) -> Vec<String> {
    const MARKER: &str = "default-filter = '''";
    let mut clauses = Vec::new();
    let mut rest = config;
    let mut blocks = 0usize;
    while let Some(start) = rest.find(MARKER) {
        let body = &rest[start + MARKER.len()..];
        let Some(end) = body.find("'''") else {
            break;
        };
        blocks += 1;
        clauses.extend(
            body[..end]
                .lines()
                .map(|l| l.trim().trim_start_matches('|').trim())
                .filter(|l| l.starts_with('(') && l.ends_with(')'))
                .map(str::to_string),
        );
        rest = &body[end + 3..];
    }
    assert!(
        blocks >= 2,
        "expected both the per-commit and the maintainer-breadth filter blocks; found {blocks} — \
         the parse is broken and this gate would pass vacuously"
    );
    assert!(
        clauses.len() >= 15,
        "expected the default filter to carry the live architectural exclusions; parsed {} — the \
         parse is broken and this gate would pass vacuously",
        clauses.len()
    );
    clauses
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

/// Scheduling reservations scale with the runner. A numeric `threads-required`
/// silently turns the machine that authored the config into a global ceiling: it
/// oversubscribes smaller runners and strands larger hosts. Whole-width tests may
/// reserve the natural host, but no fixed test-group/global cap is part of correctness.
#[test]
fn nextest_has_no_fixed_concurrency_caps() {
    let config = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let reservations = config
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("threads-required"))
        .collect::<Vec<_>>();
    assert!(
        reservations.len() >= 3,
        "the live config should exercise natural-width reservations non-vacuously"
    );
    assert!(
        reservations
            .iter()
            .all(|line| *line == "threads-required = \"num-cpus\""),
        "fixed nextest concurrency reservations are forbidden; use the runner-scaled \
         threads-required = \"num-cpus\": {reservations:?}"
    );
}

/// `run-ignored` belongs on the command line, never in the profile.
///
/// It reads like a profile key and is not one. Nextest 0.9.137 answers
/// `profile.maint-heavy.run-ignored` with
///
/// ```text
/// warning: in config file .config/nextest.toml, ignoring unknown configuration key:
///          profile.maint-heavy.run-ignored
/// ```
///
/// and then carries on. A warning does not fail anything, so the config *looks* like it
/// enables the workspace's `#[ignore]`d tests while changing nothing at all — the exact
/// shape of a gate that cannot fail (P8). This was written that way first and the full
/// `make check` passed over it; only listing the tests revealed that none had been added.
///
/// So the flag is pinned to the recipe and forbidden in the config, in both directions.
#[test]
fn the_breadth_lane_actually_runs_ignored_tests() {
    let config = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let makefile =
        std::fs::read_to_string(repo_root().join("Makefile")).expect("read the Makefile");

    let stray: Vec<&str> = config
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with('#') && t.starts_with("run-ignored")
        })
        .collect();
    assert!(
        stray.is_empty(),
        "`run-ignored` is not a nextest profile key — nextest warns `ignoring unknown \
         configuration key` and proceeds, so this setting silently does NOTHING. Pass \
         `--run-ignored all` on the maint-rust-heavy recipe instead. Found: {stray:?}"
    );

    assert!(
        makefile
            .lines()
            .any(|l| l.contains("--profile maint-heavy") && l.contains("--run-ignored all")),
        "no Makefile recipe invokes the maint-heavy profile with --run-ignored all, so every \
         #[ignore]d test in the workspace runs in no lane"
    );
}

/// Every `#[ignore]`d test function in the workspace, as `(file, fn name)`.
///
/// `#[ignore]` is a libtest attribute. No nextest filter expression can see it, so a test
/// carrying it is invisible to `default-filter` — including `all()`. The name is read from
/// the first `fn` line following the attribute; the intervening lines are other attributes.
fn ignored_test_fns() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                // A doc comment may *mention* `#[ignore]` while explaining it; only an
                // attribute in attribute position marks a test.
                if !line.trim_start().starts_with("#[ignore") {
                    continue;
                }
                if let Some(name) = lines[i + 1..]
                    .iter()
                    .take(6)
                    .find_map(|l| l.trim_start().strip_prefix("fn "))
                    .and_then(|l| l.split(['(', '<', ' ']).next())
                {
                    out.push((path.display().to_string(), name.to_string()));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&repo_root().join("crates"), &mut out);
    out
}

/// No test may be excluded from every lane.
///
/// `#[ignore]` silently removes a test from `make check` AND from `make maint-rust-heavy`,
/// because a filter expression cannot see the attribute — `default-filter = 'all()'` does
/// not un-ignore anything. Every `#[ignore]`d test in this repository was in exactly that
/// position, one of them while its own attribute text claimed "heavy lane only" (P11).
///
/// So each `#[ignore]`d test must now be reachable one of two ways, and this pins both:
/// the maintainer breadth lane runs it (`run-ignored = "all"`), or the breadth lane names
/// it in an exclusion AND some `make` target names it, so it has a lane of its own. A test
/// that is excluded there and mentioned in no target has no lane at all, which is the
/// defect this gate exists to make loud.
#[test]
fn every_ignored_test_is_reachable_from_some_lane() {
    let config = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let makefile =
        std::fs::read_to_string(repo_root().join("Makefile")).expect("read the Makefile");

    // The breadth lane must actually pass the flag. See
    // `the_breadth_lane_actually_runs_ignored_tests` for why it cannot live in the config.
    assert!(
        makefile
            .lines()
            .any(|l| { l.contains("--profile maint-heavy") && l.contains("--run-ignored all") }),
        "the maint-heavy recipe must pass --run-ignored all; without it a filter expression \
         cannot see #[ignore] and EVERY ignored test in the workspace runs in no lane at all"
    );

    let ignored = ignored_test_fns();
    assert!(
        ignored.len() >= 3,
        "expected to find the workspace's #[ignore]d tests; found {} — the scan is broken and \
         this gate would pass vacuously",
        ignored.len()
    );

    // The clauses of the maint-heavy filter are what the breadth lane refuses to schedule.
    let excluded_here: Vec<String> = default_filter_clauses(&config);

    let mut laneless = Vec::new();
    for (file, name) in &ignored {
        let excluded_from_heavy = excluded_here.iter().any(|c| c.contains(name.as_str()));
        if excluded_from_heavy && !makefile.contains(name.as_str()) {
            laneless.push(format!(
                "  - `{name}` ({file}) is #[ignore]d AND excluded from maint-heavy, but no make \
                 target names it — it runs nowhere"
            ));
        }
    }
    assert!(
        laneless.is_empty(),
        "{} #[ignore]d test(s) are excluded from every lane in this repository:\n{}",
        laneless.len(),
        laneless.join("\n")
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
    let moved = "verify_graph_accepts_a_normal_small_overlay_over_the_whole_bundle_heavy_offgate";

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

/// Every clause of the per-commit `default-filter` carries a numbered justification.
///
/// The head comment used to say only that "the exclusions are architectural lanes with an
/// independently invoked owner". That is one sentence standing in for fifteen separate
/// decisions, and a reader checking any single clause could not tell which owner it meant
/// or whether anyone had ever decided. An exclusion is a coverage decision; an
/// undocumented one is indistinguishable from an oversight.
///
/// The justification cannot live beside its clause: a nextest filterset is an EXPRESSION,
/// not a config table, and rejects `#` with "expected expression". So it sits immediately
/// above, numbered, in clause order — and this test pins that correspondence positionally
/// so the two cannot drift. A clause added without a reason reds; a reason orphaned by a
/// deleted clause reds too.
#[test]
fn every_default_filter_clause_is_justified() {
    let config = std::fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");

    // The per-commit block is the FIRST default-filter; `[profile.maint-heavy]`'s own
    // filter is a separate, smaller decision documented at its own site.
    const MARKER: &str = "default-filter = '''";
    let start = config
        .find(MARKER)
        .expect("the per-commit default-filter exists");
    let body = &config[start + MARKER.len()..];
    let end = body
        .find("'''")
        .expect("the per-commit default-filter is closed");
    let clauses: Vec<&str> = body[..end]
        .lines()
        .map(|l| l.trim().trim_start_matches('|').trim())
        .filter(|l| l.starts_with('(') && l.ends_with(')'))
        .collect();

    // The numbered justification bullets in the comment block immediately above.
    let head = &config[..start];
    // A bullet is its opening `#  N.` line PLUS every continuation line up to the next
    // bullet, so the pairing below reads the whole reason and not just its first sentence.
    let is_bullet_start = |l: &str| -> bool {
        l.trim()
            .strip_prefix('#')
            .map(str::trim_start)
            .is_some_and(|rest| {
                rest.split_once('.')
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            })
    };
    let mut reasons: Vec<String> = Vec::new();
    for line in head.lines() {
        if is_bullet_start(line) {
            reasons.push(line.trim().to_string());
        } else if let Some(current) = reasons.last_mut()
            && let Some(rest) = line.trim().strip_prefix('#')
            && !rest.trim().is_empty()
        {
            current.push(' ');
            current.push_str(rest.trim());
        }
    }

    assert!(
        clauses.len() >= 15,
        "parsed {} exclusion clause(s) — the parse is broken and this gate would pass \
         vacuously",
        clauses.len()
    );
    assert_eq!(
        reasons.len(),
        clauses.len(),
        "the per-commit default-filter has {} exclusion clause(s) but {} numbered \
         justification(s) above it. Every exclusion is a coverage decision and must say \
         which lane owns it; nextest filtersets reject inline comments, so the numbered \
         list immediately above the filter is where it goes.",
        clauses.len(),
        reasons.len()
    );

    // Positional pairing: justification N must name a package its clause N selects, so
    // reordering or inserting a clause without moving its reason is caught rather than
    // silently renumbering the whole list.
    let mut mismatched = Vec::new();
    for (i, (clause, reason)) in clauses.iter().zip(reasons.iter()).enumerate() {
        // A justification identifies its clause by naming the package, the binary, or a
        // test it selects — any of the three tells a reader which decision this is, and
        // all three move together when a clause is edited.
        let mut handles: Vec<String> = calls_of(clause, "package");
        handles.extend(calls_of(clause, "binary"));
        for payload in calls_of(clause, "test") {
            handles.extend(candidate_test_names(&payload));
        }
        if !handles.iter().any(|h| reason.contains(h.trim())) {
            mismatched.push(format!(
                "  - justification {} names none of its clause's package/binary/test \
                 handles {handles:?}\n      clause: {clause}\n      reason: {reason}",
                i + 1
            ));
        }
    }
    assert!(
        mismatched.is_empty(),
        "{} justification(s) have drifted away from the clause they document:\n{}",
        mismatched.len(),
        mismatched.join("\n")
    );
}
