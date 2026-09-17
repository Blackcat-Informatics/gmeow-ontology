// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::node::StageProduct;

/// The typed Shacl/Shex validation-shape sidecars ride the REAL gmeow.gts
/// serialize+decode: a decoded bundle exposes the SHACL surface under
/// [`purrdf::RdfLookasideKind::Shacl`] and the ShEx surface under
/// [`purrdf::RdfLookasideKind::Shex`], each resolving to the exact producer bytes.
/// This is the production-surface demonstration that a repo-free consumer reads the
/// validation surface under its typed kind (LOGIC-VALIDATION.md) without re-running
/// the compiler — the decode path exercised is the true `emit_gts` writer +
/// `read_graph`/`lookaside_from_graph` reader, never a hand-rolled shortcut.
#[test]
fn typed_shacl_shex_sidecars_round_trip_through_gmeow_gts() {
    // A minimal stage-compile-logic product carrying the two validation-shape surfaces
    // (the SINGLE source the typed sidecars and the REP_GENERATED archive both draw from).
    let shacl_bytes = b"@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
            <https://blackcatinformatics.ca/gmeow/CatShape> a sh:NodeShape .\n"
        .to_vec();
    let shex_bytes = b"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
            gmeow:CatShape { gmeow:name . }\n"
        .to_vec();
    let mut compile_arts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    compile_arts.insert(
        crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH.to_string(),
        shacl_bytes.clone(),
    );
    compile_arts.insert(
        crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH.to_string(),
        shex_bytes.clone(),
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-compile-logic".to_string(),
        StageProduct::from_artifacts("stage-compile-logic", compile_arts),
    );

    // Build the typed sidecars through the PRODUCTION helper, fold them into a
    // well-formed snapshot, and emit through the REAL gts writer (`emit_gts`).
    let typed_blobs = build_validation_shape_typed_blobs(&upstream).expect("typed sidecars");
    let mut builder = SnapshotBuilder::new();
    add_base_nq(
        &mut builder,
        b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        "base",
    )
    .expect("fold base graph");
    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        Some(vec!["gzip".to_string()]),
        typed_blobs,
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(Some(&["gzip".to_string()])),
    )
    .expect("emit snapshot");

    // DECODE the emitted bytes back through the real gts reader + lookaside fold.
    let graph = purrdf::gts::read_graph(&gts, true).expect("read_graph");
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);

    // Resolve the single resource of `kind` to its payload bytes via the content-store
    // (digest → bytes) join — exactly how a repo-free consumer reads a typed surface.
    let bytes_of = |kind: purrdf::RdfLookasideKind| -> Vec<u8> {
        let resource = lookaside
            .resources_of_kind(kind.clone())
            .next()
            .unwrap_or_else(|| panic!("a decoded {kind:?} resource is present"));
        let digest = resource
            .content_digest
            .as_deref()
            .expect("typed resource carries a content digest");
        let (_, entry) = graph
            .blobs
            .iter()
            .find(|(d, _)| d == digest)
            .expect("blob store carries the resource payload by digest");
        entry.decoded_vec().expect("decode blob payload")
    };

    // The typed Shacl kind decodes to the exact SHACL surface bytes.
    assert_eq!(
        bytes_of(purrdf::RdfLookasideKind::Shacl),
        shacl_bytes,
        "resources_of_kind(Shacl) yields the validation-shapes.ttl content"
    );
    // The typed Shex kind decodes to the exact ShEx surface bytes.
    assert_eq!(
        bytes_of(purrdf::RdfLookasideKind::Shex),
        shex_bytes,
        "resources_of_kind(Shex) yields the validation-shapes.shex content"
    );
}
