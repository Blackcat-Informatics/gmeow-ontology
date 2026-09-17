// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use std::fs::OpenOptions;
use std::io::Write as _;

use gmeow_action_cache::ActionReceipt;
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram};
use purrdf::{PipelineBundle, RdfDatasetBuilder, RdfTerm, TermId, parse_dataset};

fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
    b.intern_iri(&format!("http://example.org/{n}"))
}

const GRAPH_IRI: &str = "http://example.org/graph";

fn test_context(stage_id: &str, salt: &str) -> StageKeyContext {
    StageKeyContext::new(stage_id, "test-v1", Vec::new(), Vec::new())
        .with_dimension("test-salt", salt)
}

#[test]
fn selected_dimension_is_a_first_class_action_key_input() {
    let base = StageKeyContext::new("stage", "test-v1", Vec::new(), Vec::new());
    assert_eq!(
        base.action_context().dimensions["pipeline-cache-version"],
        CACHE_VERSION.to_string(),
        "the product shape revision must move every pipeline action key",
    );
    let english = base.clone().with_dimension("language", "en");
    let french = base.clone().with_dimension("language", "fr");
    let docs = base.with_dimension("output", "docs");
    assert_ne!(stage_key(&english), stage_key(&french));
    assert_ne!(stage_key(&english), stage_key(&docs));
    assert_eq!(
        stage_key(&english),
        stage_key(
            &StageKeyContext::new("stage", "test-v1", vec![], vec![])
                .with_dimension("language", "en")
        ),
        "identical explicit feature selections must be cache-stable",
    );
}

#[test]
fn default_graph_commitment_covers_rdf12_reifiers_and_annotations() {
    fn product(with_annotation: bool) -> StageProduct {
        let mut builder = RdfDatasetBuilder::new();
        let reifier_term = RdfTerm::iri("http://example.org/reifier");
        let reifier = purrdf::RdfReifier::new(
            reifier_term.clone(),
            purrdf::RdfTriple::new(
                RdfTerm::iri("http://example.org/s"),
                "http://example.org/p",
                RdfTerm::iri("http://example.org/o"),
            ),
        );
        builder.push_owned_reifier(&reifier);
        if with_annotation {
            builder.push_owned_annotation(&purrdf::RdfAnnotation::new(
                reifier_term,
                "http://example.org/confidence",
                RdfTerm::iri("http://example.org/high"),
            ));
        }
        let dataset = builder
            .freeze()
            .expect("valid RDF 1.2 overlay-only dataset");
        StageProduct::from_bundle(
            "default-overlay",
            Arc::new(PipelineBundle::new(
                dataset,
                RdfLookaside::default(),
                Arc::new(ContentStore::new()),
                DatasetProvenance::new(),
            )),
        )
    }

    let complete = default_graph_commitment(&product(true))
        .unwrap()
        .expect("overlay-only default graph is a committed lane");
    let reifier_only = default_graph_commitment(&product(false))
        .unwrap()
        .expect("default-graph reifier is a committed lane");
    assert_eq!(complete.structural_count, 2);
    assert_eq!(reifier_only.structural_count, 1);
    assert_ne!(complete.digest, reifier_only.digest);
}

fn full_selection(product: &StageProduct) -> ReceiptOutputSelection {
    ReceiptOutputSelection {
        graphs: product_graph_names(product),
        blob_representations: product
            .bundle()
            .lookaside()
            .blobs
            .iter()
            .filter_map(|record| record.representation.clone())
            .collect(),
        logical_artifacts: product
            .bundle()
            .lookaside()
            .resources
            .iter()
            .filter_map(|resource| resource.name.clone())
            .collect(),
        handles: product.bundle().handles().keys().cloned().collect(),
        default_graph: default_graph_commitment(product).unwrap(),
        provenance: provenance_commitment(product).unwrap(),
        content_store: content_store_commitment(product).unwrap(),
    }
}

fn product_graph_names(product: &StageProduct) -> Vec<String> {
    product
        .dataset()
        .owned_named_graphs()
        .filter_map(|term| match term {
            RdfTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect()
}

fn persist_test_product(
    cache: &PipelineCache,
    context: &StageKeyContext,
    product: &StageProduct,
) -> StageReceipt {
    cache
        .put(
            context,
            "stable",
            "persistent",
            &full_selection(product),
            product,
        )
        .unwrap()
}

fn rewrite_test_receipt(
    cache: &PipelineCache,
    action_key: &str,
    mutate: impl FnOnce(&mut ActionReceipt<StageReceipt>),
) {
    let path = cache.receipt_path(action_key);
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut receipt: ActionReceipt<StageReceipt> =
        serde_json::from_value(envelope["receipt"].clone()).unwrap();
    mutate(&mut receipt);
    let envelope = serde_json::json!({
        "receipt_digest": receipt.digest(),
        "receipt": receipt,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();
}

fn write_test_receipt(cache: &PipelineCache, receipt: StageReceipt) {
    let action_key = receipt.action_key.clone();
    rewrite_test_receipt(cache, &action_key, |common| {
        if let Some(digest) = &receipt.product_blob_digest {
            common.product_blob = BlobRef {
                digest: digest.clone(),
                bytes: receipt.product_blob_bytes,
            };
        }
        common.payload = receipt;
    });
}

/// A tiny but real [`LogicProgram`] whose canonical RDF-1.2 projection backs the
/// cache's `Logic` handle. The cache persists this complete typed value separately
/// because the governed graph projection is deliberately lossy.
fn sample_logic_program() -> LogicProgram {
    let ax = |s: &str, o: &str| {
        LogicAxiom::new(
            s,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            gmeow_logic_compile::ir::AtomicTerm::resource(o),
            false,
            ContextualScope::default(),
        )
        .expect("valid axiom")
    };
    LogicProgram::new(
        vec![
            ax(
                "https://blackcatinformatics.ca/gmeow/Animal",
                "https://blackcatinformatics.ca/logic/Kind",
            ),
            ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                "https://blackcatinformatics.ca/logic/Subkind",
            ),
        ],
        vec![],
        vec![],
        None,
    )
}

/// A non-trivial dataset: one default-graph quad plus the canonical RDF-1.2
/// projection of [`sample_logic_program`] folded into named graph [`GRAPH_IRI`]
/// (so the attached `Logic` handle has a real, re-derivable backing graph).
fn dataset_with_named_graph() -> Arc<purrdf::RdfDataset> {
    let arts = gmeow_logic_compile::projections::compile_program(&sample_logic_program(), |_| {
        Default::default()
    })
    .expect("compile sample program");
    let logic_ds = parse_dataset(
        arts.canonical_rdf12.clone().into_text().content.as_bytes(),
        "text/turtle",
        None,
    )
    .expect("parse canonical rdf12");

    let mut b = RdfDatasetBuilder::new();
    let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
    b.push_quad(s, p, o, None); // a default-graph quad
    // Fold every triple of the logic projection into the named graph GRAPH_IRI.
    let graph = RdfTerm::Iri(GRAPH_IRI.to_owned());
    for quad in logic_ds.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        b.push_owned_quad(&routed);
    }
    b.freeze().expect("valid")
}

/// Build a richly populated bundle: dataset (≥1 named graph), a lookaside Blob
/// resource + matching blob, a populated provenance, and one attached handle.
fn rich_bundle() -> PipelineBundle<PipelineHandle> {
    let dataset = dataset_with_named_graph();

    // A byte-artifact-lane Blob resource + matching content-store blob.
    let mut blobs = ContentStore::new();
    let blob_digest = blobs.insert(b"artifact-bytes".to_vec());
    let mut lookaside = RdfLookaside::default();
    lookaside.resources.push(
        RdfLookasideResource::new(RdfLookasideKind::Blob)
            .with_name("generated/x.ttl")
            .with_digest(blob_digest.to_hex()),
    );

    // A populated provenance with a registered unit + an occurrence.
    let mut prov = DatasetProvenance::new();
    let unit = prov.register_unit("slices/core/epistemics", OriginKind::Source);
    let artifact = prov.register_artifact("slices/core/epistemics/epistemics.ttl");
    prov.record_occurrence(
        QuadHandle::from_index(0),
        unit,
        artifact,
        Some("epistemics.ttl:1".to_owned()),
    );

    let mut bundle = PipelineBundle::new(dataset, lookaside, Arc::new(blobs), prov);

    // Attach the REAL typed Logic handle (C6) over the named graph: the
    // payload is the compiled program, pinned to the canonical digest of its
    // backing `graph/logic` projection.
    let program = Arc::new(sample_logic_program());
    let pinned = bundle.graph_digest(GRAPH_IRI);
    bundle
        .pin_handle(GRAPH_IRI, PipelineHandle::Logic(program), pinned)
        .expect("pin handle over the named graph");
    bundle
}

/// A synthetic compiler publication with real source-attributed loss, causal
/// edges and both present/absent frame locations for the map-shaped codec.
fn report_publication() -> crate::bundle::CompiledLogicPublication {
    use crate::bundle::{CompiledLogicPublication, LogicReportInputs};
    use gmeow_logic_compile::ir::PreservationKind;
    use gmeow_logic_compile::loss_ledger::LossLedger;
    use gmeow_logic_compile::projections::report::{ProjectionReportRow, ReportHeader};
    let program = Arc::new(sample_logic_program());
    let mut loss = LossLedger::new();
    loss.record_projection_drops_attributed(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint requires projection".into()],
        &[(
            "source-owned distinction".into(),
            Some("https://example.org/source-term".into()),
        )],
    );
    let mut nodes = loss.to_nodes();
    let actual = nodes
        .iter_mut()
        .find(|node| !node.antecedents.is_empty())
        .unwrap();
    actual.frames.push(gmeow_errors::SerFrame {
        message: "while projecting the selected source".into(),
        at: Some(gmeow_errors::SerLocation {
            file: "synthetic-source.ttl".into(),
            line: 17,
            column: 3,
        }),
    });
    actual.frames.push(gmeow_errors::SerFrame {
        message: "retained context without a location".into(),
        at: None,
    });
    let mut header = ReportHeader::of_program(&program);
    header.correspondence_count = 0;
    CompiledLogicPublication {
        program,
        report: LogicReportInputs {
            header,
            base_correspondence_count: 5,
            base_lawful_uplift_count: 3,
            projections: vec![ProjectionReportRow {
                target: "owl-dl".into(),
                is_rdf: true,
                preservation: PreservationKind::SoundUnder,
                complexity: "2NEXPTIME".into(),
            }],
            loss: LossLedger::from_nodes(nodes),
        },
    }
}

/// Publish the tiny report fixture through the same native handle commitment.
fn report_product(publication: crate::bundle::CompiledLogicPublication) -> StageProduct {
    let source = rich_bundle();
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        source.dataset_arc(),
        crate::bundle::bundle_artifacts(&source),
        source.provenance().clone(),
    );
    let pin = bundle.graph_digest(GRAPH_IRI);
    bundle
        .pin_handle(
            GRAPH_IRI,
            PipelineHandle::CompiledLogic(Arc::new(publication)),
            pin,
        )
        .unwrap();
    StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle))
}

/// CBOR hydration retains the complete ledger and compact metadata; native
/// consumers share the program and report lifetime ends with the source product.
#[test]
fn cached_compiled_logic_preserves_report_metadata_and_complete_loss() {
    let publication = report_publication();
    let expected_nodes = publication.report.loss.to_nodes();
    let expected_header = publication.report.header;
    let expected_rows = publication.report.projections.clone();
    let program = Arc::clone(&publication.program);
    let product = report_product(publication);
    let handle = &product.bundle().handle(GRAPH_IRI).unwrap().payload;
    let PipelineHandle::CompiledLogic(shared) = handle else {
        panic!("compiled publication")
    };
    let weak = Arc::downgrade(shared);
    assert!(Arc::ptr_eq(handle.logic_program().unwrap(), &program));
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let context = test_context("stage-compile-logic", "native-report");
    let receipt = persist_test_product(&cache, &context, &product);
    let restored = cache.get(&context).unwrap().unwrap();
    assert_eq!(restored.receipt, receipt);
    assert_eq!(restored.product.digest, product.digest);
    assert_eq!(restored.product.artifacts(), product.artifacts());
    let PipelineHandle::CompiledLogic(rebuilt) =
        &restored.product.bundle().handle(GRAPH_IRI).unwrap().payload
    else {
        panic!("complete report publication")
    };
    assert_eq!(rebuilt.report.header, expected_header);
    assert_eq!(rebuilt.report.projections, expected_rows);
    assert_eq!(rebuilt.report.base_correspondence_count, 5);
    assert_eq!(rebuilt.report.base_lawful_uplift_count, 3);
    assert_eq!(rebuilt.report.loss.to_nodes(), expected_nodes);
    let released = product.into_carrier_released().unwrap();
    assert!(weak.upgrade().is_none());
    assert!(released.bundle().handles().is_empty());
    assert_eq!(released.artifacts(), restored.product.artifacts());
}

/// Metadata and loss mutations fail both the handle commitment and the enclosing
/// product commitment, even with unchanged graph bytes and valid CBOR encoding.
#[test]
fn cached_compiled_logic_rejects_report_metadata_and_loss_tampering() {
    for mutation in 0..8 {
        let product = report_product(report_publication());
        let mut cached = CachedBundle::from_product(&product, &full_selection(&product)).unwrap();
        let original = cached.handles[0].typed_payload.as_ref().unwrap();
        let mut changed: crate::bundle::CompiledLogicPublication =
            ciborium::de::from_reader(original.as_slice()).unwrap();
        match mutation {
            0 => changed.report.header.formula_count += 1,
            1 => changed.report.base_correspondence_count += 1,
            2 => changed.report.base_lawful_uplift_count += 1,
            3 => changed.report.projections[0].target.push_str("-changed"),
            4 => changed.report.projections[0]
                .complexity
                .push_str("-changed"),
            5 => {
                changed.report.projections[0].preservation =
                    gmeow_logic_compile::ir::PreservationKind::CompleteOver
            }
            6 => changed.report.projections[0].is_rdf = false,
            _ => {
                let mut nodes = changed.report.loss.to_nodes();
                let actual = nodes
                    .iter_mut()
                    .find(|node| !node.antecedents.is_empty())
                    .unwrap();
                actual.observations[0].message.push_str(" changed");
                actual.frames[0].message.push_str(" changed");
                changed.report.loss =
                    gmeow_logic_compile::loss_ledger::LossLedger::from_nodes(nodes);
            }
        }
        let changed = PipelineHandle::CompiledLogic(Arc::new(changed));
        cached.handles[0].typed_payload = Some(encode_handle(&changed).unwrap());
        let encoded = bincode::serialize(&cached).unwrap();
        let stale_handle: CachedBundle = bincode::deserialize(&encoded).unwrap();
        assert!(
            stale_handle
                .into_product()
                .unwrap_err()
                .is::<crate::error::CacheMismatch>()
        );
        cached.handles[0].payload_digest = handle_payload_digest(&changed);
        assert!(
            cached
                .into_product()
                .unwrap_err()
                .is::<crate::error::CacheMismatch>()
        );
    }
}

/// Missing or noncanonical byte envelopes cannot hydrate a report publication.
#[test]
fn compiled_logic_codec_refuses_missing_truncated_and_trailing_payloads() {
    #[derive(Serialize)]
    struct ProgramOnly<'a> {
        program: &'a LogicProgram,
    }
    let publication = report_publication();
    let mut incomplete = Vec::new();
    ciborium::ser::into_writer(
        &ProgramOnly {
            program: &publication.program,
        },
        &mut incomplete,
    )
    .unwrap();
    assert!(rebuild_handle("compiled-logic", Some(&incomplete)).is_err());
    let handle = PipelineHandle::CompiledLogic(Arc::new(publication));
    let mut bytes = encode_handle(&handle).unwrap();
    assert!(rebuild_handle("compiled-logic", None).is_err());
    assert!(rebuild_handle("compiled-logic", Some(&bytes[..bytes.len() - 1])).is_err());
    bytes.push(0);
    assert!(rebuild_handle("compiled-logic", Some(&bytes)).is_err());
}

fn canon_hex(ds: &purrdf::RdfDataset) -> String {
    ContentDigest::of(canonicalize(ds).nquads.as_bytes()).to_hex()
}

#[test]
fn cached_bundle_structural_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();

    let original = rich_bundle();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(original.clone()));
    let context = test_context("stage-rich", "structural-round-trip");
    persist_test_product(&cache, &context, &product);

    let got = cache.get(&context).unwrap().expect("cache hit");
    let recon = got.product.bundle();

    // dataset: canonical hash equal.
    assert_eq!(
        canon_hex(recon.dataset()),
        canon_hex(original.dataset()),
        "dataset canonical hash preserved"
    );

    // lookaside: every resource + blob record equal.
    assert_eq!(
        recon.lookaside(),
        original.lookaside(),
        "lookaside reconstructed field-for-field"
    );

    // blobs: every digest → bytes equal.
    assert_eq!(recon.blobs(), original.blobs(), "blob store preserved");

    // Native handles are restored completely and remain pin-valid.
    assert_eq!(
        recon.handles().len(),
        original.handles().len(),
        "handle count"
    );
    let entry = recon.handle(GRAPH_IRI).expect("handle re-attached");
    let PipelineHandle::Logic(reconstituted) = &entry.payload else {
        panic!("handle arm preserved (Logic)");
    };
    // The complete typed Logic handle (C6) survives serialization while remaining
    // pinned to its governed `graph/logic` projection.
    assert_eq!(
        reconstituted.canonical_key(),
        sample_logic_program().canonical_key(),
        "the cache preserved the Logic handle's program canonical-key-equal"
    );
    // The pinned digest matches the LIVE backing graph (pin_handle re-checked it).
    assert_eq!(
        entry.content_digest,
        recon.graph_digest(GRAPH_IRI),
        "handle pin matches the reconstituted graph"
    );

    // provenance: public projection equal.
    assert_eq!(
        recon.provenance().public_projection(),
        original.provenance().public_projection(),
        "public provenance projection preserved"
    );

    // digest: bundle content fold equal.
    assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");

    // byte-artifact lane: reproduced byte-for-byte.
    assert_eq!(
        got.product.artifacts(),
        product.artifacts(),
        "byte-artifact lane reproduced exactly"
    );
    // The product's cache-key digest is preserved too.
    assert_eq!(
        got.product.digest, product.digest,
        "stage-product digest preserved"
    );
}

#[test]
fn cached_payloads_borrow_authenticated_input_without_changing_wire_bytes() {
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let owned = CachedBundle::from_product(&product, &full_selection(&product)).unwrap();
    let encoded = bincode::serialize(&owned).unwrap();
    let borrowed: CachedBundle<&[u8]> = bincode::deserialize(&encoded).unwrap();
    let assert_borrowed = |bytes: &[u8]| {
        assert!(!bytes.is_empty(), "the fixture must exercise a payload");
        let start = encoded.as_ptr().addr();
        let end = start + encoded.len();
        assert!(bytes.as_ptr().addr() >= start);
        assert!(bytes.as_ptr().addr() + bytes.len() <= end);
    };
    assert_borrowed(borrowed.dataset_pack);
    for bytes in borrowed.blobs.values() {
        assert_borrowed(bytes);
    }
    for handle in &borrowed.handles {
        assert_borrowed(handle.typed_payload.unwrap());
    }
    assert_eq!(bincode::serialize(&borrowed).unwrap(), encoded);
    assert_eq!(borrowed.into_product().unwrap().digest, product.digest);
}

#[test]
fn exact_artifact_selection_returns_only_the_requested_payload() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_artifacts(
        "stage-select",
        BTreeMap::from([
            ("wanted.json".to_owned(), b"wanted".to_vec()),
            ("other.json".to_owned(), "other".repeat(16_384).into_bytes()),
        ]),
    );
    let context = test_context("stage-select", "one-artifact");
    let receipt = persist_test_product(&cache, &context, &product);
    let hit = cache
        .get_artifact(&context, "wanted.json")
        .unwrap()
        .unwrap();
    assert_eq!(hit.receipt, receipt);
    assert_eq!(
        hit.artifacts,
        BTreeMap::from([("wanted.json".into(), b"wanted".to_vec())])
    );
    assert_eq!(
        hit.transferred_bytes, receipt.product_blob_bytes,
        "the complete product is still authenticated"
    );

    let error = cache.get_artifact(&context, "absent.json").unwrap_err();
    assert!(error.is::<crate::error::StageFailed>(), "{error}");
    assert!(
        cache
            .get_artifact(
                &test_context("stage-select", "absent-action"),
                "wanted.json"
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn selected_artifacts_share_authentication_and_fail_on_missing_members() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_artifacts(
        "stage-select",
        BTreeMap::from([
            ("first.json".to_owned(), b"first".to_vec()),
            ("second.json".to_owned(), b"second".to_vec()),
            (
                "unselected.json".to_owned(),
                "other".repeat(16_384).into_bytes(),
            ),
        ]),
    );
    let context = test_context("stage-select", "several-artifacts");
    let receipt = persist_test_product(&cache, &context, &product);
    let hit = cache
        .get_selected_artifacts(&context, &["second.json", "first.json", "second.json"])
        .unwrap()
        .unwrap();
    assert_eq!(hit.receipt, receipt);
    assert_eq!(hit.transferred_bytes, receipt.product_blob_bytes);
    assert_eq!(
        hit.artifacts,
        BTreeMap::from([
            ("first.json".to_owned(), b"first".to_vec()),
            ("second.json".to_owned(), b"second".to_vec()),
        ])
    );
    let error = cache
        .get_selected_artifacts(&context, &["first.json", "absent.json"])
        .unwrap_err();
    assert!(error.is::<crate::error::StageFailed>(), "{error}");
    let empty = cache
        .get_selected_artifacts(&context, &[])
        .unwrap()
        .unwrap();
    assert_eq!(empty.receipt, receipt);
    assert!(empty.artifacts.is_empty());
}

#[test]
fn exact_artifact_selection_rejects_corruption_in_an_unselected_payload() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_artifacts(
        "stage-select",
        BTreeMap::from([
            ("wanted.json".to_owned(), b"wanted".to_vec()),
            ("other.json".to_owned(), b"other".to_vec()),
        ]),
    );
    let context = test_context("stage-select", "unselected-corruption");
    let mut receipt = persist_test_product(&cache, &context, &product);
    let original = std::fs::read(
        cache
            .store
            .root()
            .join("blobs")
            .join(receipt.product_blob_digest.as_ref().unwrap()),
    )
    .unwrap();
    let mut manifest: CachedBundle = bincode::deserialize(&original).unwrap();
    let other_digest = ContentDigest::of(b"other").to_hex();
    manifest.blobs.get_mut(&other_digest).unwrap()[0] ^= 0xff;

    // Authenticate the enclosing bytes so only the unselected artifact's
    // own commitment can detect this internally inconsistent product.
    let corrupted = bincode::serialize(&manifest).unwrap();
    let digest = ContentDigest::of(&corrupted).to_hex();
    std::fs::write(cache.store.root().join("blobs").join(&digest), &corrupted).unwrap();
    receipt.product_blob_digest = Some(digest);
    receipt.product_blob_bytes = u64::try_from(corrupted.len()).unwrap();
    write_test_receipt(&cache, receipt);
    let error = cache.get_artifact(&context, "wanted.json").unwrap_err();
    assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
    let error = cache
        .get_selected_artifacts(&context, &["wanted.json", "wanted.json"])
        .unwrap_err();
    assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
    let error = cache.get_selected_artifacts(&context, &[]).unwrap_err();
    assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
}

#[test]
fn selective_artifact_hit_matches_full_hydration_and_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "artifact-only-hit");
    let receipt = persist_test_product(&cache, &context, &product);

    assert_eq!(
        cache
            .inspect_receipt(&context)
            .expect("receipt inspection")
            .expect("receipt exists"),
        receipt
    );
    let selective = cache
        .get_artifacts(&context)
        .expect("selective lookup")
        .expect("selective hit");
    let full = cache.get(&context).expect("full lookup").expect("full hit");
    assert_eq!(selective.receipt, full.receipt);
    assert_eq!(selective.artifacts, full.product.artifacts());
    assert_eq!(
        selective.transferred_bytes, full.hydrated_bytes,
        "both paths authenticate the same complete product blob"
    );
}

#[test]
fn selective_artifact_hit_rejects_inner_digest_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "artifact-inner-corruption");
    let mut receipt = persist_test_product(&cache, &context, &product);
    let original_blob = cache
        .store
        .root()
        .join("blobs")
        .join(receipt.product_blob_digest.as_ref().unwrap());
    let mut manifest: CachedBundle =
        bincode::deserialize(&std::fs::read(original_blob).unwrap()).unwrap();
    let artifact_digest = manifest.lookaside.resources[0]
        .content_digest
        .clone()
        .expect("artifact digest");
    manifest.blobs.get_mut(&artifact_digest).unwrap()[0] ^= 0xff;

    // Keep the enclosing product blob self-consistent so the selective reader must
    // reach and enforce the receipt's artifact-level digest, not merely the outer hash.
    let corrupted = bincode::serialize(&manifest).unwrap();
    let corrupted_digest = ContentDigest::of(&corrupted).to_hex();
    std::fs::write(
        cache.store.root().join("blobs").join(&corrupted_digest),
        &corrupted,
    )
    .unwrap();
    receipt.product_blob_digest = Some(corrupted_digest);
    receipt.product_blob_bytes = u64::try_from(corrupted.len()).unwrap();
    write_test_receipt(&cache, receipt);

    let error = cache
        .get_artifacts(&context)
        .expect_err("artifact digest mismatch must hard-fail");
    assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
}

/// A forged payload can be internally well encoded and still contradict the
/// product identity the authenticated producer published.
#[test]
fn restored_typed_product_rejects_a_changed_payload_under_the_old_digest() {
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let mut manifest = CachedBundle::from_product(&product, &full_selection(&product)).unwrap();
    let handle = &mut manifest.handles[0];
    let mut program: LogicProgram =
        bincode::deserialize(handle.typed_payload.as_ref().unwrap()).unwrap();
    program.source_iri = Some("https://example.org/forged-source".into());
    handle.typed_payload = Some(bincode::serialize(&program).unwrap());
    handle.payload_digest = handle_payload_digest(&PipelineHandle::Logic(Arc::new(program)));
    let error = manifest
        .into_product()
        .expect_err("the whole-product commitment must be recomputed");
    assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
}

/// Counts are a projection, so equal-length answers and unprojected row
/// contracts must still have different native commitments.
#[test]
fn reasoning_rows_and_declared_schema_change_native_identity() {
    use gmeow_logic::result::ResultPayload;
    use gmeow_logic::result_shape::{ColumnKind, ResultColumn, ResultShape, RowCardinality};
    let mut first = sample_reasoning_result();
    first.payload = ResultPayload::Bindings(vec![BTreeMap::from([(
        "x".into(),
        "https://example.org/a".into(),
    )])]);
    let mut second = first.clone();
    second.payload = ResultPayload::Bindings(vec![BTreeMap::from([(
        "x".into(),
        "https://example.org/b".into(),
    )])]);
    let mut schema = first.clone();
    schema.row_schema = Some(ResultShape::new(
        vec![ResultColumn::required("x", ColumnKind::Iri)],
        RowCardinality::Contains,
    ));
    let projection = gmeow_logic::result_rdf::project_reasoning_result(&first)
        .expect("project the valid initial binding result");
    for changed in [second, schema] {
        assert_eq!(
            projection,
            gmeow_logic::result_rdf::project_reasoning_result(&changed)
                .expect("project the valid changed binding result")
        );
        assert_ne!(
            handle_payload_digest(&PipelineHandle::Reasoning(Arc::new(first.clone()))),
            handle_payload_digest(&PipelineHandle::Reasoning(Arc::new(changed))),
        );
    }
}

/// Residue boundaries and optional provenance cannot be flattened into an
/// ambiguous newline-delimited cache key.
#[test]
fn relational_residue_and_source_boundaries_change_native_identity() {
    use gmeow_logic_compile::relational_core::{RcResidue, RelationalCoreProgram};
    let first = RelationalCoreProgram {
        facts: Vec::new(),
        rules: Vec::new(),
        residue: vec![RcResidue {
            reason: "a\nb".into(),
        }],
        source_iri: None,
    };
    let mut second = first.clone();
    second.residue = vec![
        RcResidue { reason: "a".into() },
        RcResidue { reason: "b".into() },
    ];
    let mut source = first.clone();
    source.source_iri = Some(String::new());
    for changed in [second, source] {
        assert_ne!(
            first.content_key().unwrap(),
            changed.content_key().unwrap(),
            "native canonical identity retains residue and source boundaries"
        );
        assert_ne!(
            handle_payload_digest(&PipelineHandle::RelationalCore(Arc::new(first.clone()))),
            handle_payload_digest(&PipelineHandle::RelationalCore(Arc::new(changed))),
        );
    }
}

#[test]
fn cached_bundle_binary_encoding_avoids_json_byte_array_expansion() {
    let payload = vec![0xff; 4096];
    let mut blobs = BTreeMap::new();
    blobs.insert("blob-digest".to_owned(), payload.clone());
    let manifest = CachedBundle {
        version: CACHE_VERSION,
        stage_id: "compact-cache-regression".to_owned(),
        digest: "product-digest".to_owned(),
        dataset_pack: payload.clone(),
        lookaside: CachedLookaside::default(),
        blobs,
        provenance: Vec::new(),
        handles: vec![CachedHandle {
            graph: "http://example.org/graph".to_owned(),
            arm: "logic".to_owned(),
            payload_digest: "payload-digest".to_owned(),
            typed_payload: Some(payload.clone()),
        }],
    };

    let binary = bincode::serialize(&manifest).expect("serialize compact cache manifest");
    let json = serde_json::to_vec(&manifest).expect("serialize comparison manifest");
    assert!(
        binary.len() * 3 < json.len(),
        "byte lanes must stay compact: binary={} JSON={}",
        binary.len(),
        json.len()
    );

    let decoded: CachedBundle =
        bincode::deserialize(&binary).expect("deserialize compact cache manifest");
    assert_eq!(decoded.dataset_pack, manifest.dataset_pack);
    assert_eq!(decoded.blobs, manifest.blobs);
    assert_eq!(decoded.handles[0].graph, manifest.handles[0].graph);
    assert_eq!(decoded.handles[0].arm, manifest.handles[0].arm);
}

#[test]
fn persistent_unit_rejects_unselected_cumulative_lanes() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "bounded-delta");

    let mut missing_graph = full_selection(&product);
    missing_graph.graphs.clear();
    let err = cache
        .put(&context, "stable", "persistent", &missing_graph, &product)
        .expect_err("an unselected graph is cumulative carrier residue");
    assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

    let mut missing_artifact = full_selection(&product);
    missing_artifact.logical_artifacts.clear();
    let err = cache
        .put(
            &context,
            "stable",
            "persistent",
            &missing_artifact,
            &product,
        )
        .expect_err("an unselected artifact is cumulative carrier residue");
    assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

    let mut missing_default_graph = full_selection(&product);
    missing_default_graph.default_graph = None;
    let err = cache
        .put(
            &context,
            "stable",
            "persistent",
            &missing_default_graph,
            &product,
        )
        .expect_err("an uncommitted default graph is cumulative carrier residue");
    assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

    let mut missing_provenance = full_selection(&product);
    missing_provenance.provenance = None;
    let err = cache
        .put(
            &context,
            "stable",
            "persistent",
            &missing_provenance,
            &product,
        )
        .expect_err("uncommitted provenance is cumulative carrier residue");
    assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

    let mut missing_content_store = full_selection(&product);
    missing_content_store.content_store = None;
    let err = cache
        .put(
            &context,
            "stable",
            "persistent",
            &missing_content_store,
            &product,
        )
        .expect_err("an uncommitted content store is cumulative carrier residue");
    assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

    assert_eq!(cache.len(), 0, "a rejected unit publishes no receipt");
}

#[test]
fn content_store_commitment_rejects_orphans_missing_bytes_and_length_drift() {
    fn product(lookaside: RdfLookaside, blobs: ContentStore, salt: &str) -> StageProduct {
        let dataset = parse_dataset(b"", "application/n-quads", None).unwrap();
        StageProduct::from_bundle(
            format!("content-store-{salt}"),
            Arc::new(PipelineBundle::new(
                dataset,
                lookaside,
                Arc::new(blobs),
                DatasetProvenance::new(),
            )),
        )
    }

    let mut orphan_store = ContentStore::new();
    orphan_store.insert(b"orphan".to_vec());
    assert!(
        content_store_commitment(&product(RdfLookaside::default(), orphan_store, "orphan",))
            .is_err()
    );

    let missing_digest = ContentDigest::of(b"missing").to_hex();
    let mut missing_lookaside = RdfLookaside::default();
    missing_lookaside.resources.push(
        RdfLookasideResource::new(RdfLookasideKind::Blob)
            .with_name("generated/missing.bin")
            .with_digest(missing_digest),
    );
    assert!(
        content_store_commitment(&product(missing_lookaside, ContentStore::new(), "missing",))
            .is_err()
    );

    let mut length_store = ContentStore::new();
    let length_digest = length_store.insert(b"bytes".to_vec());
    let mut length_lookaside = RdfLookaside::default();
    length_lookaside.blobs.push(RdfBlobRecord {
        digest: length_digest.to_hex(),
        media_type: Some("application/octet-stream".to_string()),
        representation: Some("test:length-drift".to_string()),
        decoded_len: Some(99),
        metadata: BTreeMap::new(),
        origin: None,
    });
    assert!(content_store_commitment(&product(length_lookaside, length_store, "length",)).is_err());
}

#[test]
fn tampered_handle_manifest_hard_fails_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();

    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "tampered-handle");
    let mut receipt = persist_test_product(&cache, &context, &product);

    // Tamper the persisted handle arm while keeping the packed dataset intact.
    // An unknown arm must HARD-FAIL rather than silently dropping the handle.
    let blobs_dir = cache.store.root().join("blobs");
    let blob_path = std::fs::read_dir(&blobs_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = std::fs::read(&blob_path).unwrap();
    let mut manifest: CachedBundle = bincode::deserialize(&bytes).unwrap();
    manifest.handles[0].arm = "not-a-pipeline-handle".to_owned();
    // Re-serialize + re-file under the NEW content digest (and re-point the receipt),
    // so the blob still self-verifies and we exercise the handle re-pin path.
    let new_bytes = bincode::serialize(&manifest).unwrap();
    let new_hex = ContentDigest::of(&new_bytes).to_hex();
    std::fs::write(blobs_dir.join(&new_hex), &new_bytes).unwrap();
    let new_bytes_len = u64::try_from(new_bytes.len()).unwrap();
    receipt.product_blob_digest = Some(new_hex.clone());
    receipt.product_blob_bytes = new_bytes_len;
    rewrite_test_receipt(&cache, &stage_key(&context), |common| {
        common.product_blob = BlobRef {
            digest: new_hex,
            bytes: new_bytes_len,
        };
        common.payload = receipt;
    });
    let reopened = PipelineCache::open(dir.path()).unwrap();

    let err = reopened
        .get(&context)
        .expect_err("a stale/dropped handle must hard-fail");
    assert!(
        err.is::<crate::error::Decode>(),
        "tampered handle manifest hard-fails, got {err:?}"
    );
}

#[test]
fn tampered_provenance_quad_key_hard_fails_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "tampered-provenance");
    let mut receipt = persist_test_product(&cache, &context, &product);

    let original_blob = cache
        .store
        .root()
        .join("blobs")
        .join(receipt.product_blob_digest.as_ref().unwrap());
    let mut manifest: CachedBundle =
        bincode::deserialize(&std::fs::read(original_blob).unwrap()).unwrap();
    manifest.provenance[0].quad_key = "absent-asserted-quad".to_string();
    let forged = bincode::serialize(&manifest).unwrap();
    let forged_digest = ContentDigest::of(&forged).to_hex();
    std::fs::write(
        cache.store.root().join("blobs").join(&forged_digest),
        &forged,
    )
    .unwrap();
    let forged_bytes = u64::try_from(forged.len()).unwrap();
    receipt.product_blob_digest = Some(forged_digest.clone());
    receipt.product_blob_bytes = forged_bytes;
    rewrite_test_receipt(&cache, &stage_key(&context), |common| {
        common.product_blob = BlobRef {
            digest: forged_digest,
            bytes: forged_bytes,
        };
        common.payload = receipt;
    });

    let error = cache
        .get(&context)
        .expect_err("an unresolvable stable provenance key must hard-fail");
    assert!(error.is::<crate::error::CacheMismatch>(), "{error:?}");
}

#[test]
fn receipt_is_cold_warm_identical_and_structurally_complete() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
    let context = test_context("stage-rich", "receipt-parity");
    let selection = full_selection(&product);
    let cold = persist_test_product(&cache, &context, &product);
    let warm = cache.get(&context).unwrap().expect("cache hit");
    assert_eq!(cold, warm.receipt);
    assert_eq!(cold.digest(), warm.receipt.digest());
    assert_eq!(cold.graphs.len(), 1);
    assert_eq!(cold.typed_handles.len(), 1);
    assert_eq!(cold.logical_artifacts.len(), 1);
    PipelineCache::validate_hit_receipt(&context, "stable", "persistent", &selection, &warm)
        .unwrap();

    // A self-consistent envelope that silently drops an output row is still
    // structurally invalid against the live stage declaration/product.
    let mut incomplete = cold;
    incomplete.graphs.clear();
    write_test_receipt(&cache, incomplete);
    let hit = cache.get(&context).unwrap().expect("blob remains readable");
    assert!(
        PipelineCache::validate_hit_receipt(&context, "stable", "persistent", &selection, &hit,)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );
}

#[test]
fn receipt_and_blob_corruption_matrix_hard_fails() {
    // Truncated receipt.
    let truncated = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(truncated.path()).unwrap();
    let product = StageProduct::new("stage", "digest");
    let context = test_context("stage", "truncated");
    persist_test_product(&cache, &context, &product);
    std::fs::write(cache.receipt_path(&stage_key(&context)), b"{").unwrap();
    assert!(
        cache
            .get(&context)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );

    // Referenced missing blob.
    let missing = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(missing.path()).unwrap();
    let context = test_context("stage", "missing-blob");
    let receipt = persist_test_product(&cache, &context, &product);
    std::fs::remove_file(
        cache
            .store
            .root()
            .join("blobs")
            .join(receipt.product_blob_digest.unwrap()),
    )
    .unwrap();
    assert!(
        cache
            .get(&context)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );

    // Receipt copied under a different action key.
    let wrong = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(wrong.path()).unwrap();
    let first = test_context("stage", "first-key");
    persist_test_product(&cache, &first, &product);
    let second = test_context("stage", "second-key");
    std::fs::copy(
        cache.receipt_path(&stage_key(&first)),
        cache.receipt_path(&stage_key(&second)),
    )
    .unwrap();
    assert!(
        cache
            .get(&second)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );

    // Oversized receipt root: sparse growth proves the bound without allocating
    // the forged size. The reader rejects it before JSON allocation/parsing.
    let oversized_receipt = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(oversized_receipt.path()).unwrap();
    let context = test_context("stage", "oversized-receipt");
    persist_test_product(&cache, &context, &product);
    OpenOptions::new()
        .write(true)
        .open(cache.receipt_path(&stage_key(&context)))
        .unwrap()
        .set_len(MAX_RECEIPT_BYTES + 1)
        .unwrap();
    assert!(
        cache
            .get(&context)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );

    // Oversized referenced blob: the same sparse-file attack is rejected before
    // hydration, even though the immutable receipt still names a small product.
    let oversized_blob = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(oversized_blob.path()).unwrap();
    let context = test_context("stage", "oversized-blob");
    let receipt = persist_test_product(&cache, &context, &product);
    let blob = cache
        .store
        .root()
        .join("blobs")
        .join(receipt.product_blob_digest.unwrap());
    OpenOptions::new()
        .write(true)
        .open(blob)
        .unwrap()
        .set_len(MAX_ENTRY_BYTES + 1)
        .unwrap();
    assert!(
        cache
            .get(&context)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );
}

#[test]
fn concurrent_publication_is_atomic_and_nondeterminism_fails() {
    use std::sync::Barrier;

    fn race(
        root: PathBuf,
        context: StageKeyContext,
        product: StageProduct,
        barrier: Arc<Barrier>,
    ) -> Result<StageReceipt, gmeow_errors::Diag> {
        let cache = PipelineCache::open(root).unwrap();
        barrier.wait();
        cache.put(
            &context,
            "stable",
            "persistent",
            &ReceiptOutputSelection::default(),
            &product,
        )
    }

    let dir = tempfile::tempdir().unwrap();
    let context = test_context("stage", "same-key");
    let barrier = Arc::new(Barrier::new(2));
    let left = {
        let root = dir.path().to_path_buf();
        let context = context.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || race(root, context, StageProduct::new("stage", "left"), barrier))
    };
    let right = {
        let root = dir.path().to_path_buf();
        let context = context.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            race(root, context, StageProduct::new("stage", "right"), barrier)
        })
    };
    let outcomes = [left.join().unwrap(), right.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| {
                result
                    .as_ref()
                    .is_err_and(|error| error.is::<crate::error::CacheMismatch>())
            })
            .count(),
        1,
        "same action key with different output is nondeterminism"
    );
    assert_eq!(PipelineCache::open(dir.path()).unwrap().len(), 1);

    // Different keys publish independently and neither receipt is lost.
    let dir = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["left", "right"]
        .into_iter()
        .map(|salt| {
            let root = dir.path().to_path_buf();
            let context = test_context("stage", salt);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                race(root, context, StageProduct::new("stage", salt), barrier)
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    assert_eq!(PipelineCache::open(dir.path()).unwrap().len(), 2);
}

#[test]
fn fixture_coordinator_elects_exactly_one_thread_builder() {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let root = tempfile::tempdir().unwrap();
    let context = test_context("fixture-stage", "one-builder");
    let starts = Arc::new(Barrier::new(2));
    let builds = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let root = root.path().to_path_buf();
        let context = context.clone();
        let starts = Arc::clone(&starts);
        let builds = Arc::clone(&builds);
        workers.push(std::thread::spawn(move || {
            let coordinator = FixtureCoordinator::open(&root).unwrap();
            starts.wait();
            coordinator
                .get_or_build(
                    &context,
                    "stable",
                    "persistent",
                    |_| Ok(ReceiptOutputSelection::default()),
                    || {
                        builds.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        Ok(StageProduct::new("fixture-stage", "fixture-digest"))
                    },
                )
                .unwrap()
        }));
    }
    let outcomes: Vec<FixtureOutcome> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(outcomes.iter().filter(|outcome| outcome.built).count(), 1);
    assert_eq!(outcomes[0].receipt, outcomes[1].receipt);
    assert_eq!(outcomes[0].product.digest, outcomes[1].product.digest);
}

#[test]
fn fixture_coordinator_preserves_the_producer_diagnostic_kind() {
    let root = tempfile::tempdir().unwrap();
    let context = test_context("fixture-failure", "typed-error");
    let coordinator = FixtureCoordinator::open(root.path()).unwrap();
    let error = coordinator
        .get_or_build(
            &context,
            "stable",
            "persistent",
            |_| Ok(ReceiptOutputSelection::default()),
            || {
                Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "fixture-failure".to_string(),
                    message: "intentional producer refusal".to_string(),
                }))
            },
        )
        .expect_err("a producer refusal must escape the cache coordinator");
    assert!(error.is::<crate::error::StageFailed>(), "{error}");
    assert!(!error.is::<crate::error::CacheMismatch>(), "{error}");
}

const FIXTURE_PROCESS_ROOT: &str = "GMEOW_FIXTURE_PROCESS_TEST_ROOT";

#[test]
fn fixture_coordinator_process_worker() {
    let Ok(root) = std::env::var(FIXTURE_PROCESS_ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let context = test_context("fixture-process-stage", "one-process-builder");
    let coordinator = FixtureCoordinator::open(&root).unwrap();
    let outcome = coordinator
        .get_or_build(
            &context,
            "stable",
            "persistent",
            |_| Ok(ReceiptOutputSelection::default()),
            || {
                let mut marker = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(root.join("builder.marker"))?;
                writeln!(marker, "{}", std::process::id())?;
                marker.sync_all()?;
                std::thread::sleep(std::time::Duration::from_millis(150));
                Ok(StageProduct::new(
                    "fixture-process-stage",
                    "fixture-process-digest",
                ))
            },
        )
        .unwrap();
    println!("fixture-process-built={}", outcome.built);
}

#[test]
fn fixture_coordinator_elects_exactly_one_builder_across_processes() {
    use std::process::{Command, Stdio};

    let root = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let spawn = || {
        Command::new(&executable)
            .arg("--exact")
            .arg("cache::tests::fixture_coordinator_process_worker")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(FIXTURE_PROCESS_ROOT, root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let left = spawn();
    let right = spawn();
    let outputs = [
        left.wait_with_output().unwrap(),
        right.wait_with_output().unwrap(),
    ];
    for output in &outputs {
        assert!(
            output.status.success(),
            "fixture worker failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    let stdout = outputs
        .iter()
        .map(|output| String::from_utf8_lossy(&output.stdout))
        .collect::<Vec<_>>();
    assert_eq!(
        stdout
            .iter()
            .filter(|text| text.contains("fixture-process-built=true"))
            .count(),
        1,
        "exactly one OS process must execute the fixture producer: {stdout:?}",
    );
    assert_eq!(
        stdout
            .iter()
            .filter(|text| text.contains("fixture-process-built=false"))
            .count(),
        1,
        "the losing OS process must hydrate the elected product: {stdout:?}",
    );
    assert!(root.path().join("builder.marker").is_file());
}

#[test]
fn bounded_store_evicts_only_unreachable_entries_and_ignores_crash_temps() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path())
        .unwrap()
        .with_limits(1, 1024 * 1024);
    let first = test_context("stage", "first");
    let second = test_context("stage", "second");
    persist_test_product(&cache, &first, &StageProduct::new("stage", "first"));
    std::fs::write(
        cache
            .store
            .root()
            .join("receipts")
            .join(".pipeline-cache-crash.tmp"),
        b"partial",
    )
    .unwrap();
    persist_test_product(&cache, &second, &StageProduct::new("stage", "second"));
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&first).unwrap().is_none());
    assert!(cache.get(&second).unwrap().is_some());
    assert!(
        !cache
            .store
            .root()
            .join("receipts")
            .join(".pipeline-cache-crash.tmp")
            .exists()
    );
    assert_eq!(
        std::fs::read_dir(cache.store.root().join("blobs"))
            .unwrap()
            .count(),
        1
    );

    let tiny = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(tiny.path()).unwrap().with_limits(1, 1);
    assert!(
        cache
            .put(
                &test_context("stage", "too-large"),
                "stable",
                "persistent",
                &ReceiptOutputSelection::default(),
                &StageProduct::new("stage", "digest"),
            )
            .unwrap_err()
            .is::<crate::error::StageFailed>()
    );
    assert!(cache.is_empty());
}

/// A `ReasoningResult` whose `graph/reasoning` projection backs a cache handle, so
/// the cache's re-derivation (`parse_reasoning_graph`) reconstructs a faithful
/// verdict-and-provenance result (C7).
fn sample_reasoning_result() -> gmeow_logic::result::ReasoningResult {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };
    // projection_class mirrors the result's `preservation` axis in every real
    // construction; the parser reconstructs it from that axis, so the fixture sets
    // them equal (an inconsistent fixture would test a state no real result holds).
    let mut prov = ResultProvenance::native("contract:cache-test", "http://example.org/world/w");
    prov.projection_class = PreservationClaim::exact();
    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Supported,
        prov,
        ResultPayload::Empty,
    )
}

/// A bundle whose dataset carries a `graph/reasoning` named graph (the projection
/// of [`sample_reasoning_result`]) with a typed Reasoning handle pinned to it.
fn reasoning_bundle() -> PipelineBundle<PipelineHandle> {
    let result = sample_reasoning_result();
    let parsed = gmeow_logic::result_rdf::project_reasoning_dataset(&result).unwrap();
    reasoning_bundle_with_result(result, &parsed)
}

fn reasoning_bundle_with_result(
    result: gmeow_logic::result::ReasoningResult,
    parsed: &purrdf::RdfDataset,
) -> PipelineBundle<PipelineHandle> {
    let graph_iri = gmeow_logic::result_rdf::GRAPH_REASONING;
    let mut b = RdfDatasetBuilder::new();
    let term = RdfTerm::Iri(graph_iri.to_owned());
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(term.clone());
        b.push_owned_quad(&routed);
    }
    let dataset = b.freeze().expect("freeze");
    let mut bundle = PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(graph_iri);
    bundle
        .pin_handle(
            graph_iri,
            PipelineHandle::Reasoning(Arc::new(result)),
            pinned,
        )
        .expect("pin Reasoning handle");
    bundle
}

#[test]
fn cached_reasoning_preserves_full_answers_and_schema() {
    use gmeow_logic::probabilistic::ProbBinding;
    use gmeow_logic::reason::el::InferredAxiom;
    use gmeow_logic::result::ResultPayload;
    use gmeow_logic::result_shape::{ColumnKind, ResultColumn, ResultShape, RowCardinality};

    let asserted = InferredAxiom {
        subject: "https://example.org/a".into(),
        predicate: "http://www.w3.org/2000/01/rdf-schema#subClassOf".into(),
        object: purrdf::TermValue::iri("https://example.org/b"),
        world: "http://example.org/world/w".into(),
        is_edb: true,
        rule_name: None,
        premises: Vec::new(),
        modal_evaluation: None,
    };
    let mut derived = asserted.clone();
    derived.object = purrdf::TermValue::iri("https://example.org/c");
    derived.is_edb = false;
    derived.rule_name = Some("rule:subclass-transitive".into());
    derived.premises.push((
        asserted.subject.clone(),
        asserted.predicate.clone(),
        gmeow_logic::provenance::term_display(&asserted.object),
    ));
    let bindings = BTreeMap::from([("x".into(), "https://example.org/a".into())]);
    let payloads = [
        ResultPayload::Bindings(vec![bindings.clone()]),
        ResultPayload::Marginals(vec![ProbBinding {
            vars: bindings,
            probability: 0.75,
        }]),
        ResultPayload::Inferred(vec![asserted, derived]),
    ];
    for (i, payload) in payloads.into_iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let mut result = sample_reasoning_result();
        result.payload = payload;
        result.row_schema = Some(ResultShape::new(
            vec![ResultColumn::required("x", ColumnKind::Iri)],
            RowCardinality::Contains,
        ));
        let graph = gmeow_logic::result_rdf::project_reasoning_dataset(&result).unwrap();
        let product = StageProduct::from_bundle(
            "stage-reason",
            Arc::new(reasoning_bundle_with_result(result.clone(), &graph)),
        );
        let context = test_context("stage-reason", &format!("full-reasoning-{i}"));
        persist_test_product(&cache, &context, &product);
        let restored = cache.get(&context).unwrap().unwrap();
        let entry = restored
            .product
            .bundle()
            .handle(gmeow_logic::result_rdf::GRAPH_REASONING)
            .unwrap();
        let PipelineHandle::Reasoning(actual) = &entry.payload else {
            panic!("reasoning arm retained")
        };
        assert_eq!(
            actual.as_ref(),
            &result,
            "every native field survives persistence"
        );
        assert_eq!(restored.product.digest, product.digest);
    }
}

#[test]
fn cached_reasoning_refuses_malformed_governed_summary() {
    let result = sample_reasoning_result();
    let graph = gmeow_logic::result_rdf::project_reasoning_dataset(&result).unwrap();
    let string = |value: &str| {
        RdfTerm::Literal(purrdf::RdfLiteral::typed(
            value,
            "http://www.w3.org/2001/XMLSchema#string",
        ))
    };
    let number = RdfTerm::Literal(purrdf::RdfLiteral::typed(
        "99",
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    let cases = [
        ("resultPayloadKind", None, false),
        ("resultPayloadKind", Some(string("unknown")), false),
        ("resultPayloadKind", Some(string("bindings")), true),
        ("resultPayloadCount", None, false),
        ("resultPayloadCount", Some(number.clone()), false),
        ("resultPayloadCount", Some(number), true),
        (
            "resultInput",
            Some(RdfTerm::Iri("https://example.org/unknown-axis".into())),
            false,
        ),
        ("resultContractHash", Some(string("forged-contract")), false),
    ];
    for (local, replacement, keep_original) in cases {
        let predicate = format!("https://blackcatinformatics.ca/logic/{local}");
        let mut builder = RdfDatasetBuilder::new();
        let mut changed = false;
        for mut quad in graph.owned_quads() {
            if quad.predicate == predicate {
                changed = true;
                if keep_original {
                    builder.push_owned_quad(&quad);
                }
                let Some(value) = &replacement else { continue };
                quad.object = value.clone();
            }
            builder.push_owned_quad(&quad);
        }
        assert!(changed, "the mutation must reach its declared field");
        let altered = builder.freeze().unwrap();
        let product = StageProduct::from_bundle(
            "stage-reason",
            Arc::new(reasoning_bundle_with_result(result.clone(), &altered)),
        );
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let error = cache
            .put(
                &test_context("stage-reason", "malformed-summary"),
                "stable",
                "persistent",
                &full_selection(&product),
                &product,
            )
            .expect_err("a graph pin cannot authorize a malformed governed summary");
        assert!(error.message().contains("projection binding"), "{error}");
    }
}

#[test]
fn cached_reasoning_handle_re_derives_the_result() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();

    let original = reasoning_bundle();
    let product = StageProduct::from_bundle("stage-reason", Arc::new(original.clone()));
    let context = test_context("stage-reason", "reasoning-round-trip");
    persist_test_product(&cache, &context, &product);

    let got = cache.get(&context).unwrap().expect("cache hit");
    let recon = got.product.bundle();

    let graph_iri = gmeow_logic::result_rdf::GRAPH_REASONING;
    let entry = recon
        .handle(graph_iri)
        .expect("Reasoning handle re-attached");
    let PipelineHandle::Reasoning(result) = &entry.payload else {
        panic!("the restored handle arm is Reasoning");
    };
    // The verdict-and-provenance result round-trips faithfully (axes + provenance).
    assert_eq!(
        result.as_ref(),
        &sample_reasoning_result(),
        "the cache preserved the complete native Reasoning result"
    );
    // The pin matches the reconstituted backing graph.
    assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
    // The bundle content fold round-trips.
    assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
}

/// A bundle whose dataset carries a `graph/relational-core` named graph (the
/// projection of a lowered Horn program) with a typed RelationalCore handle pinned
/// to it (C8).
fn relational_core_bundle() -> (
    PipelineBundle<PipelineHandle>,
    gmeow_logic_compile::relational_core::RelationalCoreProgram,
) {
    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
    use gmeow_logic_compile::relational_core::{lower_program, project_relational_core};
    use std::sync::Arc;
    let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    let ax = |s: &str, o: &str| {
        LogicAxiom::new(
            s,
            sc,
            gmeow_logic_compile::ir::AtomicTerm::resource(o),
            false,
            ContextualScope::default(),
        )
        .expect("axiom")
    };
    let rule = LogicRule::new(
        ax("?x", "?z"),
        vec![ax("?x", "?y"), ax("?y", "?z")],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(
        vec![ax(
            "https://blackcatinformatics.ca/gmeow/Cat",
            "https://blackcatinformatics.ca/gmeow/Animal",
        )],
        vec![rule],
        vec![],
        None,
    );
    let lowered = lower_program(&program);
    let projection = project_relational_core(&lowered);
    let parsed = parse_dataset(projection.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let graph_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
    let mut b = RdfDatasetBuilder::new();
    let term = RdfTerm::Iri(graph_iri.to_owned());
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(term.clone());
        b.push_owned_quad(&routed);
    }
    let dataset = b.freeze().expect("freeze");
    let mut bundle = PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(graph_iri);
    bundle
        .pin_handle(
            graph_iri,
            PipelineHandle::RelationalCore(Arc::new(lowered.clone())),
            pinned,
        )
        .expect("pin RelationalCore handle");
    (bundle, lowered)
}

#[test]
fn cached_relational_core_refuses_changed_body_order_even_when_semantically_equal() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let (mut bundle, mut program) = relational_core_bundle();
    let semantic_key = program.content_key().unwrap();
    program.rules[0].body.reverse();
    assert_eq!(program.content_key().unwrap(), semantic_key);
    let graph_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
    let digest = bundle.graph_digest(graph_iri);
    bundle
        .pin_handle(
            graph_iri,
            PipelineHandle::RelationalCore(Arc::new(program)),
            digest,
        )
        .unwrap();
    let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle));
    let context = test_context("stage-compile-logic", "changed-relational-body-order");
    let error = cache
        .put(
            &context,
            "stable",
            "persistent",
            &full_selection(&product),
            &product,
        )
        .expect_err("cache binding must preserve recorded execution order");
    assert!(error.is::<crate::error::Decode>(), "{error}");
    assert!(cache.get(&context).unwrap().is_none());
}

#[test]
fn cached_relational_core_handle_re_derives_the_dialect() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();

    let (original, lowered) = relational_core_bundle();
    let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
    let context = test_context("stage-compile-logic", "relational-round-trip");
    persist_test_product(&cache, &context, &product);

    let got = cache.get(&context).unwrap().expect("cache hit");
    let recon = got.product.bundle();

    let graph_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
    let entry = recon
        .handle(graph_iri)
        .expect("RelationalCore handle re-attached");
    let PipelineHandle::RelationalCore(program) = &entry.payload else {
        panic!("the restored handle arm is RelationalCore");
    };
    // The typed dialect round-trips faithfully (content-key-equal).
    assert_eq!(
        program.content_key().unwrap(),
        lowered.content_key().unwrap(),
        "the cache preserved the complete native RelationalCore program"
    );
    // The pin matches the reconstituted backing graph.
    assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
    // The bundle content fold round-trips.
    assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
}

/// A bundle whose dataset carries a `graph/correspondence` named graph (the §14
/// affine-triangle worked example) with a typed Correspondence handle pinned to it
/// (C10).
fn correspondence_bundle() -> (
    PipelineBundle<PipelineHandle>,
    gmeow_logic_compile::projections::correspondence::CorrespondenceProgram,
) {
    use gmeow_logic_compile::projections::correspondence::project_correspondence;
    use std::sync::Arc;
    let program = crate::stages::compile_logic::synthetic_affine_program();
    let projection = project_correspondence(&program);
    let parsed = parse_dataset(projection.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let graph_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
    let mut b = RdfDatasetBuilder::new();
    let term = RdfTerm::Iri(graph_iri.to_owned());
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(term.clone());
        b.push_owned_quad(&routed);
    }
    let dataset = b.freeze().expect("freeze");
    let mut bundle = PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(graph_iri);
    bundle
        .pin_handle(
            graph_iri,
            PipelineHandle::Correspondence(Arc::new(program.clone())),
            pinned,
        )
        .expect("pin Correspondence handle");
    (bundle, program)
}

#[test]
fn cached_correspondence_refuses_to_publish_a_changed_standpoint() {
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let (mut bundle, mut program) = correspondence_bundle();
    let graph_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
    let backing_digest = bundle.graph_digest(graph_iri);
    program.correspondences[0].according_to = Some("https://example.org/other-observer".into());
    bundle
        .pin_handle(
            graph_iri,
            PipelineHandle::Correspondence(Arc::new(program)),
            backing_digest,
        )
        .expect("graph bytes still match; the cache must also admit the semantic payload");
    let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle));
    let context = test_context("stage-compile-logic", "changed-correspondence-standpoint");
    let error = cache
        .put(
            &context,
            "stable",
            "persistent",
            &full_selection(&product),
            &product,
        )
        .expect_err("a different standpoint must be refused before cache publication");
    assert!(error.is::<crate::error::Decode>(), "{error}");
    assert!(
        cache.get(&context).unwrap().is_none(),
        "no invalid action receipt may be published"
    );
}

/// Complete correspondence payloads and their governed graph pins survive
/// native cache hydration without repeating the projection reader.
#[test]
fn cached_correspondence_handle_re_derives_the_program() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();

    let (original, program) = correspondence_bundle();
    let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
    let context = test_context("stage-compile-logic", "correspondence-round-trip");
    persist_test_product(&cache, &context, &product);

    let got = cache.get(&context).unwrap().expect("cache hit");
    let recon = got.product.bundle();

    let graph_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
    let entry = recon
        .handle(graph_iri)
        .expect("Correspondence handle re-attached");
    let PipelineHandle::Correspondence(re_derived) = &entry.payload else {
        panic!("the restored handle arm is Correspondence");
    };
    assert_eq!(
        re_derived.content_key(),
        program.content_key(),
        "the cache preserved the complete native Correspondence program"
    );
    assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
    assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
}
