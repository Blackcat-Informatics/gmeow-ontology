// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The WHOLE-ARTIFACT producers, audited through the branch the ontology routes them
//! to.
//!
//! `validate_mandated_frames` is the universal Rule 6 codec rule and stays applicable
//! to every one of these artifacts — none of which carries a medium registry. This
//! file proves the second half: each producer's declared `gmeow:Medium` resolves to
//! `gmeow:mediumSourceWholeArtifact`, and its output satisfies THAT branch's
//! obligation (every payload frame through one catalog entry, matching the declared
//! medium's dictionary set). A branch with no live producer would be an exemption list
//! in ontology clothing, so the routing is asserted rather than assumed.
//!
//! Three of the four live here because `gmeow-pipeline` depends on `gmeow-music` and
//! `gmeow-math`, so this is the nearest crate that can reach both plus the
//! `convert --to gts` exit. The fourth (the feedback bundle) is audited in
//! `gmeow-dev-cli`'s own `feedback_bundle` test, the only crate that can reach it.

use std::path::{Path, PathBuf};

use gmeow_pipeline::medium::registry::{MediumRegistry, MediumSourceKind};
use gmeow_pipeline::{MediumDeclaration, declared_medium_of, validate_declared_media};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// The authored medium axis plus the producer→medium map, parsed from the slice that
/// owns both. A whole-artifact producer's output carries no registry of its own, which
/// is exactly why the declaration has to come from the ontology rather than the bytes.
fn gts_slice() -> std::sync::Arc<purrdf::RdfDataset> {
    let text = std::fs::read(repo_root().join("slices/core/gts/module.ttl"))
        .expect("the gts slice is readable");
    purrdf::parse_dataset(&text, "text/turtle", Some(GMEOW)).expect("the gts slice parses")
}

/// Audit `bytes` through the branch `producer`'s declared medium routes it to, and
/// assert that branch really is the whole-artifact one.
fn audit_whole_artifact(producer: &str, bytes: &[u8]) {
    // The universal rule still holds on the very same bytes — the split is an
    // ADDITION, not a replacement.
    gmeow_pipeline::validate_mandated_frames(bytes)
        .unwrap_or_else(|e| panic!("{producer}: universal mandated-frame rule failed: {e}"));

    let ds = gts_slice();
    let registry = MediumRegistry::from_dataset(&ds).expect("the live medium axis reads");
    let medium_iri = declared_medium_of(&ds, producer)
        .unwrap_or_else(|e| panic!("{producer}: no declared gmeow:producerMedium: {e}"));
    let medium = registry
        .media()
        .get(&medium_iri)
        .unwrap_or_else(|| panic!("{producer}: <{medium_iri}> is not a declared gmeow:Medium"));
    assert_eq!(
        medium.source_kind,
        MediumSourceKind::WholeArtifact,
        "{producer} must route to the whole-artifact branch; <{medium_iri}> routes elsewhere"
    );
    validate_declared_media(
        bytes,
        &MediumDeclaration {
            medium: &medium_iri,
            registry: &registry,
        },
    )
    .unwrap_or_else(|e| panic!("{producer}: declared-media audit failed: {e}"));
}

#[test]
fn the_music_bundle_routes_to_the_whole_artifact_branch() {
    let piece = gmeow_music::Piece {
        iri: "urn:gmeow:piece:whole-artifact-media".to_string(),
        title: Some("declared-media audit fixture".to_string()),
        composer: Some("GMEOW".to_string()),
        voices: Vec::new(),
    };
    let bytes = gmeow_music::piece_to_gts_bytes(&piece).expect("the music producer emits");
    audit_whole_artifact(&format!("{GMEOW}gtsProducerMusicBundle"), &bytes);
}

#[test]
fn the_math_bundle_routes_to_the_whole_artifact_branch() {
    let bytes = gmeow_math::turtle_to_gts(
        concat!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n",
            "<urn:gmeow:math:space> a math:InnerProductSpace ; math:dimension 2 .\n",
        )
        .as_bytes(),
    )
    .expect("the math producer emits");
    audit_whole_artifact(&format!("{GMEOW}gtsProducerMathBundle"), &bytes);
}

#[test]
fn the_convert_exit_routes_to_the_whole_artifact_branch() {
    let dataset = purrdf::parse_dataset(
        concat!(
            "<https://e/s> <https://e/p> <https://e/o> .\n",
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
        )
        .as_bytes(),
        "application/n-triples",
        None,
    )
    .expect("the convert fixture parses");
    let bytes =
        gmeow_gts_profile::dataset_to_gmeow_gts(&dataset).expect("the convert --to gts exit emits");
    audit_whole_artifact(&format!("{GMEOW}gtsProducerConvertExit"), &bytes);
}

/// Every producer the ontology routes to the whole-artifact branch is EXERCISED
/// somewhere, and every producer routed elsewhere is exercised by its own gate. A
/// branch nothing reaches is an exemption list with a nicer name, so the partition is
/// pinned here rather than trusted.
#[test]
fn every_declared_producer_routes_to_a_live_branch() {
    use std::collections::BTreeMap;

    let ds = gts_slice();
    let registry = MediumRegistry::from_dataset(&ds).expect("the live medium axis reads");
    let producers: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&ds)
        .into_iter()
        .filter(|q| q.predicate == format!("{GMEOW}producerMedium"))
        .filter_map(|q| match q.subject {
            purrdf::RdfTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect();
    assert!(
        producers.len() >= 6,
        "the producer→medium map is implausibly small: {producers:?}"
    );

    let mut by_kind: BTreeMap<MediumSourceKind, Vec<String>> = BTreeMap::new();
    for producer in &producers {
        let medium_iri = declared_medium_of(&ds, producer).expect("exactly one declared medium");
        let medium = registry
            .media()
            .get(&medium_iri)
            .unwrap_or_else(|| panic!("<{medium_iri}> is not a declared gmeow:Medium"));
        by_kind
            .entry(medium.source_kind)
            .or_default()
            .push(producer.clone());
    }
    for kind in [
        MediumSourceKind::PerRep,
        MediumSourceKind::HeaderDict,
        MediumSourceKind::WholeArtifact,
    ] {
        assert!(
            by_kind.get(&kind).is_some_and(|v| !v.is_empty()),
            "no production producer routes to {kind:?} — a branch with no producer is an \
             exemption list in ontology clothing; routed: {by_kind:?}"
        );
    }
}
