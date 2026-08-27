// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end parity tests for the `gmeow-dev` binary.
//!
//! These mirror the Python `tests/test_*` suites, one Rust test per behaviour:
//!
//! | Rust test                         | mirrors Python                          |
//! |-----------------------------------|-----------------------------------------|
//! | `version_prints_package_version`  | `test_cli_dev` version surface          |
//! | `logic_query_recursive_ancestor`  | `test_logic_cli::test_logic_query_*`    |
//! | `logic_compile_unknown_mode_fails`| `test_logic_cli::…_unknown_mode_fails`  |
//! | `external_tool_mirrors_child_exit`| `test_external_tool`                    |
//! | `external_tool_success_is_clean`  | `test_external_tool`                    |
//!
//! Whole-pipeline and whole-gate commands are intentionally absent: a test must
//! never reach a repository corpus producer. Focused CLI behavior stays here.

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

/// The repository root (this crate's manifest is `<root>/crates/gmeow-dev-cli`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// A `gmeow-dev` command anchored at the repo root via `GMEOW_ROOT`.
fn dev_cmd() -> Command {
    let mut cmd = Command::cargo_bin("gmeow-dev").expect("gmeow-dev binary");
    cmd.env("GMEOW_ROOT", repo_root());
    cmd
}

#[test]
fn version_prints_package_version() {
    dev_cmd()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(env!("CARGO_PKG_VERSION")));
}

/// Scrape the subcommand names from the `Commands:` block of a clap `--help`
/// screen, asserting the help itself renders and exits 0. Returns an empty vec
/// for a leaf command (no `Commands:` block). `help` is elided.
fn subcommand_names(path: &[&str]) -> Vec<String> {
    let mut cmd = dev_cmd();
    cmd.args(path).arg("--help");
    let out = cmd.output().expect("run --help");
    assert!(
        out.status.success(),
        "`gmeow-dev {} --help` must exit 0",
        path.join(" ")
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.trim_end() == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands {
            // The block ends at the first blank line or the next `Section:` header.
            if line.trim().is_empty() || !line.starts_with(' ') {
                break;
            }
            if let Some(name) = line.split_whitespace().next()
                && name != "help"
            {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Every command and nested sub-app command answers `--help` with exit 0. A
/// cheap on-gate wiring smoke test over the whole surface: it proves each command
/// is reachable and its clap parser is well-formed, without running any gate.
#[test]
fn every_subcommand_responds_to_help() {
    let top = subcommand_names(&[]);
    assert!(
        top.len() >= 40,
        "expected the full gmeow-dev command surface, got {}: {top:?}",
        top.len()
    );
    for name in &top {
        // `subcommand_names` asserts `<name> --help` exits 0 and returns any
        // nested sub-app commands; assert `--help` on each of those too.
        for sub in subcommand_names(&[name.as_str()]) {
            dev_cmd()
                .args([name.as_str(), sub.as_str(), "--help"])
                .assert()
                .success();
        }
    }
}

/// An unknown subcommand is a clap usage error (exit code 2), never a silent 0.
#[test]
fn unknown_subcommand_is_a_usage_error() {
    dev_cmd()
        .arg("definitely-not-a-real-command")
        .assert()
        .code(2);
}

#[test]
fn logic_query_recursive_ancestor() {
    let case = repo_root().join("conformance/logic/cases/profiles/goal-recursive-ancestor");
    if !case.is_dir() {
        return;
    }
    let assert = dev_cmd()
        .arg("logic")
        .arg("query")
        .arg(case.join("input.nq"))
        .arg(case.join("queries/ancestor.logic"))
        .arg("--json")
        .assert()
        .success();
    let out = assert.get_output();
    let payload: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("query emits JSON on stdout");
    assert_eq!(payload["status"], "ok");
    let ys: std::collections::BTreeSet<String> = payload["bindings"]
        .as_array()
        .expect("bindings array")
        .iter()
        .map(|b| b["Y"].as_str().unwrap().to_owned())
        .collect();
    let expect: std::collections::BTreeSet<String> = [
        "<https://example.org/profiles/goal-recursive-ancestor/b>",
        "<https://example.org/profiles/goal-recursive-ancestor/c>",
        "<https://example.org/profiles/goal-recursive-ancestor/d>",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(ys, expect);
}

#[test]
fn logic_compile_unknown_mode_fails() {
    dev_cmd()
        .arg("logic")
        .arg("compile")
        .arg("--mode")
        .arg("bogus")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown --mode"));
}

#[test]
fn external_tool_success_is_clean() {
    // A tool that succeeds yields a clean report and a zero exit.
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("truth")
        .arg("--diagnostics-artifacts")
        .arg("none")
        .arg("--")
        .arg("true")
        .assert()
        .success();
}

#[test]
fn external_tool_mirrors_child_exit() {
    // `false` exits 1; the wrapper MIRRORS that exact non-zero code.
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("falsehood")
        .arg("--diagnostics-artifacts")
        .arg("none")
        .arg("--")
        .arg("false")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn external_tool_writes_artifacts() {
    let (_tmp, dir) = tempdir();
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("falsehood")
        .arg("--diagnostics-dir")
        .arg(&dir)
        .arg("--diagnostics-artifacts")
        .arg("json,sarif")
        .arg("--diagnostics-stem")
        .arg("ext")
        .arg("--")
        .arg("false")
        .assert()
        .failure();
    assert!(dir.join("ext.json").is_file(), "wrote the JSON artifact");
    assert!(dir.join("ext.sarif").is_file(), "wrote the SARIF artifact");
}

#[test]
fn project_view_over_the_snapshot() {
    // `project --profile gmeow` filters the committed snapshot to the pure-GMEOW
    // view and writes a Turtle artifact.
    dev_cmd()
        .arg("project")
        .arg("--profile")
        .arg("gmeow")
        .assert()
        .success()
        .stdout(predicate::str::contains("gmeow.ttl"));
}

#[test]
fn extract_reference_only_target_is_refused() {
    // The license guard refuses a reference-only (CC-BY-SA) target (exit 1) …
    dev_cmd()
        .arg("extract")
        .arg("--target")
        .arg("ontouml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("reference-only"));
    // … and permits an import-ok (MIT) target.
    dev_cmd()
        .arg("extract")
        .arg("--target")
        .arg("gufo")
        .assert()
        .success()
        .stdout(predicate::str::contains("import-ok"));
}

#[test]
#[ignore = "off-gate: live Wikidata network lookup (set GMEOW_RUN_NETWORK=1)"]
fn wikidata_existence_live_lookup() {
    if std::env::var("GMEOW_RUN_NETWORK").as_deref() != Ok("1") {
        return;
    }
    // Every referenced QID/PID must resolve on the live Wikidata endpoint.
    dev_cmd()
        .arg("wikidata")
        .arg("--existence")
        .assert()
        .success()
        .stdout(predicate::str::contains("resolve on Wikidata"));
}

/// A fresh, empty temp directory owned by the returned [`tempfile::TempDir`].
///
/// The guard must be bound to a live local (`let (_tmp, dir) = tempdir();`) for the
/// duration of the test: dropping it removes the directory and its contents, on
/// success, on early return, and on panic alike.
fn tempdir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-dev-cli-test-")
        .tempdir()
        .expect("create temp dir");
    let path = tmp.path().to_path_buf();
    (tmp, path)
}

// ── shape-equivalence: the per-increment migration verifier ──────────────────

/// Write a hermetic mini-repo (a projected `validation-shapes.ttl` plus a slice `shapes.ttl`)
/// under `root`, so the `shape-equivalence` gate can be exercised without the live tree.
fn write_shape_fixture(root: &std::path::Path, legacy_property: &str) {
    let shapes_dir = root.join("generated").join("shapes");
    std::fs::create_dir_all(&shapes_dir).expect("mk generated/shapes");
    // The projected surface: Foo with exactly-one bar.
    std::fs::write(
        shapes_dir.join("validation-shapes.ttl"),
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         <https://blackcatinformatics.ca/gmeow/Foo-shape> a sh:NodeShape ;\n\
             sh:targetClass <https://blackcatinformatics.ca/gmeow/Foo> ;\n\
             sh:property [ sh:path <https://blackcatinformatics.ca/gmeow/bar> ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
    )
    .expect("write projected shapes");
    // The production union always has all three generated members. Keep the hermetic fixture
    // structurally faithful even when this case exercises only the declarative projection.
    for member in ["constraint-shapes.ttl", "procedural-constraints.ttl"] {
        std::fs::write(
            shapes_dir.join(member),
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n",
        )
        .expect("write empty projected shape-union member");
    }
    let slice_dir = root.join("slices").join("demo");
    std::fs::create_dir_all(&slice_dir).expect("mk slices/demo");
    std::fs::write(
        slice_dir.join("shapes.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             gmeow:FooShape a sh:NodeShape ;\n\
                 sh:targetClass gmeow:Foo ;\n\
                 sh:property [ sh:path gmeow:bar ; {legacy_property} ] .\n"
        ),
    )
    .expect("write legacy shapes");
}

/// A legacy block the projector reproduces exactly is `EQUIV` and clears the gate (exit 0).
#[test]
fn shape_equivalence_reports_equiv_and_exits_zero_when_reproduced() {
    let (_tmp, root) = tempdir();
    write_shape_fixture(&root, "sh:minCount 1 ; sh:maxCount 1");
    Command::cargo_bin("gmeow-dev")
        .expect("gmeow-dev binary")
        .env("GMEOW_ROOT", &root)
        .args(["shape-equivalence", "--path", "slices/demo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[EQUIV]").and(predicate::str::contains("gmeow:FooShape")),
        );
}

/// A legacy block the projector does NOT reproduce (a tighter cardinality) is `NOT-EQUIV`
/// and reds the gate (exit non-zero) — the equivalence-before-deletion guard.
#[test]
fn shape_equivalence_reports_not_equiv_and_exits_nonzero_when_divergent() {
    let (_tmp, root) = tempdir();
    write_shape_fixture(&root, "sh:minCount 1 ; sh:maxCount 2");
    Command::cargo_bin("gmeow-dev")
        .expect("gmeow-dev binary")
        .env("GMEOW_ROOT", &root)
        .args(["shape-equivalence", "--path", "slices/demo"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[NOT-EQUIV"));
}

/// Write a mini-repo whose projected surface carries a `Foo` class shape (with the given
/// `projected_bar` facets on `bar`) AND a SEPARATE property-scoped functional shape
/// (`sh:targetSubjectsOf bar` with `sh:maxCount 1`). The legacy `Foo` shape caps `bar` at exactly
/// one. The functional shape must never rescue a class shape that DROPPED the cap: the projected
/// CLASS surface itself must carry the cap for the block to clear.
fn write_functional_credit_fixture(root: &std::path::Path, projected_bar: &str) {
    let shapes_dir = root.join("generated").join("shapes");
    std::fs::create_dir_all(&shapes_dir).expect("mk generated/shapes");
    std::fs::write(
        shapes_dir.join("validation-shapes.ttl"),
        format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://blackcatinformatics.ca/gmeow/Foo-shape> a sh:NodeShape ;\n\
                 sh:targetClass <https://blackcatinformatics.ca/gmeow/Foo> ;\n\
                 sh:property [ sh:path <https://blackcatinformatics.ca/gmeow/bar> ; {projected_bar} ] .\n\
             <https://blackcatinformatics.ca/gmeow/bar-functional> a sh:NodeShape ;\n\
                 sh:targetSubjectsOf <https://blackcatinformatics.ca/gmeow/bar> ;\n\
                 sh:property [ sh:path <https://blackcatinformatics.ca/gmeow/bar> ; sh:maxCount 1 ] .\n"
        ),
    )
    .expect("write projected shapes");
    for member in ["constraint-shapes.ttl", "procedural-constraints.ttl"] {
        std::fs::write(
            shapes_dir.join(member),
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n",
        )
        .expect("write empty projected shape-union member");
    }
    let slice_dir = root.join("slices").join("demo");
    std::fs::create_dir_all(&slice_dir).expect("mk slices/demo");
    std::fs::write(
        slice_dir.join("shapes.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         gmeow:FooShape a sh:NodeShape ;\n\
             sh:targetClass gmeow:Foo ;\n\
             sh:property [ sh:path gmeow:bar ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
    )
    .expect("write legacy shapes");
}

/// FAITHFUL: the projected CLASS shape carries `sh:maxCount 1` on `bar` itself. The block clears
/// `EQUIV` (exit 0) — legitimate functional-cap equivalence is preserved.
#[test]
fn shape_equivalence_equiv_when_class_shape_carries_the_functional_cap() {
    let (_tmp, root) = tempdir();
    write_functional_credit_fixture(&root, "sh:minCount 1 ; sh:maxCount 1");
    Command::cargo_bin("gmeow-dev")
        .expect("gmeow-dev binary")
        .env("GMEOW_ROOT", &root)
        .args(["shape-equivalence", "--path", "slices/demo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[EQUIV]").and(predicate::str::contains("gmeow:FooShape")),
        );
}

/// LOSS (the R3 regression): the projected CLASS shape DROPPED the `sh:maxCount` on `bar` even
/// though the property-scoped functional shape still carries `sh:maxCount 1`. The functional
/// credit must NOT rescue it — the block is `NOT-EQUIV` and reds the gate (exit non-zero).
#[test]
fn shape_equivalence_not_equiv_when_class_shape_drops_the_functional_cap() {
    let (_tmp, root) = tempdir();
    write_functional_credit_fixture(&root, "sh:minCount 1");
    Command::cargo_bin("gmeow-dev")
        .expect("gmeow-dev binary")
        .env("GMEOW_ROOT", &root)
        .args(["shape-equivalence", "--path", "slices/demo"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[NOT-EQUIV"));
}

/// `shape-migrate`'s namespace guard must accept every authoring namespace
/// (`gmeow_logic_compile::frontend::AUTHORING_NAMESPACES`), not just `gmeow:`: a legacy shape
/// targeting a `math:` class is eligible for injection and must NOT be skipped as
/// non-dogfooded. Regression for the injector consuming the single exported authority instead
/// of a stale local mirror of it.
#[test]
fn shape_migrate_does_not_skip_a_math_namespace_target_class() {
    let (_tmp, root) = tempdir();
    // The shared projected surface (irrelevant `gmeow:Foo` shape) — `OracleCtx::load` requires
    // all three generated shape-union members to exist.
    write_shape_fixture(&root, "sh:minCount 1 ; sh:maxCount 1");
    let slice_dir = root.join("slices").join("demo-math");
    std::fs::create_dir_all(&slice_dir).expect("mk slices/demo-math");
    std::fs::write(
        slice_dir.join("shapes.ttl"),
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         math:PointShape a sh:NodeShape ;\n\
             sh:targetClass math:Point ;\n\
             sh:property [ sh:path math:coordinate ; sh:minCount 1 ] .\n",
    )
    .expect("write legacy math shape");
    Command::cargo_bin("gmeow-dev")
        .expect("gmeow-dev binary")
        .env("GMEOW_ROOT", &root)
        .args(["shape-migrate", "--path", "slices/demo-math"])
        .assert()
        .stdout(
            predicate::str::contains("[SKIP non-dogfooded-namespace]")
                .not()
                .and(predicate::str::contains(
                    "https://blackcatinformatics.ca/math/PointShape",
                )),
        );
}
