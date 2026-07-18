// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary serves a pre-assembled
//! `gmeow:AuthoringPacket` straight from the embedded bundle via
//! `gmeow slice brief --from-bundle <slice>` — the real `Cli`/`Commands::Slice`
//! clap dispatch in `src/lib.rs`, checkout-free (no `slices/`, no `generated/shapes/`).
//! Drives the built binary through `assert_cmd`, exactly like `slice_quality_cli.rs`.

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

#[test]
fn slice_brief_from_bundle_json_serves_a_packet() {
    // The `lang` slice batch 15 ships with a present French grounding cell, so the
    // JSON envelope is non-vacuous and proves fr grounding survives the bundle.
    gmeow()
        .args([
            "slice",
            "brief",
            "--from-bundle",
            "lang",
            "--batch",
            "15",
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
