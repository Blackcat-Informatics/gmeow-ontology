// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `crates/README.md` is the workspace source map, and its authority is the set of
//! package names actually declared under `crates/*/Cargo.toml`. This gate enforces a
//! bijection between the two in both directions, fatally:
//!
//! * every backtick-quoted token in the map that has the shape of a package name in
//!   this workspace must BE a declared package name, and
//! * every declared package name must appear, backtick-quoted, somewhere in the map.
//!
//! Both halves are derived at test time from the manifests. Nothing here enumerates
//! package names, and nothing here enumerates forbidden ones. A literal alternation of
//! known-dead names would be a strictly weaker gate: it can only fail for a name someone
//! already thought to write down, so the very failure mode that matters — a package
//! renamed, extracted, or deleted after this test was authored, leaving a stale row in
//! the map — would pass silently. Deriving the valid set instead makes the gate fail for
//! names nobody anticipated, which is the whole point.
//!
//! The "shape of a package name" is likewise derived rather than assumed: the dominant
//! convention is a `gmeow-` prefix, so any backtick token with that prefix is in scope,
//! and the remaining in-scope tokens are exactly the declared names that do not follow
//! the convention. That keeps a token like `gmeow.gts` or `Cargo.toml` out of scope
//! while keeping every plausible package reference in it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The package-name prefix the overwhelming majority of workspace crates follow.
const CONVENTIONAL_PREFIX: &str = "gmeow-";

/// Walk up from this test's manifest directory to the workspace root — the first
/// ancestor that itself contains a `crates/` directory. Never a CWD-relative path:
/// `cargo test` and `cargo nextest` do not agree on the working directory.
fn repo_root() -> PathBuf {
    let mut dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("crates").is_dir() {
            return dir.to_path_buf();
        }
        dir = dir.parent().unwrap_or_else(|| {
            panic!(
                "no ancestor of {} contains a crates/ directory",
                env!("CARGO_MANIFEST_DIR")
            )
        });
    }
}

/// Extract the `name = "..."` value from a `[package]` manifest.
fn package_name(manifest: &str, path: &Path) -> String {
    for line in manifest.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("name") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        if let Some(inner) = rest.strip_prefix('"').and_then(|r| r.split('"').next()) {
            return inner.to_string();
        }
    }
    panic!("{} declares no package name", path.display());
}

/// Every package name declared by a directory directly under `crates/`.
fn declared_package_names(root: &Path) -> BTreeSet<String> {
    let crates_dir = root.join("crates");
    let mut names = BTreeSet::new();
    let entries = fs::read_dir(&crates_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", crates_dir.display()));
    for entry in entries {
        let entry = entry.expect("cannot read a crates/ directory entry");
        let manifest_path = entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest_path.display()));
        names.insert(package_name(&manifest, &manifest_path));
    }
    assert!(
        !names.is_empty(),
        "no package manifests found under {}",
        crates_dir.display()
    );
    names
}

/// Characters a package-name-shaped token may contain. Deliberately excludes `.` and
/// `/` so that paths and file names inside backticks are never mistaken for packages.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Every backtick-delimited span in the document, verbatim.
fn backtick_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        tokens.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    tokens
}

/// Levenshtein distance, used only to make a failure message actionable.
fn edit_distance(a: &str, b: &str) -> usize {
    let b_len = b.chars().count();
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut cur = vec![0usize; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b_len]
}

/// The two or three declared names closest to `token`, for the failure message.
fn nearest(token: &str, valid: &BTreeSet<String>) -> Vec<String> {
    let mut scored: Vec<(usize, &String)> =
        valid.iter().map(|n| (edit_distance(token, n), n)).collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(3).map(|(_, n)| n.clone()).collect()
}

#[test]
fn crates_readme_names_only_real_packages() {
    let root = repo_root();
    let valid = declared_package_names(&root);

    // Derived, not assumed: names that do not follow the `gmeow-` convention are in
    // scope only as themselves, and the convention covers everything else.
    let unconventional: BTreeSet<&str> = valid
        .iter()
        .filter(|n| !n.starts_with(CONVENTIONAL_PREFIX))
        .map(String::as_str)
        .collect();
    let in_scope = |token: &str| -> bool {
        (token.starts_with(CONVENTIONAL_PREFIX) && token.chars().all(is_token_char))
            || unconventional.contains(token)
    };

    let readme_path = root.join("crates/README.md");
    let readme = fs::read_to_string(&readme_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", readme_path.display()));

    // Direction 1: every package-shaped token in the map must be a declared package.
    let mut unknown: Vec<String> = backtick_tokens(&readme)
        .into_iter()
        .filter(|t| in_scope(t) && !valid.contains(t.as_str()))
        .collect();
    unknown.sort();
    unknown.dedup();
    assert!(
        unknown.is_empty(),
        "{} names {} package(s) that no crates/*/Cargo.toml declares:\n{}",
        readme_path.display(),
        unknown.len(),
        unknown
            .iter()
            .map(|t| format!(
                "  `{t}` — nearest declared: {}",
                nearest(t, &valid).join(", ")
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    // Direction 2: every declared package must appear in the map. Also fatal — the map
    // is only a map if it covers the territory.
    let mentioned: BTreeSet<String> = backtick_tokens(&readme).into_iter().collect();
    let missing: Vec<&String> = valid.iter().filter(|n| !mentioned.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "{} omits {} declared package(s):\n{}",
        readme_path.display(),
        missing.len(),
        missing
            .iter()
            .map(|n| format!("  `{n}`"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
