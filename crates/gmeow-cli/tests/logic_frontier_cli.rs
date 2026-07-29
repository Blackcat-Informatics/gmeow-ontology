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

/// The refinement outcomes must stay apart. A budget cut invites a bigger budget; an
/// out-of-fragment method set invites an authoring fix; a malformed request invites a
/// corrected one; only a complete run may be read as exhaustive. Collapsing any pair of
/// them sends an operator to the wrong remedy.
///
/// `logic:methodYields` carries ONE ordered `rdf:List`. The cells are NAMED rather than
/// blank because a decomposition that may be pinned must be addressable cell by cell.
const METHODS: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:top a logic:DecompositionMethod ; logic:methodDecomposes e:ingest ; logic:methodYields e:topCell1 .
e:topCell1 rdf:first e:ocr ; rdf:rest e:topCell2 .
e:topCell2 rdf:first e:store ; rdf:rest rdf:nil .
e:sub a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:subCell1 .
e:subCell1 rdf:first e:extract ; rdf:rest e:subCell2 .
e:subCell2 rdf:first e:verify ; rdf:rest rdf:nil .
e:alt a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:altCell1 .
e:altCell1 rdf:first e:quickExtract ; rdf:rest rdf:nil .
"#;

/// A shipped worked example of the work-orchestration slice, by file name.
///
/// Anchored on the manifest directory rather than the process CWD, so the demonstration
/// drives the SAME bytes the slice ships instead of a copy that could drift from them.
fn shipped_example(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/work-orchestration/examples")
        .join(name)
}

fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    fs::write(&p, body).expect("write fixture");
    p
}

/// The roster is DERIVED, and the derivation is visible.
///
/// Three properties, and the third is the one the retired Rust search could not have had:
///
/// 1. Both methods for `e:ocr` survive. A roster carrying one of two alternatives would be
///    quietly picking a plan on the operator's behalf while presenting the choice as
///    settled.
/// 2. `e:ocr` is marked OPEN inside the top method's step list. A roster that presented an
///    abstract task as executable work would be inviting somebody to run it.
/// 3. Every roster row names the AUTHORED RULE that concluded it and the premise it
///    concluded it from. That is what makes "why is this candidate here" answerable from
///    the graph rather than by reading an evaluator.
#[test]
fn refine_returns_a_closed_roster_when_the_derivation_settles() {
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
        // One method for e:ingest and BOTH alternatives for e:ocr.
        .stdout(predicate::str::contains("candidates:  3"))
        .stdout(predicate::str::contains("clitest/sub>"))
        .stdout(predicate::str::contains("clitest/alt>"))
        // The top method's own sequence, in the AUTHORED order.
        .stdout(predicate::str::contains(
            "steps: https://blackcatinformatics.ca/gmeow/clitest/ocr -> \
             https://blackcatinformatics.ca/gmeow/clitest/store",
        ))
        .stdout(predicate::str::contains(
            "open:  https://blackcatinformatics.ca/gmeow/clitest/ocr",
        ))
        .stdout(predicate::str::contains(
            "derived by <https://blackcatinformatics.ca/logic/ruleRefinementCandidateMethod>",
        ))
        .stdout(predicate::str::contains(
            "from <https://blackcatinformatics.ca/gmeow/clitest/top> \
             <https://blackcatinformatics.ca/logic/methodDecomposes>",
        ));
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
        .stdout(predicate::str::contains("CLOSED").not())
        // A cut leaves the session uncommitted, so there is no roster at all — never a
        // truncated one an operator could read as the roster.
        .stdout(predicate::str::contains("candidates:").not());
}

/// A task the input never mentions is a MALFORMED REQUEST, not an empty roster.
///
/// "CLOSED, 0 candidates" for a typo'd IRI is both wrong and reassuring, which is the
/// worst pair available: it tells an operator the system looked and found nothing.
#[test]
fn refine_refuses_a_task_the_input_never_mentions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "methods.ttl", METHODS);
    gmeow()
        .args([
            "logic",
            "refine",
            p.to_str().expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/clitest/nosuchtask",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid"))
        .stderr(predicate::str::contains("nothing to refine"))
        .stdout(predicate::str::contains("CLOSED").not());
}

/// A method whose ordered carrier is broken is refused, not silently shortened.
///
/// The retired reader dropped such a method on the floor, which turns a typo into a plan
/// quietly one step short — the failure mode where a system runs four fifths of a
/// procedure and reports success.
#[test]
fn refine_refuses_a_broken_yields_chain_rather_than_truncating_the_plan() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(
        &dir,
        "broken.ttl",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:c1 .
e:c1 rdf:first e:s1 ; rdf:rest e:c2 .
e:c2 rdf:first e:s2 .
"#,
    );
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
        .stderr(predicate::str::contains("malformed"))
        .stderr(predicate::str::contains("never reaches rdf:nil"));
}

/// The capability-absent scene: the refusal NAMES the missing capability, and names the
/// gap and proposal it read it from.
#[test]
fn refine_surfaces_a_capability_rejection_naming_the_missing_capability() {
    gmeow()
        .args([
            "logic",
            "refine",
            shipped_example("ocr-capability-absent.ttl")
                .to_str()
                .expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStep",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("rejected on capability"))
        .stdout(predicate::str::contains("ocr-absent/ocrCapability>"))
        .stdout(predicate::str::contains(
            "derived by <https://blackcatinformatics.ca/logic/ruleRefinementRejectedOnCapability>",
        ))
        .stdout(predicate::str::contains(
            "<https://blackcatinformatics.ca/logic/proposalMissingCapability>",
        ));
}

/// The capability-PRESENT scene: the five-step decomposition comes back in the AUTHORED
/// order, and the one approval-gated step carries its typed rejection.
#[test]
fn refine_returns_the_authored_five_step_decomposition_with_its_approval_rejection() {
    gmeow()
        .args([
            "logic",
            "refine",
            shipped_example("ocr-capability-present.ttl")
                .to_str()
                .expect("utf-8"),
            "--task",
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/ocrStep",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("CLOSED"))
        // inspect -> prepare -> OCR -> verify -> store receipt. Alphabetised, this plan
        // verifies before it extracts, so the exact sequence is the assertion.
        .stdout(predicate::str::contains(
            "steps: \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/inspectStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/prepareStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/extractTextStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/verifyStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/storeReceiptStep",
        ))
        .stdout(predicate::str::contains(
            "ocr-present/storeReceiptStep> rejected on approval",
        ))
        .stdout(predicate::str::contains(
            "derived by <https://blackcatinformatics.ca/logic/ruleRefinementRejectedOnApproval>",
        ));
}

#[test]
fn refine_refuses_an_out_of_fragment_method_set_rather_than_reporting_a_thin_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(
        &dir,
        "cyclic.ttl",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:a a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:aCell1 .
e:aCell1 rdf:first e:u ; rdf:rest rdf:nil .
e:b a logic:DecompositionMethod ; logic:methodDecomposes e:u ; logic:methodYields e:bCell1 .
e:bCell1 rdf:first e:t ; rdf:rest rdf:nil .
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

/// The shipped example's text, with the two properties every test below relies on pinned.
///
/// It ASSERTS the label under audit, and it carries the `logic:entryAction` edge through
/// which `logic:ruleFrontierBlockedCapability` reaches the entry. The second used to be
/// missing, and the example shipped a hand-asserted label the rule set could not reach —
/// the flagship demonstration of "surface the gap" carrying, at its centre, exactly the
/// unchecked assertion the discipline forbids. Asserting both here means a regression on
/// either one fails loudly instead of quietly making every test below vacuous.
fn shipped_ocr_example_text() -> String {
    let base = fs::read_to_string(shipped_ocr_example()).expect("shipped example is readable");
    assert!(
        base.contains("logic:entryLabel logic:FrontierBlockedCapabilityOrResource"),
        "the shipped example must ASSERT the label under audit, or this test proves nothing"
    );
    assert!(
        base.contains("logic:entryAction ex:ocrStep"),
        "the shipped example must bind the entry's ACTION, or no shipped rule can reach the \
         entry and its label is an assertion nothing has checked"
    );
    base
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
    let honest = shipped_ocr_example_text();
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
    let mutated = shipped_ocr_example_text().replace(
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
    // An entry whose axis witness NO shipped rule labels, carrying a hand-typed label.
    // `logic:ApprovalExpired` positions the approval axis and nothing else, and no shipped
    // logic:Rule concludes an entry label from it alone, so the authored value is a string
    // nothing has checked.
    //
    // That must read as an unverified assertion rather than as a conclusion: the difference
    // between "the reasoner concluded this step is blocked" and "somebody typed blocked" is
    // the difference between trusting the frontier and auditing it.
    //
    // The fixture is synthetic ON PURPOSE. This case used to be driven from the shipped
    // ocr-capability-absent.ttl, which asserted a label the rule set could not reach — so
    // the flagship demonstration of "surface the gap honestly" shipped, at its centre, the
    // very unchecked assertion the discipline forbids, and this test pinned it in place.
    // The example now binds its entry's action and derives; a shipped example may no longer
    // stand here, which `every_shipped_example_derives_the_labels_it_asserts` enforces.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(
        &dir,
        "unreachable-label.ttl",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:staleApproval a logic:FrontierEntry ;
    logic:entryAxisWitness logic:ApprovalExpired ;
    logic:entryLabel logic:FrontierBlockedCapabilityOrResource .
"#,
    );

    let out = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        path.to_str().expect("utf-8 path"),
    ]);
    let row = row_for(
        &out,
        "https://blackcatinformatics.ca/gmeow/clitest/staleApproval",
    );
    assert!(
        row.contains("FrontierBlockedCapabilityOrResource"),
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

/// The shipped example the flagship capability-absent scenario rests on DERIVES its label.
///
/// It used to assert one the rule set could not reach: `ex:ocrStepEntry` bound no
/// `logic:entryAction`, and `logic:ruleFrontierBlockedCapability` joins an entry to a gap
/// THROUGH the step the entry positions, so the command printed the file's headline claim
/// as `ASSERTED-UNCHECKED`. An example that demonstrates surfacing a capability gap while
/// asserting, unchecked, that the step is blocked demonstrates the failure it was written
/// to rule out.
#[test]
fn the_shipped_capability_absent_example_derives_its_label() {
    let out = stdout_of(&[
        "--console",
        "text",
        "logic",
        "frontier",
        shipped_ocr_example().to_str().expect("utf-8 path"),
    ]);
    assert!(
        !out.contains("run77Saturation"),
        "the saturation witness certifies the roster and is not a member of it; a witness \
         on the frontier is an action an operator can be asked to take and cannot, got:\n{out}"
    );
    let row = row_for(&out, OCR_ENTRY);
    assert!(
        row.contains("FrontierBlockedCapabilityOrResource"),
        "the blocked-on-capability label must be the one reported, got:\n{row}"
    );
    assert!(
        row.contains("derived (input agrees)"),
        "the shipped example asserts the label AND the rule set reproduces it, which is a \
         stronger statement than either alone; got:\n{row}"
    );
    assert!(
        !out.contains("ASSERTED-UNCHECKED"),
        "the flagship capability-absent example must carry no unchecked assertion, got:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// The sweep: no SHIPPED example may assert a frontier label the rule set does
// not derive.
//
// The CLI has printed `ASSERTED-UNCHECKED` since the provenance split was
// introduced, and until now NOTHING consumed that verdict — the string existed
// only in display code, so an example could carry an underived label forever and
// every gate stayed green. This is the consumer. It runs the shipped binary over
// every example the repository ships and fails on the first entry whose label
// the rules do not reach.
//
// Scoped to `examples/` deliberately. A `tests/counter-examples/` fixture is
// SUPPOSED to be malformed, and a conformance fixture is validated slice-scoped
// against its own expectations; an `examples/` file is the repository speaking in
// its own voice, and an unchecked assertion there is the ontology asserting
// something it cannot derive.
// ---------------------------------------------------------------------------

/// Every `*.ttl` under a `slices/**/examples/` directory, sorted.
fn shipped_examples() -> Vec<PathBuf> {
    fn walk(dir: &Path, in_examples: bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let is_examples = in_examples || path.file_name().is_some_and(|n| n == "examples");
                walk(&path, is_examples, out);
            } else if in_examples && path.extension().is_some_and(|e| e == "ttl") {
                out.push(path);
            }
        }
    }
    let slices = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices");
    let mut out = Vec::new();
    walk(&slices, false, &mut out);
    out.sort();
    assert!(
        !out.is_empty(),
        "the sweep found no shipped examples at all, which means it is sweeping nothing"
    );
    out
}

#[test]
fn every_shipped_example_derives_the_labels_it_asserts() {
    let mut swept = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for example in shipped_examples() {
        let text = fs::read_to_string(&example).expect("shipped example is readable");
        // Only a file that ASSERTS a label can offend. One that asserts none is either
        // silent about the frontier or already deriving everything it shows.
        if !text.contains("logic:entryLabel") {
            continue;
        }
        // `--console text` rather than `silent`: the JSON document goes to stdout and the
        // reporter's diagnostics to stderr, so a refusal below can be read and CHECKED
        // rather than being an empty exit code the sweep would have to guess about.
        let output = gmeow()
            .args([
                "--console",
                "text",
                "logic",
                "frontier",
                example.to_str().expect("utf-8 path"),
                "--format",
                "json",
            ])
            .output()
            .expect("the shipped binary runs");
        let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
        if !output.status.success() {
            // The command's own refusal when a file names `logic:entryLabel` only as a
            // predicate (a closure census, say) and carries no labelled entry. Nothing to
            // audit, and asserting the refusal's shape is louder than a silent skip.
            let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
            assert!(
                stderr.contains("no frontier label was derived"),
                "the sweep must not swallow an unexpected failure on {}: {stderr}",
                example.display()
            );
            continue;
        }
        swept += 1;
        let doc: serde_json::Value =
            serde_json::from_str(&stdout).expect("stdout is exactly one JSON document");
        for entry in doc["entries"].as_array().into_iter().flatten() {
            let provenance = entry["provenance"].as_str().unwrap_or_default();
            if provenance == "ASSERTED-UNCHECKED" || provenance == "DISAGREEMENT" {
                offenders.push(format!(
                    "{}: {} is {provenance} ({})",
                    example.display(),
                    entry["entry"].as_str().unwrap_or_default(),
                    entry["asserted_label"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    assert!(
        swept > 0,
        "the sweep audited no example carrying a frontier label, so it proves nothing"
    );
    assert!(
        offenders.is_empty(),
        "these SHIPPED examples assert a frontier label the rule set does not derive — an \
         example is the repository speaking in its own voice, and an unchecked label there \
         is the ontology asserting a conclusion it cannot reach; either bind the structure \
         the rule needs or stop asserting the label: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------
// Information flow: what the operator surface may NOT quietly drop.
//
// The retired reader walked the input with `TermRef::Iri`-only guards and a
// `_ => continue` fallthrough, and rebuilt every literal with `datatype: None`.
// The result was an operator reading a silently reduced world: a typed literal
// came back as a bare lexical form, a blank-node object and an RDF 1.2 triple
// term came back as nothing at all, and the screen said so nowhere.
// ---------------------------------------------------------------------------

/// One graph exercising every term shape the retired reader lost: a typed literal, a
/// language-tagged literal, a blank-node object, and an RDF 1.2 reifier binding
/// (`rdf:reifies <<( … )>>`) carrying an attributed annotation.
const MIXED_TERMS: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:ocr   a logic:FrontierEntry ; logic:entryAxisWitness logic:StepWaiting ; logic:entryAction e:ocrStep .
e:ocrStep a logic:TransactionStep .
e:gap  a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:ocrStep .
e:prop a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:ocrStep ;
    logic:proposalMissingCapability e:ocrCap ;
    logic:proposalExpectedInputs "300"^^xsd:nonNegativeInteger ;
    logic:proposalRequiredContract "Page image in, positioned text out."@en ;
    logic:proposalExpectedOutputs [ a logic:TransactionStep ] .
e:sensorClaim rdf:reifies <<( e:ocrStep logic:stepState logic:StepWaiting )>> ;
    logic:attestedBy e:opsSensor .
"#;

#[test]
fn why_not_carries_datatypes_language_tags_and_blank_nodes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "mixed.ttl", MIXED_TERMS);
    let out = stdout_of(&[
        "--console",
        "silent",
        "logic",
        "frontier",
        p.to_str().expect("utf-8"),
        "--why-not",
        "https://blackcatinformatics.ca/gmeow/clitest/ocrStep",
    ]);
    // The datatype is the value's meaning, not decoration: "300" as a string and 300 as a
    // non-negative integer are different facts, and only one of them is what was authored.
    assert!(
        out.contains("\"300\"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>"),
        "the typed literal must keep its datatype, got:\n{out}"
    );
    assert!(
        out.contains("\"Page image in, positioned text out.\"@en"),
        "the language-tagged literal must keep its tag, got:\n{out}"
    );
    // A blank-node object used to vanish entirely — a proposal field the operator was
    // never told existed.
    assert!(
        out.contains("proposalExpectedOutputs: _:"),
        "the blank-node object must be carried, got:\n{out}"
    );
}

#[test]
fn structured_frontier_carries_every_term_kind_and_reports_what_it_did_not_reason_over() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "mixed.ttl", MIXED_TERMS);
    let assert = gmeow()
        .args([
            "--console",
            "text",
            "logic",
            "frontier",
            p.to_str().expect("utf-8"),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let out = assert.get_output();
    let stdout = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    let stderr = String::from_utf8(out.stderr.clone()).expect("utf-8 stderr");
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is one JSON doc");
    let facts = doc["facts"].as_array().expect("facts array");

    let kinds: Vec<&str> = facts
        .iter()
        .flat_map(|f| [f["subject"]["kind"].as_str(), f["object"]["kind"].as_str()])
        .flatten()
        .collect();
    for kind in ["iri", "blank", "literal", "triple-term"] {
        assert!(
            kinds.contains(&kind),
            "every RDF term kind in the input must reach the structured output; {kind} is \
             missing from:\n{stdout}"
        );
    }
    // The literal facets travel as data, not as a rendered string.
    assert!(
        facts.iter().any(|f| {
            f["object"]["datatype"].as_str()
                == Some("http://www.w3.org/2001/XMLSchema#nonNegativeInteger")
        }),
        "the typed literal's datatype must be carried as a field, got:\n{stdout}"
    );
    assert!(
        facts
            .iter()
            .any(|f| f["object"]["language"].as_str() == Some("en")),
        "the language tag must be carried as a field, got:\n{stdout}"
    );
    // The RDF 1.2 attribution: the reified statement AND the annotation that attributes it.
    assert!(
        facts.iter().any(|f| {
            f["predicate"].as_str() == Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
                && f["object"]["kind"].as_str() == Some("triple-term")
        }),
        "the reifier binding must be carried whole, got:\n{stdout}"
    );
    assert!(
        facts.iter().any(|f| {
            f["predicate"].as_str() == Some("https://blackcatinformatics.ca/logic/attestedBy")
        }),
        "the reifier's attributing annotation must be carried, got:\n{stdout}"
    );
    // Carrying it is not enough: the run must SAY that the shipped rule set was not run
    // over the triple-term facts, or the operator reads the frontier as a conclusion about
    // a world the reasoner only partly saw.
    assert!(
        stderr.contains("statement-metadata-not-reasoned")
            && stderr.contains("RDF 1.2 statement-metadata fact"),
        "the withheld statement metadata must be reported, not silently withheld, got:\n{stderr}"
    );
}

#[test]
fn a_named_graph_is_refused_rather_than_flattened_into_one_world() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Two worlds. Re-homing both into the one reasoning world would let a fact asserted in
    // `e:w1` satisfy a rule body about `e:w2`, and nothing would say so.
    let p = write(
        &dir,
        "worlds.trig",
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/clitest/> .
e:w1 { e:a a logic:FrontierEntry ; logic:entryAxisWitness logic:StepReady . }
e:w2 { e:a logic:entryAxisWitness logic:ApprovalNull . }
"#,
    );
    gmeow()
        .args(["logic", "frontier", p.to_str().expect("utf-8")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("named graph"))
        .stderr(predicate::str::contains("refused rather than"));
}

// ---------------------------------------------------------------------------
// Structured output: the derived frontier must be able to flow back into a graph.
//
// A human-readable table is a terminal. The named consumer of this surface is an
// agent runtime, and a runtime that has to scrape a column layout to recover a
// conclusion cannot fold it back into the graph it came from.
// ---------------------------------------------------------------------------

/// The parsed JSON document one structured run printed on stdout.
fn json_of(args: &[&str]) -> serde_json::Value {
    serde_json::from_str(&stdout_of(args)).expect("stdout is exactly one JSON document")
}

#[test]
fn structured_frontier_carries_the_provenance_split() {
    let doc = json_of(&[
        "--console",
        "silent",
        "logic",
        "frontier",
        shipped_ocr_example().to_str().expect("utf-8"),
        "--format",
        "json",
    ]);
    let entry = doc["entries"]
        .as_array()
        .and_then(|e| e.first())
        .expect("one frontier entry");
    // The three-way split is the point: a consumer must be able to tell a conclusion from
    // an unverified assertion WITHOUT reading prose. The shipped example is the AGREEMENT
    // arm — the rules concluded the label and the author had written the same thing — which
    // is a different fact from either half alone and is carried as such.
    assert_eq!(entry["provenance"].as_str(), Some("derived (input agrees)"));
    assert_eq!(
        entry["derived_label"].as_str(),
        Some("https://blackcatinformatics.ca/logic/FrontierBlockedCapabilityOrResource")
    );
    assert_eq!(entry["input_agrees"].as_bool(), Some(true));
    assert!(
        doc["facts"]
            .as_array()
            .is_some_and(|f| f.iter().all(|r| r["asserted"].is_boolean()
                && r["derived"].is_boolean()
                && r["provenance"].is_string())),
        "every carried row must state its own provenance"
    );
}

#[test]
fn structured_explain_carries_all_five_elements_including_dissent() {
    let doc = json_of(&[
        "--console",
        "silent",
        "logic",
        "explain",
        shipped_example("contextual-recommendation.ttl")
            .to_str()
            .expect("utf-8"),
        "--action",
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/rollbackEntry",
        "--format",
        "json",
    ]);
    let elements = &doc["explanations"][0]["elements"];
    for element in ["proof", "evidence", "policy", "criterion", "dissent"] {
        assert!(
            elements[element].is_array(),
            "R3.5 element {element} must be a key of its own, present even when empty, got:\n{doc}"
        );
    }
    assert_eq!(
        elements["dissent"][0]["value"].as_str(),
        Some(
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/onCallObjection"
        ),
        "dissent must survive into the structured surface, not be averaged away"
    );
    assert_eq!(
        doc["explanations"][0]["label_verdicts"][0]["provenance"].as_str(),
        Some("derived (input agrees)"),
        "the explanation's re-derived label must carry its own provenance"
    );
}

#[test]
fn structured_refine_carries_the_status_and_every_witness() {
    let doc = json_of(&[
        "--console",
        "silent",
        "logic",
        "refine",
        shipped_example("ocr-capability-absent.ttl")
            .to_str()
            .expect("utf-8"),
        "--task",
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStep",
        "--format",
        "json",
    ]);
    assert_eq!(doc["status"].as_str(), Some("CLOSED"));
    let rejection = &doc["rejections"][0];
    assert_eq!(rejection["kind"].as_str(), Some("capability"));
    // The witness is what makes the roster re-derivable rather than merely asserted.
    assert_eq!(
        rejection["witness"]["rule_iri"].as_str(),
        Some("https://blackcatinformatics.ca/logic/ruleRefinementRejectedOnCapability")
    );
    assert!(
        rejection["witness"]["premises"]
            .as_array()
            .is_some_and(|p| !p.is_empty()),
        "the chase premises must travel with the rejection, got:\n{doc}"
    );
}

#[test]
fn structured_refine_under_a_cut_reports_incomplete_and_ships_no_roster() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = write(&dir, "methods.ttl", METHODS);
    let doc = json_of(&[
        "--console",
        "silent",
        "logic",
        "refine",
        p.to_str().expect("utf-8"),
        "--task",
        "https://blackcatinformatics.ca/gmeow/clitest/ingest",
        "--budget",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(doc["status"].as_str(), Some("INCOMPLETE"));
    // A partial roster in a machine-readable document is worse than in a table: nothing
    // downstream reads the caveat prose, so there must be no roster to misread.
    assert_eq!(doc["candidates"].as_array().map(Vec::len), Some(0));
    assert_eq!(doc["reached"].as_array().map(Vec::len), Some(0));
}

#[test]
fn structured_saga_carries_each_outcome_and_what_is_owed() {
    let doc = json_of(&[
        "--console",
        "silent",
        "logic",
        "saga",
        shipped_example("effect-boundary-unknown.ttl")
            .to_str()
            .expect("utf-8"),
        "--format",
        "json",
    ]);
    let outcomes: Vec<&str> = doc["attempts"]
        .as_array()
        .expect("attempts")
        .iter()
        .filter_map(|a| a["outcome"].as_str())
        .collect();
    // Receipted, foreclosed and undetermined are three different next actions.
    for outcome in ["receipted", "FORECLOSED", "UNDETERMINED"] {
        assert!(
            outcomes.contains(&outcome),
            "the structured saga must keep {outcome} apart from the others, got:\n{doc}"
        );
    }
    let undetermined = doc["attempts"]
        .as_array()
        .expect("attempts")
        .iter()
        .find(|a| a["outcome"].as_str() == Some("UNDETERMINED"))
        .expect("one undetermined attempt");
    assert!(
        undetermined["owed"].is_string(),
        "an undetermined attempt must say what is owed, got:\n{doc}"
    );
}
