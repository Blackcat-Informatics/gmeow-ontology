// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for the native/wasm validation attestation.
//!
//! Consumes an existing bundle; never constructs the corpus. Parity tests remain
//! read-only, and the refreshed findings must be reviewed before acceptance.

use std::path::Path;

use sha2::{Digest, Sha256};

const COUNTER_EXAMPLE: &str =
    "slices/extensions/embedding-projection/tests/counter-examples/ce-cross-space-rejected.ttl";

fn main() -> gmeow_errors::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = std::fs::read(root.join("generated/dist/gmeow.gts"))?;
    let turtle = std::fs::read(root.join(COUNTER_EXAMPLE))?;
    let report = gmeow_validate::data_validate::run_tier1(
        &turtle,
        "turtle",
        &bundle,
        "https://blackcatinformatics.ca/gmeow/",
        COUNTER_EXAMPLE,
    )?;
    if !report.findings.iter().any(|finding| {
        finding.severity == gmeow_errors::Severity::Error
            && finding.code == "shacl.MinCountConstraintComponent"
    }) {
        return Err(std::io::Error::other(
            "the validation counter-example did not produce its required cardinality violation",
        )
        .into());
    }
    let findings = serde_json::to_string(&report)?;
    for relative in [
        "crates/validate-wasm/tests/WITNESS.validate.json",
        "crates/docs/assets/validate/WITNESS.validate.json",
    ] {
        std::fs::write(root.join(relative), findings.as_bytes())?;
        println!("refreshed {relative} ({} bytes)", findings.len());
    }
    println!(
        "bundle SHA-256: {:x}; counter-example SHA-256: {:x}",
        Sha256::digest(&bundle),
        Sha256::digest(&turtle),
    );
    Ok(())
}
