// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected-theory claim ingress over tiny owned Turtle sources, through the
//! production entries only. Nothing here reads a repository corpus.

use super::*;
use crate::physical::{DomainProfile, SelectedDomains, SelectedLogicalWorld};
use crate::query_ir::Budget;
use crate::reason::{reason_program_within, reason_selected_theory_within};
use crate::result::ReasoningResult;
use gmeow_logic_compile::frontend::PreparedLogicSource;

const PREFIXES: &str = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://example.org/claims/> .
ex:Left logic:disjointWith ex:Right .
"#;

const SUBJECT: &str = "https://example.org/claims/a";
const LEFT: &str = "https://example.org/claims/Left";
const RIGHT: &str = "https://example.org/claims/Right";
const UNBOUNDED: Budget = Budget {
    max_answers: None,
    max_steps: None,
};

/// Two reports, from different sources, that type one individual into the two
/// disjoint classes.
const EVIDENCE_CLASH: &str = r#"
[] rdf:reifies <<( ex:a logic:instanceOf ex:Left )>> ; logic:provenance ex:first .
[] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; logic:confidence 0.0 .
"#;

fn compile(body: &str) -> CompiledTheory {
    let text = format!("{PREFIXES}{body}");
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
        .expect("the owned source is valid Turtle");
    PreparedLogicSource::new(&dataset)
        .expect("the owned source prepares")
        .into_compiled(None)
        .expect("the owned source compiles")
}

fn default_domain(input: &PreparedReasoningInput) -> SelectedDomains {
    SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "urn:claims:test-authority".to_owned(),
        *input.ingress_contract(),
    )
    .expect("the default world is selectable")])
    .expect("one selected world")
}

fn ingest(theory: &CompiledTheory) -> (PreparedReasoningInput, ClaimIngress) {
    prepare_selected_theory_input(
        theory,
        &LogicalGraph::Default,
        &TheorySelection::top_level_only(),
    )
    .expect("claim ingress")
}

fn decide(theory: &CompiledTheory) -> (gmeow_errors::Result<ReasoningResult>, ClaimIngress) {
    let (input, ingress) = ingest(theory);
    let domains = default_domain(&input);
    (
        reason_selected_theory_within(theory, input, &domains, &ingress, &UNBOUNDED),
        ingress,
    )
}

fn bare(theory: &CompiledTheory) -> gmeow_errors::Result<ReasoningResult> {
    let input = prepare_reasoning_input(theory.source().dataset()).expect("bare ingestion");
    let domains = default_domain(&input);
    reason_program_within(theory.program(), input, &domains, &UNBOUNDED)
}

fn consistent(result: &ReasoningResult) -> bool {
    result.native_verdict().expect("native verdict").consistent
}

/// Every premise of every derived clash, as `(subject, object)` pairs.
fn clash_premises(result: &ReasoningResult) -> Vec<(String, String)> {
    result
        .inferred()
        .iter()
        .filter(|axiom| {
            axiom.object.as_iri().and_then(|iri| {
                crate::reason::dl::EmptyClassAssertion::classify(&axiom.predicate, iri)
            }) == Some(crate::reason::dl::EmptyClassAssertion::Membership)
        })
        .flat_map(|axiom| axiom.premises.iter())
        .map(|(subject, _, object)| (subject.clone(), object.clone()))
        .collect()
}

/// PROTECTS: a claim that carries only evidence coordinates is a statement of the
/// selected theory. Two such claims that contradict each other make the theory
/// inconsistent, the clash is witnessed by both claimed statements, and both are
/// in the ledger as read with their evidence. A confidence of zero is still a claim.
#[test]
fn evidence_only_claims_that_contradict_are_a_clash() {
    let theory = compile(EVIDENCE_CLASH);
    let (result, ingress) = decide(&theory);
    let result = result.expect("the selected theory executes");

    assert!(!consistent(&result));
    let premises = clash_premises(&result);
    for class in [LEFT, RIGHT] {
        assert!(
            premises
                .iter()
                .any(|(subject, object)| subject == SUBJECT && object.contains(class)),
            "{class} is a witness premise: {premises:?}"
        );
    }
    let evidence: Vec<_> = ingress
        .read()
        .iter()
        .flat_map(|claim| claim.evidence.iter().copied())
        .collect();
    assert_eq!(
        evidence,
        [
            EvidenceCoordinate::Provenance,
            EvidenceCoordinate::Confidence
        ]
    );
    assert!(ingress.unread().is_empty());
    assert_eq!(ingress.unaccounted(theory.program()), None);
}

/// PROTECTS: a triple term asserts nothing. A reifier with no scope coordinate is a
/// quotation, even when it carries other annotations, so a quoted statement that
/// would contradict an assertion is not a clash and never enters the ledger.
#[test]
fn a_quotation_that_contradicts_an_assertion_is_not_a_clash() {
    let theory = compile(
        r#"
ex:a logic:instanceOf ex:Left .
[] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> .
ex:remark rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; rdfs:comment "someone said so" .
"#,
    );
    let (result, ingress) = decide(&theory);

    assert!(consistent(&result.expect("the selected theory executes")));
    assert!(ingress.read().is_empty());
    assert!(ingress.unread().is_empty());
}

/// PROTECTS: a claim placed in a context the selection does not translate is not a
/// statement of the selected theory. It adds no fact (the would-be clash is absent),
/// it does not make the engine refuse, and the ledger names exactly the context
/// coordinates that kept it out, never its evidence coordinates.
#[test]
fn a_claim_in_an_untranslated_context_is_named_and_adds_no_fact() {
    for (coordinate, annotation) in [
        ("logic:standpoint", "logic:standpoint ex:observer"),
        ("logic:time", r#"logic:time "2020""#),
        ("logic:modality", "logic:modality logic:epistemic"),
        ("logic:inModule", "logic:inModule ex:module"),
        (
            "logic:inModule",
            "logic:provenance ex:first ; logic:inModule ex:module",
        ),
    ] {
        let theory = compile(&format!(
            "ex:a logic:instanceOf ex:Left .
             [] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; {annotation} ."
        ));
        let (result, ingress) = decide(&theory);

        assert!(
            consistent(&result.expect("an axiom scope does not refuse the selected theory")),
            "{annotation}"
        );
        assert!(ingress.read().is_empty(), "{annotation}");
        let reasons: Vec<_> = ingress
            .unread()
            .iter()
            .map(|claim| claim.reason.clone())
            .collect();
        assert_eq!(
            reasons,
            [UnreadReason::Context(vec![coordinate])],
            "{annotation}"
        );
        assert_eq!(ingress.unaccounted(theory.program()), None, "{annotation}");
    }
}

/// PROTECTS: only a statement the compiler lowered from the carried RDF terms is
/// inserted. A claim over a variable carrier is not ground, and a claim whose
/// carrier rewrites a term states a different term than the dataset holds; neither
/// becomes a fact, and both are named.
#[test]
fn a_claim_that_does_not_state_its_carried_terms_is_named_and_adds_no_fact() {
    let theory = compile(
        r#"
ex:a logic:instanceOf ex:Left .
_:variable a logic:TermCarrier ; logic:termVariable "x" .
_:renamed a logic:TermCarrier ; logic:termIri ex:a .
[] rdf:reifies <<( _:variable logic:instanceOf ex:Right )>> ; logic:provenance ex:first .
[] rdf:reifies <<( _:renamed logic:instanceOf ex:Right )>> ; logic:provenance ex:second .
"#,
    );
    let (_, ingress) = ingest(&theory);

    assert!(ingress.read().is_empty());
    let mut reasons: Vec<_> = ingress
        .unread()
        .iter()
        .map(|claim| claim.reason.clone())
        .collect();
    reasons.sort_by_key(|reason| reason.to_string());
    assert_eq!(
        reasons,
        [UnreadReason::SourceMismatch, UnreadReason::NotGround]
    );
    assert_eq!(ingress.unaccounted(theory.program()), None);
}

/// PROTECTS: the claimed fact is made of the carrier's own RDF terms. A claim about
/// a blank node that an asserted statement also describes joins that statement in a
/// rule body, and its language-tagged object reaches the conclusion unchanged.
#[test]
fn a_claim_with_blank_and_literal_terms_joins_the_asserted_terms() {
    let theory = compile(
        r#"
_:thing logic:instanceOf ex:Left .
[] rdf:reifies <<( _:thing ex:label "chat"@fr )>> ; logic:provenance ex:first .
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:l a logic:TermCarrier ; logic:termVariable "l" .
ex:labelled a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x ex:marked _:l )>> ] ;
    logic:body [ rdf:reifies <<( _:x logic:instanceOf ex:Left )>> ] ,
               [ rdf:reifies <<( _:x ex:label _:l )>> ] .
"#,
    );
    let (result, ingress) = decide(&theory);
    let result = result.expect("the selected theory executes");

    assert_eq!(ingress.read().len(), 1);
    let marked: Vec<_> = result
        .inferred()
        .iter()
        .filter(|axiom| axiom.predicate == "https://example.org/claims/marked")
        .map(|axiom| &axiom.object)
        .collect();
    assert!(
        matches!(
            marked[..],
            [TermValue::Literal { lexical_form, language: Some(language), .. }]
                if lexical_form == "chat" && language == "fr"
        ),
        "{marked:?}"
    );
}

/// PROTECTS: the selected-theory profile stops refusing for an axiom's scope only.
/// A rule placed in a context is not executed as a rule of the selected theory: the
/// engine still declines the whole program and names the rule.
#[test]
fn a_rule_under_a_context_still_refuses_the_selected_theory() {
    let theory = compile(
        r#"
_:x a logic:TermCarrier ; logic:termVariable "x" .
ex:scoped a logic:Rule ;
    logic:standpoint ex:observer ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf ex:Right )>> ] ;
    logic:body [ rdf:reifies <<( _:x logic:instanceOf ex:Left )>> ] .
"#,
    );
    let (result, _) = decide(&theory);

    let error = result.expect_err("a scoped rule is refused");
    let refusal = error
        .downcast_ref::<crate::error::NativeRefusal>()
        .expect("a named refusal");
    assert!(
        refusal
            .reason
            .starts_with("selected-theory admission refuses")
    );
    assert!(
        refusal.gaps.iter().any(|gap| gap.starts_with("rule/0: ")),
        "{:?}",
        refusal.gaps
    );
}

/// PROTECTS: execution that ignores axiom scopes starts only from the input whose
/// claims were admitted for that program. An input the claims were never added to,
/// and a program with a scoped claim the ledger does not hold, are both refused.
#[test]
fn the_entry_refuses_an_input_or_program_the_ledger_does_not_account_for() {
    let theory = compile(EVIDENCE_CLASH);
    let (admitted, ingress) = ingest(&theory);

    let stale = prepare_reasoning_input(theory.source().dataset()).expect("bare ingestion");
    let domains = default_domain(&stale);
    let error = reason_selected_theory_within(&theory, stale, &domains, &ingress, &UNBOUNDED)
        .expect_err("the claims were never added to this input");
    assert!(
        error
            .to_string()
            .contains("does not belong to this reasoning input")
    );

    let wider = compile(&format!(
        "{EVIDENCE_CLASH}
         [] rdf:reifies <<( ex:b logic:instanceOf ex:Left )>> ; logic:inModule ex:module ."
    ));
    let domains = default_domain(&admitted);
    let error = reason_selected_theory_within(&wider, admitted, &domains, &ingress, &UNBOUNDED)
        .expect_err("a scoped claim of this program is not in the ledger");
    assert!(
        error
            .to_string()
            .contains("absent from the claim ingress ledger")
    );
}

/// PROTECTS: the bare-program entry is unchanged. It reads no scoped claim, so the
/// contradicting evidence-only claims are not a clash there, and it still refuses
/// the whole program when an axiom carries a context coordinate.
#[test]
fn the_bare_program_entry_reads_no_scoped_claim() {
    assert!(consistent(
        &bare(&compile(EVIDENCE_CLASH)).expect("evidence coordinates select no world")
    ));

    let error = bare(&compile(
        "[] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; logic:inModule ex:module .",
    ))
    .expect_err("a module-scoped axiom refuses the bare program");
    let refusal = error
        .downcast_ref::<crate::error::NativeRefusal>()
        .expect("a named refusal");
    assert!(
        refusal
            .reason
            .starts_with("world-local template admission refuses unexecuted source scope")
    );
    assert!(
        refusal.gaps.iter().all(|gap| gap.starts_with("axiom/")),
        "{:?}",
        refusal.gaps
    );
}
