// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual CLI renderers consuming native, authenticated authored-scene records.
//! These tests neither read the corpus sources nor invoke the compiler/reasoner.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::{
    OutputFormat, render_logic_explain, render_logic_frontier, render_logic_refine,
    render_logic_saga,
};
use gmeow_cli_core::Reporter;
use gmeow_errors::Report;
use gmeow_logic::operator::{
    Derivation,
    scene::{CHANNEL, Observations, SourceObservation},
};

const COUNTER: &str = "slices/grounding/logic/tests/counter-examples/unknown-outcome-retried-on-a-borrowed-licence.ttl";
const OCR_ABSENT: &str = "slices/core/work-orchestration/examples/ocr-capability-absent.ttl";
const OCR_PRESENT: &str = "slices/core/work-orchestration/examples/ocr-capability-present.ttl";
const OCR_ENTRY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStepEntry";
const OCR_ACTION: &str =
    "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStep";

fn observations() -> &'static Observations {
    static SELECTED: OnceLock<gmeow_errors::Result<(String, Observations)>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("operator scenes require the exact producer selector");
    let (selected, observed) = SELECTED
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let value = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok((selector.clone(), value))
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated operator scenes: {error}"));
    assert_eq!(
        selected, &selector,
        "operator observations cannot cross producer identities"
    );
    observed
}

fn scene(path: &str) -> &'static SourceObservation {
    let value = observations()
        .scenes
        .get(path)
        .unwrap_or_else(|| panic!("missing required operator scene {path}"));
    assert_eq!(value.source_path, path);
    if let Some(selected) = observations().examples.get(path) {
        assert_eq!(value.source_digest, selected.source_digest);
    }
    value
}

#[derive(Default)]
struct Capture(Mutex<Vec<String>>);
impl Reporter for Capture {
    fn report(&self, report: &Report) {
        self.0.lock().expect("report lock").extend(
            report
                .findings
                .iter()
                .map(|finding| finding.message.clone()),
        );
    }
    fn stage_start(&self, _stage: &str) {}
    fn stage_end(&self, _stage: &str, _elapsed: Duration) {}
    fn summary(&self, report: &Report) {
        self.report(report);
    }
}
struct Rendered {
    code: i32,
    stdout: String,
    diagnostics: String,
}
impl Rendered {
    fn success(self) -> String {
        assert_eq!(self.code, 0, "{}", self.diagnostics);
        self.stdout
    }
}
fn frontier(path: &str, changed: bool, why_not: Option<&str>, format: OutputFormat) -> Rendered {
    let scene = scene(path);
    let result = if changed {
        &scene
            .ocr
            .as_ref()
            .expect("OCR source structure")
            .mutation
            .result
    } else {
        &scene.original
    };
    let reporter = Capture::default();
    let mut stdout = String::new();
    let code = match result {
        Ok(derived) => {
            super::report_operator_boundaries(
                &reporter,
                Path::new(path),
                "gmeow-cli.logic-frontier",
                derived,
            );
            render_logic_frontier(
                &reporter,
                Path::new(path),
                &derived.rows,
                why_not,
                format,
                &mut stdout,
            )
        }
        Err(error) => super::fail(&reporter, "gmeow-cli.logic-frontier", error.to_string()),
    };
    let diagnostics = reporter.0.into_inner().expect("report lock").join("\n");
    Rendered {
        code,
        stdout,
        diagnostics,
    }
}
fn derived(path: &str) -> &'static Derivation {
    scene(path)
        .original
        .as_ref()
        .expect("native operator result")
}
fn saga(path: &str, format: OutputFormat) -> String {
    let reporter = Capture::default();
    let mut stdout = String::new();
    let code = render_logic_saga(
        &reporter,
        Path::new(path),
        &derived(path).rows,
        format,
        &mut stdout,
    );
    assert_eq!(
        code,
        0,
        "{:?}",
        reporter.0.into_inner().expect("report lock")
    );
    stdout
}
fn refinement(path: &str) -> String {
    refinement_format(path, OutputFormat::Text)
}
fn refinement_format(path: &str, format: OutputFormat) -> String {
    let report = scene(path)
        .refinement
        .as_ref()
        .expect("required native refinement record");
    let reporter = Capture::default();
    let mut stdout = String::new();
    let code = render_logic_refine(
        &reporter,
        Path::new(path),
        report,
        1000,
        format,
        &mut stdout,
    );
    assert_eq!(
        code,
        0,
        "{:?}",
        reporter.0.into_inner().expect("report lock")
    );
    stdout
}
fn assert_ocr_source_and_mutation() {
    let ocr = scene(OCR_ABSENT)
        .ocr
        .as_ref()
        .expect("original OCR source structure");
    assert_eq!(ocr.entry, OCR_ENTRY);
    assert_eq!(ocr.action, OCR_ACTION);
    assert!(
        ocr.asserted_labels.iter().any(|label| label
            == "https://blackcatinformatics.ca/logic/FrontierBlockedCapabilityOrResource"),
        "the source must assert the label under audit"
    );
    assert!(
        ocr.entry_actions.iter().any(|action| action == OCR_ACTION),
        "the original source must bind the entry action reached by its rule"
    );
    assert_eq!(
        ocr.mutation.predicate,
        "https://blackcatinformatics.ca/logic/entryLabel"
    );
    assert_eq!(
        ocr.mutation.removed,
        "https://blackcatinformatics.ca/logic/FrontierBlockedCapabilityOrResource"
    );
    assert_eq!(
        ocr.mutation.inserted,
        "https://blackcatinformatics.ca/logic/FrontierReadyAuthorized"
    );
    assert_eq!(
        (ocr.mutation.removed_rows, ocr.mutation.inserted_rows),
        (1, 1),
        "the source mutation must actually replace exactly one assertion"
    );
}
fn row_for<'a>(stdout: &'a str, entry: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.starts_with(entry))
        .unwrap_or_else(|| panic!("no frontier row for {entry} in:\n{stdout}"))
}

#[test]
fn saga_is_never_silent_on_a_graph_carrying_effect_records() {
    assert!(
        !scene(COUNTER).has_attempt_of_intent,
        "the original fixture reaches its attempts without logic:attemptOfIntent"
    );
    let stdout = saga(COUNTER, OutputFormat::Text);
    assert!(
        !stdout.trim().is_empty(),
        "a surface whose whole job is 'what is owed' must never print nothing on a graph \
         full of effect records"
    );
    // Both attempts are rostered: the retried one and the one the licence actually covers.
    assert!(
        stdout.contains("invoice901Charge") && stdout.contains("invoice902Charge"),
        "both attempts must be rostered, got:\n{stdout}"
    );
    // The retry and the licence it borrows are NAMED — the relation is the finding, and a
    // reader that printed only the outcome would leave the operator to spot it themselves.
    assert!(
        stdout.contains("invoice901Retry")
            && stdout.contains("invoice902Idempotency")
            && stdout.contains("BORROWED"),
        "the retry, its borrowed licence, and the fact that it IS borrowed must all be \
         named, got:\n{stdout}"
    );
    assert!(
        stdout.contains("UNDETERMINED") && stdout.contains("STOP THE RETRY"),
        "the undetermined outcome and the move that must be stopped first must both be \
         said, got:\n{stdout}"
    );
}

#[test]
fn structured_saga_carries_the_borrowed_licence_relation() {
    let doc: serde_json::Value =
        serde_json::from_str(&saga(COUNTER, OutputFormat::Json)).expect("one JSON answer");
    let attempts = doc["attempts"].as_array().expect("attempts");
    assert_eq!(attempts.len(), 2, "both attempts are rostered:\n{doc}");
    let retried = attempts
        .iter()
        .find(|a| {
            a["attempt"]
                .as_str()
                .is_some_and(|s| s.ends_with("invoice901Charge"))
        })
        .expect("the retried attempt is rostered");
    assert_eq!(retried["outcome"].as_str(), Some("UNDETERMINED"), "{doc}");
    let retries = retried["retries"].as_array().expect("retries");
    assert_eq!(retries.len(), 1, "one retry:\n{doc}");
    assert_eq!(
        retries[0]["borrowed_licence"].as_bool(),
        Some(true),
        "the licence is borrowed, and the structured form must say so:\n{doc}"
    );
    assert!(
        retries[0]["licence"]
            .as_str()
            .is_some_and(|s| s.ends_with("invoice902Idempotency")),
        "the borrowed licence must be named:\n{doc}"
    );
}

#[test]
fn refine_surfaces_a_capability_rejection_naming_the_missing_capability() {
    let stdout = refinement(OCR_ABSENT);
    assert!(stdout.contains("rejected on capability"), "{stdout}");
    assert!(stdout.contains("ocr-absent/ocrCapability>"), "{stdout}");
    assert!(
        stdout.contains(
            "derived by <https://blackcatinformatics.ca/logic/ruleRefinementRejectedOnCapability>"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("<https://blackcatinformatics.ca/logic/proposalMissingCapability>"),
        "{stdout}"
    );
}

#[test]
fn refine_returns_the_authored_five_step_decomposition_with_its_approval_rejection() {
    let stdout = refinement(OCR_PRESENT);
    assert!(stdout.contains("CLOSED"), "{stdout}");
    assert!(stdout.contains("steps: \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/inspectStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/prepareStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/extractTextStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/verifyStep -> \
             https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/storeReceiptStep"), "{stdout}");
    assert!(
        stdout.contains("ocr-present/storeReceiptStep> rejected on approval"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "derived by <https://blackcatinformatics.ca/logic/ruleRefinementRejectedOnApproval>"
        ),
        "{stdout}"
    );
}

#[test]
fn refine_derives_the_pin_from_the_authorized_candidate_on_the_shipped_example() {
    let stdout = refinement(OCR_PRESENT);
    const NS: &str =
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/";
    assert!(stdout.contains("pins:        1"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "[0] <{NS}ocrByStageCandidate> selected by <{NS}refineRun78>"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("instantiates: <{NS}scannedIngestMethod>")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "frozen steps: {NS}inspectStep -> {NS}prepareStep -> {NS}extractTextStep -> \
             {NS}verifyStep -> {NS}storeReceiptStep"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "digest:       \
             b3:2f5c8d1a94b70e63f8c2a5d417e09b3c6a8f2d5e71b04c93a6d8f125e370b4ca"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("authority:    <{NS}run78PinAuthorization>")),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "derived by <https://blackcatinformatics.ca/logic/rulePinnedStepSequenceFromMethod> \
             (proof height 1)"
        ),
        "{stdout}"
    );
}

#[test]
fn frontier_prints_the_derived_label_and_flags_a_contradicting_authored_one() {
    assert_ocr_source_and_mutation();
    let out = frontier(OCR_ABSENT, true, None, OutputFormat::Text).success();
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
    let clean = frontier(OCR_ABSENT, false, None, OutputFormat::Text).success();
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
    assert_ocr_source_and_mutation();
    let out = frontier(OCR_ABSENT, true, Some(OCR_ACTION), OutputFormat::Text).success();
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
fn the_shipped_capability_absent_example_derives_its_label() {
    let out = frontier(OCR_ABSENT, false, None, OutputFormat::Text).success();
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

#[test]
fn every_shipped_example_derives_the_labels_it_asserts() {
    let mut swept = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    assert!(
        !observations().examples.is_empty(),
        "the sweep must inventory every selected example"
    );
    for (path, selected) in &observations().examples {
        if !selected.contains_entry_label_marker {
            continue;
        }
        let example = Path::new(path);
        let output = frontier(path, false, None, OutputFormat::Json);
        let stdout = output.stdout;
        if output.code != 0 {
            // The command's own refusal when a file names `logic:entryLabel` only as a
            // predicate (a closure census, say) and carries no labelled entry. Nothing to
            // audit, and asserting the refusal's shape is louder than a silent skip.
            let stderr = output.diagnostics;
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

fn explanation(path: &str, action: &str) -> serde_json::Value {
    let reporter = Capture::default();
    let mut stdout = String::new();
    let code = render_logic_explain(
        &reporter,
        Path::new(path),
        &derived(path).rows,
        action,
        OutputFormat::Json,
        &mut stdout,
    );
    assert_eq!(
        code,
        0,
        "{:?}",
        reporter.0.into_inner().expect("report lock")
    );
    serde_json::from_str(&stdout).expect("one explanation JSON document")
}

#[test]
fn structured_frontier_carries_the_provenance_split() {
    let doc: serde_json::Value =
        serde_json::from_str(&frontier(OCR_ABSENT, false, None, OutputFormat::Json).success())
            .expect("one frontier JSON answer");
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
    let doc = explanation(
        "slices/core/work-orchestration/examples/contextual-recommendation.ttl",
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/rollbackEntry",
    );
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
fn structured_explain_derives_the_contested_claim_a_recommendation_rests_on() {
    let doc = explanation(
        "slices/core/work-orchestration/examples/contextual-recommendation.ttl",
        "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/rollback",
    );
    let contested = doc["contested"].as_array().expect("contested array");
    assert_eq!(
        contested.len(),
        1,
        "the two co-equal vantages over `rollback ≻ hotfix` must derive exactly one \
         contestation, got:\n{doc}"
    );
    let row = &contested[0];
    assert_eq!(
        row["affirming_attribution"].as_str(),
        Some(
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/cmdrOnRollbackOverHotfix"
        )
    );
    assert_eq!(
        row["opposing_attribution"].as_str(),
        Some(
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/recommendation/onCallOnRollbackOverHotfix"
        )
    );
    assert!(
        row["statement"]
            .as_str()
            .is_some_and(|s| s.contains("/rollback")
                && s.contains("gmeow/strictlyOver")
                && s.contains("/hotfix")),
        "the contested STATEMENT must be named — 'somebody disagrees' without saying about \
         what is not something an operator can weigh; got:\n{doc}"
    );
    assert_eq!(
        row["derived_by"].as_str(),
        Some("https://blackcatinformatics.ca/logic/ruleAttributionContestedByOpposingVantage"),
        "the contestation must name the authored rule that concluded it"
    );
}

#[test]
fn structured_refine_carries_the_status_and_every_witness() {
    let doc: serde_json::Value =
        serde_json::from_str(&refinement_format(OCR_ABSENT, OutputFormat::Json))
            .expect("one refinement JSON answer");
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
fn structured_saga_carries_each_outcome_and_what_is_owed() {
    let doc: serde_json::Value = serde_json::from_str(&saga(
        "slices/core/work-orchestration/examples/effect-boundary-unknown.ttl",
        OutputFormat::Json,
    ))
    .expect("one saga JSON answer");
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
