// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored query inventory shared by the logic compiler and producer admission.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn verify_queries(workspace: &Path) -> BTreeMap<String, PathBuf> {
    let mut queries = BTreeMap::new();
    collect_queries(&workspace.join("queries/verify"), &mut queries);
    collect_slices(&workspace.join("slices"), &mut queries);
    assert!(
        !queries.is_empty(),
        "embedded verify queries must not be empty"
    );
    queries
}

fn children(directory: &Path) -> Vec<PathBuf> {
    let mut paths = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read authored query directory entry").path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn collect_queries(directory: &Path, queries: &mut BTreeMap<String, PathBuf>) {
    for path in children(directory) {
        if path.extension().is_some_and(|extension| extension == "rq") {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("UTF-8 verify query stem")
                .to_owned();
            if let Some(previous) = queries.insert(stem.clone(), path.clone()) {
                panic!(
                    "duplicate verify query {stem}: {} and {}",
                    previous.display(),
                    path.display()
                );
            }
        }
    }
}

fn collect_slices(directory: &Path, queries: &mut BTreeMap<String, PathBuf>) {
    for path in children(directory) {
        if path.is_dir() {
            let verify = path.join("queries/verify");
            if verify.is_dir() {
                collect_queries(&verify, queries);
            }
            collect_slices(&path, queries);
        }
    }
}
