// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic allocation evidence for the OWL-RL subsumption oracle scan.
//!
//! Dataset construction and one warm-up closure stay outside every measured
//! region. Each sample then runs the production entry point over the same frozen
//! input and reports allocator totals; wall time is deliberately absent.

use gmeow_cost_measure::{CountingAllocator, measure};
use gmeow_logic::entail_oracle::owlrl_subsumptions;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

fn input() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();

    // The scan must reject a substantial non-subsumption closure. Unique named
    // terms ensure predicate resolution happens for real closure rows rather than
    // being optimized away by input deduplication.
    for i in 0..4_096 {
        let quad = RdfQuad::new(
            RdfTerm::iri(format!("https://example.test/subject/{i}")),
            "https://example.test/predicate/edge",
            RdfTerm::iri(format!("https://example.test/object/{i}")),
        );
        builder.push_owned_quad(&quad);
    }

    for class in ["A", "B", "C"] {
        let quad = RdfQuad::new(
            RdfTerm::iri(format!("https://example.test/class/{class}")),
            RDF_TYPE,
            RdfTerm::iri(OWL_CLASS),
        );
        builder.push_owned_quad(&quad);
    }
    for (sub, sup) in [("A", "B"), ("B", "C")] {
        let quad = RdfQuad::new(
            RdfTerm::iri(format!("https://example.test/class/{sub}")),
            RDFS_SUBCLASS_OF,
            RdfTerm::iri(format!("https://example.test/class/{sup}")),
        );
        builder.push_owned_quad(&quad);
    }

    builder.freeze().expect("allocation corpus must freeze")
}

fn assert_result(pairs: &[(String, String)]) {
    let a = "https://example.test/class/A";
    let b = "https://example.test/class/B";
    let c = "https://example.test/class/C";
    assert_eq!(pairs.len(), 3, "only the named class chain is comparable");
    assert!(pairs.iter().any(|(sub, sup)| sub == a && sup == b));
    assert!(pairs.iter().any(|(sub, sup)| sub == b && sup == c));
    assert!(pairs.iter().any(|(sub, sup)| sub == a && sup == c));
}

fn main() {
    let dataset = input();
    assert_result(&owlrl_subsumptions(dataset.as_ref()));

    for sample_index in 1..=5 {
        let (pairs, sample) = measure(|| owlrl_subsumptions(dataset.as_ref()));
        assert_result(&pairs);
        println!(
            "sample={sample_index}\talloc_bytes={}\talloc_count={}\tpeak_live_bytes={}",
            sample.bytes, sample.count, sample.peak_live,
        );
    }
}
