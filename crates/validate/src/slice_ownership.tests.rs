// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::slice::EdgeKind;
use purrdf::slice::NamedNode;
use std::collections::HashMap;

fn nn(iri: &str) -> NamedNode {
    NamedNode::new(iri).unwrap()
}

#[test]
fn diagnostics_map_to_the_gate_severity_split() {
    let cause = purrdf::slice::QueryArtifactError::Utf8(
        String::from_utf8(vec![b'a', 0xff])
            .expect_err("the query artifact contains invalid UTF-8")
            .utf8_error(),
    );
    let cause_message = cause.to_string();
    let report = OwnershipReport {
        ownership: HashMap::default(),
        edges: Vec::new(),
        diagnostics: vec![
            OwnershipDiagnostic::Conflict {
                term: nn("https://blackcatinformatics.ca/gmeow/Foo"),
                claimants: vec!["s/a".into(), "s/b".into()],
            },
            OwnershipDiagnostic::UndeclaredDependency {
                from_slice: "s/1".into(),
                to_slice: "s/2".into(),
                edge_kind: EdgeKind::Ontology,
            },
            OwnershipDiagnostic::UnparseableQuery {
                slice: "s/query".into(),
                logical_path: "queries/invalid.rq".into(),
                cause,
            },
            // The analyzer also emits an Unowned diagnostic; it must be
            // skipped here (the ownership-table pass owns it) — no double count.
            OwnershipDiagnostic::Unowned {
                term: nn("https://blackcatinformatics.ca/gmeow/Bar"),
            },
        ],
    };

    let findings = ownership_findings(&report);

    assert_eq!(
        findings.len(),
        3,
        "Unowned diagnostic is not double-counted"
    );
    let conflict = findings
        .iter()
        .find(|f| f.code == "slice-ownership.conflict")
        .expect("conflict finding");
    assert_eq!(conflict.severity, Severity::Error); // ownership defect → gates
    assert!(conflict.message.contains("Foo"));
    let undeclared = findings
        .iter()
        .find(|f| f.code == "slice-ownership.undeclared-dependency")
        .expect("undeclared-dependency finding");
    assert_eq!(undeclared.severity, Severity::Error); // R4/AC1: gates make validate
    let query = findings
        .iter()
        .find(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNPARSEABLE_QUERY)
        .expect("query artifact finding");
    assert_eq!(query.severity, Severity::Warning);
    assert_eq!(
        query.message,
        format!("s/query: query queries/invalid.rq failed to parse — {cause_message}")
    );
    assert_eq!(query.locations[0].logical.as_deref(), Some("s/query"));
}

#[test]
fn unowned_table_status_becomes_one_error_finding() {
    let term = nn("https://blackcatinformatics.ca/gmeow/Orphan");
    let mut ownership = HashMap::default();
    ownership.insert(
        term.clone(),
        purrdf::slice::TermOwnership {
            term,
            declared_owner: "s/declarer".into(),
            physical_origin: None,
            status: purrdf::slice::OwnershipStatus::Unowned,
        },
    );
    let report = OwnershipReport {
        ownership,
        edges: Vec::new(),
        diagnostics: Vec::new(),
    };

    let findings = ownership_findings(&report);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "slice-ownership.unowned");
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].message.contains("Orphan"));
}
