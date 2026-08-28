// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for the reviewed-translation coverage golden.

use std::path::PathBuf;

#[path = "../tests/support/reviewed_coverage.rs"]
mod reviewed_coverage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let map = reviewed_coverage::reviewed_coverage_map(&root);
    let json = serde_json::to_string_pretty(&map)?;
    let path = root.join("crates/docs/tests/fixtures/reviewed_coverage_golden.json");
    std::fs::write(&path, format!("{json}\n"))?;
    println!(
        "refreshed {} from {} live catalog(s) and {} reviewed entries",
        path.display(),
        map.len(),
        map.values().map(Vec::len).sum::<usize>()
    );
    Ok(())
}
