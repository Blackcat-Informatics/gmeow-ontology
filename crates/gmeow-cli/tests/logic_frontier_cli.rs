// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that `gmeow logic frontier` and `gmeow logic saga` DERIVE what
//! they print.
//!
//! Most fixtures below assert NOT ONE `logic:entryLabel`. Every label those tests expect is
//! computed by the shipped `logic:Rule` set from the entry's lifecycle-axis witnesses, so a
//! passing run proves the operator surface is showing the reasoner's conclusion rather than
//! echoing an author's assertion back at them. A test that asserted the labels in its own
//! input would pass just as happily with the whole rule set deleted.
//!
//! The provenance tests at the foot of the file do the complementary job: they hand the
//! command an input that DOES assert a label — once agreeing with the rules, once flatly
//! contradicting its own axis witness, once where no rule speaks at all — and pin the three
//! different things the operator must be told in those three cases.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// The captured stdout of a successful run of the shipped binary.
fn stdout_of(args: &[&str]) -> String {
    let out = gmeow().args(args).assert().success();
    String::from_utf8(out.get_output().stdout.clone()).expect("utf-8 stdout")
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

/// The three search outcomes must stay three. A budget cut invites a bigger budget; an
/// out-of-fragment method set invites an authoring fix; only a complete run may be read
/// as exhaustive. Collapsing any pair of them sends an operator to the wrong remedy.
const METHODS: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:top a logic:DecompositionMethod ; logic:methodDecomposes e:ingest ; logic:methodYields e:ocr , e:store .
e:sub a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:extract , e:verify .
e:alt a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:quickExtract .
"#;

fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    fs::write(&p, body).expect("write fixture");
    p
}

#[test]
fn refine_returns_a_closed_roster_when_the_search_exhausts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "methods.ttl", METHODS);
    gmeow()
        .args([
            "logic",
            "refine",
            p.to_str().expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/clitest/ingest",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("CLOSED"))
        // Two methods decompose ex:ocr, so both alternatives must survive: a search that
        // returned one would be quietly picking a plan on the operator's behalf.
        .stdout(predicate::str::contains("candidates:  2"));
}

#[test]
fn refine_under_a_cut_says_the_roster_is_not_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "methods.ttl", METHODS);
    gmeow()
        .args([
            "logic",
            "refine",
            p.to_str().expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/clitest/ingest",
            "--budget",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("INCOMPLETE"))
        .stdout(predicate::str::contains("NOT closed"))
        .stdout(predicate::str::contains("CLOSED").not());
}

#[test]
fn refine_refuses_an_out_of_fragment_method_set_rather_than_reporting_a_thin_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(
        &dir,
        "cyclic.ttl",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:a a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:u .
e:b a logic:DecompositionMethod ; logic:methodDecomposes e:u ; logic:methodYields e:t .
"#,
    );
    // Exiting 0 with an empty candidate list would read as "no decomposition exists",
    // which is a different and far more comforting claim than "your method set does not
    // terminate" — so this must FAIL, and say which task closes the loop.
    gmeow()
        .args([
            "logic",
            "refine",
            p.to_str().expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/clitest/t",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "outside the declared search fragment",
        ))
        .stderr(predicate::str::contains("decomposition cycle"))
        // The remedy must be named: a budget increase cannot fix a cycle.
        .stderr(predicate::str::contains("No budget increase would help"));
}

#[test]
fn explain_prints_all_five_elements_including_dissent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(
        &dir,
        "explain.ttl",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:rollbackEntry a logic:FrontierEntry ;
    logic:entryAxisWitness logic:StepReady , logic:ApprovalNull ;
    logic:entryAction e:rollback .
e:rollback a logic:TransactionStep .
e:exp a gmeow:FrontierExplanation ;
    gmeow:explainsEntry e:rollbackEntry ;
    gmeow:explanationProof e:proofTerm ;
    gmeow:explanationEvidence e:incidentTimeline ;
    gmeow:explanationPolicy e:fastestRestore ;
    gmeow:explanationCriterion e:timeToRestore ;
    gmeow:explanationDissent e:onCallObjection .
"#,
    );
    gmeow()
        .args([
            "logic",
            "explain",
            p.to_str().expect("utf-8"),
            "--action",
            "https://blackcatinformatics.ca/gmeow/clitest/rollback",
        ])
        .assert()
        .success()
        // R3.5's five elements, each asserted by name: a report missing one silently
        // would still look complete.
        .stdout(predicate::str::contains("proof"))
        .stdout(predicate::str::contains("evidence"))
        .stdout(predicate::str::contains("policy"))
        .stdout(predicate::str::contains("criterion"))
        // Dissent is the one most easily dropped, and the one whose absence most changes
        // what an operator would decide.
        .stdout(predicate::str::contains("onCallObjection"))
        // The label is re-derived here too, so an explanation cannot disagree with the
        // frontier it explains.
        .stdout(predicate::str::contains("(derived)"));
}

// ---------------------------------------------------------------------------
// Provenance: what the command may and may not call "derived".
//
// These tests drive the SHIPPED example, not a hand-built graph, because the
// defect they pin was invisible precisely on real input: the command unioned the
// asserted EDB with the derived quads, printed the union under a heading that
// said DERIVED, and had no way left to tell the two apart. Any fixture that
// avoids asserting a label avoids the bug.
// ---------------------------------------------------------------------------

/// The shipped worked example: an OCR step blocked by an absent capability.
fn shipped_ocr_example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/work-orchestration/examples/ocr-capability-absent.ttl")
}

/// The shipped example plus the one structural link the blocked-on-capability rule needs to
/// reach it: the entry's action.
///
/// Without it no rule concludes anything about `ex:ocrStepEntry`, which is a real and
/// separate case — covered by [`frontier_marks_an_authored_label_no_rule_derives`] — but not
/// the one where a derivation and an assertion can contradict each other.
fn rule_reachable_ocr_example() -> String {
    let base = fs::read_to_string(shipped_ocr_example()).expect("shipped example is readable");
    assert!(
        base.contains("logic:entryLabel logic:FrontierBlockedCapabilityOrResource"),
        "the shipped example must ASSERT the label under audit, or this test proves nothing"
    );
    format!(
        "{base}\nex:ocrStepEntry logic:entryAction ex:ocrStep .\n\
         ex:ocrCapabilityGap logic:gapBlockedStep ex:ocrStep .\n"
    )
}

/// The line of the frontier table that carries `entry`'s label, excluding the marker lines
/// printed beneath it.
fn row_for<'a>(stdout: &'a str, entry: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.starts_with(entry))
        .unwrap_or_else(|| panic!("no frontier row for {entry} in:\n{stdout}"))
}

const OCR_ENTRY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStepEntry";

#[test]
fn frontier_prints_the_derived_label_and_flags_a_contradicting_authored_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let honest = rule_reachable_ocr_example();
    // The mutation is a direct contradiction, not a near miss: the entry's own axis witness
    // is logic:StepWaiting on a step an operational capability gap blocks, so "ready and
    // authorized" is the one thing it demonstrably is not. An operator acting on the
    // author's word here would dispatch a step that cannot run.
    let mutated = honest.replace(
        "logic:entryLabel logic:FrontierBlockedCapabilityOrResource",
        "logic:entryLabel logic:FrontierReadyAuthorized",
    );
    assert_ne!(
        honest, mutated,
        "the mutation must actually change the file"
    );

    let honest_path = write(&dir, "ocr-honest.ttl", &honest);
    let mutated_path = write(&dir, "ocr-mutated.ttl", &mutated);

    let out = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        mutated_path.to_str().expect("utf-8 path"),
    ]);
    let row = row_for(&out, OCR_ENTRY);
    // The row is the reasoner's conclusion. The author's contradicting string may appear in
    // the disagreement marker — that is the point of the marker — but never as the label.
    assert!(
        row.contains("FrontierBlockedCapabilityOrResource"),
        "the derived label must occupy the label column, got:\n{row}"
    );
    assert!(
        !row.contains("FrontierReadyAuthorized"),
        "the hand-typed label must NOT be printed as this entry's label, got:\n{row}"
    );
    assert!(
        row.contains("derived"),
        "the row must state that the label was derived, got:\n{row}"
    );
    assert!(
        out.contains("DISAGREEMENT"),
        "a contradicted assertion must be reported, not silently dropped, got:\n{out}"
    );
    assert!(
        out.contains("FrontierReadyAuthorized"),
        "the disagreement must NAME the stale value, or an author cannot find and fix it, \
         got:\n{out}"
    );

    // The unmutated example: the rules agree with the author, so there is nothing to warn
    // about — and a command that cried disagreement here would be as useless as one that
    // never did.
    let clean = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        honest_path.to_str().expect("utf-8 path"),
    ]);
    assert!(
        !clean.contains("DISAGREEMENT"),
        "an example whose asserted label the rules reproduce must report no disagreement, \
         got:\n{clean}"
    );
    let clean_row = row_for(&clean, OCR_ENTRY);
    assert!(
        clean_row.contains("FrontierBlockedCapabilityOrResource") && clean_row.contains("derived"),
        "the agreed label must still be reported as DERIVED, not merely echoed, got:\n\
         {clean_row}"
    );
    // Agreement is itself information: it says the author wrote the label AND the reasoner
    // reproduced it, which is a stronger statement than either alone.
    assert!(
        clean_row.contains("input agrees"),
        "an asserted label the rules reproduce must be reported as such, got:\n{clean_row}"
    );
}

#[test]
fn why_not_never_stamps_a_hand_typed_label_as_derived() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mutated = rule_reachable_ocr_example().replace(
        "logic:entryLabel logic:FrontierBlockedCapabilityOrResource",
        "logic:entryLabel logic:FrontierReadyAuthorized",
    );
    let path = write(&dir, "ocr-mutated.ttl", &mutated);

    let out = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        path.to_str().expect("utf-8 path"),
        "--why-not",
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStep",
    ]);
    assert!(
        out.contains("label:   FrontierBlockedCapabilityOrResource   (derived)"),
        "the derived label must be the one stamped (derived), got:\n{out}"
    );
    // The original defect in one assertion: `(derived)` appended to a string an author typed.
    assert!(
        !out.contains("FrontierReadyAuthorized   (derived)"),
        "an EDB-read value must never be stamped (derived), got:\n{out}"
    );
    assert!(
        out.contains("DISAGREEMENT"),
        "the single-action view owes the same warning as the table, got:\n{out}"
    );
}

#[test]
fn frontier_marks_an_authored_label_no_rule_derives() {
    // The shipped example EXACTLY as it ships. Its saturation-witness entry carries an
    // authored logic:FrontierCompleted and no axis witness at all, so no rule reaches it.
    // That must read as an unverified assertion rather than as a conclusion: the difference
    // between "the reasoner concluded the run is complete" and "somebody typed complete" is
    // the difference between trusting the frontier and auditing it.
    let out = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        shipped_ocr_example().to_str().expect("utf-8 path"),
    ]);
    let row = row_for(
        &out,
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/\
         run77Saturation",
    );
    assert!(
        row.contains("FrontierCompleted"),
        "the authored label must still be shown — hiding it would be its own dishonesty, \
         got:\n{row}"
    );
    assert!(
        row.contains("ASSERTED-UNCHECKED"),
        "an authored label no rule derives must be marked unchecked, got:\n{row}"
    );
    assert!(
        !row.contains("derived"),
        "nothing derived this label, so the row must not say derived, got:\n{row}"
    );
    assert!(
        out.contains("UNCHECKED     no shipped logic:Rule derives a label for this entry"),
        "the marker must say WHY the value is unchecked, got:\n{out}"
    );
}
