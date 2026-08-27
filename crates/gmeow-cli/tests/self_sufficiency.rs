// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Self-sufficiency parity harness for the consumer `gmeow` binary.
//!
//! The required lane proves the installed binary reads its embedded bundle with the
//! source repo blinded (no repo tree discoverable above the process cwd). Focused
//! production-chain tests own lift, MAXIMAL fan-out, and internal-tag suppression on
//! every commit. The exhaustive installed-binary transpile remains registered in the
//! maintained breadth lane, where it proves those same laws over the whole snapshot.

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

// ── 1-3: transpile from the bundle, source repo blinded ─────────────────────

/// Required installed-binary boundary: a process with no environment and no repository
/// above its cwd reads the real embedded bundle and returns its non-vacuous census.
#[test]
fn installed_binary_reads_embedded_bundle_from_blinded_cwd() {
    let blind_cwd = tempfile::TempDir::new().expect("create temp dir");
    let output = run_subcommand_in(blind_cwd.path(), &["--console", "silent", "info"]);
    assert!(
        output.status.success(),
        "info must read the embedded bundle from a blinded cwd: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("info stdout is UTF-8");
    assert!(
        stdout.contains("terms") && stdout.contains("quads"),
        "{stdout}"
    );
}

/// Mirrors the retired `test_bundle_selfsufficient.py`'s core assertion:
/// `gmeow transpile` runs against the embedded bundle alone from a scratch cwd
/// with no repo tree discoverable above it, and both output artifacts land.
/// Generalized (item 2/3 of the transformational framing): the lift is
/// genuinely non-trivial (bundled lift map read, MAXIMAL fan-out fires: folded
/// `.gts` quads outnumber the asserted lift lines), and zero internal
/// `x-gmeow-*` tag survives into the consumer-facing folded graph.
#[test]
fn transpile_blinded_lifts_and_fans_out_without_x_gmeow_leak_heavy_offgate() {
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

// Per-command correctness remains owned by the focused CLI suites. Re-running every
// command twice under two cwd values duplicated those command proofs while decoding the
// same embedded corpus twelve more times. This one blinded, environment-cleared
// end-to-end drive is the cwd/repository-independence witness for the shared consumer
// binary boundary.
