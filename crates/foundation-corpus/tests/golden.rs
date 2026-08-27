// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Identity checks for the committed foundation-corpus outputs.
//!
//! Tests are consumers only: they authenticate these already-produced goldens and
//! never invoke the corpus importer.

use std::path::Path;

use sha2::{Digest, Sha256};

/// Exact identities supplied to the consumer test for every committed output.
const AUTHENTICATED_GOLDENS: [(&str, &str); 8] = [
    (
        "budget-report.txt",
        "b14c9c6ff165f3c4125e661a0615e6446d3e6bdb8df870b4c62819ed4b7960c7",
    ),
    (
        "dracor.csv",
        "a1ff06560d4163058307cf88f52231ba4a976e3d8e814f8fd0ac3aef55c0e0b0",
    ),
    (
        "foundation.ttl",
        "5a84886bee491ee6ad74ec895791d051cba35cdf54dd531a639826cb2e0652b5",
    ),
    (
        "schema-org.jsonld",
        "f820deb5c2f4b5646629e8e5c55dedabe3f04c6e28e4c694748b73c813b89908",
    ),
    (
        "syuzhet.csv",
        "ed4499d43c26c3ee0b269522f72adf0b63976f02f145541e568eca926b858177",
    ),
    (
        "tei.xml",
        "66369710bc4ef6f712f88b4864f762aa89b642b9c03dcc22584d7f804053f7f3",
    ),
    (
        "training-manifest.jsonl",
        "898a1890f4337a22c583d8f3c263e29d5ba63af776a580dd5f86753d30110b0d",
    ),
    (
        "web-annotation.jsonld",
        "13665c3ea567b46c8a92ef5120fa7209eaa71e221ccad5982a31d3ac4b3e7e77",
    ),
];

#[test]
fn committed_outputs_match_supplied_identities() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let goldens = crate_dir.join("tests/goldens");

    for (name, expected) in AUTHENTICATED_GOLDENS {
        let golden = std::fs::read(goldens.join(name))
            .unwrap_or_else(|e| panic!("missing golden {name}: {e}"));
        let actual = format!("{:x}", Sha256::digest(&golden));
        assert_eq!(
            actual, expected,
            "committed {name} identity does not match the test contract"
        );
    }
}

#[test]
fn nq_reconciliation_is_written_and_orders_by_count() {
    use gmeow_foundation_corpus::{NQ_PREDICATE_STATUS, reconcile_nq};

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
         MAPPED → gmeow:Event + flat gmeow:narrates"
    );
    // Unknown predicate maps to UNREVIEWED.
    assert!(report.contains("never_seen (1): UNREVIEWED"));
}
