// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build the docs fixture cache (model, the rendered site for every available
//! language, and the default mdBook render) once, before the gmeow-docs test
//! processes spawn. The Makefile test lanes and the CI test job run this
//! immediately before `cargo nextest`, so no individual test pays the (contended)
//! ~12 s model build, any per-language site render, or the book render — every
//! test then loads the warm cache. A no-op when the cache is already present
//! (content-addressed on the slice inputs).

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf();
    gmeow_docs::fixture::prime(&root);
}
