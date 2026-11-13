// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The declared-versus-recorded read comparison: what a declaration permits, what counts
//! as undeclared, and what counts as never read.

use super::*;

fn set(reads: &[Read]) -> ReadSet {
    reads.iter().cloned().collect()
}

/// A whole-carrier declaration permits every carrier lane and no byte artifact, and the
/// whole artifact lane permits any artifact and no carrier lane.
#[test]
fn a_declaration_permits_exactly_the_lanes_it_covers() {
    let carrier = set(&[Read::Carrier]);
    assert!(permits(&carrier, &Read::Dataset));
    assert!(permits(&carrier, &Read::Handle("h".into())));
    assert!(permits(&carrier, &Read::BlobRep("r".into())));
    assert!(!permits(&carrier, &Read::Artifact("generated/x".into())));

    let artifacts = set(&[Read::AllArtifacts]);
    assert!(permits(&artifacts, &Read::Artifact("generated/x".into())));
    assert!(!permits(&artifacts, &Read::Dataset));

    let one = set(&[Read::Artifact("pipeline/a".into())]);
    assert!(permits(&one, &Read::Artifact("pipeline/a".into())));
    assert!(!permits(&one, &Read::Artifact("pipeline/b".into())));
    assert!(!permits(&one, &Read::AllArtifacts));
}

/// An access outside the declaration is reported as undeclared with the Turtle that would
/// declare it, and a declared read no access exercised is reported as never read.
#[test]
fn undeclared_and_unmade_reads_are_both_reported() {
    let edge = EdgeAccess::new(
        "stage-b",
        "stage-a",
        set(&[Read::Dataset, Read::Artifact("pipeline/a".into())]),
    );
    edge.record(Read::Dataset);
    edge.record(Read::Handle("https://example.org/h".into()));

    assert_eq!(
        edge.undeclared(),
        set(&[Read::Handle("https://example.org/h".into())])
    );
    assert_eq!(edge.unread(), set(&[Read::Artifact("pipeline/a".into())]));

    let lines = mismatches(&[edge]);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("gmeow:flowReadsHandle <https://example.org/h>"));
    assert!(lines[1].contains("gmeow:flowReadsArtifact \"pipeline/a\""));
}

/// Only carrier lanes keep a producer's carrier alive; a declared internal artifact is
/// listed for retention through release and a committed one is not.
#[test]
fn retention_follows_the_declared_lanes() {
    let reads = set(&[
        Read::Artifact("pipeline/kept".into()),
        Read::Artifact("generated/committed".into()),
    ]);
    assert!(!needs_carrier(&reads));
    assert_eq!(
        internal_artifacts(&reads).collect::<Vec<_>>(),
        ["pipeline/kept"]
    );
    assert!(needs_carrier(&set(&[Read::BlobRep("spans".into())])));
}
