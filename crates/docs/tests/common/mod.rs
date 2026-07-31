// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared, once-per-run fixture for the gmeow-docs integration tests.
//!
//! The cache machinery lives in [`gmeow_docs::fixture`]; this module only pins
//! the repo root (via the crate manifest dir) and exposes the loaders under the
//! `common::cached_model()` / `common::cached_site()` / `common::cached_book()`
//! names the binaries call.
//! The cache is primed once before the test processes spawn by the
//! `prime-docs-fixture` example, which the Makefile test lanes and the CI test
//! job run immediately before `cargo nextest`, so no test pays the ~12 s model
//! build or the site render; on a plain `cargo test` (no prime step) the first
//! caller builds and caches it.

#![allow(dead_code)] // not every binary uses every helper

use std::path::PathBuf;

use gmeow_docs::DocsModel;
use gmeow_docs::render::Site;

/// The repository root, derived from this crate's manifest dir (`<repo>/crates/docs`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf()
}

/// The live documentation model, loaded from the shared once-per-run cache.
pub fn cached_model() -> DocsModel {
    gmeow_docs::fixture::load(&repo_root())
}

/// The rendered English static site, loaded from the shared once-per-run cache.
/// The canonical carrier render (`render_site` ≡ `render_site_lang(_, "english")`);
/// tests that only need the site (not the live render path) load it from here so
/// the suite pays the full render once, not once per process.
pub fn cached_site() -> Site {
    gmeow_docs::fixture::load_site(&repo_root())
}

/// The rendered static site for `lang`, loaded from the shared once-per-run cache.
/// The English carrier and every translation (`fr`, `zh`, …) are cached
/// symmetrically by `prime`, so a per-language round-trip test reads its tree from
/// here instead of paying a live `render_site_lang` walk.
pub fn cached_site_lang(lang: &str) -> Site {
    gmeow_docs::fixture::load_site_lang(&repo_root(), lang)
}

/// The default mdBook render (`render_book(&model, &ExecutableDocsData::default())`),
/// loaded from the shared once-per-run cache. `mdbook_render` tests that render the
/// default book read it from here so the suite pays the full book render once, not
/// once per process. Tests that mutate the model or pass custom executable data
/// still call `render_book` directly.
pub fn cached_book() -> Site {
    gmeow_docs::fixture::load_book(&repo_root())
}

// ── shipped-document path resolution ────────────────────────────────────────

/// The file extensions that make a backticked token a PATH rather than prose.
///
/// A CLOSED list, deliberately. The alternative — "anything containing a dot" — reads
/// `example.org`, `cache.addAll`, `inputSchema.required` and `mcp.segment-not-loaded` as
/// file names, and a gate that reports prose as a missing file is a gate somebody turns off.
const PATH_EXTENSIONS: &[&str] = &[
    "blake3",
    "css",
    "gts",
    "html",
    "js",
    "json",
    "license",
    "md",
    "mjs",
    "png",
    "rs",
    "svg",
    "toml",
    "ts",
    "ttl",
    "txt",
    "wasm",
    "webmanifest",
];

/// Every backticked token in `markdown` that names a path, in document order.
///
/// Fenced blocks are skipped: a shell transcript names commands and scratch directories,
/// not members of the distribution. Inside prose, a token qualifies when it is free of the
/// punctuation that marks it as code or a specifier (`logic:ActionSchema`,
/// `configure({ assetBase })`, `@blackcatinformatics/…`, `role=alert`) AND either ends in
/// `/` or carries one of [`PATH_EXTENSIONS`] with a non-empty stem — which is what keeps
/// `.gts` (a format, named by its suffix) out and `gmeow.gts` in.
#[must_use]
pub fn readme_paths(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        for token in line.split('`').skip(1).step_by(2) {
            if let Some(path) = as_path(token) {
                out.push(path);
            }
        }
    }
    out
}

/// `token` as a distribution-relative path, or `None` when it is prose.
fn as_path(token: &str) -> Option<String> {
    if token.is_empty()
        || token.starts_with('@')
        || token.starts_with('#')
        || token.starts_with('$')
        || token.starts_with('-')
        || token.contains([
            ' ', '<', '>', '(', ')', '{', '}', '"', '=', ',', ';', ':', '*', '\\',
        ])
    {
        return None;
    }
    let path = token.strip_prefix("./").unwrap_or(token);
    if path.ends_with('/') {
        return Some(path.to_string());
    }
    let name = path.rsplit('/').next()?;
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && PATH_EXTENSIONS.contains(&extension)).then(|| path.to_string())
}

/// Every path a shipped README names that its own distribution cannot answer for, as
/// reportable messages.
///
/// Three kinds of reference, each checked in ITS OWN direction, so the answer is never
/// "somebody said it was fine":
///
/// * a `crates/…` path is a REPOSITORY reference (`crates/docs/src/console.rs`) and must
///   exist on disk;
/// * a path listed in `elsewhere` is named precisely because ANOTHER distribution carries it
///   — the npm README's "it does not ship `sw.mjs`" — so it must be ABSENT here; a listed
///   path that turns up in the distribution is reported too, because the sentence around it
///   has then become false;
/// * every other path must be a key of `distribution`, either verbatim or under `prefix`
///   (the deployed README sits at `console/README.md` and names both `element.mjs`, its own
///   neighbour, and `assets/gmeow.gts`, a tree-root path).
///
/// `distribution` is the file set the distribution actually ships. A trailing `/` names a
/// directory and is satisfied by any member under it.
#[must_use]
pub fn unresolved_readme_paths(
    markdown: &str,
    distribution: &std::collections::BTreeSet<String>,
    prefix: &str,
    elsewhere: &[&str],
) -> Vec<String> {
    let carried = |path: &str| -> bool {
        let local = format!("{prefix}{path}");
        if path.ends_with('/') {
            distribution
                .iter()
                .any(|key| key.starts_with(path) || key.starts_with(&local))
        } else {
            distribution.contains(path) || distribution.contains(&local)
        }
    };
    let mut problems = Vec::new();
    for path in readme_paths(markdown) {
        if let Some(repository) = path.strip_prefix("crates/") {
            let on_disk = repo_root().join("crates").join(repository);
            if !on_disk.exists() {
                problems.push(format!(
                    "`{path}` is named as a repository path and does not exist"
                ));
            }
        } else if elsewhere.contains(&path.as_str()) {
            if carried(&path) {
                problems.push(format!(
                    "`{path}` is declared as belonging to another distribution, but this one \
                     ships it"
                ));
            }
        } else if !carried(&path) {
            problems.push(format!(
                "`{path}` is named as a member of this distribution, which does not carry it"
            ));
        }
    }
    problems.sort();
    problems.dedup();
    problems
}
