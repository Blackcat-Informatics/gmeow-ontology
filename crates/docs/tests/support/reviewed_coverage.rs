// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared computation behind the reviewed-translation golden's read-only consumer and
//! explicit maintainer producer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_docs::i18n_compile::{
    counts_as_reviewed_coverage, expand_predicate, language_from_po, parse_po,
};

/// Recursively collect every `*.po` file under `dir` whose parent directory is
/// named `i18n`.
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
            .is_some_and(|n| n.starts_with("ontology-docs-templates.") && n.ends_with(".po"))
        {
            out.push(path);
        }
    }
}

fn rel_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn stem_lang(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

fn reviewed_set(text: &str, fallback_lang: &str) -> BTreeSet<String> {
    let lang = language_from_po(text)
        .expect("reviewed-coverage language_from_po")
        .unwrap_or_else(|| fallback_lang.to_string());
    let mut set = BTreeSet::new();
    for entry in parse_po(text, false).expect("reviewed-coverage parse_po") {
        if !counts_as_reviewed_coverage(&entry, &lang) {
            continue;
        }
        if let Some((term, pred)) = entry.msgctxt.split_once('|') {
            set.insert(format!("{term}|{}", expand_predicate(pred)));
        }
    }
    set
}

/// Parse every live slice PO, documentation template PO, and test fixture through the
/// survivor parser. This is kept separate from [`reviewed_coverage_map`] because the
/// golden intentionally grades only slice-owned catalogs.
pub fn assert_all_catalogs_parse(root: &Path) {
    let mut paths = live_slice_pos(root);
    collect_docs_template_pos(root, &mut paths);
    paths.push(root.join("tests/fixtures/i18n/fr.po"));
    paths.push(root.join("tests/fixtures/i18n/zh.po"));

    for path in paths {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let parsed = parse_po(&text, false);
        assert!(
            parsed.is_ok(),
            "survivor parse_po failed for {}: {:?}",
            path.display(),
            parsed.err()
        );
    }
}

/// Recompute the sorted reviewed-coverage map over all live slice POs.
pub fn reviewed_coverage_map(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    for path in live_slice_pos(root) {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let set = reviewed_set(&text, &stem_lang(&path));
        map.insert(rel_key(root, &path), set.into_iter().collect());
    }
    map
}
