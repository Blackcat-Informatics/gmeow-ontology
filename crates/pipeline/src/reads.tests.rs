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

/// A product with one committed and two internal artifacts, as one consumer sees it
/// under `declared`, released the way the scheduler releases it for that declaration.
fn consumer_view_of(
    declared: &ReadSet,
    released: bool,
) -> (crate::node::StageProduct, Arc<EdgeAccess>) {
    let product = crate::node::StageProduct::from_artifacts(
        "stage-a",
        std::collections::BTreeMap::from([
            ("generated/x".to_owned(), b"x".to_vec()),
            ("pipeline/named".to_owned(), b"n".to_vec()),
            ("pipeline/unnamed".to_owned(), b"u".to_vec()),
        ]),
    )
    .expect("synthetic product");
    let product = if released {
        let kept: BTreeSet<&str> = internal_artifacts(declared).collect();
        product
            .into_carrier_released_keeping(&kept)
            .expect("release")
    } else {
        product
    };
    let edge = EdgeAccess::new("stage-b", "stage-a", declared.clone());
    (product.consumer_view(Arc::clone(&edge)), edge)
}

/// An internal artifact the edge names is kept through the release and readable, and the
/// whole-lane accessors return it beside the committed artifacts, with every read
/// declared. Protects the "named internal artifact" half of the all-artifacts rule.
#[test]
fn a_named_internal_artifact_is_kept_and_readable_beside_the_whole_lane() {
    let declared = set(&[Read::AllArtifacts, Read::Artifact("pipeline/named".into())]);
    let (view, edge) = consumer_view_of(&declared, true);
    assert_eq!(view.artifact("pipeline/named"), Some(b"n".as_slice()));
    let lane: Vec<String> = view.artifacts().into_keys().collect();
    assert_eq!(lane, ["generated/x", "pipeline/named"]);
    assert!(edge.undeclared().is_empty(), "{:?}", edge.undeclared());
    assert!(edge.unread().is_empty(), "{:?}", edge.unread());
}

/// Under an all-artifacts declaration alone an internal artifact is outside the permitted
/// lane: a direct read is reported as undeclared (even where the carrier is still whole,
/// as under the recording policy), and the whole-lane accessors do not return it.
/// Protects the "unnamed internal artifact" half of the all-artifacts rule.
#[test]
fn an_unnamed_internal_artifact_is_undeclared_under_the_whole_lane() {
    let declared = set(&[Read::AllArtifacts]);
    assert!(!permits(
        &declared,
        &Read::Artifact("pipeline/unnamed".into())
    ));
    let (view, edge) = consumer_view_of(&declared, false);
    let lane: Vec<String> = view
        .artifact_refs()
        .expect("lane")
        .keys()
        .map(|k| (*k).to_owned())
        .collect();
    assert_eq!(lane, ["generated/x"]);
    assert_eq!(view.artifacts().len(), 1);
    assert!(edge.undeclared().is_empty());
    let _ = view.artifact("pipeline/unnamed");
    assert_eq!(
        edge.undeclared(),
        set(&[Read::Artifact("pipeline/unnamed".into())])
    );
}
