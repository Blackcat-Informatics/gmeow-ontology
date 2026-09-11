// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared source inventory for producer build admission and runtime freshness.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "embedded_logic_inputs.rs"]
mod embedded_logic_inputs;
#[path = "path_dependency_inputs.rs"]
mod path_dependency_inputs;

/// Collect the producer's runtime source closure and embedded authored inputs.
///
/// The selected root crate contributes all its implementation inputs; dependency
/// crates contribute library inputs. Include the build controller, Cargo policy,
/// and shared query inventory so build and runtime admission hash the same set.
pub fn paths(workspace: &Path, root_crate: &Path) -> BTreeSet<PathBuf> {
    let mut inputs = BTreeSet::new();
    for crate_dir in path_dependency_inputs::transitive_path_dependency_dirs(root_crate) {
        for path in path_dependency_inputs::crate_input_paths(&crate_dir) {
            if crate_dir == root_crate || is_library_input(&path, &crate_dir) {
                inputs.insert(path);
            }
        }
    }
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config",
        ".cargo/config.toml",
        "build-support/path_dependency_inputs.rs",
        "build-support/producer_inputs.rs",
        "build-support/embedded_logic_inputs.rs",
        "slices/grounding/logic/module.ttl",
        "slices/grounding/math/module.ttl",
        "crates/xtask/src/producer.rs",
        "crates/xtask/src/main.rs",
        "crates/xtask/Cargo.toml",
    ] {
        let path = workspace.join(relative);
        if path.is_file() {
            inputs.insert(path);
        }
    }
    inputs.extend(embedded_logic_inputs::verify_queries(workspace).into_values());
    inputs
}

/// Exclude dependency binaries and docs-only assets from the library input set.
fn is_library_input(path: &Path, crate_dir: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(crate_dir) else {
        return true;
    };
    if relative == Path::new("src/main.rs") || relative.starts_with("src/bin") {
        return false;
    }
    crate_dir.file_name().is_none_or(|name| name != "docs")
        || [
            "assets/console/pkg",
            "assets/console/smoke",
            "assets/console/tests",
            "assets/tests",
        ]
        .iter()
        .all(|excluded| !relative.starts_with(excluded))
}
