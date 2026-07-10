// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Self-sufficiency parity harness for the consumer `gmeow` binary.
//!
//! Recreates natively the invariants the retired
//! `tests/test_bundle_selfsufficient.py` pinned — the embedded bundle alone
//! drives `gmeow transpile` with the source repo blinded (no repo tree
//! discoverable above the process cwd), the transpile lift + MAXIMAL fan-out
//! is genuinely non-trivial (the bundled lift map is actually read and the
//! multi-vocab fan-out actually fires), and zero internal `x-gmeow-*` tag
//! leaks into consumer-facing output — then generalizes the old test's single
//! blinded `transpile` check into a **naturality-over-cwd** law: every
//! consumer subcommand (`transpile`, `project`, `describe`, `validate`)
//! produces byte-identical stdout AND byte-identical output artifacts whether
//! run from a blinded scratch cwd (no repo above it) or from inside the repo
//! root — proving the WHOLE consumer surface is repo-free, not just
//! `transpile`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};

use gmeow_validate::language_tags::is_internal_tag;

// ── shared helpers (mirrors the `cli.rs` `fixture()` convention; scratch dirs
// are owned `tempfile::TempDir`s, panic-safe via `Drop`) ─────────────────────

/// The repo-root path of a committed validate fixture.
fn fixture(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/validate")
        .join(name);
    path.canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize fixture {}: {e}", path.display()))
}

/// The repo root (this crate's manifest dir, two levels up) — a real git
/// worktree, used as the "repo-cwd" leg of the parity harness.
fn repo_root() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    path.canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize repo root {}: {e}", path.display()))
}

/// The built `gmeow` binary's absolute path.
fn gmeow_bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin("gmeow")
}

/// Run the built `gmeow` binary with `args` from `cwd`, with a fully cleared
/// environment (no ambient `GMEOW_*`/`PATH`/anything) — the shared primitive
/// for both legs of the wheel-mode == repo-mode parity harness. Invoked by
/// absolute binary path, so a cleared `PATH` does not prevent exec.
fn run_subcommand_in(cwd: &Path, args: &[&str]) -> Output {
    StdCommand::new(gmeow_bin())
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .output()
        .unwrap_or_else(|e| panic!("run gmeow {args:?} in {}: {e}", cwd.display()))
}

/// Recursively snapshot every regular file under `dir` as `{relative path:
/// bytes}`. Returns an empty map for a directory with no files (or that
/// doesn't exist), so it's safe to call on an out-dir a command never wrote
/// to (e.g. `describe`/`validate`, which write nothing).
fn snapshot_files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.expect("read_dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).expect("under root").to_path_buf();
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                out.insert(rel, bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

// ── 1-3: transpile from the bundle, source repo blinded ─────────────────────

/// Mirrors the retired `test_bundle_selfsufficient.py`'s core assertion:
/// `gmeow transpile` runs against the embedded bundle alone from a scratch cwd
/// with no repo tree discoverable above it, and both output artifacts land.
/// Generalized (item 2/3 of the transformational framing): the lift is
/// genuinely non-trivial (bundled lift map read, MAXIMAL fan-out fires: folded
/// `.gts` quads outnumber the asserted lift lines), and zero internal
/// `x-gmeow-*` tag survives into the consumer-facing folded graph.
#[test]
fn transpile_blinded_lifts_and_fans_out_without_x_gmeow_leak() {
    let blind_cwd = tempfile::TempDir::new().expect("create temp dir");
    let out = tempfile::TempDir::new().expect("create temp dir");
    let src = fixture("selfsuff-transpile.ttl");

    let output = run_subcommand_in(
        blind_cwd.path(),
        &[
            "transpile",
            src.to_str().expect("utf8 path"),
            "--out",
            out.path().to_str().expect("utf8 path"),
        ],
    );
    assert!(
        output.status.success(),
        "transpile must succeed from a blinded cwd (no repo tree above it): stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let nt_path = out.path().join("selfsuff-transpile.gmeow.nt");
    let gts_path = out.path().join("selfsuff-transpile.gts");
    assert!(nt_path.exists(), "{} must exist", nt_path.display());
    assert!(gts_path.exists(), "{} must exist", gts_path.display());

    // 2. Non-trivial lift + fan-out: the bundled lift map was genuinely read
    // (lift-line-count > 0) and the MAXIMAL multi-vocab fan-out genuinely
    // fired (folded quads > lift lines — "output > asserted").
    let lift_text = std::fs::read_to_string(&nt_path).expect("read lift nt");
    let lift_lines = lift_text.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        lift_lines > 0,
        "bundled lift map must have been read (nonzero lift): {lift_text}"
    );

    let gts_bytes = std::fs::read(&gts_path).expect("read gts");
    let graph = purrdf::gts::reader::read(&gts_bytes, true, None);
    assert!(
        graph.quads.len() > lift_lines,
        "the MAXIMAL fan-out must produce MORE folded quads than were asserted: \
         folded {} quads vs {lift_lines} lift lines",
        graph.quads.len()
    );

    // 3. Zero x-gmeow leak: every language-tagged literal term in the folded
    // consumer output graph must carry a PUBLIC BCP-47 tag, never an internal
    // `x-gmeow-*` private-use one.
    let leaked: Vec<&str> = graph
        .terms
        .iter()
        .filter_map(|t| t.lang.as_deref())
        .filter(|lang| is_internal_tag(lang))
        .collect();
    assert!(
        leaked.is_empty(),
        "internal x-gmeow-* tag(s) leaked into consumer transpile output: {leaked:?}"
    );
}

// ── 4: wheel-mode == repo-mode (parity harness) ──────────────────────────────

/// Run `build_args(out_dir)` once from a blinded scratch cwd (no repo above
/// it) and once from the real repo root, feeding the SAME absolute `out_dir`
/// (a fresh `TempDir` this function creates and owns) to both invocations
/// (so only cwd varies and the printed "wrote …" lines are byte-identical
/// across legs by construction). Asserts:
/// - both legs succeed with byte-identical stdout,
/// - any files the command wrote under `out_dir` are byte-identical across
///   legs (captured via a full snapshot after the first leg, before the
///   second leg overwrites),
/// - neither leg's stdout contains either cwd's absolute path substring
///   (rules out a subtler cwd-derived-content bug masking as accidental
///   byte-parity).
fn assert_cwd_parity(tag: &str, build_args: impl Fn(&Path) -> Vec<String>) {
    let blind_cwd = tempfile::TempDir::new().expect("create temp dir");
    let out = tempfile::TempDir::new().expect("create temp dir");
    let out_dir = out.path();
    let repo_cwd = repo_root();

    let args_owned = build_args(out_dir);
    let args: Vec<&str> = args_owned.iter().map(String::as_str).collect();

    let blind_output = run_subcommand_in(blind_cwd.path(), &args);
    assert!(
        blind_output.status.success(),
        "{tag}: blinded-cwd leg must succeed: stderr={}",
        String::from_utf8_lossy(&blind_output.stderr)
    );
    let blind_artifacts = snapshot_files(out_dir);

    let repo_output = run_subcommand_in(&repo_cwd, &args);
    assert!(
        repo_output.status.success(),
        "{tag}: repo-cwd leg must succeed: stderr={}",
        String::from_utf8_lossy(&repo_output.stderr)
    );
    let repo_artifacts = snapshot_files(out_dir);

    assert_eq!(
        blind_output.stdout,
        repo_output.stdout,
        "{tag}: stdout must be byte-identical across cwd legs.\nblind stdout={}\nrepo stdout={}",
        String::from_utf8_lossy(&blind_output.stdout),
        String::from_utf8_lossy(&repo_output.stdout)
    );
    assert_eq!(
        blind_artifacts,
        repo_artifacts,
        "{tag}: output artifacts under {} must be byte-identical across cwd legs",
        out_dir.display()
    );

    // Neither cwd's absolute path may leak into stdout or any artifact.
    let blind_cwd_str = blind_cwd.path().to_string_lossy().into_owned();
    let repo_cwd_str = repo_cwd.to_string_lossy().into_owned();
    for (leg, out) in [("blind", &blind_output), ("repo", &repo_output)] {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains(&blind_cwd_str),
            "{tag} {leg} leg: blinded cwd path leaked into stdout: {stdout}"
        );
        assert!(
            !stdout.contains(&repo_cwd_str),
            "{tag} {leg} leg: repo cwd path leaked into stdout: {stdout}"
        );
    }
    for (rel, bytes) in &repo_artifacts {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains(&blind_cwd_str),
            "{tag}: blinded cwd path leaked into artifact {}",
            rel.display()
        );
        assert!(
            !text.contains(&repo_cwd_str),
            "{tag}: repo cwd path leaked into artifact {}",
            rel.display()
        );
    }
}

#[test]
fn transpile_wheel_mode_equals_repo_mode() {
    let src = fixture("selfsuff-transpile.ttl");
    assert_cwd_parity("transpile", |out_dir| {
        vec![
            "transpile".to_owned(),
            src.to_string_lossy().into_owned(),
            "--out".to_owned(),
            out_dir.to_string_lossy().into_owned(),
        ]
    });
}

#[test]
fn project_wheel_mode_equals_repo_mode() {
    // Filters the bundled snapshot itself through the `schema-org` vocabulary
    // profile (the same invocation shape already proven live by
    // `cli.rs::project_schema_org_view_filter`) — no source file, so this
    // exercises the "nothing for source but nothing repo-derived either"
    // consumer surface (the bundled ontology is embedded, never read from
    // the repo tree).
    assert_cwd_parity("project", |out_dir| {
        vec![
            "project".to_owned(),
            "--profile".to_owned(),
            "schema-org".to_owned(),
            "--out".to_owned(),
            out_dir.to_string_lossy().into_owned(),
        ]
    });
}

#[test]
fn describe_wheel_mode_equals_repo_mode() {
    // `Entity` is the known-stable kernel term already proven live by
    // `cli.rs::describe_known_term_renders_prose`.
    assert_cwd_parity("describe", |_out_dir| {
        vec!["describe".to_owned(), "Entity".to_owned()]
    });
}

#[test]
fn validate_wheel_mode_equals_repo_mode() {
    // Reuses the same `clean.ttl` fixture `cli.rs::validate_clean_file_passes`
    // already proves live: zero findings, so "validation passed" on stdout.
    let clean = fixture("clean.ttl");
    assert_cwd_parity("validate", |_out_dir| {
        vec!["validate".to_owned(), clean.to_string_lossy().into_owned()]
    });
}
