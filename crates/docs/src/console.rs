// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The standalone-console producer.
//!
//! [`console_files`] is the SINGLE authority for "what the console is made of": the
//! `console/…` shell files (each a pinned `include_str!` / `include_bytes!` build input)
//! plus EVERY key of [`crate::render::interactive_asset_files`], so the console and the
//! documentation site ship the byte-identical engine the native↔wasm witness lanes prove.
//! There is no second list, and nothing is copied by hand.
//!
//! The service worker's `SHELL` array is GENERATED here, from the assembled map's own
//! keys. A hand-authored offline manifest is a second source of truth for the same set,
//! and it drifts the moment a file is added; deriving it means the two can only agree.
//! The substitution is fail-closed: a `sw.mjs` whose marker is missing panics rather than
//! shipping a service worker that caches nothing.
//!
//! Determinism is structural: the map is a [`BTreeMap`], every value is either a pinned
//! build input or a deterministic function of `exec`, and the generated `SHELL` is the
//! sorted key set. Two calls over the same `exec` are byte-identical.

use std::collections::BTreeMap;

use gmeow_docs_model::exec::ExecutableDocsData;

use crate::render::interactive_asset_files;

/// The site-relative directory the console shell is emitted under.
pub const CONSOLE_PREFIX: &str = "console/";

/// The marker in the authored `sw.mjs` that the generated `SHELL` array replaces.
///
/// Public so the acceptance assertion can prove the authored source still carries it —
/// a renamed marker would otherwise degrade silently into "the substitution found
/// nothing", which the panic below already refuses at run time.
pub const SHELL_MARKER: &str = "[\"__GMEOW_CONSOLE_SHELL__\"]";

/// One console shell file: its `console/`-relative name and its pinned bytes.
///
/// Every entry is an `include_*!` of a real file under `crates/docs/assets/console/`, so
/// the shipped console is exactly the reviewed source — never a string assembled here.
const SHELL_FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../assets/console/index.html")),
    (
        "element.mjs",
        include_bytes!("../assets/console/element.mjs"),
    ),
    (
        "engine.worker.mjs",
        include_bytes!("../assets/console/engine.worker.mjs"),
    ),
    (
        "session.mjs",
        include_bytes!("../assets/console/session.mjs"),
    ),
    (
        "examples/gallery.mjs",
        include_bytes!("../assets/console/examples/gallery.mjs"),
    ),
    (
        "manifest.webmanifest",
        include_bytes!("../assets/console/manifest.webmanifest"),
    ),
    ("README.md", include_bytes!("../assets/console/README.md")),
    (
        "smoke/package.json",
        include_bytes!("../assets/console/smoke/package.json"),
    ),
    (
        "smoke/package-lock.json",
        include_bytes!("../assets/console/smoke/package-lock.json"),
    ),
];

/// The authored service worker, whose `SHELL` array [`console_files`] rewrites.
const SW_SOURCE: &str = include_str!("../assets/console/sw.mjs");

/// The complete standalone-console tree for an exec-backed render.
///
/// Keys span two prefixes and that is deliberate: `console/…` is the shell, and `assets/…`
/// is the shared engine set the documentation site already emits — the console does not
/// carry a second copy of a 7 MB wasm image.
///
/// Empty when the render is not interactive: a console with no engine is not a smaller
/// console, it is a broken one, so none of it is emitted rather than half of it.
#[must_use]
pub fn console_files(exec: &ExecutableDocsData) -> BTreeMap<String, Vec<u8>> {
    let mut files = interactive_asset_files(exec);
    if files.is_empty() {
        return files;
    }
    for (name, bytes) in SHELL_FILES {
        files.insert(format!("{CONSOLE_PREFIX}{name}"), bytes.to_vec());
    }
    // The service worker is generated LAST, over the finished key set, and then inserted —
    // so its own path is part of the shell it caches while its bytes are still a pure
    // function of everything else.
    let mut shell: Vec<String> = files.keys().map(|key| sw_relative(key)).collect();
    shell.push(sw_relative(&format!("{CONSOLE_PREFIX}sw.mjs")));
    shell.sort();
    files.insert(
        format!("{CONSOLE_PREFIX}sw.mjs"),
        service_worker(&shell).into_bytes(),
    );
    files
}

/// A console-tree key expressed relative to `console/sw.mjs` (the worker's own URL).
///
/// `console/element.mjs` → `./element.mjs`; `assets/mcp-core/index.mjs` →
/// `../assets/mcp-core/index.mjs`. The worker resolves each against its own location, so
/// the console works at any site depth and from `file://`.
fn sw_relative(key: &str) -> String {
    match key.strip_prefix(CONSOLE_PREFIX) {
        Some(rest) => format!("./{rest}"),
        None => format!("../{key}"),
    }
}

/// The service-worker source with its `SHELL` array replaced by `shell`.
///
/// # Panics
///
/// If the authored `sw.mjs` no longer carries [`SHELL_MARKER`]. That is a build-input
/// break, not a runtime condition: shipping the unsubstituted source would cache the
/// literal string `__GMEOW_CONSOLE_SHELL__` and produce a console that appears to work
/// online and fails offline.
fn service_worker(shell: &[String]) -> String {
    assert!(
        SW_SOURCE.contains(SHELL_MARKER),
        "crates/docs/assets/console/sw.mjs no longer carries the {SHELL_MARKER} marker — the \
         generated SHELL cannot be substituted"
    );
    let rendered = shell
        .iter()
        .map(|path| format!("  {},", json_string(path)))
        .collect::<Vec<_>>()
        .join("\n");
    SW_SOURCE.replacen(SHELL_MARKER, &format!("[\n{rendered}\n]"), 1)
}

/// A JSON string literal (the only escapes a site-relative path can need).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The generated `SHELL` set of an assembled console tree, read back out of its `sw.mjs`.
///
/// Exposed so the acceptance assertion can compare the GENERATED set against the ASSEMBLED
/// key set in both directions, over the shipped bytes rather than over the producer's
/// intermediate state.
///
/// # Panics
///
/// If the tree carries no `console/sw.mjs`, or its `SHELL` array cannot be read.
#[must_use]
pub fn generated_shell(files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let sw = files
        .get(&format!("{CONSOLE_PREFIX}sw.mjs"))
        .expect("an assembled console tree carries console/sw.mjs");
    let text = String::from_utf8(sw.clone()).expect("sw.mjs is UTF-8");
    let open = text
        .find("const SHELL = [")
        .expect("sw.mjs declares const SHELL")
        + "const SHELL = [".len();
    let close = open + text[open..].find(']').expect("the SHELL array closes");
    text[open..close]
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.trim_matches('"').to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `exec` that makes a render interactive (the renderer only checks non-emptiness).
    fn interactive_exec() -> ExecutableDocsData {
        ExecutableDocsData {
            full_bundle_gts: b"gts-bundle-sentinel-bytes".to_vec(),
            conjectures_ttl: b"@prefix ex: <http://example/> .".to_vec(),
            ..Default::default()
        }
    }

    /// Determinism: the same `exec` yields byte-identical maps.
    #[test]
    fn console_files_is_deterministic() {
        let exec = interactive_exec();
        assert_eq!(console_files(&exec), console_files(&exec));
    }

    /// The console never re-cuts a shared asset: every key both maps carry is byte-equal.
    #[test]
    fn shared_asset_bytes_are_identical() {
        let exec = interactive_exec();
        let console = console_files(&exec);
        let shared = interactive_asset_files(&exec);
        assert!(!shared.is_empty(), "the fixture must be interactive");
        for (key, bytes) in &shared {
            assert_eq!(
                console.get(key),
                Some(bytes),
                "shared asset {key} differs between the console and the site"
            );
        }
    }

    /// The generated `SHELL` set EQUALS the assembled key set, in both directions.
    #[test]
    fn generated_shell_equals_the_assembled_key_set() {
        let files = console_files(&interactive_exec());
        let generated: std::collections::BTreeSet<String> =
            generated_shell(&files).into_iter().collect();
        let assembled: std::collections::BTreeSet<String> =
            files.keys().map(|key| sw_relative(key)).collect();
        assert_eq!(
            generated, assembled,
            "the generated SHELL must equal the assembled key set"
        );
        assert!(
            generated.contains("./sw.mjs"),
            "the worker caches its own script: {generated:?}"
        );
        assert!(
            generated.iter().any(|path| path.starts_with("../assets/")),
            "the out-of-scope engine assets must be pre-cached: {generated:?}"
        );
    }

    /// A non-interactive render emits NO console key at all.
    #[test]
    fn non_interactive_render_emits_no_console() {
        assert!(console_files(&ExecutableDocsData::default()).is_empty());
    }

    /// The substituted worker carries no marker residue.
    #[test]
    fn the_marker_is_fully_substituted() {
        let files = console_files(&interactive_exec());
        let sw = String::from_utf8(files[&format!("{CONSOLE_PREFIX}sw.mjs")].clone()).unwrap();
        assert!(
            !sw.contains("__GMEOW_CONSOLE_SHELL__"),
            "unsubstituted marker survived"
        );
    }
}
