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

#[test]
fn shared_bytes_dedup_but_both_paths_reconstruct() {
    // Two artifacts with identical bytes share one content-store blob, yet both
    // logical paths must reconstruct the bytes (the resource index is per-path).
    let artifacts = arts(&[("x", b"same"), ("y", b"same")]);
    let bundle = bundle_from_artifacts(artifacts.clone(), DatasetProvenance::new());
    assert_eq!(bundle.blobs().len(), 1, "equal bytes stored once");
    assert_eq!(bundle_artifacts(&bundle), artifacts);
}

#[test]
fn bundle_digest_changes_with_artifacts_and_is_stable() {
    let a = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
    let b = bundle_from_artifacts(arts(&[("p", b"two")]), DatasetProvenance::new());
    let a2 = bundle_from_artifacts(arts(&[("p", b"one")]), DatasetProvenance::new());
    assert_ne!(a.digest(), b.digest(), "different bytes → different digest");
    assert_eq!(a.digest(), a2.digest(), "same artifacts → same digest");
}

#[test]
fn pipeline_handle_logic_carries_the_compiled_program() {
    // The Logic arm now carries the REAL typed IR (C6), not a backing-graph
    // placeholder: an empty program is a valid, cloneable payload.
    let program = Arc::new(LogicProgram::new(vec![], vec![], vec![], None));
    let h = PipelineHandle::Logic(program);
    assert!(matches!(h, PipelineHandle::Logic(_)));
}
