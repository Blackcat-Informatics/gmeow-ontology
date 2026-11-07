// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny authored carrier/catalog inputs and an isolated authenticated cache only.

use super::*;
use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::cache::{PipelineCache, ReceiptOutputSelection, StageKeyContext};
use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::ir::{CorrespondenceLaw, DischargeVerdict, LawClaimIr};
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, parse_correspondence, project_correspondence_dataset,
};
use gmeow_logic_compile::projections::grounding_catalog::OwnedGroundingSource;

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

fn admitted_catalog(ontology: &[u8]) -> GroundingCatalog {
    let ontology = purrdf::parse_dataset(ontology, "text/turtle", None).unwrap();
    let owned = [OwnedGroundingSource::new(
        "https://blackcatinformatics.ca/gmeow/slices/lang",
        "slices/grounding/lang/mappings/authored.ttl",
        br#"@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
            <urn:owned:left> <http://www.w3.org/2004/02/skos/core#closeMatch> <urn:owned:right>
                {| gm:sssomFile "authored.sssom.tsv" |} ."#,
    )
    .unwrap()];
    let authority = GroundingCatalog::from_sources(&owned, &DslView::new(&ontology)).unwrap();
    let source = purrdf::parse_dataset(NATIVE_PROGRAM.as_bytes(), "text/turtle", None).unwrap();
    let mut cells = authority.program().correspondences.clone();
    cells.extend(parse_correspondence(&source).unwrap().correspondences);
    let program = CorrespondenceProgram::new(cells, authority.program().preservation)
        .with_native_sources(authority.program().native_sources.clone())
        .unwrap();
    GroundingCatalog::from_program(program, &DslView::new(&ontology), &owned).unwrap()
}

fn claim() -> LawClaimIr {
    LawClaimIr {
        law: CorrespondenceLaw::GetPut,
        verdict: DischargeVerdict::ObligationDischarged,
        condition: None,
    }
}

fn authored_product(
    catalog: GroundingCatalog,
    graph: &str,
    byte_bait: bool,
    publish_handle: bool,
) -> StageProduct {
    let source = project_correspondence_dataset(catalog.program()).unwrap();
    let dataset = super::super::carrier::rooted_in_graph(&source, graph).unwrap();
    let artifacts = if byte_bait {
        BTreeMap::from([(
            super::super::carrier::CORRESPONDENCE_LAWS_PATH.to_owned(),
            NATIVE_PROGRAM.as_bytes().to_vec(),
        )])
    } else {
        BTreeMap::new()
    };
    let mut bundle = bundle_from_artifacts_over(dataset, artifacts, Default::default());
    if publish_handle {
        let pin = bundle
            .graph_digest(
                graph,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
            )
            .unwrap();
        bundle
            .pin_handle(
                graph,
                PipelineHandle::GroundingCatalog(Arc::new(catalog)),
                pin,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
            )
            .unwrap();
    }
    StageProduct::from_bundle("stage-mappings", Arc::new(bundle))
        .expect("the stage carrier digests")
}

#[test]
fn research_catalog_reuses_admitted_authority_and_final_law_evidence() {
    let catalog = admitted_catalog(b"");
    let mut final_program = catalog.program().clone();
    let bridge = final_program
        .correspondences
        .iter_mut()
        .find(|cell| cell.iri == "urn:authored:bridge")
        .unwrap();
    bridge.law_claims.push(claim());
    let catalog = catalog.with_discharged_program(final_program).unwrap();
    let mut changed_authority = catalog.program().clone();
    let owned = changed_authority
        .correspondences
        .iter_mut()
        .find(|cell| cell.iri != "urn:authored:bridge")
        .unwrap();
    owned.law_claims.push(claim());
    assert!(
        catalog
            .clone()
            .with_discharged_program(changed_authority)
            .is_err()
    );
    let product = authored_product(
        catalog,
        super::super::carrier::GRAPH_CORRESPONDENCE_LAWS,
        false,
        true,
    );
    assert!(
        product
            .artifact(super::super::carrier::CORRESPONDENCE_LAWS_PATH)
            .is_none()
    );
    let upstream = BTreeMap::from([("stage-mappings".to_owned(), product)]);
    let catalog = mappings_grounding_catalog(&upstream).unwrap();
    let bridge = catalog
        .program()
        .correspondences
        .iter()
        .find(|cell| cell.iri == "urn:authored:bridge")
        .unwrap();
    assert_eq!(bridge.law_claims, vec![claim()]);
    let entry = upstream["stage-mappings"]
        .bundle()
        .handle(super::super::carrier::GRAPH_CORRESPONDENCE_LAWS)
        .unwrap();
    let PipelineHandle::GroundingCatalog(published) = &entry.payload else {
        panic!("the native catalog is mandatory");
    };
    assert!(std::ptr::eq(catalog, published.as_ref()));
}

#[test]
fn research_catalog_graph_and_byte_bait_cannot_replace_required_publication() {
    for (graph, publish_handle) in [
        ("urn:authored:wrong-graph", true),
        (super::super::carrier::GRAPH_CORRESPONDENCE_LAWS, false),
    ] {
        let product = authored_product(admitted_catalog(b""), graph, true, publish_handle);
        assert!(
            product
                .artifact(super::super::carrier::CORRESPONDENCE_LAWS_PATH)
                .is_some()
        );
        let upstream = BTreeMap::from([("stage-mappings".to_owned(), product)]);
        assert!(mappings_grounding_catalog(&upstream).is_err());
    }
    assert!(mappings_grounding_catalog(&BTreeMap::new()).is_err());
    let product = authored_product(
        admitted_catalog(b""),
        super::super::carrier::GRAPH_CORRESPONDENCE_LAWS,
        false,
        true,
    );
    let released = product.into_carrier_released().unwrap();
    let upstream = BTreeMap::from([("stage-mappings".to_owned(), released)]);
    assert!(mappings_grounding_catalog(&upstream).is_err());
}

#[test]
fn research_catalog_authenticated_restore_preserves_graph_and_native_program() {
    let directory = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(directory.path()).unwrap();
    let context =
        StageKeyContext::new("stage-mappings", "authored-catalog", Vec::new(), Vec::new());
    let ontology = br#"<urn:property> <https://blackcatinformatics.ca/logic/range>
        <http://www.w3.org/2000/01/rdf-schema#Literal> ;
        <http://www.w3.org/2000/01/rdf-schema#isDefinedBy> "owner label"@fr ."#;
    let before = admitted_catalog(ontology);
    let changed = admitted_catalog(
        b"<urn:property> <https://blackcatinformatics.ca/logic/range> <urn:other-range> .",
    );
    assert_eq!(before.identity(), changed.identity());
    let product = authored_product(
        before.clone(),
        super::super::carrier::GRAPH_CORRESPONDENCE_LAWS,
        false,
        true,
    );
    let changed_product = authored_product(
        changed,
        super::super::carrier::GRAPH_CORRESPONDENCE_LAWS,
        false,
        true,
    );
    assert_ne!(
        product.digest, changed_product.digest,
        "property support participates in native identity"
    );
    let selection = ReceiptOutputSelection {
        graphs: vec![super::super::carrier::GRAPH_CORRESPONDENCE_LAWS.to_owned()],
        handles: vec![super::super::carrier::GRAPH_CORRESPONDENCE_LAWS.to_owned()],
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
    let upstream = BTreeMap::from([("stage-mappings".to_owned(), restored)]);
    let after = mappings_grounding_catalog(&upstream).unwrap();
    assert_eq!(
        after, &before,
        "native authority and property values survive hydration"
    );
}
