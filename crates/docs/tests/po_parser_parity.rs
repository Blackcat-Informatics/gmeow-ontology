// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pre-deletion golden sequencing for the PO-parser unification.
//!
//! Two fuzzy-blind PO parsers once existed in `gmeow-docs`: the fuzzy-aware survivor
//! `i18n_compile::parse_po` and the inferior `i18n::parse_po`. Before deleting the
//! loser, a `reviewed_coverage_survivor_matches_loser` differential proved — over
//! EVERY live slice catalog — that the reviewed translation-coverage set was
//! identical whether measured through the loser or the survivor. That was the
//! executable proof that the deletion changes no measured coverage; it has served
//! its purpose and is removed now that the loser is gone (a survivor-vs-survivor
//! re-run would be a tautology).
//!
//! The durable pin is `reviewed_coverage_matches_frozen_golden`, which recomputes
//! the survivor set and asserts it against a checked-in golden.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_docs::i18n_compile::{
    counts_as_reviewed_coverage, expand_predicate, language_from_po, parse_po as parse_po_survivor,
};

/// The repo root: `crates/docs` → `../..`, canonicalized.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Recursively collect every `*.po` file under `dir` whose parent directory is
/// named `i18n` (std-only walk, no new deps).
fn collect_i18n_pos(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_i18n_pos(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("po")
            && path
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                == Some("i18n")
        {
            out.push(path);
        }
    }
}

/// Every live slice PO under `<root>/slices/**/i18n/*.po`, sorted.
fn live_slice_pos(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_i18n_pos(&root.join("slices"), &mut out);
    out.sort();
    out
}

/// Recursively collect every `ontology-docs-templates.*.po` file under `dir`,
/// skipping heavy build/VCS trees.
fn collect_docs_template_pos(dir: &Path, out: &mut Vec<PathBuf>) {
    let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if matches!(name, "target" | ".git" | "node_modules") {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_docs_template_pos(&path, out);
        } else if path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with("ontology-docs-templates.") && n.ends_with(".po"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// The slice-relative path of a live PO, as the stable golden/differential key.
fn rel_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// The file stem (`fr`, `zh`, …) used as a language fallback when a catalog has no
/// `Language:` header.
fn stem_lang(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

/// The reviewed-coverage set for `text` measured through the SURVIVOR parser: the
/// sorted `"{term}|{expanded-predicate}"` keys of every entry that
/// [`counts_as_reviewed_coverage`].
fn survivor_reviewed_set(text: &str, fallback_lang: &str) -> BTreeSet<String> {
    let lang = language_from_po(text)
        .expect("survivor language_from_po")
        .unwrap_or_else(|| fallback_lang.to_string());
    let mut set = BTreeSet::new();
    for entry in parse_po_survivor(text, false).expect("survivor parse_po") {
        if !counts_as_reviewed_coverage(&entry, &lang) {
            continue;
        }
        if let Some((term, pred)) = entry.msgctxt.split_once('|') {
            set.insert(format!("{term}|{}", expand_predicate(pred)));
        }
    }
    set
}

/// ENH-A: every live slice PO, every `ontology-docs-templates.*.po`, and the two
/// fixtures parse cleanly under the survivor.
#[test]
fn all_live_po_files_parse_under_survivor() {
    let root = repo_root();
    let mut paths = live_slice_pos(&root);
    collect_docs_template_pos(&root, &mut paths);
    paths.push(root.join("tests/fixtures/i18n/fr.po"));
    paths.push(root.join("tests/fixtures/i18n/zh.po"));

    for path in paths {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let parsed = parse_po_survivor(&text, false);
        assert!(
            parsed.is_ok(),
            "survivor parse_po failed for {}: {:?}",
            path.display(),
            parsed.err()
        );
    }
}

/// The frozen golden path (a `BTreeMap<slice-rel po path, sorted reviewed keys>`).
fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reviewed_coverage_golden.json")
}

/// Recompute the survivor reviewed-coverage map over all live slice POs.
fn survivor_golden_map(root: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map = std::collections::BTreeMap::new();
    for path in live_slice_pos(root) {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let set = survivor_reviewed_set(&text, &stem_lang(&path));
        map.insert(rel_key(root, &path), set.into_iter().collect::<Vec<_>>());
    }
    map
}

/// ENH-B pin (survives deletion): the survivor reviewed-coverage map equals the
/// checked-in golden. The test is strictly read-only.
#[test]
fn reviewed_coverage_matches_frozen_golden() {
    let root = repo_root();
    let computed = survivor_golden_map(&root);
    let path = golden_path();

    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}; refresh it through the explicit maintainer producer",
            path.display()
        )
    });
    let golden: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&text).expect("parse golden JSON");
    assert_eq!(
        golden, computed,
        "survivor reviewed-coverage drifted from the frozen golden"
    );
}
