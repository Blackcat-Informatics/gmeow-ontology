// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! List the exact local files capable of changing one source-built producer binary.
//!
//! This is compiled directly with `rustc` by the CI fingerprint script, before Cargo
//! cache admission. It deliberately has no package or third-party dependency.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

#[path = "path_dependency_inputs.rs"]
mod path_dependency_inputs;

fn main() {
    let root_crate = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: list_producer_inputs CRATE_DIR");
    let workspace = std::env::current_dir().expect("current workspace directory");
    let root_crate = workspace.join(root_crate);
    let mut inputs = BTreeSet::new();
    for crate_dir in path_dependency_inputs::transitive_path_dependency_dirs(&root_crate) {
        for path in path_dependency_inputs::crate_input_paths(&crate_dir) {
            if crate_dir == root_crate || is_library_input(&path, &crate_dir) {
                inputs.insert(path);
            }
        }
    }
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "Makefile",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config",
        ".cargo/config.toml",
        ".github/workflows/ci.yml",
        "build-support/path_dependency_inputs.rs",
        "build-support/list_producer_inputs.rs",
        "scripts/rust-producer-input-digest.sh",
    ] {
        let path = workspace.join(relative);
        if path.is_file() {
            inputs.insert(path);
        }
    }

    let mut stdout = std::io::stdout().lock();
    for path in inputs {
        let relative = path.strip_prefix(&workspace).unwrap_or(&path);
        stdout
            .write_all(relative.as_os_str().as_encoded_bytes())
            .expect("write producer input path");
        stdout.write_all(&[0]).expect("write NUL separator");
    }
}

fn is_library_input(path: &Path, crate_dir: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(crate_dir) else {
        return true;
    };
    if relative == Path::new("src/main.rs") || relative.starts_with("src/bin") {
        return false;
    }
    let docs_non_library_assets = [
        "assets/console/pkg",
        "assets/console/smoke",
        "assets/console/tests",
        "assets/tests",
    ];
    crate_dir.file_name().is_none_or(|name| name != "docs")
        || docs_non_library_assets
            .iter()
            .all(|excluded| !relative.starts_with(excluded))
}
