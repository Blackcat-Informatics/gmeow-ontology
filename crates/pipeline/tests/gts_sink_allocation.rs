// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic allocation gate for the snapshot stratum's canonical digest.
//!
//! The production path must keep one id-native flat union. The reference below is
//! deliberately test-only: it preserves the former two owned-quad expansions and
//! two flat datasets so the gate can prove both byte parity and a strict allocation
//! win without relying on host RSS or wall-clock timing.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_cost_measure::{CountingAllocator, measure};
use gmeow_pipeline::medium::envelope::{
    DigestStratum, FrameDigestFacts, FrameFacts, MediumEnvelope, seal, seal_digests,
};
use gmeow_pipeline::medium::registry::{MediumRegistry, MediumSelection};
use gmeow_pipeline::medium::{SNAPSHOT_WIRE_REP, blake3_digest};
use gmeow_pipeline::stages::carrier::{snapshot_stratum_digest, with_owl_rdfs_projection};
use gmeow_pipeline::stages::medium_dictionaries::{frame_iri, seal_bundle_envelopes};
use purrdf::gts_compose::{BlobRow, DictSelection as WireDictSelection, FrameSlot, MediumPlan};
use purrdf::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, parse_dataset};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

const ROWS_PER_SOURCE: usize = 4_096;

fn synthetic_source(seed: usize) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let predicate = builder.intern_iri("https://example.org/predicate");
    let annotation_predicate = builder.intern_iri("https://example.org/confidence");
    let graph = builder.intern_iri(&format!("https://example.org/graph/{seed}"));

    for row in 0..ROWS_PER_SOURCE {
        let subject = if row % 257 == 0 {
            builder.intern_blank("shared", BlankScope((row % 3) as u32))
        } else {
            builder.intern_iri(&format!("https://example.org/{seed}/subject/{row:05}"))
        };
        let object = builder.intern_literal(RdfLiteral::simple(format!(
            "source={seed};row={row:05};payload=the-canonical-stratum-keeps-this-lexical-value"
        )));
        builder.push_quad(subject, predicate, object, (row % 11 == 0).then_some(graph));

        if row % 997 == 0 {
            let triple = builder.intern_triple(subject, predicate, object);
            let reifier =
                builder.intern_iri(&format!("https://example.org/{seed}/reifier/{row:05}"));
            builder.push_reifier(reifier, triple);
            let confidence = builder.intern_literal(RdfLiteral::simple("0.99"));
            builder.push_annotation(reifier, annotation_predicate, confidence);
        }
    }

    builder.freeze().expect("the synthetic source is valid")
}

fn legacy_stratum_digest(
    carrier: &RdfDataset,
    extra_graphs: &[Arc<RdfDataset>],
) -> (String, usize) {
    let mut sources = vec![purrdf::flat_rdf_quads_from_dataset(carrier)];
    for graph in extra_graphs {
        sources.push(purrdf::flat_rdf_quads_from_dataset(graph));
    }
    let borrowed: Vec<&[purrdf::RdfQuad]> = sources.iter().map(Vec::as_slice).collect();
    let union = purrdf::flat_dataset_from_quad_sources(&borrowed).expect("legacy union freezes");
    let canonical = purrdf::canonical_flat_nquads(&union).expect("legacy union canonicalizes");
    drop(union);
    drop(borrowed);
    drop(sources);
    let len = canonical.len();
    let digest = blake3_digest(canonical.as_bytes());
    drop(canonical);
    (digest, len)
}

#[test]
fn id_native_stratum_is_byte_identical_and_strictly_lowers_allocations() {
    let carrier = synthetic_source(0);
    let extra_graphs = vec![synthetic_source(1)];

    let (candidate, candidate_alloc) = measure(|| {
        snapshot_stratum_digest(&carrier, &extra_graphs).expect("id-native digest succeeds")
    });
    let (legacy, legacy_alloc) = measure(|| legacy_stratum_digest(&carrier, &extra_graphs));

    assert_eq!(
        candidate, legacy,
        "the ownership change must preserve bytes"
    );
    assert!(
        candidate_alloc.bytes < legacy_alloc.bytes,
        "id-native total allocated bytes did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
    assert!(
        candidate_alloc.count < legacy_alloc.count,
        "id-native allocation count did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
    assert!(
        candidate_alloc.peak_live < legacy_alloc.peak_live,
        "id-native peak live bytes did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
}

fn projection_source() -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let rdf_type = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let logic_class = builder.intern_iri("https://blackcatinformatics.ca/logic/Class");
    let logic_subclass = builder.intern_iri("https://blackcatinformatics.ca/logic/subClassOf");
    let logic_thing = builder.intern_iri("https://blackcatinformatics.ca/logic/Thing");
    let graph = builder.intern_iri("https://example.org/graph/projection");
    let annotation_predicate = builder.intern_iri("https://example.org/confidence");
    for row in 0..ROWS_PER_SOURCE {
        let subject = builder.intern_iri(&format!("https://example.org/class/{row:05}"));
        builder.push_quad(subject, rdf_type, logic_class, None);
        builder.push_quad(subject, logic_subclass, logic_thing, Some(graph));
        if row == 0 {
            let triple = builder.intern_triple(subject, logic_subclass, logic_thing);
            let reifier = builder.intern_iri("https://example.org/reifier/projection");
            let confidence = builder.intern_literal(RdfLiteral::simple("0.99"));
            builder.push_reifier_in_graph(reifier, triple, Some(graph));
            builder.push_annotation_in_graph(
                reifier,
                annotation_predicate,
                confidence,
                Some(graph),
            );
        }
    }
    builder.freeze().expect("the projection source is valid")
}

fn legacy_with_owl_rdfs_projection(dataset: &RdfDataset) -> Arc<RdfDataset> {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let mut builder = RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        let predicate = gmeow_ns::owl_view_of_predicate(&quad.predicate);
        let object = match &quad.object {
            purrdf::RdfTerm::Iri(iri) if quad.predicate == RDF_TYPE => {
                gmeow_ns::owl_view_of_type_marker(iri)
            }
            purrdf::RdfTerm::Iri(iri) if gmeow_ns::is_class_position_predicate(&quad.predicate) => {
                match iri.as_str() {
                    gmeow_ns::LOGIC_THING => Some(gmeow_ns::OWL_THING),
                    gmeow_ns::LOGIC_NOTHING => Some(gmeow_ns::OWL_NOTHING),
                    _ => None,
                }
            }
            _ => None,
        };
        for (predicate, object) in [(predicate, object), (predicate, None), (None, object)] {
            if predicate.is_none() && object.is_none() {
                continue;
            }
            let mut projected = quad.clone();
            if let Some(predicate) = predicate {
                projected.predicate = predicate.to_owned();
            }
            if let Some(object) = object {
                projected.object = purrdf::RdfTerm::iri(object);
            }
            builder.push_owned_quad(&projected);
        }
        builder.push_owned_quad(&quad);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder
        .freeze()
        .expect("the legacy OWL/RDFS projection freezes")
}

#[test]
fn id_native_owl_projection_is_byte_identical_and_strictly_lowers_allocations() {
    let source = projection_source();
    let (candidate, candidate_alloc) = measure(|| with_owl_rdfs_projection(source.as_ref()));
    let (legacy, legacy_alloc) = measure(|| legacy_with_owl_rdfs_projection(source.as_ref()));

    let candidate_nquads =
        purrdf::canonical_flat_nquads(candidate.as_ref()).expect("candidate canonicalizes");
    let legacy_nquads =
        purrdf::canonical_flat_nquads(legacy.as_ref()).expect("legacy canonicalizes");
    assert_eq!(
        candidate_nquads, legacy_nquads,
        "the id-native projection must preserve the exact projected carrier"
    );
    assert!(
        candidate_alloc.bytes < legacy_alloc.bytes,
        "id-native projection total allocated bytes did not strictly fall: \
         candidate={candidate_alloc:?}, legacy={legacy_alloc:?}"
    );
    assert!(
        candidate_alloc.count < legacy_alloc.count,
        "id-native projection allocation count did not strictly fall: \
         candidate={candidate_alloc:?}, legacy={legacy_alloc:?}"
    );
    // Both paths return the same immutable dataset, so its retained allocation is
    // the irreducible peak-live floor. The id-native path must reach that floor
    // without exceeding the legacy path while strictly lowering bytes and calls.
    assert!(
        candidate_alloc.peak_live <= legacy_alloc.peak_live,
        "id-native projection peak live bytes increased: \
         candidate={candidate_alloc:?}, legacy={legacy_alloc:?}"
    );
}

fn envelope_registry() -> MediumRegistry {
    let source = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:corpusTrainingSplitV1 a gmeow:CorpusTrainingSplit ;
    gmeow:splitHeldOutStride 8 ; gmeow:splitHeldOutOffset 0 .
gmeow:corpusCore a gmeow:DictionaryCorpus ;
    gmeow:corpusSelectsBlobRep "cells-archive" .
gmeow:dictCore a gmeow:CompressionDictionary ;
    gmeow:dictionaryId "gmeow-core-v1" ; gmeow:dictionaryVersion "1" ;
    gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;
    gmeow:dictionaryTargetLength 4096 ; gmeow:trainsOverCorpus gmeow:corpusCore .
gmeow:payloadSchemaCells a gmeow:PayloadSchema ;
    gmeow:payloadSchemaId "cells-archive" ;
    gmeow:payloadSchemaMedium gmeow:mediumDist ;
    gmeow:payloadSchemaDictionary gmeow:dictCore .
gmeow:payloadSchemaSnapshot a gmeow:PayloadSchema ;
    gmeow:payloadSchemaId "gmeow:snapshot/wire" ;
    gmeow:payloadSchemaMedium gmeow:mediumBaseline .
gmeow:mediumDist a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ; gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourcePerRep ;
    gmeow:requiresReaderCapability "zstd-dictionary", "zstd-rsyncable" ;
    gmeow:mediumDictionary gmeow:dictCore .
gmeow:mediumBaseline a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ; gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourceWholeArtifact ;
    gmeow:requiresReaderCapability "zstd-rsyncable" .
"#;
    let dataset = parse_dataset(source.as_bytes(), "text/turtle", None)
        .expect("the focused medium registry parses");
    MediumRegistry::from_dataset(dataset.as_ref()).expect("the focused medium registry is valid")
}

fn baseline_plan() -> MediumPlan {
    MediumPlan {
        dicts: Vec::new(),
        assignment: BTreeMap::from([
            (
                FrameSlot::Blob("cells-archive".to_string()),
                WireDictSelection::Baseline,
            ),
            (FrameSlot::Snapshot, WireDictSelection::Baseline),
        ]),
        zstd_level: Some(12),
    }
}

fn legacy_bundle_envelopes(
    registry: &MediumRegistry,
    selection: &MediumSelection,
    plan: &MediumPlan,
    blobs: &[&BlobRow],
    snapshot_content_digest: &str,
    snapshot_strata_digest: &str,
) -> Vec<MediumEnvelope> {
    let dictionary_of = |slot: &FrameSlot| match plan.assignment.get(slot) {
        Some(WireDictSelection::Named(id)) => Some(id.as_str()),
        Some(WireDictSelection::Baseline) | None => None,
    };
    let mut envelopes = Vec::with_capacity(blobs.len() + 1);
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for blob in blobs {
        let digest = blake3_digest(&blob.data);
        let frame = frame_iri(&blob.rep, &digest);
        assert!(
            seen.insert(frame.clone(), blob.rep.clone()).is_none(),
            "the focused legacy population has unique frame identities"
        );
        envelopes.push(
            seal(
                registry,
                selection,
                &FrameFacts {
                    frame: &frame,
                    rep: &blob.rep,
                    payload: &blob.data,
                    stratum_bytes: &blob.data,
                    stratum: DigestStratum::WholePayload,
                    dictionary_id: dictionary_of(&FrameSlot::Blob(blob.rep.clone())),
                },
            )
            .expect("legacy blob seal succeeds"),
        );
    }
    envelopes.push(
        seal_digests(
            registry,
            selection,
            &FrameDigestFacts {
                frame: &frame_iri(SNAPSHOT_WIRE_REP, snapshot_content_digest),
                rep: SNAPSHOT_WIRE_REP,
                content_digest: snapshot_content_digest,
                strata_digest: snapshot_strata_digest,
                stratum: DigestStratum::PayloadExcludingMediumEnvelope,
                dictionary_id: dictionary_of(&FrameSlot::Snapshot),
            },
        )
        .expect("legacy snapshot seal succeeds"),
    );
    envelopes
}

#[test]
fn prehashed_blob_sealing_is_identical_and_strictly_lowers_allocations() {
    let registry = envelope_registry();
    let selection =
        MediumSelection::Uniform("https://blackcatinformatics.ca/gmeow/mediumBaseline".to_string());
    let plan = baseline_plan();
    let blob = BlobRow {
        data: vec![b'x'; 4 * 1024 * 1024],
        media_type: "application/x-tar".to_string(),
        rep: "cells-archive".to_string(),
    };
    let blobs = [&blob];
    let snapshot_content = blake3_digest(b"snapshot-content");
    let snapshot_strata = blake3_digest(b"snapshot-strata");

    let (candidate, candidate_alloc) = measure(|| {
        seal_bundle_envelopes(
            &registry,
            &selection,
            &plan,
            &blobs,
            &snapshot_content,
            &snapshot_strata,
        )
        .expect("prehashed bundle seal succeeds")
    });
    let (legacy, legacy_alloc) = measure(|| {
        legacy_bundle_envelopes(
            &registry,
            &selection,
            &plan,
            &blobs,
            &snapshot_content,
            &snapshot_strata,
        )
    });

    assert_eq!(candidate, legacy, "prehashing must preserve envelope facts");
    assert!(
        candidate_alloc.bytes < legacy_alloc.bytes,
        "prehashed total bytes did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
    assert!(
        candidate_alloc.count < legacy_alloc.count,
        "prehashed allocation count did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
    assert!(
        candidate_alloc.peak_live < legacy_alloc.peak_live,
        "prehashed peak live bytes did not strictly fall: candidate={candidate_alloc:?}, \
         legacy={legacy_alloc:?}"
    );
}
