// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{SemanticUnitDisposition, SourceContractDeclaration};

const PREFIXES: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
"#;

/// Inspect the native declarations carried by actual complete source admission.
fn declarations(admission: &SourceAdmission) -> Vec<&SourceContractDeclaration> {
    admission.configuration_declarations().collect()
}

#[test]
/// A catalogue may describe different disciplines without executing all of them.
fn configuration_declarations_preserve_facets_without_switching_proof_semantics() {
    let source = format!(
        r#"{PREFIXES}
ex:classical a logic:ReasoningContract ; logic:modelSemantics logic:ClassicalSemantics .
ex:stable a logic:ReasoningContract ; logic:modelSemantics logic:StableModelSemantics ;
    logic:negationOperator logic:DefaultNegation .
ex:subject ex:property ex:object .
"#
    );
    let (admission, projection) = project(&source);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let declarations = declarations(&admission);
    assert_eq!(declarations.len(), 2);
    assert!(declarations.iter().any(|declaration| {
        declaration.configuration.model_semantics.as_deref() == Some("StableModelSemantics")
    }));
    assert!(admission.profile.world.contains("classical"));
    assert!(admission.profile.contracts.contains("metatheory catalogue"));
    assert_eq!(projection.profile_id, crate::frontend::FOF_SOURCE_PROFILE);
    let body = projection.problem.unwrap();
    for name in [
        "ClassicalSemantics",
        "StableModelSemantics",
        "DefaultNegation",
    ] {
        assert!(
            !body.contains(name),
            "metatheory configuration {name} leaked into the object theory"
        );
    }
    assert!(body.contains("https://example.org/property"));
    for unit in admission.units.iter().filter(|unit| {
        matches!(
            unit.disposition,
            SemanticUnitDisposition::ConfigurationDeclaration { .. }
        )
    }) {
        let facts: Vec<_> = projection
            .sentences
            .iter()
            .filter(|sentence| sentence.source_units.contains(&unit.id))
            .collect();
        assert!(
            facts.is_empty(),
            "catalogue evidence cannot become FOF axioms"
        );
        let SemanticUnitDisposition::ConfigurationDeclaration {
            declaration,
            reason,
        } = &unit.disposition
        else {
            unreachable!()
        };
        assert!(!declaration.facts.is_empty());
        assert!(
            reason.contains("does not request their execution"),
            "the proof task must explain its configuration disposition"
        );
    }
}

#[test]
/// Closed-world settings remain facts about their configuration, not global NAF.
fn closure_entries_retain_blank_identity_keys_and_native_configuration() {
    let (admission, projection) = project(&format!(
        r#"{PREFIXES}
ex:contract a logic:ReasoningContract ; logic:closureEntry [
    a logic:ClosureEntry ; logic:closureKey "https://example.org/p" ;
    logic:closureValue logic:ClosedWorldClosure ] .
ex:subject ex:property ex:object .
"#
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let declarations = declarations(&admission);
    assert_eq!(declarations.len(), 1);
    let declaration = declarations[0];
    assert_eq!(
        declaration
            .configuration
            .closure_entries
            .get("https://example.org/p")
            .map(String::as_str),
        Some("ClosedWorldClosure")
    );
    let link = declaration
        .facts
        .iter()
        .find(|fact| fact.predicate.ends_with("/closureEntry"))
        .unwrap();
    assert!(matches!(&link.object, crate::ir::AtomicTerm::Blank(_)));
    let key = declaration
        .facts
        .iter()
        .find(|fact| fact.predicate.ends_with("/closureKey"))
        .unwrap();
    assert_eq!(key.subject, link.object);
    assert!(!projection.problem.unwrap().contains("ClosedWorldClosure"));
}

#[test]
/// The native declaration cannot depend on an excluded support document.
fn contract_support_requires_selected_closure_and_facet_type_facts() {
    for (root, support) in [
        (
            "ex:contract a logic:ReasoningContract ; logic:closureEntry ex:entry .",
            "ex:entry a logic:ClosureEntry ; logic:closureKey \"p\" ; logic:closureValue logic:OpenWorldClosure .",
        ),
        (
            "ex:contract a logic:ReasoningContract ; logic:expandsToFacet logic:ClassicalSemantics .",
            "logic:ClassicalSemantics a logic:ModelSemantics .",
        ),
    ] {
        let theory = compile_documents(&[
            (
                "contract.ttl",
                &format!("{PREFIXES}{root}\nex:s ex:p ex:o ."),
                None,
            ),
            ("support.ttl", &format!("{PREFIXES}{support}"), None),
        ]);
        let mut selection = SourceSelection::all_classical_roots(&theory);
        let full = SourceAdmission::classical_fof(&theory, selection.clone());
        assert_eq!(
            project_tptp_fof(&theory, &full).status,
            FofProjectionStatus::Complete
        );
        selection.documents = DocumentSelection::Exact(BTreeSet::from(["contract.ttl".into()]));
        let admission = SourceAdmission::classical_fof(&theory, selection);
        assert!(declarations(&admission).is_empty());
        let projection = project_tptp_fof(&theory, &admission);
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
        assert!(
            projection
                .blockers
                .iter()
                .any(|blocker| blocker.code == "UNSELECTED_CONTRACT_SUPPORT")
        );
    }
}

#[test]
/// Preserved metatheory cannot authorize a vacuous object-theory proof problem.
fn configuration_catalogue_alone_does_not_authorize_a_proof() {
    let (admission, projection) = project(&format!(
        "{PREFIXES}ex:contract a logic:ReasoningContract ."
    ));
    assert_eq!(admission.configuration_declarations().count(), 1);
    assert_eq!(admission.admitted().count(), 0);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| blocker.code == "EMPTY_SELECTED_THEORY")
    );
}

#[test]
/// Every singleton facet rejects a source that would otherwise choose its last value.
fn conflicting_singleton_contract_facets_fail_native_admission() {
    for property in [
        "formulaFragment",
        "modelSemantics",
        "truthAlgebra",
        "admissibleValuation",
        "designatedValues",
        "evolution",
        "argumentation",
        "revision",
        "equalityPolicy",
        "defaultClosure",
    ] {
        let theory = compile(&format!(
            "{PREFIXES}ex:contract a logic:ReasoningContract ; logic:{property} ex:first, ex:second ."
        ));
        assert!(
            theory
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "INVALID_CONTRACT_FACET"
                    && diagnostic.message.contains("conflicting values")),
            "the native reader must reject conflicting {property}"
        );
        let admission =
            SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
        assert_eq!(admission.configuration_declarations().count(), 0);
        assert_eq!(
            project_tptp_fof(&theory, &admission).status,
            FofProjectionStatus::Blocked
        );
    }
}

#[test]
/// Closure-map conflicts fail even though each individual entry is well formed.
fn conflicting_closure_values_do_not_get_an_arbitrary_native_winner() {
    let theory = compile(&format!(
        r#"{PREFIXES}
ex:contract a logic:ReasoningContract ; logic:closureEntry
    [ logic:closureKey "p" ; logic:closureValue logic:OpenWorldClosure ],
    [ logic:closureKey "p" ; logic:closureValue logic:ClosedWorldClosure ] .
"#
    ));
    assert!(
        theory
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "AMBIGUOUS_CLOSURE_ENTRY"
                && diagnostic.message.contains("conflicting closure values"))
    );
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(admission.configuration_declarations().count(), 0);
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Blocked
    );
}

#[test]
/// Ambiguity inside one entry is rejected before assembling the closure map.
fn closure_entries_cannot_choose_one_of_several_keys_or_values() {
    for fields in [
        "logic:closureKey \"p\", \"q\" ; logic:closureValue logic:OpenWorldClosure",
        "logic:closureKey \"p\" ; logic:closureValue logic:OpenWorldClosure, logic:ClosedWorldClosure",
    ] {
        let theory = compile(&format!(
            "{PREFIXES}ex:contract a logic:ReasoningContract ; logic:closureEntry [ {fields} ] ."
        ));
        assert!(
            theory
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "AMBIGUOUS_CLOSURE_ENTRY"
                    && diagnostic.message.contains("multiple distinct"))
        );
        let admission =
            SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
        assert_eq!(admission.configuration_declarations().count(), 0);
        assert_eq!(
            project_tptp_fof(&theory, &admission).status,
            FofProjectionStatus::Blocked
        );
    }
}

#[test]
/// Repeated equal declarations and set-valued selections preserve all source evidence.
fn repeated_values_and_set_valued_facets_remain_valid() {
    let (admission, projection) = project(&format!(
        r#"{PREFIXES}
ex:contract a logic:ReasoningContract ;
    logic:modelSemantics logic:ClassicalSemantics ;
    logic:expandsToFacet logic:ClassicalSemantics ;
    logic:resourcePolicy logic:BudgetBoundedResource, logic:CertifiedFragmentResource ;
    logic:closureEntry
        [ logic:closureKey "p" ; logic:closureValue logic:OpenWorldClosure ],
        [ logic:closureKey "p" ; logic:closureValue logic:OpenWorldClosure ] .
logic:ClassicalSemantics a logic:ModelSemantics .
ex:subject ex:property ex:object .
"#
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let declarations = declarations(&admission);
    assert_eq!(declarations.len(), 1);
    let declaration = declarations[0];
    assert_eq!(
        declaration.configuration.model_semantics.as_deref(),
        Some("ClassicalSemantics")
    );
    assert_eq!(
        declaration.configuration.resource_policies,
        BTreeSet::from([
            "BudgetBoundedResource".into(),
            "CertifiedFragmentResource".into()
        ])
    );
    assert_eq!(declaration.configuration.closure_entries.len(), 1);
    assert_eq!(
        declaration
            .facts
            .iter()
            .filter(|fact| fact.predicate.ends_with("/closureKey"))
            .count(),
        2
    );
}

#[test]
/// Declaration support does not authorize context erasure or unrelated owners.
fn scoped_contracts_and_other_unsupported_owner_families_remain_blocked() {
    for source in [
        "ex:contract a logic:ReasoningContract ; logic:standpoint ex:observer .",
        "ex:path a logic:PathShape ; logic:pathStepPredicate ex:property .",
    ] {
        let (_, projection) = project(&format!("{PREFIXES}{source}"));
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
        assert!(projection.problem.is_none());
        assert!(projection.blockers.iter().any(|blocker| matches!(
            blocker.code.as_str(),
            "CONTEXT_ERASURE_FORBIDDEN" | "UNSUPPORTED_SELECTED_OWNER"
        )));
    }
}
