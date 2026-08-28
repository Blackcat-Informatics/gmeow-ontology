// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared computation behind the reviewed-translation golden and its maintainer producer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_docs::i18n_compile::{
    counts_as_reviewed_coverage, expand_predicate, language_from_po, parse_po,
};

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}

/// The repository root (`crates/docs` -> `../..`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// The checked-in reviewed-coverage golden.
pub fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reviewed_coverage_golden.json")
}

/// Recursively collect every `*.po` file under `dir` whose parent is `i18n`.
fn collect_i18n_pos(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_i18n_pos(&path, out)?;
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
    Ok(())
}

/// Every live slice PO under `<root>/slices/**/i18n/*.po`, sorted.
pub fn live_slice_pos(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_i18n_pos(&root.join("slices"), &mut out)?;
    out.sort();
    Ok(out)
}

/// Collect the optional root `i18n/ontology-docs-templates.*.po` catalogs.
#[allow(dead_code)] // consumed by the test build; the producer build needs only slice catalogs
pub fn docs_template_pos(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let directory = root.join("i18n");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| {
                name.starts_with("ontology-docs-templates.") && name.ends_with(".po")
            })
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn stem_language(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

fn reviewed_set(
    text: &str,
    fallback_language: &str,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let language = language_from_po(text)
        .map_err(|error| {
            fail(format!(
                "reviewed-coverage language header failed: {}",
                error.message()
            ))
        })?
        .unwrap_or_else(|| fallback_language.to_string());
    let entries = parse_po(text, false).map_err(|error| {
        fail(format!(
            "reviewed-coverage parse failed: {}",
            error.message()
        ))
    })?;
    Ok(entries
        .into_iter()
        .filter(|entry| counts_as_reviewed_coverage(entry, &language))
        .filter_map(|entry| {
            entry
                .msgctxt
                .split_once('|')
                .map(|(term, predicate)| format!("{term}|{}", expand_predicate(predicate)))
        })
        .collect())
}

/// Recompute the survivor reviewed-coverage map over every live slice PO.
pub fn reviewed_coverage_map(
    root: &Path,
) -> Result<BTreeMap<String, Vec<String>>, Box<dyn std::error::Error>> {
    let mut map = BTreeMap::new();
    for path in live_slice_pos(root)? {
        let text = std::fs::read_to_string(&path)?;
        let set = reviewed_set(&text, &stem_language(&path))?;
        map.insert(relative_key(root, &path), set.into_iter().collect());
    }
    Ok(map)
}
