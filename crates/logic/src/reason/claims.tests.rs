// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected-theory claim ingress over tiny owned Turtle sources, through the
//! production entries only. Nothing here reads a repository corpus.

use super::*;
use crate::physical::{DomainProfile, SelectedDomains, SelectedLogicalWorld};
use crate::query_ir::Budget;
use crate::reason::{
    NativeSourceAdmission, prepare_reasoning_input, prepare_selected_theory,
    reason_admitted_theory_within, reason_program_within, reason_selected_theory_within,
};
use crate::result::ReasoningResult;
use gmeow_logic_compile::frontend::{PreparedLogicSource, SourceSelection};
use gmeow_logic_compile::source_inventory::{SourceInventory, SourceObligationOrigin};

const PREFIXES: &str = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
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
    let dataset = purrdf::parse_dataset(text.as_bytes(), "application/trig", None)
        .expect("the owned source is valid TriG");
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
    let inventory = whole_inventory(theory);
    let selection = inventory
        .resolve_theory_selection(&TheorySelection::top_level_only())
        .expect("top-level source resolution");
    prepare_selected_theory_input(&inventory, &LogicalGraph::Default, &selection)
        .expect("claim ingress")
}

fn whole_inventory(theory: &CompiledTheory) -> SourceInventory<'_> {
    SourceInventory::new(theory, SourceSelection::all_classical_roots(theory))
}

fn ingest_modules(
    inventory: &SourceInventory<'_>,
    roots: &[&str],
) -> (PreparedReasoningInput, ClaimIngress) {
    let request = TheorySelection::composed_modules(roots.iter().map(|root| (*root).to_owned()))
        .expect("absolute module roots");
    let selection = inventory
        .resolve_theory_selection(&request)
        .expect("source module resolution");
    prepare_selected_theory_input(inventory, &LogicalGraph::Default, &selection)
        .expect("module ingress")
}

/// PROTECTS: module claims and scoped rule/head/body syntax execute together;
/// excluded schema and NAF contribute no premise while genuinely shared terms survive.
/// Source, root, preparation, input and domain identities authorize the same run.
/// Descriptive carrier roles and text remain ordinary source facts for authored consumers.
#[test]
fn module_execution_preserves_owners_shared_support_and_invocation_authority() {
    const INCLUDED: &str = "https://example.org/claims/Included";
    const EXCLUDED: &str = "https://example.org/claims/Excluded";
    let body = r#"
ex:Included a logic:Module . ex:Excluded a logic:Module .
ex:p a logic:DatatypeProperty ; logic:domain logic:Nothing ; logic:inModule ex:Excluded .
ex:chain logic:propertyChainAxiom ex:collection ; logic:inModule ex:Excluded .
ex:collection rdf:first ex:p ; rdf:rest rdf:nil .
ex:a ex:p ex:b .
ex:claim rdf:reifies <<( ex:a ex:label "chat"@fr )>> ; logic:inModule ex:Included ;
    gmeow:graphBoxRole gmeow:boxCBox, ex:futureRole ;
    gmeow:statedRationale "سبب"@ar--rtl .
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:label logic:termVariable "label" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:mark a logic:Rule ; logic:inModule ex:Included ;
    logic:head [ rdf:reifies <<( _:x ex:marked _:label )>> ; logic:inModule ex:Included ] ;
    logic:body [ rdf:reifies <<( _:x ex:label _:label )>> ; logic:inModule ex:Included ] .
_:carrier a logic:TermCarrier ; logic:termVariable "carrier" .
_:role a logic:TermCarrier ; logic:termVariable "role" .
ex:roleReceipt a logic:Rule ; logic:inModule ex:Included ;
    logic:head [ rdf:reifies <<( _:carrier ex:describedBy _:role )>> ] ;
    logic:body [ rdf:reifies <<( _:carrier gmeow:graphBoxRole _:role )>> ] .
ex:rationaleReceipt a logic:Rule ; logic:inModule ex:Included ;
    logic:head [ rdf:reifies <<( _:carrier ex:describedBy _:role )>> ] ;
    logic:body [ rdf:reifies <<( _:carrier gmeow:statedRationale _:role )>> ] .
ex:absence a logic:Rule ; logic:inModule ex:Excluded ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:x ex:p _:y )>> ] ;
    logic:negatedBody [ rdf:reifies <<( _:x ex:q _:y )>> ] .
ex:catalogue a logic:Correspondence ; logic:inModule ex:Excluded ;
    logic:correspondenceRelation logic:RelatedMatch ; logic:morphismClass logic:LossyLens ;
    logic:morphismKind logic:InstitutionMorphism ; logic:recoveryCase ex:case .
ex:case a logic:RecoveryCase ; logic:recoveryTransform ex:recovery .
ex:recovery a logic:Formula ; logic:relation ex:label ; logic:argument _:x, _:label .
_:x logic:termIndex 0 . _:label logic:termIndex 1 .
"#;
    let theory = compile(body);
    let inventory = whole_inventory(&theory);
    let (input, ingress) = ingest_modules(&inventory, &[INCLUDED]);
    assert!(
        ingress
            .resolved_selection()
            .program_view(&inventory)
            .expect("the selected view belongs to this source")
            .axioms()
            .all(|(_, axiom)| !matches!(
                axiom.predicate.as_str(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#first"
                    | "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"
            )),
        "an excluded chain contributes no executable collection cells"
    );
    let selected = prepare_selected_theory(&inventory, &ingress).expect("selected module lowering");
    let mut admission =
        NativeSourceAdmission::inspect_selected(&inventory, ingress.resolved_selection())
            .expect("module source admission");
    let domains = default_domain(&input);
    let (other_input, other_ingress) = ingest_modules(&inventory, &[EXCLUDED]);
    let other_selected =
        prepare_selected_theory(&inventory, &other_ingress).expect("other root preparation");
    let other_domains = default_domain(&other_input);
    for (program, candidate, domain) in [
        (&other_selected, input.clone(), &domains),
        (&selected, other_input, &domains),
        (&selected, input.clone(), &other_domains),
    ] {
        reason_admitted_theory_within(
            &mut admission,
            program,
            candidate,
            domain,
            &ingress,
            &UNBOUNDED,
        )
        .expect_err("different root, input, preparation or domain cannot borrow authority");
    }
    let result = reason_admitted_theory_within(
        &mut admission,
        &selected,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect("the exact selected invocation executes");
    assert!(
        consistent(&result),
        "excluded schema and procedural absence cannot refute the selected module"
    );
    assert_eq!(ingress.read().len(), 1);
    assert!(
        ingress.read()[0].evidence.is_empty(),
        "module assertion needs no invented evidence"
    );
    assert!(result.inferred().iter().any(|row| row.predicate == "https://example.org/claims/marked"
        && matches!(&row.object, TermValue::Literal { lexical_form, language: Some(language), .. }
            if lexical_form == "chat" && language == "fr")), "shared variable and literal identity reach the rule head");
    for role in [
        "https://blackcatinformatics.ca/gmeow/boxCBox",
        "https://example.org/claims/futureRole",
    ] {
        assert!(
            result.inferred().iter().any(|row| {
                row.subject == "https://example.org/claims/claim"
                    && row.predicate == "https://example.org/claims/describedBy"
                    && row.object.as_iri() == Some(role)
            }),
            "the actual authored role remains executable source data: {role}"
        );
    }
    assert!(
        result
            .inferred()
            .iter()
            .any(|row| row.subject == "https://example.org/claims/claim"
                && row.predicate == "https://example.org/claims/describedBy"
                && matches!(&row.object, TermValue::Literal {
                lexical_form, datatype, language: Some(language),
                direction: Some(purrdf::RdfTextDirection::Rtl), ..
            } if lexical_form == "سبب"
                && datatype == "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
                && language == "ar")),
        "the actual authored rationale reaches the rule head with its language and direction"
    );
    assert!(
        admission
            .complete_for(&result)
            .expect("live module completion"),
        "{:?}",
        admission.drops().collect::<Vec<_>>()
    );
    assert!(
        admission.source_dispositions().any(|(unit, disposition)| {
            let gmeow_logic_compile::source_inventory::SourceObligationOrigin::Statement(binding) =
                unit.origin()
            else {
                return false;
            };
            let dataset = theory.source().dataset();
            matches!(
                dataset.resolve(binding.canonical.p),
                purrdf::TermRef::Iri("https://blackcatinformatics.ca/logic/termVariable")
            ) && matches!(
                dataset.resolve(binding.canonical.o),
                purrdf::TermRef::Literal {
                    lexical: "label",
                    ..
                }
            ) && matches!(
                disposition,
                crate::reason::NativeSourceDisposition::StructuralSupport { .. }
            )
        }),
        "selected rule support remains reachable through its actual owner, not the excluded catalogue"
    );
    let blocked = compile(&format!(
        r#"{body}
_:label logic:inModule ex:Excluded .
ex:scopedProbe rdf:reifies <<( ex:a ex:probe _:label )>> ; logic:inModule ex:Included .
"#
    ));
    let inventory = whole_inventory(&blocked);
    let (input, ingress) = ingest_modules(&inventory, &[INCLUDED]);
    let probe = blocked
        .program()
        .axioms
        .iter()
        .position(|axiom| axiom.predicate == "https://example.org/claims/probe")
        .expect("native scoped payload lowers");
    assert!(
        matches!(
            ingress
                .resolved_selection()
                .program_view(&inventory)
                .expect("same source view")
                .axiom_selection(probe)
                .expect("original scoped axiom"),
            gmeow_logic_compile::source_inventory::TheoryUnitSelection::Blocked {
                code: "REQUIRED_CHILD_OUTSIDE_THEORY",
                ..
            }
        ),
        "a scoped reifier also retains its required native payload carrier"
    );
    let selected = prepare_selected_theory(&inventory, &ingress).expect("blocked rule preparation");
    let mut admission =
        NativeSourceAdmission::inspect_selected(&inventory, ingress.resolved_selection())
            .expect("blocked owner remains accounted");
    let domains = default_domain(&input);
    let error =
        reason_selected_theory_within(&inventory, input.clone(), &domains, &ingress, &UNBOUNDED)
            .expect_err("required untyped carrier inside a quoted body/head cannot be dropped");
    let refusal = error
        .downcast_ref::<crate::error::NativeRefusal>()
        .expect("whole-owner refusal");
    assert!(
        refusal
            .gaps
            .iter()
            .any(|gap| gap.contains("mark") && gap.contains("REQUIRED_CHILD_OUTSIDE_THEORY")),
        "{refusal:?}"
    );
    let result = reason_admitted_theory_within(
        &mut admission,
        &selected,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect("source proof executes the admitted fragment");
    assert!(consistent(&result));
    assert!(
        !result
            .inferred()
            .iter()
            .any(|row| row.predicate == "https://example.org/claims/marked"),
        "a blocked owner cannot execute after dropping a required child"
    );
    assert!(
        !admission
            .complete_for(&result)
            .expect("live fragment result")
    );
    assert!(
        admission.drops().any(
            |drop| drop.code == "REQUIRED_CHILD_OUTSIDE_THEORY" && drop.detail.contains("mark")
        )
    );
}

/// PROTECTS: selecting a module never relaxes the full-source claim envelope or
/// translates another context axis, even when filtered input omits the offending row.
/// A qualified duplicate and unrelated blocked positive rule keep their boundaries
/// without cancelling a valid carrier or suppressing an admitted contradiction.
/// Ordinary source assertions about an unread carrier still supply native premises.
#[test]
fn module_claims_retain_independent_context_and_full_source_envelope_boundaries() {
    for (extra, ordinary_clash) in [
        ("ex:claim logic:standpoint ex:observer .", false),
        ("ex:claim logic:modality logic:none .", false),
        ("ex:g { ex:claim ex:unknownQualifier ex:value . }", false),
        (
            "ex:g { ex:claim gmeow:graphBoxRole gmeow:boxCBox . }",
            false,
        ),
        (
            "ex:claim gmeow:graphBoxRole \"not a role resource\" .",
            false,
        ),
        ("ex:claim gmeow:statedRationale 1 .", false),
        (
            "ex:g { ex:claim gmeow:statedRationale \"scope notes\" . }",
            false,
        ),
        (
            "ex:annotation a logic:AnnotationProperty . ex:claim ex:annotation ex:value .",
            false,
        ),
        (
            "ex:mark logic:domain logic:Nothing . ex:claim ex:mark ex:value .",
            true,
        ),
    ] {
        let theory = compile(&format!(
            r#"
ex:Included a logic:Module .
ex:a logic:instanceOf ex:Left .
ex:claim rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; logic:inModule ex:Included ;
    gmeow:graphBoxRole gmeow:boxCBox ; gmeow:statedRationale "source reason" .
{extra}
"#
        ));
        let inventory = whole_inventory(&theory);
        let (input, ingress) = ingest_modules(&inventory, &["https://example.org/claims/Included"]);
        assert!(ingress.read().is_empty(), "{extra}");
        assert_eq!(ingress.unread().len(), 1, "{extra}");
        let domains = default_domain(&input);
        let result =
            reason_selected_theory_within(&inventory, input, &domains, &ingress, &UNBOUNDED)
                .expect("the supported ordinary theory executes");
        assert_eq!(
            !consistent(&result),
            ordinary_clash,
            "only the independent ordinary assertion may refute: {extra}"
        );
    }
    const VALID_LEFT: &str =
        "ex:left rdf:reifies <<( ex:a logic:instanceOf ex:Left )>> ; logic:inModule ex:Included ; gmeow:graphBoxRole gmeow:boxCBox .
ex:alsoLeft rdf:reifies <<( ex:a logic:instanceOf ex:Left )>> ; logic:inModule ex:Included .";
    for (left, expected_clash, read_count) in [(VALID_LEFT, true, 2), ("", false, 1)] {
        let theory = compile(&format!(
            r#"
ex:Included a logic:Module .
{left}
ex:right rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; logic:inModule ex:Included .
ex:qualified rdf:reifies <<( ex:a logic:instanceOf ex:Left )>> ;
    logic:inModule ex:Included ; gmeow:graphBoxRole gmeow:boxCBox ; ex:qualifier ex:unknown .
_:r a logic:TermCarrier ; logic:termVariable "r" .
ex:blocked a logic:Rule ; logic:inModule ex:Included ; logic:standpoint ex:observer ;
    logic:head [ rdf:reifies <<( _:r logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:r logic:instanceOf ex:Right )>> ] .
"#
        ));
        let inventory = whole_inventory(&theory);
        let (input, ingress) = ingest_modules(&inventory, &["https://example.org/claims/Included"]);
        assert_eq!(ingress.read().len(), read_count);
        assert_eq!(ingress.unread().len(), 1);
        let refused = &ingress.unread()[0];
        assert_eq!(
            refused.carrier.as_ref().expect("original carrier").name,
            "https://example.org/claims/qualified"
        );
        assert!(
            matches!(&refused.reason, UnreadReason::Qualified { predicate, .. }
            if predicate == "https://example.org/claims/qualifier")
        );
        let carriers: Vec<_> = ingress
            .read()
            .iter()
            .flat_map(|claim| &claim.carriers)
            .map(|carrier| carrier.name.as_str())
            .collect();
        assert!(!carriers.contains(&"https://example.org/claims/qualified"));
        if expected_clash {
            assert!(carriers.contains(&"https://example.org/claims/left"));
            assert!(carriers.contains(&"https://example.org/claims/alsoLeft"));
        }
        let selected = prepare_selected_theory(&inventory, &ingress).expect("selected claims");
        let mut admission =
            NativeSourceAdmission::inspect_selected(&inventory, ingress.resolved_selection())
                .expect("source account");
        let domains = default_domain(&input);
        let result = reason_admitted_theory_within(
            &mut admission,
            &selected,
            input,
            &domains,
            &ingress,
            &UNBOUNDED,
        )
        .expect("the admitted fragment executes");
        assert_eq!(!consistent(&result), expected_clash);
        if expected_clash {
            let premises = clash_premises(&result);
            for class in [LEFT, RIGHT] {
                assert!(
                    premises
                        .iter()
                        .any(|(subject, object)| subject == SUBJECT && object.contains(class)),
                    "{premises:?}"
                );
            }
        }
        assert!(!admission.complete_for(&result).expect("live completeness"));
        assert!(admission.drops().any(
            |drop| drop.code == "CONTEXT_ERASURE_FORBIDDEN" && drop.detail.contains("blocked")
        ));
        assert!(
            admission.source_dispositions().any(|(unit, disposition)| {
                matches!(unit.origin(), SourceObligationOrigin::Quotation { reifier, .. }
                if matches!(theory.source().dataset().resolve(reifier),
                    purrdf::TermRef::Iri("https://example.org/claims/qualified")))
                    && matches!(
                        disposition,
                        crate::reason::NativeSourceDisposition::Blocked {
                            code: "SCOPED_CLAIM_NOT_READ",
                            ..
                        }
                    )
            }),
            "a valid sibling cannot account the qualified carrier's quotation"
        );
    }
}

/// PROTECTS: shared formula IR retains each admitted module assertion and authored
/// name without promoting owned children or dropping required descendants.
#[test]
fn selected_formula_occurrences_keep_names_and_required_descendants() {
    let body = r#"
ex:Included a logic:Module ; logic:hasFormula ex:first, ex:second, ex:consequence, ex:other .
ex:Excluded a logic:Module ; logic:hasFormula ex:other, ex:excluded, ex:shared .
ex:container logic:hasFormula ex:shared .
ex:namedGraph { ex:Included logic:hasFormula ex:first . }
ex:a ex:seed ex:b .
ex:first a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
ex:second a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication ; logic:inModule ex:Included .
ex:other a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication ; logic:inModule ex:Excluded .
ex:excluded a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
ex:shared a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
ex:implication a logic:Formula ; logic:antecedent ex:guard ; logic:consequent ex:consequence .
ex:guard a logic:Formula ; logic:relation ex:seed ; logic:argument ex:x, ex:bTerm .
ex:consequence a logic:Formula ; logic:relation ex:marked ; logic:argument ex:x, ex:bTerm .
ex:x a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "x" .
ex:bTerm logic:termIndex 1 ; logic:termIri ex:b .
"#;
    for (extra, executes) in [
        ("", true),
        ("ex:consequence logic:inModule ex:Excluded .", false),
        ("ex:bTerm logic:inModule ex:Excluded .", false),
    ] {
        let theory = compile(&format!("{body}{extra}"));
        let inventory = whole_inventory(&theory);
        let (input, ingress) = ingest_modules(&inventory, &["https://example.org/claims/Included"]);
        let selected =
            prepare_selected_theory(&inventory, &ingress).expect("selected formula preparation");
        let names = selected
            .authored_rule_formulas(&theory)
            .expect("same source attribution");
        let names: BTreeSet<_> = names.values().flatten().map(String::as_str).collect();
        if executes {
            assert_eq!(
                names,
                BTreeSet::from([
                    "https://example.org/claims/first",
                    "https://example.org/claims/second",
                    "https://example.org/claims/shared"
                ])
            );
        } else {
            assert!(
                names.is_empty(),
                "blocked parent contributes no executable clause"
            );
        }
        let mut admission =
            NativeSourceAdmission::inspect_selected(&inventory, ingress.resolved_selection())
                .expect("selected formula source account");
        let domains = default_domain(&input);
        let result = reason_admitted_theory_within(
            &mut admission,
            &selected,
            input,
            &domains,
            &ingress,
            &UNBOUNDED,
        )
        .expect("supported source remainder executes");
        assert_eq!(
            result
                .inferred()
                .iter()
                .any(|row| row.predicate == "https://example.org/claims/marked"),
            executes
        );
        let links: Vec<_> = admission
            .source_dispositions()
            .filter_map(|(unit, disposition)| {
                let anchor = unit.anchor();
                anchor
                    .contains("/hasFormula")
                    .then_some((anchor, disposition))
            })
            .collect();
        assert_eq!(links.len(), 9);
        assert!(links.iter().any(|(anchor, disposition)| {
            anchor.contains("/namedGraph")
                && matches!(
                    disposition,
                    crate::reason::NativeSourceDisposition::OutsideSelection { .. }
                )
        }));
        for (link, expected) in [
            ("consequence", "MODULE_LINK_TO_OWNED_FORMULA"),
            ("other", "CONFLICTING_FORMULA_MODULE_COORDINATE"),
        ] {
            assert!(links.iter().any(|(anchor, disposition)| {
                anchor.contains(&format!("/{link}"))
                    && matches!(disposition, crate::reason::NativeSourceDisposition::Blocked {
                        code, ..
                    } if *code == expected)
            }));
        }
        assert_eq!(
            links
                .iter()
                .filter(|(_, disposition)| matches!(
                    disposition,
                    crate::reason::NativeSourceDisposition::OutsideRequestedTheory { .. }
                ))
                .count(),
            3
        );
        assert_eq!(
            links
                .iter()
                .filter(|(_, disposition)| matches!(
                    disposition,
                    crate::reason::NativeSourceDisposition::StructuralSupport { .. }
                ))
                .count(),
            if executes { 2 } else { 0 }
        );
        if !executes {
            assert!(
                !admission
                    .complete_for(&result)
                    .expect("same source completion")
            );
        }
    }
}

fn decide(theory: &CompiledTheory) -> (gmeow_errors::Result<ReasoningResult>, ClaimIngress) {
    let (input, ingress) = ingest(theory);
    let domains = default_domain(&input);
    (
        reason_selected_theory_within(
            &whole_inventory(theory),
            input,
            &domains,
            &ingress,
            &UNBOUNDED,
        ),
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
/// in the ledger as read with their evidence. A confidence of zero is still a claim;
/// default-graph descriptive roles and rationale never become evidence or cancel assertion.
#[test]
fn evidence_only_claims_that_contradict_are_a_clash() {
    for body in [
        EVIDENCE_CLASH.to_owned(),
        EVIDENCE_CLASH.replace(
            " .",
            " ; gmeow:graphBoxRole gmeow:boxCBox, ex:futureRole, [] .",
        ),
        EVIDENCE_CLASH.replace(
            " .",
            r#" ; gmeow:statedRationale "stated reason", "motif"@fr, "سبب"@ar--rtl ."#,
        ),
    ] {
        let theory = compile(&body);
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
ex:described rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ;
    gmeow:graphBoxRole gmeow:boxABox, ex:futureRole .
ex:explained rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ;
    gmeow:statedRationale "someone said so" .
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
             [] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; gmeow:graphBoxRole gmeow:boxCBox ; {annotation} ."
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

/// PROTECTS: procedural negation may run through the generic selected-theory
/// entry, but its absence-based clash cannot become source-bound proof evidence.
#[test]
fn a_procedural_naf_clash_is_refused_before_source_proof_execution() {
    let theory = compile(
        r#"
ex:a ex:p ex:b .
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:absenceClash a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:x ex:p _:y )>> ] ;
    logic:negatedBody [ rdf:reifies <<( _:x ex:q _:y )>> ] .
"#,
    );
    let (procedural, _) = decide(&theory);
    let procedural = procedural.expect("the generic procedural selection still executes NAF");
    assert!(!consistent(&procedural));

    let inventory = SourceInventory::new(&theory, SourceSelection::all_classical_roots(&theory));
    let mut admission = NativeSourceAdmission::inspect(&inventory).expect("whole source admission");
    let (input, ingress) = ingest(&theory);
    let selected =
        prepare_selected_theory(&inventory, &ingress).expect("procedural lowering prepares");
    let domains = default_domain(&input);
    let error = reason_admitted_theory_within(
        &mut admission,
        &selected,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect_err("absence cannot authorize a classical refutation");
    let refusal = error
        .downcast_ref::<crate::error::NativeRefusal>()
        .expect("unsupported proof semantics has a typed refusal");
    assert!(
        refusal
            .gaps
            .iter()
            .any(|gap| { gap.contains("program:rule:0") && gap.contains("absenceClash") }),
        "{refusal:?}"
    );
    assert!(admission.drops().any(|drop| {
        drop.code == "NEGATION_AS_FAILURE_IS_NOT_CLASSICAL_NEGATION"
            && drop.detail.contains("absenceClash")
    }));
    assert!(
        admission.complete_for(&procedural).is_err(),
        "the refused operation issues no live source completion"
    );
}

/// PROTECTS: execution that ignores axiom scopes starts only from the input whose
/// claims were admitted for that program. An input the claims were never added to,
/// and a program with a scoped claim the ledger does not hold, are both refused.
#[test]
fn the_entry_refuses_an_input_or_program_the_ledger_does_not_account_for() {
    let theory = compile(EVIDENCE_CLASH);
    let (admitted, ingress) = ingest(&theory);

    let inventory = SourceInventory::new(&theory, SourceSelection::all_classical_roots(&theory));
    let mut admission = NativeSourceAdmission::inspect(&inventory).expect("whole source admission");
    let selected = prepare_selected_theory(&inventory, &ingress).expect("selected theory prepares");
    let stale = prepare_reasoning_input(theory.source().dataset()).expect("bare ingestion");
    let domains = default_domain(&stale);
    let error = reason_admitted_theory_within(
        &mut admission,
        &selected,
        stale,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
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
    let error = reason_selected_theory_within(
        &whole_inventory(&wider),
        admitted,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect_err("a scoped claim of this program is not in the ledger");
    assert!(
        error
            .to_string()
            .contains("different immutable compiler source")
    );
    // Equal scoped ledgers do not authorize a different unscoped source.
    let unscoped = compile("ex:a ex:knows ex:b .");
    let named_target = LogicalGraph::Named(purrdf::TermValue::iri("urn:claims:unselected"));
    let unscoped_inventory = whole_inventory(&unscoped);
    let selection = unscoped_inventory
        .resolve_theory_selection(&TheorySelection::top_level_only())
        .expect("source resolution");
    assert!(
        prepare_selected_theory_input(&unscoped_inventory, &named_target, &selection).is_err(),
        "the selected-theory profile does not execute a named target"
    );
    let (mut selected_input, own_ingress) = ingest(&unscoped);
    let selected_contract = *selected_input.ingress_contract();
    selected_input
        .assert_facts(
            &named_target,
            [Fact {
                subject: purrdf::TermValue::iri("urn:claims:subject"),
                predicate: "urn:claims:predicate".to_owned(),
                object: purrdf::TermValue::iri("urn:claims:object"),
            }],
        )
        .expect_err("a source edit cannot add an unselected world");
    assert_eq!(*selected_input.ingress_contract(), selected_contract);
    let generic = prepare_reasoning_input(unscoped.source().dataset()).expect("all-graph input");
    assert_ne!(
        generic.ingress_contract(),
        &selected_contract,
        "identical default facts do not erase the selected source-graph policy"
    );
    let own_selected =
        prepare_selected_theory(&unscoped_inventory, &own_ingress).expect("own preparation");
    let own_inventory =
        SourceInventory::new(&unscoped, SourceSelection::all_classical_roots(&unscoped));
    let mut own_admission =
        NativeSourceAdmission::inspect(&own_inventory).expect("own source admission");
    let generic_domains = default_domain(&generic);
    reason_admitted_theory_within(
        &mut own_admission,
        &own_selected,
        generic,
        &generic_domains,
        &own_ingress,
        &UNBOUNDED,
    )
    .expect_err("generic all-graph input cannot borrow selected default-graph admission");
    let other = compile("ex:c ex:knows ex:d .");
    let (input, ingress) = ingest(&unscoped);
    let inventory = SourceInventory::new(&other, SourceSelection::all_classical_roots(&other));
    let (_, other_ingress) = ingest(&other);
    let selected = prepare_selected_theory(&inventory, &other_ingress)
        .expect("other unscoped program prepares");
    let mut admission = NativeSourceAdmission::inspect(&inventory).expect("whole other source");
    let domains = default_domain(&input);
    let error = reason_admitted_theory_within(
        &mut admission,
        &selected,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect_err("equal scoped axioms cannot authorize a foreign source");
    assert!(
        error
            .to_string()
            .contains("different immutable compiler source")
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

const ACCORDING_TO: &str = "https://blackcatinformatics.ca/gmeow/accordingTo";

const BLANK: &str = "_:";
const CLAIM: &str = "https://example.org/claims/claim";

fn qualified(carrier: &str, predicate: &str, statements: usize) -> UnreadReason {
    UnreadReason::Qualified {
        carrier: carrier.to_owned(),
        predicate: predicate.to_owned(),
        graph: None,
        statements,
    }
}

fn mentioned(carrier: &str, by: &str, statements: usize) -> UnreadReason {
    UnreadReason::Mentioned {
        carrier: carrier.to_owned(),
        by: by.to_owned(),
        graph: None,
        statements,
    }
}

/// The reasons in program order. A blank carrier's label is assigned by the source
/// canonicalization, so it is reduced to [`BLANK`].
fn unread_reasons(ingress: &ClaimIngress) -> Vec<UnreadReason> {
    let anonymous = |name: &mut String| {
        if name.starts_with(BLANK) {
            BLANK.clone_into(name);
        }
    };
    ingress
        .unread()
        .iter()
        .map(|claim| {
            let mut reason = claim.reason.clone();
            match &mut reason {
                UnreadReason::Qualified { carrier, .. } => anonymous(carrier),
                UnreadReason::Mentioned { carrier, by, .. } => {
                    anonymous(carrier);
                    anonymous(by);
                }
                _ => {}
            }
            reason
        })
        .collect()
}

/// PROTECTS: a contradiction across standpoints is not a contradiction of the
/// theory. Two claims that contradict each other, each placed in its own standpoint
/// with `gmeow:accordingTo` and each carrying provenance, are not read: the verdict
/// is not inconsistent and both are named with the predicate that kept them out.
#[test]
fn claims_placed_in_standpoints_by_according_to_are_named_and_are_not_a_clash() {
    let theory = compile(
        r#"
[] rdf:reifies <<( ex:a logic:instanceOf ex:Left )>> ; gmeow:accordingTo ex:S1 ; logic:provenance ex:first .
[] rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; gmeow:accordingTo ex:S2 ; logic:provenance ex:second .
"#,
    );
    let (result, ingress) = decide(&theory);

    assert!(consistent(&result.expect("the selected theory executes")));
    assert!(ingress.read().is_empty());
    assert_eq!(
        unread_reasons(&ingress),
        [
            qualified(BLANK, ACCORDING_TO, 1),
            qualified(BLANK, ACCORDING_TO, 1)
        ]
    );
}

/// PROTECTS: the carrier contract stays closed around descriptive roles. Provenance
/// does not admit an independent qualification about its carrier: a
/// denial stance, an annotation this reader does not know, a second type, a
/// statement that points at the carrier, or any `logic:modality` value including
/// `logic:none`. Each such claim adds no fact and is named with the first offending
/// predicate and the number of offending statements.
#[test]
fn a_claim_with_an_independent_qualification_is_named_and_adds_no_fact() {
    const MODALITY: &str = "https://blackcatinformatics.ca/logic/modality";
    for (carrier, expected) in [
        (
            "gmeow:standpointModality gmeow:refuted ; logic:provenance ex:first",
            vec![qualified(
                CLAIM,
                "https://blackcatinformatics.ca/gmeow/standpointModality",
                1,
            )],
        ),
        (
            r#"ex:note "hearsay" ; ex:seen true ; logic:provenance ex:first"#,
            vec![qualified(CLAIM, "https://example.org/claims/note", 2)],
        ),
        (
            "a ex:Rumour ; logic:provenance ex:first",
            vec![qualified(
                CLAIM,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                1,
            )],
        ),
        (
            "logic:provenance ex:first . ex:denial ex:denies ex:claim",
            vec![qualified(CLAIM, "https://example.org/claims/denies", 1)],
        ),
        (
            "logic:modality logic:none ; logic:provenance ex:first",
            vec![qualified(CLAIM, MODALITY, 1)],
        ),
        (
            "logic:modality logic:none, logic:deontic ; logic:provenance ex:first",
            vec![
                UnreadReason::Context(vec!["logic:modality"]),
                qualified(CLAIM, MODALITY, 2),
            ],
        ),
    ] {
        let theory = compile(&format!(
            "ex:a logic:instanceOf ex:Left .
             ex:claim rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; gmeow:graphBoxRole gmeow:boxCBox ; {carrier} ."
        ));
        let (result, ingress) = decide(&theory);

        assert!(
            consistent(&result.expect("the selected theory executes")),
            "{carrier}"
        );
        assert!(ingress.read().is_empty(), "{carrier}");
        let mut reasons = unread_reasons(&ingress);
        reasons.sort_by_key(|reason| reason.to_string());
        let mut expected = expected;
        expected.sort_by_key(|reason| reason.to_string());
        assert_eq!(reasons, expected, "{carrier}");
    }
    // A raw annotation row without a recognised structural edge still carries a
    // source obligation. Source hashing alone cannot make its unknown law read.
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let subject = builder
        .intern_iri("urn:annotation:subject")
        .expect("source subject");
    let relation = builder
        .intern_iri("urn:annotation:kept")
        .expect("source relation");
    let value = builder
        .intern_iri("urn:annotation:value")
        .expect("source value");
    builder
        .push_quad(subject, relation, value, None)
        .expect("asserted source fact");
    let unknown = builder
        .intern_iri("https://blackcatinformatics.ca/logic/unregisteredBoundary")
        .expect("unknown interpreted relation");
    builder
        .push_annotation(subject, unknown, value)
        .expect("annotation-table obligation");
    let source = builder.freeze().expect("owned source freezes");
    let theory = PreparedLogicSource::new(&source)
        .expect("source prepares")
        .into_compiled(None)
        .expect("source compiles");
    let inventory = SourceInventory::new(&theory, SourceSelection::all_classical_roots(&theory));
    let mut admission = NativeSourceAdmission::inspect(&inventory).expect("whole source admission");
    let (input, ingress) = ingest(&theory);
    let selected = prepare_selected_theory(&inventory, &ingress).expect("source program prepares");
    let domains = default_domain(&input);
    let result = reason_admitted_theory_within(
        &mut admission,
        &selected,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect("native closure executes supported facts");
    assert!(consistent(&result));
    assert!(
        !admission
            .complete_for(&result)
            .expect("same live execution binding"),
        "the unaccounted annotation prevents a whole-source consistent claim"
    );
    assert!(
        admission
            .drops()
            .any(|drop| drop.code == "UNIT_NOT_ACCOUNTED"
                && drop.detail.contains("annotation")
                && drop.detail.contains("unregisteredBoundary"))
    );
}

/// PROTECTS: the classic `rdf:Statement` carrier follows the same rule as the RDF
/// 1.2 reifier. With only evidence about it, its statement is read and can clash;
/// placed in a standpoint, it is named and not read.
#[test]
fn a_classic_statement_carrier_is_read_only_when_it_is_evidence_only() {
    let classic = |name: &str, class: &str, extra: &str| {
        format!(
            "ex:{name} a rdf:Statement ; rdf:subject ex:a ; rdf:predicate logic:instanceOf ;
                 rdf:object ex:{class} ; logic:provenance ex:{name}Source {extra} .\n"
        )
    };

    let evidence_only = compile(&format!(
        "{}{}",
        classic(
            "first",
            "Left",
            "; gmeow:graphBoxRole gmeow:boxCBox, ex:futureRole"
        ),
        classic("second", "Right", "; logic:confidence 0.9")
    ));
    let (result, ingress) = decide(&evidence_only);
    assert!(!consistent(&result.expect("the selected theory executes")));
    assert_eq!(ingress.read().len(), 2);
    assert!(ingress.unread().is_empty());

    let placed = compile(&format!(
        "{}{}",
        classic("first", "Left", ""),
        classic("second", "Right", "; gmeow:accordingTo ex:S2")
    ));
    let (result, ingress) = decide(&placed);
    assert!(consistent(&result.expect("the selected theory executes")));
    assert_eq!(ingress.read().len(), 1);
    assert_eq!(
        unread_reasons(&ingress),
        [qualified(
            "https://example.org/claims/second",
            ACCORDING_TO,
            1
        )]
    );
}

const R1: &str = "https://example.org/claims/r1";
const R2: &str = "https://example.org/claims/r2";

/// An assertion, and an evidence-only claim `ex:r1` that would contradict it.
const CONTESTED: &str = r#"
ex:a logic:instanceOf ex:Left .
ex:r1 rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> ; logic:provenance ex:first ;
    gmeow:graphBoxRole gmeow:boxCBox .
"#;

/// PROTECTS: a statement about a claim keeps that claim out even when the statement
/// is itself only carried by another reifier. A second evidence-only claim that
/// refutes the first claim's reifier, or that names it as an object, sits inside a
/// triple term; the first claim is then not read, so the theory is not inconsistent,
/// and it is named with the reifier that mentions it. The same holds when the
/// mention is a bare quotation, is nested deeper in a triple term, is made by a
/// classic statement node, or is a plain statement that uses the reifier as its
/// predicate.
#[test]
fn a_claim_another_carried_statement_mentions_is_named_and_adds_no_fact() {
    for mention in [
        "ex:r2 rdf:reifies <<( ex:r1 gmeow:standpointModality gmeow:refuted )>> ; logic:provenance ex:second .",
        "ex:r2 rdf:reifies <<( ex:auditor ex:retracts ex:r1 )>> ; logic:provenance ex:audit .",
        "ex:r2 rdf:reifies <<( ex:auditor ex:retracts ex:r1 )>> .",
        "ex:r2 rdf:reifies <<( ex:auditor ex:says <<( ex:r1 ex:is ex:Wrong )>> )>> ; logic:provenance ex:audit .",
        "ex:r2 a rdf:Statement ; rdf:subject ex:r1 ; rdf:predicate ex:is ; rdf:object ex:Wrong ; logic:provenance ex:audit .",
        "ex:r2 ex:r1 ex:Wrong .",
    ] {
        let theory = compile(&format!("{CONTESTED}{mention}"));
        let (result, ingress) = decide(&theory);

        assert!(
            consistent(&result.expect("the selected theory executes")),
            "{mention}"
        );
        assert_eq!(
            unread_reasons(&ingress),
            [mentioned(R1, R2, 1)],
            "{mention}"
        );
    }
}

/// PROTECTS: a mention counts whoever makes it, so no fixpoint is taken. In a chain
/// where `ex:r3` speaks about `ex:r2` and `ex:r2` speaks about `ex:r1`, both
/// mentioned claims stay out, although the claim that mentions `ex:r1` is itself
/// not read; only the unmentioned `ex:r3` is read.
#[test]
fn a_chain_of_claims_about_claims_reads_only_the_unmentioned_one() {
    let theory = compile(&format!(
        "{CONTESTED}
         ex:r2 rdf:reifies <<( ex:r1 gmeow:standpointModality gmeow:refuted )>> ; logic:provenance ex:second .
         ex:r3 rdf:reifies <<( ex:r2 gmeow:standpointModality gmeow:refuted )>> ; logic:provenance ex:third ."
    ));
    let (result, ingress) = decide(&theory);

    assert!(consistent(&result.expect("the selected theory executes")));
    assert_eq!(ingress.read().len(), 1);
    let mut reasons = unread_reasons(&ingress);
    reasons.sort_by_key(|reason| reason.to_string());
    assert_eq!(
        reasons,
        [
            mentioned(R1, R2, 1),
            mentioned(R2, "https://example.org/claims/r3", 1)
        ]
    );
}

/// PROTECTS: only a mention of a claim's carrier keeps a claim out. A carried
/// statement about an ordinary resource, here the source of the first claim, leaves
/// both claims read, and the contradiction is found.
#[test]
fn a_claim_about_an_ordinary_resource_leaves_other_claims_read() {
    let theory = compile(&format!(
        "{CONTESTED}
         ex:r2 rdf:reifies <<( ex:auditor ex:trusts ex:first )>> ; logic:provenance ex:audit ."
    ));
    let (result, ingress) = decide(&theory);

    assert!(!consistent(&result.expect("the selected theory executes")));
    assert_eq!(ingress.read().len(), 2);
    assert!(ingress.unread().is_empty());
}

/// PROTECTS: what a named graph says about a claim's carrier keeps the claim out
/// and never lets it in. A refutation carried by a reifier in a named graph, a
/// plain statement about the carrier there, and the carrier's use as a graph name
/// each leave the claim unread, named with the graph. An allow-listed statement
/// about the carrier in a named graph is neither evidence nor an obstacle: the
/// claim is read on its default-graph evidence alone, and a reifier whose only
/// evidence is in a named graph stays a quotation.
#[test]
fn a_named_graph_can_keep_a_claim_out_but_is_never_its_evidence() {
    let graph = Some("https://example.org/claims/g".to_owned());
    for (named, expected) in [
        (
            "ex:g { ex:r2 rdf:reifies <<( ex:r1 gmeow:standpointModality gmeow:refuted )>> . }",
            UnreadReason::Mentioned {
                carrier: R1.to_owned(),
                by: R2.to_owned(),
                graph: graph.clone(),
                statements: 1,
            },
        ),
        (
            "ex:g { ex:r1 gmeow:accordingTo ex:S1 . }",
            UnreadReason::Qualified {
                carrier: R1.to_owned(),
                predicate: ACCORDING_TO.to_owned(),
                graph: graph.clone(),
                statements: 1,
            },
        ),
        (
            "ex:r1 { ex:b logic:instanceOf ex:Left . }",
            UnreadReason::GraphName {
                carrier: R1.to_owned(),
            },
        ),
    ] {
        let theory = compile(&format!("{CONTESTED}{named}"));
        let (result, ingress) = decide(&theory);

        assert!(
            consistent(&result.expect("the selected theory executes")),
            "{named}"
        );
        assert_eq!(unread_reasons(&ingress), [expected], "{named}");
    }

    let theory = compile(&format!(
        "{CONTESTED}
         ex:quoted rdf:reifies <<( ex:a logic:instanceOf ex:Right )>> .
         ex:g {{ ex:r1 logic:confidence 0.9 . ex:quoted logic:provenance ex:second . }}"
    ));
    let (result, ingress) = decide(&theory);
    assert!(!consistent(&result.expect("the selected theory executes")));
    assert_eq!(ingress.read().len(), 1);
    assert!(ingress.unread().is_empty());
}

/// PROTECTS: a preparation is executed only for the theory and selection it was made
/// from. The ingress and input of the run authorize the second theory, so only the
/// preparation's own identity can refuse the first theory's lowering; the same
/// preparation still runs for its own theory.
#[test]
fn a_preparation_made_from_another_theory_is_refused() {
    let theory = compile(EVIDENCE_CLASH);
    let other = compile(&format!(
        "{EVIDENCE_CLASH}\nex:Third logic:disjointWith ex:Fourth ."
    ));
    let (_, other_ingress) = ingest(&other);
    let foreign = prepare_selected_theory(&whole_inventory(&other), &other_ingress)
        .expect("the other theory prepares");

    let inventory = SourceInventory::new(&theory, SourceSelection::all_classical_roots(&theory));
    let mut admission = NativeSourceAdmission::inspect(&inventory).expect("whole source admission");
    let (input, ingress) = ingest(&theory);
    let domains = default_domain(&input);
    let refusal = reason_admitted_theory_within(
        &mut admission,
        &foreign,
        input,
        &domains,
        &ingress,
        &UNBOUNDED,
    )
    .expect_err("a preparation of another theory must not run");
    assert!(
        refusal
            .to_string()
            .contains("different immutable compiler source"),
        "{refusal}"
    );

    let own = prepare_selected_theory(&inventory, &ingress).expect("the theory prepares");
    let (input, ingress) = ingest(&theory);
    let domains = default_domain(&input);
    let mut result =
        reason_admitted_theory_within(&mut admission, &own, input, &domains, &ingress, &UNBOUNDED)
            .expect("its own preparation runs");
    assert!(!consistent(&result));
    admission
        .complete_for(&result)
        .expect("actual same-run source binding validates");
    let decoded: ReasoningResult = serde_json::from_slice(
        &serde_json::to_vec(result.as_ref()).expect("execution observation serializes"),
    )
    .expect("execution observation decodes");
    assert!(
        admission.complete_for(&decoded).is_err(),
        "decoded matching bytes cannot supply the actual live result"
    );
    std::sync::Arc::make_mut(&mut result).information =
        crate::result::InformationState::Undetermined;
    assert!(
        admission.complete_for(&result).is_err(),
        "editing the result cannot preserve the original live completion"
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.rules = gmeow_logic_compile::frontend::RootSelection::None;
    let narrowed = SourceInventory::new(&theory, selection);
    assert!(
        NativeSourceAdmission::inspect(&narrowed).is_err(),
        "source filtering requires its own executor before a narrowed source can be admitted"
    );
}
