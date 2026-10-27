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

    let mut reference = include_str!("fixtures/semantic-unit-identities-v1.txt").lines();
    for unit in &admission.units {
        let preimage = purrdf_lex::json::record::to_vec(&(
            unit.kind,
            &unit.anchor,
            &unit.documents,
            &unit.disposition,
        ));
        let original_preimage = reference
            .next()
            .and_then(|line| line.strip_prefix("UNIT_PREIMAGE="))
            .expect("original complete semantic-unit preimage");
        assert_eq!(preimage, original_preimage.as_bytes());
        let original_id = reference
            .next()
            .and_then(|line| line.strip_prefix("UNIT_ID="))
            .expect("actual original semantic-unit ID");
        assert_eq!(unit.id, original_id);
    }
    assert!(
        reference.next().is_none(),
        "complete original unit reference consumed"
    );

    use crate::frontend::{
        AdmittedSemanticUnit, PropertyCharacteristic, SemanticUnitKind, SourceAssertion,
        SourceClassDefinition,
    };
    use crate::ir::{
        AtomicTerm, ComplexityClass, Formula, ReasoningContract, SemanticProfileId, Term,
    };
    use std::collections::BTreeMap;

    let literal = purrdf::RdfLiteral {
        lexical_form: "raw literal\0中".into(),
        datatype: None,
        language: Some("fr".into()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let assertion = SourceAssertion {
        subject: AtomicTerm::Var("?raw\0中".into()),
        predicate: "raw predicate".into(),
        object: AtomicTerm::Literal(literal.clone()),
    };
    let formula = Formula::Forall {
        vars: vec!["z".into(), "a".into(), "z".into()],
        body: Box::new(Formula::Atom {
            relation: Term::Iri("urn:raw:relation".into()),
            args: vec![Term::Var("?raw".into()), Term::Literal(literal)],
        }),
    };
    let definition = SourceClassDefinition {
        formula: formula.clone(),
        constants: BTreeMap::from([
            (
                "z literal\0中".into(),
                AtomicTerm::Literal(purrdf::RdfLiteral::typed(
                    "NaN",
                    "http://www.w3.org/2001/XMLSchema#double",
                )),
            ),
            ("a iri".into(), AtomicTerm::Iri("?raw explicit iri".into())),
            ("b blank".into(), AtomicTerm::Blank("raw blank\0中".into())),
            (
                "c variable".into(),
                AtomicTerm::Var("urn:raw:variable".into()),
            ),
        ]),
    };
    let declaration = SourceContractDeclaration {
        configuration: ReasoningContract {
            preset: Some(SemanticProfileId::StableModel),
            model_semantics: Some("raw semantics\0中".into()),
            negation_operators: BTreeSet::from(["z negation".into(), "a negation".into()]),
            closure_entries: BTreeMap::from([
                ("z predicate\0中".into(), "raw closure z".into()),
                ("a predicate".into(), "raw closure a".into()),
            ]),
            complexity: Some(
                ComplexityClass::new(" raw complexity\0中 ")
                    .expect("nonempty authored complexity label retains its bytes"),
            ),
            ..ReasoningContract::default()
        },
        facts: vec![
            assertion.clone(),
            SourceAssertion {
                subject: AtomicTerm::Blank("another blank".into()),
                predicate: "another predicate".into(),
                object: AtomicTerm::Iri("another explicit iri".into()),
            },
            assertion.clone(),
        ],
    };
    let dispositions = [
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Assertion(assertion)),
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Formula { index: 17 }),
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Rule { index: 29 }),
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::PropertyCharacteristic(
            PropertyCharacteristic {
                property: "raw property\0中".into(),
                characteristic: "raw characteristic".into(),
            },
        )),
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::ClassDefinition(definition)),
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::ClassDefinition(
            SourceClassDefinition {
                formula,
                constants: BTreeMap::new(),
            },
        )),
        SemanticUnitDisposition::ConfigurationDeclaration {
            declaration: Box::new(declaration),
            reason: "raw declaration reason\0中".into(),
        },
        SemanticUnitDisposition::ConfigurationDeclaration {
            declaration: Box::new(SourceContractDeclaration {
                configuration: ReasoningContract::default(),
                facts: Vec::new(),
            }),
            reason: String::new(),
        },
        SemanticUnitDisposition::StructuralSupport {
            detail: "raw support\0中".into(),
        },
        SemanticUnitDisposition::OutsideSelection {
            reason: "raw exclusion\0中".into(),
        },
        SemanticUnitDisposition::QuotedUnasserted {
            detail: "raw quotation\0中".into(),
        },
        SemanticUnitDisposition::Blocked {
            code: "raw blocked code".into(),
            reason: "raw blocked reason\0中".into(),
        },
        SemanticUnitDisposition::Malformed {
            code: "raw malformed code".into(),
            reason: "raw malformed reason\0中".into(),
        },
        SemanticUnitDisposition::Observed {
            detail: "raw observation\0中".into(),
        },
    ];
    let kinds = [
        SemanticUnitKind::Assertion,
        SemanticUnitKind::Formula,
        SemanticUnitKind::Rule,
        SemanticUnitKind::PropertyCharacteristic,
        SemanticUnitKind::ClassDefinition,
        SemanticUnitKind::Import,
        SemanticUnitKind::StructuralSyntax,
        SemanticUnitKind::Quotation,
        SemanticUnitKind::Diagnostic,
        SemanticUnitKind::Owner,
    ];
    let atlas: Vec<_> = dispositions
        .iter()
        .enumerate()
        .map(|(index, disposition)| {
            let documents = if index % 2 == 0 {
                BTreeSet::new()
            } else {
                BTreeSet::from(["z source\0中.ttl".to_owned(), "a source.ttl".to_owned()])
            };
            (
                kinds[index % kinds.len()],
                format!("raw anchor {index}\0中"),
                documents,
                disposition,
            )
        })
        .collect();
    let atlas_bytes = purrdf_lex::json::record::to_vec(&atlas);
    assert!(!atlas_bytes.ends_with(b"\n"));
    assert_eq!(
        atlas_bytes,
        include_bytes!("fixtures/semantic-unit-atlas-v1.json")
            .strip_suffix(b"\n")
            .expect("original atlas reference has one storage LF")
    );
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
fn probabilistic_preset_is_retained_as_metatheory_without_a_selected_model() {
    for typing in ["a", "logic:instanceOf"] {
        let (admission, projection) = project(&format!(
            "{PREFIXES}
             logic:ProbabilisticProfile {typing} logic:ReasoningPreset ;
                 logic:expandsToFacet logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure .
             ex:subject ex:property ex:object ."
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{typing}: {:?}",
            projection.blockers
        );
        let declarations = declarations(&admission);
        assert_eq!(declarations.len(), 1);
        assert!(
            declarations[0]
                .configuration
                .uncertainty_measures
                .contains("ProbabilisticMeasure")
        );
        assert!(declarations[0].facts.iter().any(|fact| {
            fact.predicate == "https://blackcatinformatics.ca/logic/expandsToFacet"
                && matches!(&fact.object, crate::ir::AtomicTerm::Iri(iri)
                    if iri == "https://blackcatinformatics.ca/logic/ProbabilisticMeasure")
        }));
        let problem = projection.problem.unwrap();
        assert!(problem.contains("https://example.org/property"));
        assert!(!problem.contains("ProbabilisticMeasure"));
        assert!(!problem.contains("ProbabilisticProfile"));
    }
}

#[test]
fn redundant_contract_typing_keeps_the_same_probabilistic_catalogue() {
    let (admission, projection) = project(&format!(
        "{PREFIXES}
         logic:ProbabilisticProfile a logic:ReasoningPreset, logic:ReasoningContract ;
             logic:expandsToFacet logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .
         ex:subject ex:property ex:object ."
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    assert_eq!(declarations(&admission).len(), 1);
    assert!(!projection.problem.unwrap().contains("ProbabilisticMeasure"));
}

#[test]
fn direct_probabilistic_contract_is_a_declaration_without_executing_a_query() {
    let (admission, projection) = project(&format!(
        "{PREFIXES}
         ex:contract a logic:ReasoningContract ; logic:uncertaintyMeasure logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .
         ex:subject ex:property ex:object ."
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    assert_eq!(declarations(&admission).len(), 1);
    assert!(!projection.problem.unwrap().contains("/uncertaintyMeasure"));
}

#[test]
fn facet_support_does_not_consume_other_facts_about_the_same_resource() {
    let (_, projection) = project(&format!(
        "{PREFIXES}
         logic:ProbabilisticProfile a logic:ReasoningPreset ; logic:expandsToFacet logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure, ex:DomainFacet ; ex:observed ex:value ."
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let problem = projection.problem.unwrap();
    assert!(problem.contains("https://example.org/DomainFacet"));
    assert!(problem.contains("https://example.org/observed"));
    assert!(!problem.contains("/UncertaintyMeasure"));
}

#[test]
fn probabilistic_preset_catalogue_alone_cannot_authorize_a_proof() {
    let (admission, projection) = project(&format!(
        "{PREFIXES}
         logic:ProbabilisticProfile a logic:ReasoningPreset ; logic:expandsToFacet logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure ."
    ));
    assert_eq!(declarations(&admission).len(), 1);
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

/// Every singleton facet rejects a source that would otherwise choose its last value.
#[test]
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
            theory.diagnostics().iter().any(|diagnostic| diagnostic.code
                == "INVALID_CONTRACT_FACET"
                && diagnostic.severity == crate::frontend::Severity::Error
                && diagnostic.subject.as_deref() == Some("https://example.org/contract")
                && diagnostic.message.contains("first")
                && diagnostic.message.contains("second")
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
