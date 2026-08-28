// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for `tests/witness/describe.nt`.

#[path = "../tests/support/explorer_describe.rs"]
mod explorer_describe;

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = explorer_describe::repo_root();
    let bundle_path = root.join("generated/dist/gmeow.gts");
    let snapshot = std::fs::read(&bundle_path)?;
    let archive =
        gmeow_bundle_view::bundle_blobs::Bundle::from_snapshot(&snapshot).map_err(|error| {
            fail(format!(
                "read {}: {}",
                bundle_path.display(),
                error.message()
            ))
        })?;
    let core = archive.dataset().map_err(|error| {
        fail(format!(
            "fold {}: {}",
            bundle_path.display(),
            error.message()
        ))
    })?;
    let witness = explorer_describe::verified_describe(&snapshot, core.as_ref())?;
    let output = explorer_describe::attestation_path();
    std::fs::write(&output, witness.rendered.as_bytes())?;
    println!(
        "refreshed {} from verified native + query_local describe of {} over {} ({} bytes)",
        output.display(),
        witness.subject,
        bundle_path.display(),
        witness.rendered.len()
    );
    Ok(())
}
