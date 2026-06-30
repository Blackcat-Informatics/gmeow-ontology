// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS-codec-hygiene gate — the boundary lock for the native RDF codec seam.
//!
//! The whole RDF codec seam is now native. `crates/rdf/src/native_codecs/` parses and
//! serializes RDF on the first-party IR with NO `gmeow_gts` codec and NO oxigraph in the
//! middle, and the JSON-LD / YAML-LD surfaces no longer call gmeow-gts codecs. The ONLY
//! legitimate remaining `gmeow_gts::` use is gmeow.gts CONTAINER I/O — the file/structural
//! seam (reader / writer / model / verify / policy / codec::CodecError / ulid / …). This
//! gate is a STRUCTURAL source scan, mirroring the carrier-purity gate's shape (production
//! region + line-comment stripping + an allow-list + negative-arm self-tests so the
//! detector can never silently pass), that LOCKS that boundary in three rules:
//!
//!   RULE 1 — the codec seam is TOTALLY clean: no `gmeow_gts` token AND no oxigraph-family
//!            token anywhere (production OR test) under `crates/rdf/src/native_codecs/`.
//!
//!   RULE 2 — RDF-codec ENTRYPOINTS are banned in PRODUCTION across all `crates/*/src`:
//!            the `gmeow_gts::` codec call surfaces (nquads / trig / yamlld / rdf_xml /
//!            rdf_codecs / rdf:: dataset model). gmeow.gts CONTAINER symbols (reader,
//!            writer, model, verify, policy, wire, db, ulid, codec::, openpgp, examples)
//!            are EXPLICITLY allowed and must not be flagged.
//!
//!   RULE 3 — oxigraph-family tokens are banned in PRODUCTION across all `crates/*/src`
//!            (oxigraph is removed from the workspace; this keeps it out at the SOURCE
//!            level, complementing the crate-dependency `rdf-core-hygiene` gate).
//!
//! A violation of any rule turns this test red. The `tests` module proves the detector is
//! not vacuous: synthetic sources carrying a forbidden token must be flagged, and an
//! allowed container token must NOT be.

use std::path::{Path, PathBuf};

/// The oxigraph crate family. None of these may appear in production source anywhere in
/// `crates/*/src`, and none may appear ANYWHERE under `native_codecs/`.
const OXIGRAPH_TOKENS: [&str; 11] = [
    "oxigraph",
    "oxrdf",
    "oxsdatatypes",
    "oxiri",
    "spargebra",
    "spareval",
    "sparopt",
    "sparesults",
    "oxttl",
    "oxrdfio",
    "oxrdfxml",
];
// NOTE: `oxjsonld` is a substring of nothing else and is the JSON-LD oxigraph codec; it is
// caught by the `oxrdfio`/`oxjsonld` family. Keep it explicit:
const OXJSONLD_TOKEN: &str = "oxjsonld";

/// The `gmeow_gts::` RDF-codec ENTRYPOINTS banned in production. These are the codec call
/// surfaces (text RDF serialize/parse + the gmeow-gts RDF-dataset model/adapters). Each is
/// a `gmeow_gts::`-qualified path prefix, so a CONTAINER symbol like `gmeow_gts::reader`
/// can never match one.
const FORBIDDEN_GTS_CODEC_PATHS: [&str; 9] = [
    "gmeow_gts::rdf_codecs::",
    "gmeow_gts::nquads::",
    "gmeow_gts::from_nquads",
    "gmeow_gts::trig::",
    "gmeow_gts::from_trig",
    "gmeow_gts::yamlld",
    "gmeow_gts::from_yamlld",
    "gmeow_gts::rdf_xml",
    "gmeow_gts::rdf::",
];

/// gmeow.gts CONTAINER symbols that are ALLOWED in production — the file/structural seam.
/// These must NEVER be flagged by RULE 2. Listed so a reviewer sees exactly what stays.
const ALLOWED_GTS_CONTAINER_PREFIXES: [&str; 12] = [
    "gmeow_gts::reader",
    "gmeow_gts::writer",
    "gmeow_gts::model",
    "gmeow_gts::verify",
    "gmeow_gts::policy",
    "gmeow_gts::wire",
    "gmeow_gts::db",
    "gmeow_gts::ulid",
    "gmeow_gts::codec::",
    "gmeow_gts::openpgp",
    "gmeow_gts::examples",
    "gmeow_gts::Error", // CodecError et al. re-exported at the crate root are container errors
];

/// The workspace root: walk up from this crate's manifest dir until a `crates/` directory
/// is found alongside a `Cargo.toml`. The test's CWD/`CARGO_MANIFEST_DIR` is the crate dir
/// (`crates/rdf`), so we ascend to the directory that CONTAINS `crates/`.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("crates").is_dir() && dir.join("Cargo.toml").is_file() {
            return dir;
        }
        if !dir.pop() {
            panic!("gts-codec-hygiene: could not locate the workspace root (no ancestor with a `crates/` dir)");
        }
    }
}

/// Every `.rs` file under each `crates/*/src` directory, as `(crate-relative label, path)`.
/// The label is workspace-root-relative for legible violation messages.
fn crate_src_rust_files(root: &Path) -> Vec<(String, PathBuf)> {
    let crates_dir = root.join("crates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).unwrap_or_else(|e| {
        panic!(
            "gts-codec-hygiene: cannot read {}: {e}",
            crates_dir.display()
        )
    }) {
        let entry = entry.expect("dir entry");
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rust_files(&src, root, &mut out);
        }
    }
    out.sort();
    out
}

/// Every `.rs` file under a directory tree, recursively.
fn rust_files_under(dir: &Path, root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    collect_rust_files(dir, root, &mut out);
    out.sort();
    out
}

fn collect_rust_files(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("gts-codec-hygiene: cannot read {}: {e}", dir.display()))
    {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, root, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let label = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((label, path));
        }
    }
}

/// The PRODUCTION region of a Rust source: everything before the first top-level
/// `#[cfg(test)]` attribute (the in-crate test modules, which legitimately build oracles).
fn production_region(source: &str) -> &str {
    match source.find("\n#[cfg(test)]") {
        Some(idx) => &source[..idx],
        None => source,
    }
}

/// Strip Rust line-comments so a doc-comment NAMING a forbidden token is not a false
/// positive (the carrier-purity gate's idiom; block comments are not used for these
/// mentions, so line-stripping suffices).
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// All oxigraph-family tokens (including `oxjsonld`).
fn oxigraph_tokens() -> Vec<&'static str> {
    let mut v = OXIGRAPH_TOKENS.to_vec();
    v.push(OXJSONLD_TOKEN);
    v
}

/// True if `line` REFERENCES the oxigraph-family crate `token` as code — i.e. the token
/// appears as a whole identifier (word boundaries on both sides) used as a path/import:
/// `token::…`, `use token…`, or `extern crate token`. This deliberately does NOT match:
///   * a substring of a larger identifier (e.g. the local `insert_oxiri` fn — the `r` after
///     `oxiri` is an identifier char, so there is no trailing word boundary), and
///   * a bare mention in PROSE inside a string literal (e.g. `"the oxigraph/PyO3 adapter"`
///     or `"decodable via the oxigraph path"` — neither is followed by `::` and neither is
///     a `use`/`extern crate` import), which is documentation of the architecture, not a
///     dependency. Real oxigraph CODE always reaches the crate through a `::` path segment
///     or an import, so this captures every genuine usage while staying free of false
///     positives. (The crate-DEP `rdf-core-hygiene` gate independently bans the manifest
///     dependency; this is the complementary source-level lock.)
fn references_oxigraph_crate(line: &str, token: &str) -> bool {
    let is_ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let bytes = line.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = line[from..].find(token) {
        let start = from + rel;
        let end = start + token.len();
        // Left word boundary: preceding char must not be an identifier char.
        let left_ok = start == 0 || !is_ident_char(line[..start].chars().next_back().unwrap());
        // Right word boundary: following char must not be an identifier char.
        let after = &line[end..];
        let right_ok = after.chars().next().is_none_or(|c| !is_ident_char(c));
        if left_ok && right_ok {
            // A whole-identifier match. Treat it as a CODE reference only when it is a
            // path segment (`token::`) or an import (`use token` / `extern crate token`).
            let trimmed = line.trim_start();
            let is_path = after.starts_with("::");
            let is_use = trimmed.starts_with(&format!("use {token}"))
                || trimmed.contains(&format!("use {token}::"));
            let is_extern = trimmed.starts_with(&format!("extern crate {token}"));
            if is_path || is_use || is_extern {
                return true;
            }
        }
        from = end.max(start + 1);
        if from >= bytes.len() {
            break;
        }
    }
    false
}

// ---------------------------------------------------------------------------------------
// RULE 1 — native_codecs is totally clean (no gmeow_gts, no oxigraph, prod OR test).
// ---------------------------------------------------------------------------------------

/// Scan the WHOLE source (production AND test) for any `gmeow_gts` token or any
/// oxigraph-family token. Returns `(token, line snippet)` violations.
fn scan_native_codecs_violations(label: &str, source: &str) -> Vec<(String, String)> {
    let code = strip_line_comments(source);
    let mut violations = Vec::new();
    for (lineno, line) in code.lines().enumerate() {
        if line.contains("gmeow_gts") {
            violations.push((
                "gmeow_gts".to_string(),
                format!("{label}:{} | {}", lineno + 1, line.trim()),
            ));
        }
        for token in oxigraph_tokens() {
            if references_oxigraph_crate(line, token) {
                violations.push((
                    token.to_string(),
                    format!("{label}:{} | {}", lineno + 1, line.trim()),
                ));
            }
        }
    }
    violations
}

// ---------------------------------------------------------------------------------------
// RULE 2 — gmeow_gts RDF-codec entrypoints banned in PRODUCTION (container symbols allowed).
// ---------------------------------------------------------------------------------------

/// Scan a module's PRODUCTION region for a banned `gmeow_gts::` codec entrypoint. Container
/// symbols never match (the forbidden list is the codec paths only); the allow-list is
/// asserted not-flagged by the negative-arm self-test.
fn scan_gts_codec_entrypoints(label: &str, source: &str) -> Vec<(String, String)> {
    let prod = production_region(source);
    let code = strip_line_comments(prod);
    let mut violations = Vec::new();
    for (lineno, line) in code.lines().enumerate() {
        for token in FORBIDDEN_GTS_CODEC_PATHS {
            if line.contains(token) {
                violations.push((
                    token.to_string(),
                    format!("{label}:{} | {}", lineno + 1, line.trim()),
                ));
            }
        }
    }
    violations
}

// ---------------------------------------------------------------------------------------
// RULE 3 — oxigraph-family tokens banned in PRODUCTION across all crates/*/src.
// ---------------------------------------------------------------------------------------

fn scan_oxigraph_production(label: &str, source: &str) -> Vec<(String, String)> {
    let prod = production_region(source);
    let code = strip_line_comments(prod);
    let mut violations = Vec::new();
    for (lineno, line) in code.lines().enumerate() {
        for token in oxigraph_tokens() {
            if references_oxigraph_crate(line, token) {
                violations.push((
                    token.to_string(),
                    format!("{label}:{} | {}", lineno + 1, line.trim()),
                ));
            }
        }
    }
    violations
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("gts-codec-hygiene: cannot read {}: {e}", path.display()))
}

#[test]
fn native_codecs_seam_is_totally_clean() {
    let root = workspace_root();
    let native_codecs = root
        .join("crates")
        .join("rdf")
        .join("src")
        .join("native_codecs");
    assert!(
        native_codecs.is_dir(),
        "gts-codec-hygiene: expected the native codec seam at {}",
        native_codecs.display()
    );
    let mut all: Vec<(String, String)> = Vec::new();
    for (label, path) in rust_files_under(&native_codecs, &root) {
        all.extend(scan_native_codecs_violations(&label, &read(&path)));
    }
    assert!(
        all.is_empty(),
        "RULE 1 FAILED: the native RDF codec seam (crates/rdf/src/native_codecs/) must be \
         100% free of gmeow_gts AND oxigraph-family tokens (production OR test). The codec \
         operates on the first-party IR with NO gmeow-gts codec and NO oxigraph in the \
         middle. Violations:\n{}",
        render(&all)
    );
}

#[test]
fn no_gts_rdf_codec_entrypoint_in_production() {
    let root = workspace_root();
    let mut all: Vec<(String, String)> = Vec::new();
    for (label, path) in crate_src_rust_files(&root) {
        all.extend(scan_gts_codec_entrypoints(&label, &read(&path)));
    }
    assert!(
        all.is_empty(),
        "RULE 2 FAILED: a gmeow_gts RDF-codec entrypoint appears in PRODUCTION source. The RDF \
         codec seam is native (crates/rdf/src/native_codecs/); gmeow_gts is for gmeow.gts \
         CONTAINER I/O ONLY (reader/writer/model/verify/policy/codec::/ulid/…). Route RDF \
         text/dataset I/O through gmeow_rdf instead. Violations:\n{}",
        render(&all)
    );
}

#[test]
fn no_oxigraph_in_production_source() {
    let root = workspace_root();
    let mut all: Vec<(String, String)> = Vec::new();
    for (label, path) in crate_src_rust_files(&root) {
        all.extend(scan_oxigraph_production(&label, &read(&path)));
    }
    assert!(
        all.is_empty(),
        "RULE 3 FAILED: an oxigraph-family token appears in PRODUCTION source. Oxigraph is \
         removed from the workspace; RDF semantics are native (gmeow_rdf / gmeow_rdf_core). \
         Violations:\n{}",
        render(&all)
    );
}

fn render(violations: &[(String, String)]) -> String {
    violations
        .iter()
        .map(|(tok, loc)| format!("  - `{tok}` at {loc}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RULE 1 negative arm -----------------------------------------------------------

    #[test]
    fn rule1_flags_gmeow_gts_anywhere_in_native_codecs() {
        let with_codec = r#"
fn serialize(d: &RdfDataset) -> String {
    gmeow_gts::nquads::to_nquads(&gts_graph_from(d))
}
"#;
        let v = scan_native_codecs_violations("crates/rdf/src/native_codecs/x.rs", with_codec);
        assert!(
            v.iter().any(|(t, _)| t == "gmeow_gts"),
            "RULE 1 detector must flag any gmeow_gts token in native_codecs, got {v:?}"
        );
    }

    #[test]
    fn rule1_flags_oxigraph_even_in_test_region() {
        // RULE 1 scans the WHOLE file (prod AND test), so an oxigraph oracle in a
        // native_codecs test must ALSO be flagged — the seam is totally clean.
        let with_test_oracle = r#"
fn parse(b: &[u8]) -> RdfDataset { native(b) }

#[cfg(test)]
mod tests {
    use oxigraph::store::Store;
    fn oracle() { let _ = Store::new(); }
}
"#;
        let v =
            scan_native_codecs_violations("crates/rdf/src/native_codecs/x.rs", with_test_oracle);
        assert!(
            v.iter().any(|(t, _)| t == "oxigraph"),
            "RULE 1 detector must flag oxigraph in the TEST region of native_codecs too, got {v:?}"
        );
    }

    #[test]
    fn rule1_clean_native_source_is_not_flagged() {
        let clean = r#"
use crate::native_codecs::ser_model::SerGraph;
fn serialize(g: &SerGraph) -> String { native_nquads(g) }
"#;
        let v = scan_native_codecs_violations("crates/rdf/src/native_codecs/x.rs", clean);
        assert!(
            v.is_empty(),
            "clean native source must not be flagged, got {v:?}"
        );
    }

    // --- RULE 2 negative + allow-list arms ---------------------------------------------

    #[test]
    fn rule2_flags_each_forbidden_codec_entrypoint() {
        for token in FORBIDDEN_GTS_CODEC_PATHS {
            let src = format!("fn f() {{ let _ = {token}foo(); }}\n");
            let v = scan_gts_codec_entrypoints("crates/x/src/y.rs", &src);
            assert!(
                v.iter().any(|(t, _)| t == token),
                "RULE 2 detector must flag the codec entrypoint `{token}`, got {v:?}"
            );
        }
    }

    #[test]
    fn rule2_allows_every_container_symbol() {
        // The allow-listed gmeow.gts CONTAINER symbols are the legitimate file/structural
        // seam and MUST NOT be flagged by RULE 2.
        for prefix in ALLOWED_GTS_CONTAINER_PREFIXES {
            let src = format!("use {prefix};\nfn f() {{ let _ = {prefix}::thing(); }}\n");
            let v = scan_gts_codec_entrypoints("crates/x/src/y.rs", &src);
            assert!(
                v.is_empty(),
                "RULE 2 must NOT flag the allowed container symbol `{prefix}`, got {v:?}"
            );
        }
    }

    #[test]
    fn rule2_excludes_the_cfg_test_region() {
        // A codec call in a #[cfg(test)] oracle is PRODUCTION-exempt under RULE 2 (the test
        // surfaces legitimately re-render to assert byte-isomorphism). Only production code
        // is scanned by RULE 2.
        let with_test_codec = r#"
fn f() { native(); }

#[cfg(test)]
mod tests {
    #[test]
    fn oracle() { let nq = gmeow_gts::nquads::to_nquads(&g); }
}
"#;
        let v = scan_gts_codec_entrypoints("crates/x/src/y.rs", with_test_codec);
        assert!(
            v.is_empty(),
            "RULE 2 must exclude the #[cfg(test)] region, got {v:?}"
        );
    }

    #[test]
    fn rule2_doc_comment_mention_is_not_flagged() {
        let doc = r#"
/// The native inverse of the old `gmeow_gts::nquads::to_nquads` + re-parse round-trip.
fn f() { native(); }
"#;
        let v = scan_gts_codec_entrypoints("crates/x/src/y.rs", doc);
        assert!(
            v.is_empty(),
            "RULE 2 must not flag a doc-comment mention of the retired codec path, got {v:?}"
        );
    }

    // --- RULE 3 negative arm -----------------------------------------------------------

    #[test]
    fn rule3_flags_oxigraph_family_in_production() {
        for token in oxigraph_tokens() {
            let src = format!("use {token}::thing;\nfn f() {{ let _ = {token}_call(); }}\n");
            let v = scan_oxigraph_production("crates/x/src/y.rs", &src);
            assert!(
                v.iter().any(|(t, _)| t == token),
                "RULE 3 detector must flag the oxigraph-family token `{token}`, got {v:?}"
            );
        }
    }

    #[test]
    fn rule3_does_not_flag_prose_or_identifier_substrings() {
        // The token is precise — a CODE reference (`::` path / `use` / `extern crate`), not
        // a substring of a larger identifier nor a bare mention in string-literal prose.
        let benign = r#"
fn insert_oxiri(node: &NamedNode, out: &mut Set) { out.insert(node); }
fn f() {
    insert_oxiri(&n, &mut out);
    return Err(format!("codec is not decodable via the oxigraph path; use nquads"));
    let _ = "the oxigraph/PyO3 adapter re-exports the ring-fenced core";
}
"#;
        let v = scan_oxigraph_production("crates/x/src/y.rs", benign);
        assert!(
            v.is_empty(),
            "RULE 3 must NOT flag the local `insert_oxiri` fn nor prose mentions of oxigraph, got {v:?}"
        );
    }

    #[test]
    fn rule3_excludes_the_cfg_test_region() {
        let with_test_ox = r#"
fn f() { native(); }

#[cfg(test)]
mod tests {
    use oxigraph::store::Store;
    fn oracle() { let _ = Store::new(); }
}
"#;
        let v = scan_oxigraph_production("crates/x/src/y.rs", with_test_ox);
        assert!(
            v.is_empty(),
            "RULE 3 (production-only) must exclude the #[cfg(test)] region, got {v:?}"
        );
    }

    #[test]
    fn workspace_root_is_locatable_and_carries_the_crates_dir() {
        let root = workspace_root();
        assert!(
            root.join("crates").join("rdf").join("src").is_dir(),
            "workspace root {} must contain crates/rdf/src",
            root.display()
        );
    }
}
