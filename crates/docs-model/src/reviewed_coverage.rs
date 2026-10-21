// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned reviewed translation coverage and catalog parse observations.

mod paths;
pub use paths::{golden_path, repo_root};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::i18n_compile::{
    counts_as_reviewed_coverage, expand_predicate, language_from_po, parse_po,
};

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

/// Compact producer observations consumed by coverage and parser controls.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ReviewedCoverageObservation {
    /// Every successfully parsed catalog and its entry count.
    pub parsed_catalogs: BTreeMap<String, usize>,
    /// Reviewed term/predicate identities for every slice catalog.
    pub reviewed_coverage: BTreeMap<String, Vec<String>>,
}

/// Declare all selected catalog files before recording an observation.
pub fn input_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = live_slice_pos(root)?;
    paths.extend(docs_template_pos(root)?);
    paths.push(root.join("tests/fixtures/i18n/fr.po"));
    paths.push(root.join("tests/fixtures/i18n/zh.po"));
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Parse the declared catalogs once and retain their reviewed coverage.
pub fn observe(root: &Path) -> gmeow_errors::Result<ReviewedCoverageObservation> {
    let mut observation = ReviewedCoverageObservation {
        parsed_catalogs: BTreeMap::new(),
        reviewed_coverage: BTreeMap::new(),
    };
    for path in input_files(root)? {
        let text = std::fs::read_to_string(&path)?;
        let entries = parse_po(&text, false).map_err(|error| {
            std::io::Error::other(format!("parse catalog {}: {error}", path.display()))
        })?;
        let key = relative_key(root, &path);
        observation
            .parsed_catalogs
            .insert(key.clone(), entries.len());
        if key.starts_with("slices/") {
            let language = language_from_po(&text)?.unwrap_or_else(|| stem_language(&path));
            observation
                .reviewed_coverage
                .insert(key, reviewed_keys(entries, &language).into_iter().collect());
        }
    }
    Ok(observation)
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

fn reviewed_set(text: &str, fallback_language: &str) -> gmeow_errors::Result<BTreeSet<String>> {
    let language = language_from_po(text)?.unwrap_or_else(|| fallback_language.to_string());
    let entries = parse_po(text, false)?;
    Ok(reviewed_keys(entries, &language))
}

fn reviewed_keys(entries: Vec<crate::i18n_compile::PoEntry>, language: &str) -> BTreeSet<String> {
    entries
        .into_iter()
        .filter(|entry| counts_as_reviewed_coverage(entry, language))
        .filter_map(|entry| {
            entry
                .msgctxt
                .split_once('|')
                .map(|(term, predicate)| format!("{term}|{}", expand_predicate(predicate)))
        })
        .collect()
}

/// Recompute the survivor reviewed-coverage map over every live slice PO.
pub fn reviewed_coverage_map(root: &Path) -> gmeow_errors::Result<BTreeMap<String, Vec<String>>> {
    let mut map = BTreeMap::new();
    for path in live_slice_pos(root)? {
        let text = std::fs::read_to_string(&path)?;
        let set = reviewed_set(&text, &stem_language(&path))?;
        map.insert(relative_key(root, &path), set.into_iter().collect());
    }
    Ok(map)
}

#[cfg(test)]
#[path = "reviewed_coverage.tests.rs"]
mod tests;
