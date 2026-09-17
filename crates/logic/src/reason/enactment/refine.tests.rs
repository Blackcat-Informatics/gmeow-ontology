// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{MEANS_END_PREDICATES, OperationOutcome, RefineReport, RejectionKind};
use purrdf::RdfDataset;

/// The eleven authored means–end rules, by IRI local name.
///
/// Spelled out rather than counted, because a count alone would stay green if one rule
/// silently dropped out of the module and an unrelated one arrived. This is the census
/// the module doc's claim rests on: the expansion an operator reads is exactly the
/// authored rule set, and nothing supplements it in Rust.
const MEANS_END_RULES: [&str; 11] = [
    "ruleMethodStep",
    "ruleMethodYieldCellHead",
    "ruleMethodYieldCellRest",
    "ruleRefinementCandidateMethod",
    "ruleRefinementExpands",
    "ruleRefinementReachesBase",
    "ruleRefinementReachesTransitive",
    "ruleRefinementRejectedOnApproval",
    "ruleRefinementRejectedOnCapability",
    "ruleRefinementRejectedOnPrecondition",
    "ruleRefinementRejectedOnResource",
];

fn dataset(turtle: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("parse fixture")
}

fn means_end_program() -> &'static gmeow_logic_compile::ir::LogicProgram {
    crate::operator_rules::fixture().means_end_preparation().0
}

fn refine(input: &RdfDataset, task: &str, fragment: &str, budget: u32) -> RefineReport {
    super::refine(
        input,
        task,
        fragment,
        budget,
        crate::operator_rules::fixture(),
    )
}

const ACYCLIC: &str = "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod";
const NS: &str = "https://blackcatinformatics.ca/gmeow/refinetest/";

/// A five-step ordered method whose sequence is neither alphabetical nor
/// reverse-alphabetical, so a reader that sorted it would fail visibly.
const ORDERED: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/refinetest/> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:c1 .
e:c1 rdf:first e:inspect ; rdf:rest e:c2 .
e:c2 rdf:first e:prepare ; rdf:rest e:c3 .
e:c3 rdf:first e:extract ; rdf:rest e:c4 .
e:c4 rdf:first e:verify  ; rdf:rest e:c5 .
e:c5 rdf:first e:store   ; rdf:rest rdf:nil .
"#;

#[test]
fn every_authored_means_end_rule_is_selected_into_the_program() {
    let program = means_end_program();
    let selected: Vec<&str> = program
        .rules
        .iter()
        .filter_map(|rule| rule.scope.provenance.as_deref())
        .collect();
    let missing: Vec<&str> = MEANS_END_RULES
        .iter()
        .copied()
        .filter(|name| {
            !selected
                .iter()
                .any(|iri| iri.ends_with(&format!("/{name}")))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "these authored means–end rules are not in the sub-program the refinement runs, so \
             the expansion silently does less than the module says: {missing:?}"
    );
}

/// The six pin-derivation rules reach the sub-program the refinement runs.
///
/// Selected by IRI rather than head predicate, so a rename in the module would silently
/// drop one and the refinement would report a roster with no commitment — which reads
/// exactly like "nothing was authorized".
#[test]
fn every_authored_pin_rule_is_selected_into_the_program() {
    let program = means_end_program();
    let selected: Vec<&str> = program
        .rules
        .iter()
        .filter_map(|rule| rule.scope.provenance.as_deref())
        .collect();
    let missing: Vec<&str> = super::PIN_RULES
        .iter()
        .copied()
        .filter(|name| {
            !selected
                .iter()
                .any(|iri| iri.ends_with(&format!("/{name}")))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "these authored pin rules are not in the sub-program, so an authorized candidate \
             would produce no commitment while the roster looked complete: {missing:?}; \
             selected: {selected:?}"
    );
}

/// Every selected rule's head is one of the declared means–end predicates, and every
/// declared predicate is headed by some rule.
///
/// The second half is the one that matters: a predicate this module READS but no rule
/// DERIVES would make the corresponding column of every report permanently empty while
/// the refinement reported a clean, closed roster.
#[test]
fn the_declared_means_end_predicates_are_exactly_the_derived_ones() {
    let program = means_end_program();
    let headed: std::collections::BTreeSet<&str> = program
        .rules
        .iter()
        .map(|rule| rule.head.predicate.as_str())
        .collect();
    let underived: Vec<&str> = MEANS_END_PREDICATES
        .iter()
        .copied()
        .filter(|predicate| !headed.contains(predicate))
        .collect();
    assert!(
        underived.is_empty(),
        "these means–end predicates are read but never derived, so the report's \
             corresponding column is permanently empty: {underived:?}"
    );
}

/// The whole point: the authored order survives the derivation.
#[test]
fn a_methodised_task_yields_its_authored_sequence_in_order() {
    let input = dataset(ORDERED);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
    assert!(
        report.is_closed(),
        "a well-formed method set must settle: {:?}",
        report.outcome
    );
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(
        report.candidates[0].steps,
        vec![
            format!("{NS}inspect"),
            format!("{NS}prepare"),
            format!("{NS}extract"),
            format!("{NS}verify"),
            format!("{NS}store"),
        ],
        "the list order IS the plan; alphabetised, this one verifies before it extracts"
    );
}

/// Every roster row carries the rule that concluded it — the property the predecessor
/// could not have, because nothing concluded anything.
#[test]
fn a_candidate_carries_the_authored_rule_that_derived_it() {
    let input = dataset(ORDERED);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
    let witness = &report.candidates[0].witness;
    assert!(
        witness.rule_iri.ends_with("/ruleRefinementCandidateMethod"),
        "a roster row must name the authored rule that concluded it, got {}",
        witness.rule_iri
    );
    assert!(
        !witness.premises.is_empty(),
        "a proof witness with no premises explains nothing"
    );
}

#[test]
fn two_methods_for_one_task_are_two_alternatives() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:fast a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:f1 .
e:f1 rdf:first e:quick ; rdf:rest rdf:nil .
e:thorough a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:t1 .
e:t1 rdf:first e:extract ; rdf:rest e:t2 .
e:t2 rdf:first e:verify ; rdf:rest rdf:nil .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
    assert!(report.is_closed());
    assert_eq!(
        report.candidates.len(),
        2,
        "a roster that dropped one would be picking a plan on the operator's behalf"
    );
}

#[test]
fn a_nested_method_set_reaches_the_deep_steps_and_marks_the_open_one() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:top a logic:DecompositionMethod ; logic:methodDecomposes e:ingest ; logic:methodYields e:p1 .
e:p1 rdf:first e:ocr ; rdf:rest e:p2 .
e:p2 rdf:first e:store ; rdf:rest rdf:nil .
e:sub a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:s1 .
e:s1 rdf:first e:extract ; rdf:rest rdf:nil .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}ingest"), ACYCLIC, 10_000);
    assert!(report.is_closed());
    assert!(
        report.reached.contains(&format!("{NS}extract")),
        "the transitive expansion must reach past the first level: {:?}",
        report.reached
    );
    let top = report
        .candidates
        .iter()
        .find(|c| c.method == format!("{NS}top"))
        .expect("the top method is a candidate");
    assert_eq!(
        top.open_steps,
        vec![format!("{NS}ocr")],
        "a step with a method of its own is OPEN, and a roster that hid that would present \
             an abstract task as executable work"
    );
}

/// A cycle is an out-of-fragment refusal, and it names the task that closes the loop.
#[test]
fn a_method_cycle_is_out_of_fragment_and_names_the_looping_task() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:a a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:a1 .
e:a1 rdf:first e:u ; rdf:rest rdf:nil .
e:b a logic:DecompositionMethod ; logic:methodDecomposes e:u ; logic:methodYields e:b1 .
e:b1 rdf:first e:t ; rdf:rest rdf:nil .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
    assert!(
        matches!(report.outcome, OperationOutcome::UnsupportedFragment { .. }),
        "a cyclic method set is out-of-fragment, never a budget problem: {:?}",
        report.outcome
    );
    assert!(
        report.cycles.contains(&format!("{NS}t")),
        "the refusal must name WHICH task closes the loop: {:?}",
        report.cycles
    );
    assert!(
        report.candidates.is_empty(),
        "an out-of-fragment refusal must not also publish a roster"
    );
}

/// A budget cut must never present as a closed roster.
#[test]
fn a_budget_cut_is_incomplete_and_never_closed() {
    let input = dataset(ORDERED);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 1);
    assert!(
        matches!(report.outcome, OperationOutcome::Incomplete { .. }),
        "a one-derivation budget cannot settle a five-step method: {:?}",
        report.outcome
    );
    assert!(!report.is_closed());
    assert!(
        report.candidates.is_empty(),
        "the engine commits only on a complete run, so a cut leaves no roster to \
             misread as closed"
    );
}

#[test]
fn a_malformed_yields_chain_is_an_invalid_request_not_a_quietly_shorter_plan() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:c1 .
e:c1 rdf:first e:s1 ; rdf:rest e:c2 .
e:c2 rdf:first e:s2 .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
    assert!(
        matches!(report.outcome, OperationOutcome::Invalid { .. }),
        "a chain that never reaches rdf:nil denotes no sequence: {:?}",
        report.outcome
    );
}

#[test]
fn a_method_naming_two_yields_lists_is_an_invalid_request() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:t ;
    logic:methodYields e:a1 , e:b1 .
e:a1 rdf:first e:s1 ; rdf:rest rdf:nil .
e:b1 rdf:first e:s2 ; rdf:rest rdf:nil .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
    assert!(matches!(report.outcome, OperationOutcome::Invalid { .. }));
}

#[test]
fn a_task_the_input_never_mentions_is_an_invalid_request_not_an_empty_roster() {
    let input = dataset(ORDERED);
    let report = refine(input.as_ref(), &format!("{NS}nosuchtask"), ACYCLIC, 10_000);
    assert!(
        matches!(report.outcome, OperationOutcome::Invalid { .. }),
        "a typo'd task must not read as 'no decomposition exists': {:?}",
        report.outcome
    );
}

#[test]
fn an_undeclared_search_fragment_is_an_invalid_request() {
    let input = dataset(ORDERED);
    let report = refine(
        input.as_ref(),
        &format!("{NS}ocr"),
        "https://example.org/NotAFragment",
        10_000,
    );
    assert!(matches!(report.outcome, OperationOutcome::Invalid { .. }));
}

/// The capability rejection names the MISSING CAPABILITY, which is what makes the
/// refusal actionable rather than merely honest.
#[test]
fn an_operational_capability_gap_rejects_the_step_and_names_the_capability() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:ocr .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:ocr ;
    logic:proposalMissingCapability e:ocrCapability .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    let rejection = report
        .rejections
        .iter()
        .find(|r| r.kind == RejectionKind::Capability)
        .expect("the capability gap must reject the step it blocks");
    assert_eq!(rejection.witness_iri, format!("{NS}ocrCapability"));
    assert!(
        rejection
            .witness
            .rule_iri
            .ends_with("/ruleRefinementRejectedOnCapability")
    );
}

#[test]
fn an_undetached_approval_rejects_the_step_it_gates() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:entry a logic:FrontierEntry ;
    logic:entryAction e:publish ;
    logic:entryAxisWitness logic:StepReady , logic:ApprovalCreated .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}publish"), ACYCLIC, 10_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    let rejection = report
        .rejections
        .iter()
        .find(|r| r.kind == RejectionKind::Approval)
        .expect("a created-but-undetached approval gates the step");
    assert_eq!(rejection.witness_iri, format!("{NS}entry"));
}

#[test]
fn an_awaited_resource_under_a_lease_rejects_the_step_on_resource() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:entry a logic:FrontierEntry ;
    logic:entryAction e:closeValve ;
    logic:entryAxisWitness logic:StepWaiting ;
    logic:entryAwaits e:valve104 .
e:valveLease a logic:ResourceLease ; logic:leaseScope e:valve104 .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}closeValve"), ACYCLIC, 10_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    let rejection = report
        .rejections
        .iter()
        .find(|r| r.kind == RejectionKind::Resource)
        .expect("a lease over the awaited resource is what stands in the way");
    assert_eq!(rejection.witness_iri, format!("{NS}valveLease"));
}

#[test]
fn a_denied_action_gate_rejects_the_step_on_its_precondition() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:extract a logic:ActionSchema ; logic:precondition e:pagesRasterised .
e:probe a logic:GateProbe ;
    logic:probesSchema e:extract ;
    logic:gateVerdict logic:GateDenied .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}extract"), ACYCLIC, 10_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    let rejection = report
        .rejections
        .iter()
        .find(|r| r.kind == RejectionKind::Precondition)
        .expect("a denied gate rejects on the precondition it judged");
    assert_eq!(rejection.witness_iri, format!("{NS}pagesRasterised"));
}

/// All four typed reasons are reachable on one graph, which is what "per-candidate
/// precondition / capability / resource / approval reasons" actually requires.
#[test]
fn all_four_rejection_kinds_are_derivable_over_one_expansion() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:root ; logic:methodYields e:c1 .
e:c1 rdf:first e:needsCapability ; rdf:rest e:c2 .
e:c2 rdf:first e:needsApproval   ; rdf:rest e:c3 .
e:c3 rdf:first e:needsResource   ; rdf:rest e:c4 .
e:c4 rdf:first e:needsPrecondition ; rdf:rest rdf:nil .

e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:needsCapability .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:needsCapability ;
    logic:proposalMissingCapability e:ocrCapability .

e:approvalEntry a logic:FrontierEntry ;
    logic:entryAction e:needsApproval ;
    logic:entryAxisWitness logic:ApprovalCreated .

e:resourceEntry a logic:FrontierEntry ;
    logic:entryAction e:needsResource ;
    logic:entryAwaits e:valve104 .
e:valveLease a logic:ResourceLease ; logic:leaseScope e:valve104 .

e:needsPrecondition a logic:ActionSchema ; logic:precondition e:pagesRasterised .
e:probe a logic:GateProbe ;
    logic:probesSchema e:needsPrecondition ;
    logic:gateVerdict logic:GateDenied .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}root"), ACYCLIC, 100_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    let kinds: std::collections::BTreeSet<RejectionKind> =
        report.rejections.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds.len(),
        4,
        "all four typed reasons must be derivable over one expansion, got {kinds:?}"
    );
    for rejection in &report.rejections {
        assert!(
            !rejection.witness.rule_iri.is_empty() && !rejection.witness.premises.is_empty(),
            "every rejection must carry the chase premises that established it: {rejection:?}"
        );
    }
}

// ── The pin half: an authorized candidate becomes a commitment ON THE CHASE ────────
//
// Three laws read a logic:PinnedExecutableSubgraph and nothing in the repository could
// produce one: `logic:rule…Pin…` matched no rule, and `gmeow logic refine` emitted no
// pin, so a pin could only ever be hand-authored — the same defect the kernel condemns
// for frontier labels, at the other end of the same episode. These two tests are the
// red/green pair for the six rules that close it.

/// A roster whose candidate carries its method and its licensing proof — every input
/// the pin derivation needs. Nothing here IS a pin: no `logic:PinnedExecutableSubgraph`
/// type, no `logic:pinnedStepSequence`, no `logic:pinDigest`, no `logic:pinAuthority`,
/// no `logic:selectedPin`. All five are derived.
const AUTHORIZED_CANDIDATE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/refinetest/> .

e:byPagePartition a logic:DecompositionMethod ;
    logic:methodDecomposes e:ocr ;
    logic:methodYields e:c1 ;
    logic:methodDigest "b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61" .
e:c1 rdf:first e:inspect ; rdf:rest e:c2 .
e:c2 rdf:first e:extract ; rdf:rest e:c3 .
e:c3 rdf:first e:verify  ; rdf:rest rdf:nil .

e:episode a logic:RefinementEpisode ;
    logic:refinesStep e:ocr ;
    logic:searchFragment logic:FragmentAcyclicMethod ;
    logic:producedCandidateSet e:roster .
e:roster a logic:RefinementCandidateSet ;
    logic:refinementCandidate e:ocrByPagePartitionCandidate .
e:ocrByPagePartitionCandidate logic:candidateInstantiatesMethod e:byPagePartition .

e:ocrAuthProof a logic:AuthorizationProof ;
    logic:proofEstablishes e:ocrByPagePartitionCandidate .
"#;

/// GREEN — the authorized candidate yields a complete pin, and every field of it is
/// concluded by an authored rule the witness names.
#[test]
fn an_authorized_candidate_yields_a_derived_pin_on_the_chase() {
    let input = dataset(AUTHORIZED_CANDIDATE);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    assert_eq!(
        report.pins.len(),
        1,
        "the authorized candidate must yield exactly one pin: {:?}",
        report.pins
    );
    let pin = &report.pins[0];
    assert_eq!(pin.pin, format!("{NS}ocrByPagePartitionCandidate"));
    assert_eq!(pin.episode, format!("{NS}episode"));
    assert_eq!(pin.method, format!("{NS}byPagePartition"));
    assert_eq!(
        pin.steps,
        vec![
            format!("{NS}inspect"),
            format!("{NS}extract"),
            format!("{NS}verify"),
        ],
        "the frozen sequence IS the method's yielded sequence, in the authored order — \
             which is what makes the steps-match-method law unviolatable by a derived pin"
    );
    assert_eq!(
        pin.digest, "b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61",
        "the content address is the method version's own; a derived pin never invents one"
    );
    assert_eq!(pin.authority, format!("{NS}ocrAuthProof"));
    assert!(
        pin.witness
            .rule_iri
            .ends_with("/rulePinnedStepSequenceFromMethod"),
        "the frozen content must name the authored rule that concluded it, got {}",
        pin.witness.rule_iri
    );
    assert!(
        !pin.witness.premises.is_empty(),
        "a pin whose derivation cites no premises is a pin nobody can check"
    );
}

/// RED — the SAME roster with the authorization withdrawn derives no pin at all.
///
/// The only edit is the proof's type: the record still exists, still names the
/// candidate, still sits in the same graph. Deleting it would make the scene pin-free
/// for a reason that has nothing to do with authority — no record, no join, no pin —
/// and would leave rules that fire on any roster equally well. What changes here is
/// whether the thing that licensed the candidate IS an authorization proof, which is
/// the whole content of "an AUTHORIZED candidate".
#[test]
fn an_unauthorized_candidate_yields_no_pin() {
    let unlicensed = AUTHORIZED_CANDIDATE.replace(
        "e:ocrAuthProof a logic:AuthorizationProof ;",
        "e:ocrAuthProof a logic:Advisory ;",
    );
    assert_ne!(
        unlicensed, AUTHORIZED_CANDIDATE,
        "the edit must actually change the fixture, or the red half proves nothing"
    );
    let input = dataset(&unlicensed);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    assert!(
        report.pins.is_empty(),
        "advice may motivate a pin and never licenses one, so a candidate whose only \
             backing is a logic:Advisory must freeze nothing: {:?}",
        report.pins
    );
    assert!(
        !report.candidates.is_empty(),
        "the roster must still be there — otherwise the red half is 'the search found \
             nothing', which is a different claim entirely"
    );
}

/// A method with no content address yields NO pin, rather than an unaddressed one.
///
/// The completeness law requires the digest, so a pin derived without one would be a
/// commitment the kernel is about to condemn. Refusing to report it is the honest
/// failure: 'content-addressed' is a claim about the METHOD, and a method that never
/// made it cannot lend it to a pin.
#[test]
fn a_method_with_no_digest_yields_no_pin_rather_than_an_unaddressed_one() {
    let undigested = AUTHORIZED_CANDIDATE.replace(
            " ;\n    logic:methodDigest \"b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61\" .",
            " .",
        );
    assert_ne!(
        undigested, AUTHORIZED_CANDIDATE,
        "the edit must remove the digest"
    );
    let input = dataset(&undigested);
    let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    assert!(
        report.pins.is_empty(),
        "an unaddressed fragment is not a pin, and reporting one would tell an operator \
             something was frozen when nothing was: {:?}",
        report.pins
    );
}

/// A rejection standing outside the refined task's expansion is not this refinement's
/// business, and reporting it would attribute an unrelated blockage to this plan.
#[test]
fn a_rejection_outside_the_expansion_is_not_reported() {
    let input = dataset(&format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:root ; logic:methodYields e:c1 .
e:c1 rdf:first e:innocent ; rdf:rest rdf:nil .
e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:elsewhere .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:elsewhere ;
    logic:proposalMissingCapability e:someCapability .
"#
    ));
    let report = refine(input.as_ref(), &format!("{NS}root"), ACYCLIC, 100_000);
    assert!(report.is_closed(), "{:?}", report.outcome);
    assert!(
        report.rejections.is_empty(),
        "an unrelated step's blockage must not be attributed to this expansion: {:?}",
        report.rejections
    );
}
