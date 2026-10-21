// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Paths shared by the frozen-golden consumer and explicit maintainer producer.

use std::path::{Path, PathBuf};

/// Resolve the source checkout containing the documentation model crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// The existing reviewed-coverage golden consumed by documentation tests.
pub fn golden_path() -> PathBuf {
    repo_root().join("crates/docs/tests/fixtures/reviewed_coverage_golden.json")
}
