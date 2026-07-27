// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that `gmeow logic frontier` and `gmeow logic saga` DERIVE what
//! they print.
//!
//! The fixtures below assert NOT ONE `logic:entryLabel`. Every label these tests expect is
//! computed by the shipped `logic:Rule` set from the entry's lifecycle-axis witnesses, so a
//! passing run proves the operator surface is showing the reasoner's conclusion rather than
//! echoing an author's assertion back at them. A test that asserted the labels in its own
//! input would pass just as happily with the whole rule set deleted.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Four entries, four distinct axis tuples, zero asserted labels.
const FRONTIER: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:aggregate a logic:FrontierEntry ; logic:entryAxisWitness logic:StepReady , logic:ApprovalNull .
e:publish   a logic:FrontierEntry ; logic:entryAxisWitness logic:StepReady , logic:ApprovalCreated .
e:charge    a logic:FrontierEntry ; logic:entryAxisWitness logic:EffectAttempted .
e:ocr       a logic:FrontierEntry ; logic:entryAxisWitness logic:StepWaiting ; logic:entryAction e:ocrStep .
e:ocrStep   a logic:TransactionStep .
e:gap  a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:ocrStep .
e:prop a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:ocrStep ;
    logic:proposalMissingCapability e:ocrCap ;
    logic:proposalRequiredContract "Page image in, positioned text out." .
"#;

#[test]
fn frontier_labels_are_derived_not_echoed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("frontier.ttl");
    fs::write(&path, FRONTIER).expect("write fixture");

    assert!(
        !FRONTIER.contains("entryLabel"),
        "the fixture must assert no label, or this test proves nothing"
    );

    gmeow()
        .args(["logic", "frontier", path.to_str().expect("utf-8 path")])
        .assert()
        .success()
        // Ready and gated differ in exactly one axis position; both must appear, or the
        // separation of readiness from approval is not actually being computed.
        .stdout(predicate::str::contains("FrontierReadyAuthorized"))
        .stdout(predicate::str::contains("FrontierReadyApprovalRequired"))
        // The no-blind-retry law, stated positively.
        .stdout(predicate::str::contains("FrontierReconciliationRequired"))
        .stdout(predicate::str::contains(
            "FrontierBlockedCapabilityOrResource",
        ))
        // The axis tuple must be shown, not just the label: an operator asking "why" needs
        // the witness, and a label with no witness is an opinion.
        .stdout(predicate::str::contains("StepReady"))
        .stdout(predicate::str::contains("EffectAttempted"));
}

#[test]
fn why_not_surfaces_the_gap_and_its_proposal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("frontier.ttl");
    fs::write(&path, FRONTIER).expect("write fixture");

    gmeow()
        .args([
            "logic",
            "frontier",
            path.to_str().expect("utf-8 path"),
            "--why-not",
            "https://blackcatinformatics.ca/gmeow/clitest/ocrStep",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "FrontierBlockedCapabilityOrResource",
        ))
        .stdout(predicate::str::contains("(derived)"))
        .stdout(predicate::str::contains("gap:"))
        // A gap report is only actionable if it carries the remedy, so the proposal fields
        // must reach the operator rather than staying in the graph.
        .stdout(predicate::str::contains("proposalMissingCapability"))
        .stdout(predicate::str::contains(
            "Page image in, positioned text out.",
        ));
}

#[test]
fn why_not_refuses_an_action_no_entry_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("frontier.ttl");
    fs::write(&path, FRONTIER).expect("write fixture");

    // A silent empty answer here would read as "nothing blocks it", which is the opposite
    // of the truth when the action simply is not on the frontier at all.
    gmeow()
        .args([
            "logic",
            "frontier",
            path.to_str().expect("utf-8 path"),
            "--why-not",
            "https://blackcatinformatics.ca/gmeow/clitest/nosuchstep",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no frontier entry names"));
}

#[test]
fn saga_names_what_is_owed_at_an_undetermined_boundary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("saga.ttl");
    fs::write(
        &path,
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:intent   a logic:DispatchIntent .
e:attempt  a logic:EffectAttempt ; logic:attemptOfIntent e:intent .
e:unknown  a logic:ExternalOutcomeUnknown ; logic:unknownOfAttempt e:attempt .
e:intent2  a logic:DispatchIntent .
e:attempt2 a logic:EffectAttempt ; logic:attemptOfIntent e:intent2 .
e:receipt2 a logic:ExternalEffectReceipt ; logic:receiptOfAttempt e:attempt2 .
"#,
    )
    .expect("write fixture");

    gmeow()
        .args(["logic", "saga", path.to_str().expect("utf-8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("UNDETERMINED"))
        // The settled attempt must NOT be reported as owing anything, or the command would
        // send an operator to reconcile an effect that already came back.
        .stdout(predicate::str::contains("receipted"))
        // Naming the owed action is the whole point: "unknown" alone tells an operator
        // nothing about what they may safely do next.
        .stdout(predicate::str::contains("RECONCILIATION"))
        .stdout(predicate::str::contains("duplicate effect"));
}
