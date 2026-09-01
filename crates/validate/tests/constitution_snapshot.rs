// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Retained Python↔Rust parity regression test for the constitution gate.
//!
//! The old Python ``gmeow_tools.constitution`` authority was deleted once the
//! Rust-native path reached parity. This test pins the Rust
//! ``constitution_full_report`` output for the real repository against an insta
//! golden snapshot so future toolchain changes cannot silently drift the report
//! format or the set of findings surfaced to Python consumers.

use std::path::Path;

use gmeow_errors::Report;
use gmeow_validate::constitution::constitution_full_report;

#[test]
fn constitution_full_report_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");

    let findings = constitution_full_report(&manifest, &constitution, root);

    let mut report = Report::new("constitution");
    for finding in findings {
        report.add_finding(finding);
    }

    insta::assert_json_snapshot!(report.normalized());
}
