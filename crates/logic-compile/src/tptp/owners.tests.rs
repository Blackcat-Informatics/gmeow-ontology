// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{RootSelection, SemanticUnitDisposition, SemanticUnitKind};

const CONTRACT: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
ex:contract a logic:ReasoningContract .
"#;

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
