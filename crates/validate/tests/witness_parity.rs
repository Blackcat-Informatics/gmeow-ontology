// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W1 native↔wasm validation parity WITNESS (T1/F1).
//!
//! The `gmeow-validate-wasm` engine is a thin `#[wasm_bindgen]` shim over exactly this
//! `validate_json` core, so running the SAME Tier-1 validation over the SAME
//! `(counter-example, bundle)` inputs must yield byte-identical findings JSON in native
//! and in wasm. This test pins the NATIVE output to a committed content-addressed
//! **attestation** (`crates/validate-wasm/tests/WITNESS.validate.json`); the Node lane
//! (`crates/validate-wasm/js/tests/witness.test.mjs`) drives that crate's OWN built
//! `js/pkg/` and asserts the WASM output equals the same attestation. Both matching the
//! one attestation proves native ≡ wasm.
//!
//! The attestation lives WITH the engine it attests. It used to live under
//! the docs site's own asset tree, back when the site vendored a copy of this engine and
//! that directory was the only place a browser build existed. The console
//! consolidated the site onto the MCP segments and that vendored copy is gone, but
//! `gmeow-validate-wasm` remains a published npm package, so its native≡wasm evidence is
//! still load-bearing — it MOVED rather than being dropped with the asset.
//!
//! Refreshed via `GMEOW_WITNESS_BLESS=1`.

use std::path::PathBuf;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";
/// A real, authored counter-example — designed to VIOLATE, so the witness exercises
/// a non-empty findings path (with `helpUri` anchors into the constraint catalog).
const COUNTER_EXAMPLE: &str =
    "slices/extensions/embedding-projection/tests/counter-examples/ce-cross-space-rejected.ttl";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn attestation_path() -> PathBuf {
    repo_root().join("crates/validate-wasm/tests/WITNESS.validate.json")
}

#[test]
fn native_tier1_validation_matches_the_witness_attestation() {
    let root = repo_root();
    let bundle_path = root.join("generated/dist/gmeow.gts");
    let bundle = match std::fs::read(&bundle_path) {
        Ok(b) => b,
        // The bundle is a generated artifact; without it (a bare checkout that has
        // not run `make regen`) the parity witness cannot run. That is unfinished
        // work for the sync gate, not a pass — surface it loudly.
        Err(e) => panic!(
            "witness needs the generated bundle {} (run `make regen`): {e}",
            bundle_path.display()
        ),
    };
    let turtle = std::fs::read(root.join(COUNTER_EXAMPLE)).expect("read counter-example fixture");

    let findings = gmeow_validate::data_validate::validate_json(
        &turtle,
        "turtle",
        &bundle,
        NS,
        COUNTER_EXAMPLE,
    )
    .expect("native Tier-1 validation runs");

    // The counter-example must genuinely VIOLATE — a non-empty findings path with
    // the expected SHACL constraint code — otherwise the witness proves nothing. (The
    // helpUri catalog anchor is derived from the finding `code` client-side, not
    // carried in the validator's report.)
    assert!(
        findings.contains("\"severity\":\"error\""),
        "the counter-example must produce an error finding: {findings}"
    );
    assert!(
        findings.contains("shacl.MinCountConstraintComponent"),
        "the counter-example must produce the expected SHACL violation code: {findings}"
    );

    let path = attestation_path();
    // Require the EXACT documented value: `GMEOW_WITNESS_BLESS=0` or an empty value must
    // NOT overwrite the committed parity attestation (an accidental export would otherwise
    // silently replace the witness).
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, findings.as_bytes()).expect("write witness attestation");
        eprintln!("blessed witness attestation at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "witness attestation {} missing (run `GMEOW_WITNESS_BLESS=1 cargo test -p gmeow-validate --test witness_parity`): {e}",
            path.display()
        )
    });
    assert_eq!(
        findings, committed,
        "native Tier-1 findings drifted from the committed witness attestation — \
         re-bless with GMEOW_WITNESS_BLESS=1 after confirming the wasm Node lane still agrees"
    );
}
