// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Every repo-relative path, `make` target, and published package name cited by the
//! reader-facing documents must resolve to something that exists.
//!
//! Documentation rots silently: a crate moves, a target is renamed, a package is
//! rescoped, and the prose keeps pointing at the old name with nothing to catch it.
//! This branch alone found the Constitution citing five crates that no longer existed
//! and a crate table naming fifteen dead packages, both of which had been wrong long
//! enough that nobody trusted the prose any more.
//!
//! # Why an explicit file list
//!
//! The list below is a constant, so the predicate is determinate at test time. "The
//! files this work touches" is not observable to a committed test — it would either rot
//! into a stale list or red on unrelated pre-existing drift in documents this gate never
//! agreed to own. Adding a document here is a deliberate act of putting it under the
//! gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The documents this gate owns. Adding one is deliberate; it puts that file's citations
/// under the resolver below.
const GATED_DOCUMENTS: &[&str] = &[
    "README.md",
    "crates/README.md",
    "crates/docs/assets/console/README.md",
    "docs/design/external-docs-distribution.md",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Repo-relative paths cited as `](./some/path)` or `` `some/path` `` that look like
/// files or directories in this tree.
fn cited_paths(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    // Markdown links of the form `](./path)` or `](path)`.
    for (idx, _) in body.match_indices("](") {
        let rest = &body[idx + 2..];
        let Some(end) = rest.find(')') else { continue };
        let target = &rest[..end];
        if target.starts_with("http") || target.starts_with('#') || target.starts_with("mailto:") {
            continue;
        }
        let target = target.split('#').next().unwrap_or(target);
        let target = target.strip_prefix("./").unwrap_or(target);
        if target.is_empty() {
            continue;
        }
        // Only gate things that name a tracked tree location, not anchors or bare words.
        if target.contains('/') || target.ends_with(".md") {
            out.insert(target.to_string());
        }
    }
    out
}

/// `make <target>` invocations cited as a COMMAND, i.e. inside a backtick code span.
///
/// Bare prose is not a citation: "…make constitutional drift a build failure" is English,
/// not an invocation, and a matcher that cannot tell the difference reports noise and
/// trains readers to ignore it.
fn cited_make_targets(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for span in body.split('`').skip(1).step_by(2) {
        let span = span.trim();
        let Some(rest) = span.strip_prefix("make ") else {
            continue;
        };
        let target: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !target.is_empty() {
            out.insert(target);
        }
    }
    out
}

/// `@blackcatinformatics/...` package names cited in prose.
fn cited_package_names(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    const SCOPE: &str = "@blackcatinformatics/";
    for (idx, _) in body.match_indices(SCOPE) {
        let rest = &body[idx + SCOPE.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
            .collect();
        // `.` is admitted because npm names may contain it — but a name never ENDS in one,
        // while a sentence citing a package routinely does ("…published as
        // @blackcatinformatics/gmeow-console."). Without this trim, correct prose minted a
        // package name no registry has and failed the gate.
        let name = name.trim_end_matches('.');
        if !name.is_empty() {
            out.insert(format!("{SCOPE}{name}"));
        }
    }
    out
}

/// Every `name =` in the repo's `package.json` files, plus the GTS transport packages that
/// are published from their own repositories rather than from this tree.
fn known_package_names(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    // Published from sibling repositories, not from this tree.
    out.insert("@blackcatinformatics/gmeow-gts".to_string());
    out.insert("@blackcatinformatics/purrdf".to_string());
    for dir in [
        "crates/validate-wasm/js",
        "crates/reason-wasm/js",
        "crates/gmn-wasm/js",
        "crates/mcp-wasm/js",
        "crates/mcp-core-wasm/js",
        "crates/docs/assets/console",
    ] {
        let manifest = root.join(dir).join("package.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        if let Some(idx) = text.find("\"name\"") {
            let rest = &text[idx..];
            if let Some(open) = rest.find(':') {
                let after = &rest[open + 1..];
                if let Some(start) = after.find('"') {
                    let tail = &after[start + 1..];
                    if let Some(end) = tail.find('"') {
                        out.insert(tail[..end].to_string());
                    }
                }
            }
        }
    }
    out
}

#[test]
fn every_cited_path_target_and_package_resolves() {
    let root = repo_root();
    let makefile = std::fs::read_to_string(root.join("Makefile")).expect("Makefile reads");
    let packages = known_package_names(&root);

    let mut failures: Vec<String> = Vec::new();

    for doc in GATED_DOCUMENTS {
        let path = root.join(doc);
        let body = match std::fs::read_to_string(&path) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{doc}: gated document does not read: {e}"));
                continue;
            }
        };

        // A markdown link resolves relative to the DOCUMENT, not to the repo root:
        // `crates/README.md` citing `logic/src/README.md` means `crates/logic/src/...`.
        let doc_dir = path.parent().unwrap_or(&root).to_path_buf();
        for cited in cited_paths(&body) {
            if !doc_dir.join(&cited).exists() && !root.join(&cited).exists() {
                failures.push(format!("{doc}: cites path `{cited}`, which does not exist"));
            }
        }

        for target in cited_make_targets(&body) {
            // A target is declared as `name:` at the start of a line.
            let declared = makefile
                .lines()
                .any(|l| l.starts_with(&format!("{target}:")));
            if !declared {
                failures.push(format!(
                    "{doc}: cites `make {target}`, which the Makefile does not declare"
                ));
            }
        }

        for name in cited_package_names(&body) {
            if !packages.contains(&name) {
                failures.push(format!(
                    "{doc}: cites package `{name}`, which no package.json in this tree declares"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "reader-facing documentation cites things that do not exist:\n  {}",
        failures.join("\n  ")
    );
}
