// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The `yaml-ld-archive` owns NO committed `generated/` path, so the double-carry
/// hazard cannot arise for it AT ALL — there is no path for a second rep to also
/// claim. That is a structural property of the rep, not a coincidence of the current
/// member list, so it is asserted on both authorities the superset gate consults:
/// [`archive_rep_carries_generated`] (does this rep back committed files?) and
/// [`committed_path_for_archive_member`] (which committed file does a member back?).
///
/// If a future change gave the archive committed members, BOTH assertions red — which
/// is the moment `opaque_already_carried` would have to start refusing that family,
/// exactly as it does for the lang projections above.
#[test]
fn the_yaml_ld_archive_owns_no_committed_generated_path() {
    use crate::stages::archive_blobs::{REP_YAMLLD, YAMLLD_JSONLD_MEMBER, YAMLLD_YAMLLD_MEMBER};
    assert!(
        !archive_rep_carries_generated(REP_YAMLLD),
        "{REP_YAMLLD} must not be declared as backing committed generated/ files — its \
             members are bundle-only serializations of the claim corpus"
    );
    for member in [YAMLLD_JSONLD_MEMBER, YAMLLD_YAMLLD_MEMBER] {
        assert_eq!(
            committed_path_for_archive_member(REP_YAMLLD, member),
            None,
            "{member} must resolve to no committed path"
        );
        assert!(
            !member.starts_with("generated/"),
            "{member} must not be named like a committed generated/ path"
        );
    }
}

/// The INTERNAL lane the claim serializations ride from `stage-statements` into the
/// archive is refused by the generated-opaque archive, so the SAME bytes can never
/// reach both `generated-opaque-archive` and `yaml-ld-archive`. A double-carry would
/// hand one payload to two separately-sealed frames and key the superset gate's
/// blob-member map on a `pipeline/` path no committed file backs. Borrowed and
/// owned members retain their canonical archive bytes.
#[test]
fn the_internal_dataflow_lane_never_reaches_the_generated_opaque_archive() {
    let mut members = BTreeMap::new();
    take_opaque(
        &mut members,
        [
            (crate::stages::statements::RDF12_JSONLD_PATH, &b"j"[..]),
            (crate::stages::statements::RDF12_YAMLLD_PATH, b"y"),
            ("pipeline/medium/gmeow-core-v1.zdict", b"d"),
            // A near miss: a COMMITTED path whose name merely starts with the same
            // letters must still ride the opaque archive.
            ("generated/pipeline-notes.md", b"n"),
            ("generated/references/refs.md", b"r"),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect(),
    );
    let path = crate::stages::verify_attestation::VERIFY_JSON_PATH;
    let stage = "stage-verify-attestation";
    let upstream = BTreeMap::from([(
        stage.to_owned(),
        StageProduct::from_artifacts(stage, BTreeMap::from([(path.to_owned(), b"{}".to_vec())]))
            .expect("verification artifact"),
    )]);
    let bytes = producer_artifact(stage, path, &upstream).expect("borrow verification artifact");
    members.insert(path.to_owned(), bytes.into());

    let archive = opaque_blob_from_members(members).expect("opaque archive");
    let expected = purrdf::ustar::write_archive(&[
        (path.to_owned(), b"{}".to_vec()),
        ("generated/pipeline-notes.md".to_owned(), b"n".to_vec()),
        ("generated/references/refs.md".to_owned(), b"r".to_vec()),
    ])
    .expect("canonical archive bytes");
    assert_eq!(archive.data, expected);
}
