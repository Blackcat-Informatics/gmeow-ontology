// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{RootSelection, SemanticUnitDisposition, SemanticUnitKind};

const CONTRACT: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
ex:contract a logic:ReasoningContract .
"#;

const DENIAL: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
ex:ban a logic:ForbiddenPatternConstraint ;
    logic:onClass ex:Guard ; logic:forbiddenPredicate ex:edge ;
    logic:formalizes ex:Guard .
"#;

/// The selected source owner contributes a denial rather than its RDF syntax.
#[test]
fn selected_forbidden_pattern_projects_its_canonical_denial() {
    for (pin, existential) in [
        ("", true),
        ("ex:ban logic:forbiddenValue ex:blocked .", false),
    ] {
        let theory = compile(&format!("{DENIAL}\n{pin}"));
        let publication = crate::tptp::CanonAdmissionPublication::from_compiled_source(&theory);
        let admission = publication.admission();
        let projection = project_tptp_fof(&theory, admission);
        let owner = admission
            .units
            .iter()
            .find(|unit| unit.kind == SemanticUnitKind::Owner)
            .expect("denial owner is inventoried");
        assert!(matches!(
            owner.disposition,
            SemanticUnitDisposition::Admitted(
                crate::frontend::AdmittedSemanticUnit::ForbiddenPattern { .. }
            )
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{:?}",
            projection.blockers
        );
        let sentence = projection
            .sentences
            .iter()
            .find(|sentence| sentence.source_units.contains(&owner.id))
            .expect("the projected denial retains its owner identity");
        assert!(sentence.body.contains(" => (~ "));
        assert_eq!(sentence.body.contains("? ["), existential);
        let observed = crate::tptp::CanonAdmissionPublication::from_bytes(
            &publication.to_bytes().expect("encode the admitted denial"),
        )
        .expect("decode the admitted denial observation");
        assert!(
            observed
                .admission()
                .units
                .iter()
                .any(|unit| unit.id == owner.id && unit.disposition == owner.disposition)
        );
        assert!(!observed.admission().matches(&theory));
    }
}

/// Excluding a compact owner does not turn its failure metadata into domain facts.
#[test]
fn excluded_forbidden_pattern_keeps_its_exact_support_outside_the_problem() {
    let theory = compile(&format!(
        "{DENIAL}\nex:ban <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:ForbiddenValue .\nex:item ex:separateFact ex:value ."
    ));
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.owners = RootSelection::None;
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let field = admission
        .units
        .iter()
        .find(|unit| unit.anchor.contains("/enforcesFailureClass"))
        .expect("the exact failure metadata row is inventoried");
    assert!(matches!(
        field.disposition,
        SemanticUnitDisposition::OutsideSelection { .. }
    ));
    assert!(!projection.sentences.iter().any(|sentence| {
        sentence.source_units.contains(&field.id) || sentence.body.contains("enforcesFailureClass")
    }));
    assert!(
        projection
            .sentences
            .iter()
            .any(|sentence| sentence.body.contains("separateFact"))
    );
}

/// Each consumed field must remain in the selected source, without erasing scope.
#[test]
fn forbidden_pattern_projection_refuses_unread_or_scoped_fields() {
    let theory = compile_documents(&[
        (
            "owner.ttl",
            &DENIAL.replace("logic:forbiddenPredicate ex:edge ;", ""),
            None,
        ),
        (
            "edge.ttl",
            "<https://example.org/ban> <https://blackcatinformatics.ca/logic/forbiddenPredicate> <https://example.org/edge> .",
            None,
        ),
    ]);
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["owner.ttl".to_owned()]));
    let unread = SourceAdmission::classical_fof(&theory, selection);
    assert!(
        unread
            .units
            .iter()
            .any(|unit| unit.kind == SemanticUnitKind::Owner
                && matches!(&unit.disposition, SemanticUnitDisposition::Blocked { code, .. }
            if code == "UNADMITTED_FORBIDDEN_PATTERN_SUPPORT"))
    );
    assert_eq!(
        project_tptp_fof(&theory, &unread).status,
        FofProjectionStatus::Blocked
    );

    let (scoped, projection) = project(&format!("{DENIAL}\nex:ban logic:inContext ex:world ."));
    assert!(
        scoped
            .units
            .iter()
            .any(|unit| unit.kind == SemanticUnitKind::Owner
                && matches!(&unit.disposition, SemanticUnitDisposition::Blocked { code, .. }
            if code == "CONTEXT_ERASURE_FORBIDDEN"))
    );
    assert_eq!(projection.status, FofProjectionStatus::Blocked);

    for predicate in ["logic:termEqual", "logic:termGreater"] {
        let (procedural, projection) = project(&format!(
            "{}\nex:ban logic:forbiddenValue 0 .",
            DENIAL.replace("ex:edge", predicate),
        ));
        assert!(
            procedural
                .units
                .iter()
                .any(|unit| unit.kind == SemanticUnitKind::Owner
                    && matches!(&unit.disposition, SemanticUnitDisposition::Blocked { code, .. }
                if code == "UNSUPPORTED_FORBIDDEN_PATTERN"))
        );
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
    }
}

#[test]
/// Root exclusion remains a reported decision, never an absent owner row.
fn excluded_owner_roots_remain_in_the_complete_admission_inventory() {
    let theory = compile(CONTRACT);
    let all = SourceSelection::all_classical_roots(&theory);
    let selected = SourceAdmission::classical_fof(&theory, all.clone());
    let owner = selected
        .units
        .iter()
        .find(|unit| unit.kind == SemanticUnitKind::Owner)
        .expect("selected contract owner");
    assert!(matches!(
        &owner.disposition,
        SemanticUnitDisposition::ConfigurationDeclaration { .. }
    ));

    for roots in [RootSelection::None, RootSelection::Exact(BTreeSet::new())] {
        let mut selection = all.clone();
        selection.owners = roots;
        let admission = SourceAdmission::classical_fof(&theory, selection);
        let excluded = admission
            .units
            .iter()
            .find(|unit| unit.kind == SemanticUnitKind::Owner)
            .expect("explicitly excluded owner remains inventoried");
        assert_eq!(excluded.anchor, owner.anchor);
        assert_eq!(excluded.documents, owner.documents);
        assert!(
            matches!(&excluded.disposition, SemanticUnitDisposition::OutsideSelection { reason }
            if reason.contains("root set"))
        );
        assert_ne!(selected.selection_digest, admission.selection_digest);
        assert!(admission.matches(&theory));
    }

    let documents = [
        DocumentSelection::AnonymousSource,
        DocumentSelection::Exact(BTreeSet::new()),
        DocumentSelection::Exact(BTreeSet::from([
            "z source\0中.ttl".to_owned(),
            "a source.ttl".to_owned(),
        ])),
    ];
    let roots = [
        RootSelection::None,
        RootSelection::All,
        RootSelection::Exact(BTreeSet::new()),
        RootSelection::Exact(BTreeSet::from([
            "urn:root:z\0中".to_owned(),
            "urn:root:a".to_owned(),
        ])),
    ];
    let mut reference = include_str!("fixtures/source-selection-identities-v1.txt").lines();
    for documents in documents {
        for index in 0..roots.len() {
            let selection = SourceSelection {
                graph: crate::frontend::SourceGraphSelection::DefaultGraph,
                documents: documents.clone(),
                formulas: roots[index].clone(),
                rules: roots[(index + 1) % roots.len()].clone(),
                owners: roots[(index + 2) % roots.len()].clone(),
            };
            let preimage = purrdf_lex::json::record::to_vec(&selection);
            let original_preimage = reference
                .next()
                .and_then(|line| line.strip_prefix("SELECTION_PREIMAGE="))
                .expect("original complete selection preimage");
            assert_eq!(preimage, original_preimage.as_bytes());
            let original_digest = reference
                .next()
                .and_then(|line| line.strip_prefix("SELECTION_DIGEST="))
                .expect("original actual selection digest");
            assert_eq!(
                selection.digest(),
                original_digest,
                "the exact selection preimage and hash domain are stable"
            );
        }
    }
    assert!(
        reference.next().is_none(),
        "complete original selection reference consumed"
    );
}

#[test]
/// Document selection retains the excluded declaration's original source receipt.
fn excluded_owner_documents_keep_their_source_receipt() {
    let theory = compile_documents(&[
        (
            "domain.ttl",
            "<https://example.org/s> <https://example.org/p> <https://example.org/o> .",
            Some(SourceBase {
                iri: "https://example.org/source-base/中/".into(),
                origin: SourceBaseOrigin::Caller,
            }),
        ),
        ("contract.ttl", CONTRACT, None),
    ]);
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["domain.ttl".into()]));
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let owner = admission
        .units
        .iter()
        .find(|unit| unit.kind == SemanticUnitKind::Owner)
        .expect("excluded document owner remains inventoried");
    assert_eq!(owner.documents, BTreeSet::from(["contract.ttl".into()]));
    assert!(matches!(
        owner.disposition,
        SemanticUnitDisposition::OutsideSelection { .. }
    ));
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Complete
    );
    let mut receipts: Vec<_> = theory
        .source()
        .origins()
        .iter()
        .map(|origin| purrdf_lex::json::record::to_vec(&origin.document))
        .collect();
    receipts.sort_unstable();
    let mut reference = include_str!("fixtures/source-receipts-identity-v1.txt").lines();
    for receipt in &receipts {
        let original_receipt = reference
            .next()
            .and_then(|line| line.strip_prefix("RECEIPT_PREIMAGE="))
            .expect("original complete sorted receipt preimage");
        assert_eq!(receipt, original_receipt.as_bytes());
    }
    let original_source_digest = reference
        .next()
        .and_then(|line| line.strip_prefix("SOURCE_DIGEST="))
        .expect("original actual whole-source digest");
    assert_eq!(admission.source_digest, original_source_digest);
    assert!(
        reference.next().is_none(),
        "complete original receipt reference consumed"
    );
}

#[test]
/// Unknown preset identity is an actual rejected compiler owner.
fn rejected_selected_owners_report_malformed_instead_of_missing_semantics() {
    let theory = compile(&format!(
        "{CONTRACT}\nex:contract a logic:ReasoningPreset ."
    ));
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let owner = admission
        .units
        .iter()
        .find(|unit| unit.kind == SemanticUnitKind::Owner)
        .expect("rejected owner remains inventoried");
    assert!(
        matches!(&owner.disposition, SemanticUnitDisposition::Malformed { code, reason }
        if code == "MALFORMED_SELECTED_OWNER" && reason.contains("Contract"))
    );
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Blocked
    );
}

#[test]
/// Emitting partial IR cannot discard the owner's hard compiler diagnostics.
fn emitted_owner_ir_does_not_hide_its_compiler_errors() {
    let theory = compile(&format!(
        "{CONTRACT}\nex:contract logic:admissibleValuation logic:AdmitAllFour ;
            logic:revision logic:EntrenchmentRevision ."
    ));
    assert!(theory.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "UNSUPPORTED_CONTRACT"
            && diagnostic.severity == crate::frontend::Severity::Error
    }));
    assert_eq!(theory.program().contracts.len(), 1);
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let owner = admission
        .units
        .iter()
        .find(|unit| unit.kind == SemanticUnitKind::Owner)
        .expect("emitted owner remains inventoried");
    assert!(
        matches!(&owner.disposition, SemanticUnitDisposition::Malformed { code, reason }
        if code == "MALFORMED_SELECTED_OWNER" && reason.contains("compiler errors"))
    );
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Blocked
    );
}
