// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic allocation gate for the snapshot stratum's canonical digest.
//!
//! The production path must keep one id-native flat union. The reference below is
//! deliberately test-only: it preserves the former two owned-quad expansions and
//! two flat datasets so the gate can prove both byte parity and a strict allocation
//! win without relying on host RSS or wall-clock timing.

use std::sync::Arc;

use gmeow_cost_measure::{CountingAllocator, measure};
use gmeow_pipeline::medium::blake3_digest;
use gmeow_pipeline::stages::carrier::snapshot_stratum_digest;
use purrdf::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral};

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
