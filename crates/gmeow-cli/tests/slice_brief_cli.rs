// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary serves a pre-assembled
//! `gmeow:AuthoringPacket` straight from the embedded bundle via
//! `gmeow slice brief --from-bundle <slice>` — the real `Cli`/`Commands::Slice`
//! clap dispatch in `src/lib.rs`, checkout-free (no `slices/`, no `generated/shapes/`).
//! Drives the built binary through `assert_cmd`, exactly like the retained focused CLI tests.

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Which `lang` batch currently carries a present French grounding cell. The
/// deterministic term-batch numbering shifts whenever lang terms are added or
/// removed, so the batch is discovered dynamically (a bare `--from-bundle lang`
/// request with no `--batch` serves EVERY packet) rather than hardcoded — this
/// keeps the test valid across future renumbering.
fn lang_batch_with_present_fr_grounding() -> u64 {
    let output = gmeow()
        .args([
            "slice",
            "brief",
            "--from-bundle",
            "lang",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON envelope");
    out["packets"]
        .as_array()
        .expect("packets array")
        .iter()
        .find(|p| {
            p["grounding"].as_array().is_some_and(|g| {
                g.iter()
                    .any(|c| c["attribute"] == "groundingFr" && c["value"].is_string())
            })
        })
        .unwrap_or_else(|| panic!("no `lang` batch carries a present French grounding cell: {out}"))
        ["batch"]
        .as_u64()
        .expect("batch is a number")
}

#[test]
fn slice_brief_from_bundle_json_serves_a_packet() {
    // The JSON envelope is non-vacuous and proves fr grounding survives the bundle
    // round-trip, on whichever batch currently carries a present fr cell.
    let batch = lang_batch_with_present_fr_grounding().to_string();
    gmeow()
        .args([
            "slice",
            "brief",
            "--from-bundle",
            "lang",
            "--batch",
            &batch,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "https://blackcatinformatics.ca/gmeow/slices/lang",
        ))
        .stdout(predicate::str::contains("\"groundingFr\""))
        .stdout(predicate::str::contains("\"packet_count\": 1"));
}

#[test]
fn slice_brief_from_bundle_turtle_emits_the_packet_body() {
    // This assertion is content-agnostic (every packet, regardless of batch
    // renumbering, carries these two predicates), so batch 14 stays a fixed probe.
    gmeow()
        .args([
            "slice",
            "brief",
            "--from-bundle",
            "lang",
            "--batch",
            "14",
            "--format",
            "turtle",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("AuthoringPacket"))
        .stdout(predicate::str::contains("packetSourceSlice"));
}

#[test]
fn slice_brief_from_bundle_unknown_slice_hard_fails() {
    gmeow()
        .args(["slice", "brief", "--from-bundle", "no-such-slice-xyz"])
        .assert()
        .failure();
}

#[test]
fn slice_brief_ambiguous_source_hard_fails() {
    // Passing BOTH a dir and --from-bundle is an explicit hard error (no silent default).
    gmeow()
        .args(["slice", "brief", "some-dir", "--from-bundle", "lang"])
        .assert()
        .failure();
}

#[test]
fn slice_brief_missing_source_hard_fails() {
    // Neither a dir nor --from-bundle: explicit hard error.
    gmeow().args(["slice", "brief"]).assert().failure();
}
