// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build the docs-model fixture cache once, before the gmeow-docs test processes
//! spawn. The nextest setup script runs this so no individual test pays the
//! (contended) ~12 s model build — every test then loads the warm cache. A no-op
//! when the cache is already present (content-addressed on the slice inputs).

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf();
    gmeow_docs::fixture::prime(&root);
}
