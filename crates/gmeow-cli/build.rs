// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolve, guard, and expose every producer-authored asset embedded in `gmeow`,
//! and stamp the build identity `gmeow version --verbose` reports.
//!
//! The bundle is a git-ignored local/release product materialized by the
//! producer (`cargo xtask cli build`, `make check`, or `make install`), never a
//! committed input — so this build script resolves an ABSOLUTE path
//! (independent of the build's CWD) and fails the build closed, with an
//! actionable message naming the bootstrap command, when the file is absent or
//! zero-length (empty/truncated) rather than letting a bare `include_bytes!`
//! "file not found" or a silently truncated embed reach a consumer.
//!
//! Release/package flows may override either producer path. The same
//! absent/empty guard applies to an override, so no build can silently ship a
//! partial consumer.
//!
//! The identity stamp is equally mandatory. The source revision comes from
//! `GMEOW_SOURCE_REVISION` (40 hex digits, with `GMEOW_SOURCE_TREE` naming the
//! tree state), which `cargo xtask cli build` always sets; otherwise from
//! `git rev-parse HEAD`, recorded as `unverified` because a build script cannot
//! observe worktree dirtiness without rerunning on every build. With neither, the
//! build fails: a `gmeow` binary whose source revision is unknown must not exist.
//! The digests of the embedded payloads are NOT stamped here; the binary computes
//! them from its own embedded bytes at run time, so they cannot disagree.
//!
//! Dependency-free (std only) so this never perturbs the build graph.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Bind every mandatory producer artifact and identity field into the consumer compile.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let workspace = Path::new(&manifest).join("..").join("..");
    let generated = workspace.join("generated");
    expose_asset(
        "GMEOW_BUNDLE_PATH",
        generated.join("dist").join("gmeow.gts"),
        "gmeow.gts bundle",
    );
    expose_asset(
        "GMEOW_CODEBOOK_PATH",
        generated
            .join("projections")
            .join("lang")
            .join("gmn-codebook.cbor"),
        "GMN codebook",
    );
    stamp_identity(Path::new(&manifest), &workspace);
}

/// Resolve one default/override path, reject an absent payload, and expose its
/// canonical absolute path to `include_bytes!`.
fn expose_asset(env_name: &str, default: PathBuf, label: &str) {
    // A flip of the override (set/unset/changed) must re-resolve and re-embed.
    println!("cargo:rerun-if-env-changed={env_name}");

    let raw = std::env::var_os(env_name).map_or(default, PathBuf::from);

    // Absolutize before existence is even checked, so the guard message (and
    // any emitted env var) never depends on the build's current working
    // directory.
    let absolute = if raw.is_absolute() {
        raw.clone()
    } else {
        std::env::current_dir()
            .expect("current working directory")
            .join(&raw)
    };

    let valid = std::fs::metadata(&absolute)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false);
    if !valid {
        panic!(
            "gmeow: staged {label} {} is missing, empty, or not a regular file — run \
             `cargo xtask cli build` (or `make install`) to materialize every embedded \
             producer asset before building this consumer. It is a git-ignored \
             local/release product, not a committed input.",
            absolute.display()
        );
    }

    // Presence is confirmed — canonicalize away any `..`/symlinks so
    // `include_bytes!` resolves the identical bytes regardless of CWD.
    let resolved = absolute.canonicalize().unwrap_or_else(|e| {
        panic!(
            "gmeow: cannot canonicalize staged {label} path {}: {e}",
            absolute.display()
        )
    });

    println!("cargo:rerun-if-changed={}", resolved.display());
    println!("cargo:rustc-env={env_name}={}", resolved.display());
}

/// Emit the source, substrate, and toolchain identity as compile-time env vars.
fn stamp_identity(manifest: &Path, workspace: &Path) {
    for name in [
        "GMEOW_SOURCE_REVISION",
        "GMEOW_SOURCE_TREE",
        "GMEOW_BUILD_CPU_BASELINE",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let (revision, tree) = source_revision(manifest);
    println!("cargo:rustc-env=GMEOW_BUILD_SOURCE_REVISION={revision}");
    println!("cargo:rustc-env=GMEOW_BUILD_SOURCE_TREE={tree}");

    let lock = workspace.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let lock_text = std::fs::read_to_string(&lock).unwrap_or_else(|e| {
        panic!(
            "gmeow: cannot read {} to stamp the purrdf version: {e}",
            lock.display()
        )
    });
    let purrdf = locked_version(&lock_text, "purrdf").unwrap_or_else(|| {
        panic!(
            "gmeow: {} selects no `purrdf` package; the substrate version cannot be stamped",
            lock.display()
        )
    });
    println!("cargo:rustc-env=GMEOW_BUILD_PURRDF_VERSION={purrdf}");

    let target = required_env("TARGET");
    println!("cargo:rustc-env=GMEOW_BUILD_TARGET={target}");
    let profile = required_env("PROFILE");
    println!("cargo:rustc-env=GMEOW_BUILD_PROFILE={profile}");
    let baseline =
        std::env::var("GMEOW_BUILD_CPU_BASELINE").unwrap_or_else(|_| "unspecified".to_owned());
    if baseline.is_empty() || baseline.contains(char::is_whitespace) {
        panic!("gmeow: GMEOW_BUILD_CPU_BASELINE must be one non-empty token, got {baseline:?}");
    }
    println!("cargo:rustc-env=GMEOW_BUILD_CPU_BASELINE={baseline}");
    // The target features Cargo resolved from the rustflags this package is
    // compiled with: the observed CPU contract, not the requested one.
    let features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    println!("cargo:rustc-env=GMEOW_BUILD_TARGET_FEATURES={features}");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let version = Command::new(&rustc)
        .arg("-vV")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.lines().next().map(str::to_owned))
        .unwrap_or_else(|| panic!("gmeow: `{rustc} -vV` failed; the compiler cannot be stamped"));
    println!("cargo:rustc-env=GMEOW_BUILD_RUSTC={version}");
}

/// The explicit revision override, or `git rev-parse HEAD` recorded as unverified.
fn source_revision(manifest: &Path) -> (String, &'static str) {
    if let Ok(revision) = std::env::var("GMEOW_SOURCE_REVISION") {
        if !is_full_revision(&revision) {
            panic!("gmeow: GMEOW_SOURCE_REVISION must be a 40-hex-digit commit, got {revision:?}");
        }
        let tree = match std::env::var("GMEOW_SOURCE_TREE").as_deref() {
            Ok("clean") => "clean",
            Ok("dirty") => "dirty",
            other => panic!(
                "gmeow: GMEOW_SOURCE_REVISION is set, so GMEOW_SOURCE_TREE must be `clean` or \
                 `dirty`, got {other:?}"
            ),
        };
        return (revision, tree);
    }
    let Some(revision) = git(manifest, &["rev-parse", "HEAD"]).filter(|r| is_full_revision(r))
    else {
        panic!(
            "gmeow: no source revision: GMEOW_SOURCE_REVISION=<40-hex> is unset and \
             `git rev-parse HEAD` found no commit in {}. Build with `cargo xtask cli build`, \
             or set GMEOW_SOURCE_REVISION and GMEOW_SOURCE_TREE.",
            manifest.display()
        );
    };
    // Re-stamp when HEAD moves: the HEAD file, the branch it names, and packed refs.
    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(branch) = git(manifest, &["symbolic-ref", "-q", "HEAD"]) {
        watched.push(branch);
    }
    for name in watched {
        if let Some(path) = git(
            manifest,
            &["rev-parse", "--path-format=absolute", "--git-path", &name],
        ) && Path::new(&path).is_file()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    (revision, "unverified")
}

/// Run one git query in `dir`, returning trimmed stdout on success.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
        .map(|text| text.trim().to_owned())
}

/// Exactly 40 lowercase hexadecimal digits.
fn is_full_revision(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The version Cargo.lock selects for the single package named `name`.
fn locked_version(lock: &str, name: &str) -> Option<String> {
    let wanted = format!("name = \"{name}\"");
    let mut versions = lock.split("[[package]]").filter_map(|block| {
        let mut lines = block.lines().map(str::trim);
        lines.find(|line| *line == wanted)?;
        lines
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_owned)
    });
    let first = versions.next()?;
    // Two locked versions of the substrate would make the stamp ambiguous.
    versions.next().is_none().then_some(first)
}

/// A Cargo-provided build-script variable that is always present.
fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("gmeow: Cargo did not set {name}"))
}
