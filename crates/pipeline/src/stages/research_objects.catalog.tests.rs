// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny authored carrier/catalog inputs and an isolated authenticated cache only.

use super::*;
use crate::cache::{PipelineCache, ReceiptOutputSelection, StageKeyContext};
use gmeow_logic_compile::{ingest::DslView, projections::grounding_catalog::GroundingCatalog};

const NATIVE_PROGRAM: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
logic:correspondence-program a logic:CorrespondenceProgram ;
    logic:hasPreservation logic:SoundUnderApproximation ;
    logic:hasCorrespondence <urn:authored:bridge> .
<urn:authored:bridge> a logic:Correspondence ;
    logic:correspondenceRelation logic:RelatedMatch ;
    logic:morphismClass logic:BridgeView ;
    logic:morphismKind logic:CommitmentShiftingBridge ;
    logic:preservationKind logic:ValidationOnly .
"#;

fn authored_product(graph: &str, byte_bait: bool) -> StageProduct {
    let source = purrdf::parse_dataset(NATIVE_PROGRAM.as_bytes(), "text/turtle", None).unwrap();
    let dataset = super::super::carrier::rooted_in_graph(&source, graph).unwrap();
    let artifacts = if byte_bait {
        BTreeMap::from([(
            super::super::carrier::CORRESPONDENCE_LAWS_PATH.to_owned(),
            NATIVE_PROGRAM.as_bytes().to_vec(),
        )])
    } else {
        BTreeMap::new()
    };
    StageProduct::from_artifacts_over("stage-mappings", dataset, artifacts)
}

fn admitted_catalog(product: StageProduct) -> GroundingCatalog {
    let upstream = BTreeMap::from([("stage-mappings".to_owned(), product)]);
    let program = mappings_correspondence_program(&upstream).unwrap();
    let ontology = purrdf::parse_dataset(b"", "text/turtle", None).unwrap();
    // This fixture declares an ordinary bridge, with no owned grounding alias.
    GroundingCatalog::from_program(program, &DslView::new(&ontology), &[]).unwrap()
}

#[test]
fn research_catalog_graph_without_byte_artifact_reaches_native_admission() {
    let product = authored_product(super::super::carrier::GRAPH_CORRESPONDENCE_LAWS, false);
    assert!(
        product
            .artifact(super::super::carrier::CORRESPONDENCE_LAWS_PATH)
            .is_none()
    );
    let catalog = admitted_catalog(product);
    assert_eq!(catalog.program().correspondences.len(), 1);
    assert_eq!(
        catalog.program().correspondences[0].iri,
        "urn:authored:bridge"
    );
    assert_eq!(
        catalog.program().preservation,
        gmeow_logic_compile::ir::PreservationKind::SoundUnder
    );
}

#[test]
fn research_catalog_byte_bait_cannot_replace_required_named_graph() {
    let product = authored_product("urn:authored:wrong-graph", true);
    assert!(
        product
            .artifact(super::super::carrier::CORRESPONDENCE_LAWS_PATH)
            .is_some()
    );
    let upstream = BTreeMap::from([("stage-mappings".to_owned(), product)]);
    assert!(mappings_correspondence_program(&upstream).is_err());
    assert!(mappings_correspondence_program(&BTreeMap::new()).is_err());
}

#[test]
fn research_catalog_authenticated_restore_preserves_graph_and_native_program() {
    let directory = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(directory.path()).unwrap();
    let context =
        StageKeyContext::new("stage-mappings", "authored-catalog", Vec::new(), Vec::new());
    let product = authored_product(super::super::carrier::GRAPH_CORRESPONDENCE_LAWS, false);
    let selection = ReceiptOutputSelection {
        graphs: vec![super::super::carrier::GRAPH_CORRESPONDENCE_LAWS.to_owned()],
        default_graph: crate::cache::default_graph_commitment(&product).unwrap(),
        provenance: crate::cache::provenance_commitment(&product).unwrap(),
        content_store: crate::cache::content_store_commitment(&product).unwrap(),
        ..ReceiptOutputSelection::default()
    };
    cache
        .put(&context, "stable", "persistent", &selection, &product)
        .unwrap();
    let restored = cache.get(&context).unwrap().unwrap().product;
    assert_eq!(restored.digest, product.digest);
    assert!(
        restored
            .artifact(super::super::carrier::CORRESPONDENCE_LAWS_PATH)
            .is_none()
    );
    let before = admitted_catalog(product);
    let after = admitted_catalog(restored);
    assert_eq!(after.identity(), before.identity());
    assert_eq!(after.program(), before.program());
}
