// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Byte-golden integration test: `run_import` against the synthetic fixture must
//! reproduce the six projections + `budget-report.txt` byte-for-byte.
//!
//! The goldens were produced by the Python `run_import`; `foundation.ttl` is NOT
//! byte-checked (its serialization differs from rdflib's, by design).

use std::path::Path;

use gmeow_foundation_corpus::run_import;

/// The seven byte-exact targets (the six projections + the budget report).
const BYTE_EXACT: [&str; 7] = [
    "dracor.csv",
    "syuzhet.csv",
    "schema-org.jsonld",
    "tei.xml",
    "web-annotation.jsonld",
    "training-manifest.jsonl",
    "budget-report.txt",
];

#[test]
fn projections_and_budget_match_goldens() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = crate_dir.join("tests/fixtures/synthetic-corpus.jsonl");
    let goldens = crate_dir.join("tests/goldens");

    let tmp = tempfile::tempdir().expect("tempdir");
    run_import(&fixture, tmp.path(), None).expect("run_import");

    for name in BYTE_EXACT {
        let produced = std::fs::read(tmp.path().join(name))
            .unwrap_or_else(|e| panic!("missing produced {name}: {e}"));
        let golden = std::fs::read(goldens.join(name))
            .unwrap_or_else(|e| panic!("missing golden {name}: {e}"));
        assert_eq!(
            produced,
            golden,
            "byte mismatch in {name}\n--- produced ---\n{}\n--- golden ---\n{}",
            String::from_utf8_lossy(&produced),
            String::from_utf8_lossy(&golden),
        );
    }
}

#[test]
fn foundation_ttl_is_written() {
    // foundation.ttl is reference-only; assert it is produced and non-empty.
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = crate_dir.join("tests/fixtures/synthetic-corpus.jsonl");
    let tmp = tempfile::tempdir().expect("tempdir");
    run_import(&fixture, tmp.path(), None).expect("run_import");
    let ttl = std::fs::read_to_string(tmp.path().join("foundation.ttl")).expect("ttl");
    assert!(!ttl.is_empty());
}

#[test]
fn nq_reconciliation_is_written_and_orders_by_count() {
    use gmeow_foundation_corpus::{reconcile_nq, NQ_PREDICATE_STATUS};

    let tmp = tempfile::tempdir().expect("tempdir");
    let nq = tmp.path().join("corpus.nq");
    // Two key_event predicates, one goal_score, one unknown predicate.
    std::fs::write(
        &nq,
        concat!(
            "<urn:a> <http://lillith.internal/principia/key_event> <urn:e1> <urn:g> .\n",
            "<urn:b> <http://lillith.internal/principia/key_event> <urn:e2> <urn:g> .\n",
            "<urn:c> <http://lillith.internal/principia/goal_score> \"0.9\" <urn:g> .\n",
            "<urn:d> <http://lillith.internal/principia/never_seen> <urn:x> <urn:g> .\n",
        ),
    )
    .expect("write nq");

    let report = reconcile_nq(&nq, &NQ_PREDICATE_STATUS).expect("reconcile");
    let mut lines = report.lines();
    assert_eq!(
        lines.next().unwrap(),
        "NQ RECONCILIATION (predicate → status)"
    );
    // most_common: key_event (count 2) precedes the count-1 entries.
    assert_eq!(
        lines.next().unwrap(),
        "  http://lillith.internal/principia/key_event (2): \
         MAPPED → gmeow:Event + flat gmeow:narrates (#360)"
    );
    // Unknown predicate maps to UNREVIEWED.
    assert!(report.contains("never_seen (1): UNREVIEWED"));
}

#[test]
fn run_import_writes_nq_reconciliation_when_given() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = crate_dir.join("tests/fixtures/synthetic-corpus.jsonl");
    let tmp = tempfile::tempdir().expect("tempdir");
    let nq = tmp.path().join("in.nq");
    std::fs::write(
        &nq,
        "<urn:a> <http://lillith.internal/principia/thematic_tag> \"x\" <urn:g> .\n",
    )
    .expect("write nq");
    run_import(&fixture, tmp.path(), Some(&nq)).expect("run_import");
    let report =
        std::fs::read_to_string(tmp.path().join("nq-reconciliation.txt")).expect("reconciliation");
    assert!(report.starts_with("NQ RECONCILIATION (predicate → status)\n"));
    assert!(report.ends_with('\n'));
}
