// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn arts(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect()
}

#[test]
fn artifact_lane_round_trips_exact_bytes() {
    let artifacts = arts(&[
        ("generated/a.ttl", b"alpha"),
        ("generated/b.nq", b"bravo"),
        ("pipeline/base.nq", b""), // empty bytes are representable
    ]);
    let bundle = bundle_from_artifacts(artifacts.clone(), DatasetProvenance::new());
    assert_eq!(
        bundle_artifact(&bundle, "generated/a.ttl"),
        Some(&b"alpha"[..])
    );
    assert_eq!(bundle_artifact(&bundle, "pipeline/base.nq"), Some(&b""[..]));
    assert_eq!(bundle_artifact(&bundle, "missing"), None);
    assert_eq!(bundle_artifacts(&bundle), artifacts);
}

/// Representation changes preserve the same allocation while artifact paths still
/// reference its digest.
#[test]
fn artifact_paths_keep_shared_bytes_when_a_representation_is_removed() {
    let artifacts = arts(&[("x", b"same"), ("y", b"same")]);
    let bundle = bundle_from_artifacts(artifacts.clone(), DatasetProvenance::new());
    let previous = bundle.clone();
    let bundle = attach_rep_blob(bundle, "sample:shared", "text/plain", b"same".to_vec())
        .expect("attach the aliased representation");
    assert_eq!(bundle.blobs().len(), 1, "equal bytes stored once");
    let representation = bundle_rep_blob(&bundle, "sample:shared").expect("attached bytes");
    let original = bundle_artifact(&previous, "x").expect("original artifact bytes");
    assert!(std::ptr::eq(representation, original));

    let mut stripped = Arc::new(bundle);
    retain_rep_blobs(&mut stripped, |rep| rep != "sample:shared")
        .expect("remove the representation");
    assert!(bundle_rep_blob(&stripped, "sample:shared").is_none());
    assert_eq!(bundle_artifacts(&stripped), artifacts);
    let surviving = bundle_artifact(&stripped, "y").expect("surviving artifact bytes");
    assert!(std::ptr::eq(surviving, original));
}

/// A representation can share a digest with an artifact without reclaiming the artifact.
#[test]
fn artifact_survives_representation_attachment_and_reclamation() {
    let payload = b"shared";
    let original = bundle_from_artifacts(arts(&[("artifact", payload)]), DatasetProvenance::new());
    let attached = attach_rep_blob(
        original,
        "diagnostic",
        "application/octet-stream",
        payload.to_vec(),
    )
    .expect("attach a representation with the same digest");
    assert_eq!(attached.blobs().len(), 1);
    assert_eq!(
        bundle_rep_blob(&attached, "diagnostic"),
        Some(payload.as_slice())
    );

    let released = release_carrier(&attached, &std::collections::BTreeSet::new())
        .expect("release the carrier and keep committed artifacts");
    let mut stripped = Arc::new(attached);
    retain_rep_blobs(&mut stripped, |rep| rep != "diagnostic").expect("strip the representation");
    for bundle in [stripped.as_ref(), &released] {
        assert_eq!(bundle_rep_blob(bundle, "diagnostic"), None);
        assert_eq!(
            bundle_artifact(bundle, "artifact"),
            Some(payload.as_slice())
        );
        assert_eq!(bundle.blobs().len(), 1);
    }
}

#[test]
fn bundle_digest_changes_with_artifacts_and_is_stable() {
    let a = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
    let b = bundle_from_artifacts(arts(&[("p", b"two")]), DatasetProvenance::new());
    let a2 = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
    let digest = |bundle: &purrdf::PipelineBundle<PipelineHandle>| {
        bundle
            .digest(&mut purrdf::CanonicalizationControl::new(
                purrdf::CANON_CORPUS_POLICY,
                None,
            ))
            .expect("the carrier digests")
    };
    assert_ne!(digest(&a), digest(&b), "different bytes → different digest");
    assert_eq!(digest(&a), digest(&a2), "same artifacts → same digest");
}

#[test]
fn pipeline_handle_logic_carries_the_compiled_program() {
    // The Logic arm now carries the REAL typed IR (C6), not a backing-graph
    // placeholder: an empty program is a valid, cloneable payload.
    let program = Arc::new(LogicProgram::new(vec![], vec![], vec![], None));
    let h = PipelineHandle::Logic(program);
    assert!(matches!(h, PipelineHandle::Logic(_)));
}

/// The predicate of the filtered lane copy is asked only about complete entries, and an
/// entry it refuses is not returned (and so never cloned): the unserved name is offered
/// to the predicate by name only, never its bytes.
#[test]
fn the_filtered_lane_copy_returns_only_the_entries_the_predicate_accepts() {
    let artifacts = arts(&[("generated/a", b"a"), ("pipeline/big", b"big")]);
    let bundle = bundle_from_artifacts(artifacts, DatasetProvenance::new());
    let mut asked = Vec::new();
    let lane = bundle_artifacts_where(&bundle, |path| {
        asked.push(path.to_owned());
        !path.starts_with("pipeline/")
    });
    assert_eq!(lane, arts(&[("generated/a", b"a")]));
    assert_eq!(asked, ["generated/a", "pipeline/big"]);
    assert_eq!(
        bundle_artifacts(&bundle).len(),
        2,
        "the wrapper accepts everything"
    );
}
