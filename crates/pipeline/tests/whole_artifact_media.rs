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

use gmeow_pipeline::medium::registry::MediumSourceKind;
use gmeow_pipeline::{MediumDeclaration, validate_declared_media};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Read the exact source-local registry and producer declarations selected for this run.
fn declared_source() -> gmeow_pipeline::medium::source_observation::SourceMediumRegistry {
    gmeow_pipeline::medium::source_observation::authenticated(&repo_root())
        .expect("authenticated source medium declarations")
}

/// Audit `bytes` through the branch `producer`'s declared medium routes it to, and
/// assert that branch really is the whole-artifact one.
fn audit_whole_artifact(producer: &str, bytes: &[u8]) {
    // The universal rule still holds on the very same bytes — the split is an
    // ADDITION, not a replacement.
    gmeow_pipeline::validate_mandated_frames(bytes)
        .unwrap_or_else(|e| panic!("{producer}: universal mandated-frame rule failed: {e}"));

    let source = declared_source();
    let registry = &source.registry;
    let medium_iri = source
        .producer_media
        .get(producer)
        .unwrap_or_else(|| panic!("{producer}: no declared gmeow:producerMedium"));
    let medium = registry
        .media()
        .get(medium_iri)
        .unwrap_or_else(|| panic!("{producer}: <{medium_iri}> is not a declared gmeow:Medium"));
    assert_eq!(
        medium.source_kind,
        MediumSourceKind::WholeArtifact,
        "{producer} must route to the whole-artifact branch; <{medium_iri}> routes elsewhere"
    );
    validate_declared_media(
        bytes,
        &MediumDeclaration {
            medium: medium_iri,
            registry,
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
    // gmeow-test-input: synthetic-only
    let bytes = {
        let emission = gmeow_music::piece_to_gts_bytes(&piece).expect("the music producer emits");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
    audit_whole_artifact(&format!("{GMEOW}gtsProducerMusicBundle"), &bytes);
}

#[test]
fn the_math_bundle_routes_to_the_whole_artifact_branch() {
    // gmeow-test-input: synthetic-only
    let bytes = {
        let emission = gmeow_math::turtle_to_gts(
            concat!(
                "@prefix math: <https://blackcatinformatics.ca/math/> .\n",
                "<urn:gmeow:math:space> a math:InnerProductSpace ; math:dimension 2 .\n",
            )
            .as_bytes(),
        )
        .expect("the math producer emits");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
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
    // gmeow-test-input: synthetic-only
    let bytes = {
        let emission = gmeow_gts_profile::view_to_gmeow_gts(&dataset)
            .expect("the convert --to gts exit emits");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
    audit_whole_artifact(&format!("{GMEOW}gtsProducerConvertExit"), &bytes);
}

/// Every producer the ontology routes to the whole-artifact branch is EXERCISED
/// somewhere, and every producer routed elsewhere is exercised by its own gate. A
/// branch nothing reaches is an exemption list with a nicer name, so the partition is
/// pinned here rather than trusted.
#[test]
fn every_declared_producer_routes_to_a_live_branch() {
    use std::collections::BTreeMap;

    let source = declared_source();
    let registry = &source.registry;
    let producers: Vec<String> = source.producer_media.keys().cloned().collect();
    assert!(
        producers.len() >= 6,
        "the producer→medium map is implausibly small: {producers:?}"
    );

    let mut by_kind: BTreeMap<MediumSourceKind, Vec<String>> = BTreeMap::new();
    for producer in &producers {
        let medium_iri = source
            .producer_media
            .get(producer)
            .expect("exactly one declared medium");
        let medium = registry
            .media()
            .get(medium_iri)
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
