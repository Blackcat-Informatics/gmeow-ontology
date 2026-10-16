// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for the reviewed-translation coverage golden.

#[path = "../tests/support/reviewed_coverage.rs"]
mod reviewed_coverage;

fn main() -> gmeow_errors::Result<()> {
    let root = reviewed_coverage::repo_root();
    let map = gmeow_docs_model::reviewed_coverage::reviewed_coverage_map(&root)?;
    let catalog_count = map.len();
    let reviewed_count = map.values().map(Vec::len).sum::<usize>();
    let mut bytes = serde_json::to_vec(&map)?;
    bytes.push(b'\n');

    let output = reviewed_coverage::golden_path();
    std::fs::write(&output, bytes)?;
    println!(
        "refreshed {} from {catalog_count} catalogs ({reviewed_count} reviewed entries)",
        output.display()
    );
    Ok(())
}
