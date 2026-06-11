// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests of the `gts` binary against the frozen corpus —
//! pinning the §14.1 composition-tooling contract (refuse-don't-trust).

use std::path::PathBuf;
use std::process::{Command, Output};

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated/gts-vectors")
}

fn gts(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(args)
        .output()
        .expect("gts binary runs")
}

#[test]
fn fold_emits_nquads() {
    let v = vectors().join("01-minimal.gts");
    let out = gts(&["fold", v.to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text,
        "<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#label> \
         \"Cat\"@en .\n"
    );
}

#[test]
fn verify_flags_damage_with_exit_1() {
    let v = vectors().join("04-damaged-frame.gts");
    let out = gts(&["verify", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("DamagedFrame"), "ledger lists the diagnostic");
}

#[test]
fn cat_composes_clean_inputs_as_raw_concatenation() {
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("14-bnode-label.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(out.status.success());
    let mut raw = std::fs::read(&a).unwrap();
    raw.extend(std::fs::read(&b).unwrap());
    // §3.1: a validating composer adds checks, never transformation —
    // the output IS the byte concatenation.
    assert_eq!(out.stdout, raw);
}

#[test]
fn cat_refuses_a_damaged_input() {
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("04-damaged-frame.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("refusing"), "stderr names the refusal");
}

#[test]
fn cat_refuses_a_composition_whose_suppressions_hide_everything() {
    // 09's suppress targets its own term 0 (the Cat IRI); after the
    // value-union that hides 01-minimal's only quad too (§11) — a
    // composition that suppresses the whole graph is refused (§14.1).
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("09-suppression.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("hide every quad"), "stderr names the reason");
}

#[test]
fn cat_refuses_fewer_than_two_inputs() {
    let a = vectors().join("01-minimal.gts");
    let out = gts(&["cat", a.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
}
