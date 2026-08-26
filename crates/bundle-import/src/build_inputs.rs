// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Derived local dependency closure for the bundle-import producer fingerprint.
//!
//! This module is compiled by `build.rs` and by the library's tests. Keeping the
//! traversal in one file makes the live test exercise the exact authority that
//! selects build-fingerprint inputs.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

const CRATE_INPUT_SUBPATHS: [&str; 5] = ["src", "assets", "templates", "Cargo.toml", "build.rs"];

/// Return the complete transitive closure of non-dev local path dependencies.
pub fn transitive_path_dependency_dirs(root_crate: &Path) -> BTreeSet<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![normalize_lexically(root_crate)];
    while let Some(crate_dir) = queue.pop() {
        let crate_dir = normalize_lexically(&crate_dir);
        if !seen.insert(crate_dir.clone()) {
            continue;
        }
        let manifest =
            std::fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap_or_else(|error| {
                panic!("read {}: {error}", crate_dir.join("Cargo.toml").display())
            });
        for dependency in manifest_path_dependencies(&manifest) {
            queue.push(crate_dir.join(dependency));
        }
    }
    seen
}

/// Return every implementation input hashed for one crate in the closure.
pub fn crate_input_paths(crate_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for relative in CRATE_INPUT_SUBPATHS {
        collect_files(&crate_dir.join(relative), &mut paths);
    }
    paths.sort();
    paths.dedup();
    paths
}

fn manifest_path_dependencies(manifest: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut production_dependencies = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            production_dependencies =
                header.ends_with("dependencies") && !header.ends_with("dev-dependencies");
            continue;
        }
        if !production_dependencies || line.starts_with('#') {
            continue;
        }
        for (offset, _) in line.match_indices("path") {
            let rest = line[offset + "path".len()..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let Some(rest) = rest.trim_start().strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else {
                continue;
            };
            paths.push(rest[..end].to_owned());
            break;
        }
    }
    paths
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        panic!(
            "build fingerprint input must not be a symlink: {}",
            path.display()
        );
    }
    if metadata.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let mut children = std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("read {} entry: {error}", path.display()))
                .path()
        })
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        collect_files(&child, out);
    }
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                assert!(
                    normalized.pop(),
                    "path dependency escapes its filesystem root: {}",
                    path.display()
                );
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
