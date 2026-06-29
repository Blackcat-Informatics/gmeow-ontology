// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Coverage graph-analysis over the vendored entity-slice fixtures (#579).
//!
//! PyO3-free. Mirrors `gmeow_tools.coverage.analyze` EXACTLY: it loads the merged
//! fixture graphs into one oxigraph [`Store`], collects every distinct class
//! (`rdf:type` object) and predicate IRI, then classifies each as *covered* or a
//! *gap*. The SSSOM-derived `aligned` set (every external IRI GMEOW links to) is
//! computed in Python and passed in — the TSV parsing stays on the Python side of
//! the seam.
//!
//! The classification rule is byte-for-byte the Python one:
//!
//! * **ignored** — the IRI sits in the RDF / RDFS / OWL / XSD plumbing namespaces;
//!   it is neither covered nor a gap.
//! * **covered** — the IRI is in the GMEOW namespace, in a *recommended* namespace
//!   GMEOW reuses wholesale (SKOS), or in the `aligned` SSSOM set.
//! * **gap** — used in the slice but not covered.
//!
//! Classes additionally require the IRI to start with `http` (matching the Python
//! `iri.startswith("http")` guard); predicates do not (a predicate is always an
//! IRI, so the guard would be a no-op there and Python omits it).

use std::collections::BTreeSet;
use std::path::PathBuf;

use gmeow_rdf::{DatasetView, GraphMatch, TermRef, TermValue};

use crate::model::rdf;
use crate::store;

/// The four sorted coverage sets, ready to hand back across the FFI boundary.
///
/// Each field is a [`BTreeSet`] so iteration order is the sorted order the Python
/// `CoverageReport` consumers (and the parity golden) expect.
#[derive(Debug, Default, Clone)]
pub struct CoverageSets {
    /// Used classes GMEOW covers.
    pub covered_classes: BTreeSet<String>,
    /// Used classes GMEOW does not yet align to.
    pub gap_classes: BTreeSet<String>,
    /// Used predicates GMEOW covers.
    pub covered_predicates: BTreeSet<String>,
    /// Used predicates GMEOW does not yet align to.
    pub gap_predicates: BTreeSet<String>,
}

/// RDF-plumbing namespaces that are neither covered nor counted as gaps.
///
/// These are the literal prefix strings from `config.PREFIXES` (rdf / rdfs / owl
/// / xsd) — the Python `_IGNORED` tuple.
const IGNORED: [&str; 4] = [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "http://www.w3.org/2000/01/rdf-schema#",
    "http://www.w3.org/2002/07/owl#",
    "http://www.w3.org/2001/XMLSchema#",
];

/// Namespaces GMEOW reuses wholesale (recommended value vocabularies) — the
/// Python `_RECOMMENDED` tuple (SKOS).
const RECOMMENDED: [&str; 1] = ["http://www.w3.org/2004/02/skos/core#"];

/// Mirror of `coverage._is_ignored`.
fn is_ignored(iri: &str) -> bool {
    IGNORED.iter().any(|ns| iri.starts_with(ns))
}

/// Mirror of `coverage._is_covered`.
fn is_covered(iri: &str, aligned: &BTreeSet<String>, namespace: &str) -> bool {
    iri.starts_with(namespace)
        || RECOMMENDED.iter().any(|ns| iri.starts_with(ns))
        || aligned.contains(iri)
}

/// Classify the classes and predicates used across the fixture graphs.
///
/// Mirrors `coverage.analyze` over `coverage.load_fixtures`: builds one merged
/// store from `fixture_paths`, collects the distinct `rdf:type` objects (used
/// classes) and distinct predicates (used predicates), and routes each IRI into
/// the covered / gap / ignored buckets.
///
/// `aligned` is the SSSOM-derived external-IRI set (`coverage.covered_iris()`),
/// computed in Python; `namespace` is `config.NAMESPACE`.
///
/// # Errors
///
/// Returns `Err(message)` if any fixture fails to read or parse.
pub fn coverage_analyze(
    fixture_paths: &[PathBuf],
    aligned: &BTreeSet<String>,
    namespace: &str,
) -> Result<CoverageSets, String> {
    let ds = store::dataset_from_paths(fixture_paths)?;
    let mut sets = CoverageSets::default();

    // Used classes: the distinct named-node objects of rdf:type.
    let mut classes: BTreeSet<String> = BTreeSet::new();
    if let Some(type_id) = ds.term_id_by_value(&TermValue::iri(rdf::TYPE)) {
        for q in ds.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
            if let TermRef::Iri(n) = ds.resolve(q.o) {
                classes.insert(n.to_owned());
            }
        }
    }
    for iri in classes {
        if !iri.starts_with("http") || is_ignored(&iri) {
            continue;
        }
        if is_covered(&iri, aligned, namespace) {
            sets.covered_classes.insert(iri);
        } else {
            sets.gap_classes.insert(iri);
        }
    }

    // Used predicates: every distinct predicate IRI in the merged graph.
    let mut predicates: BTreeSet<String> = BTreeSet::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(p) = ds.resolve(q.p) {
            predicates.insert(p.to_owned());
        }
    }
    for iri in predicates {
        if is_ignored(&iri) {
            continue;
        }
        if is_covered(&iri, aligned, namespace) {
            sets.covered_predicates.insert(iri);
        } else {
            sets.gap_predicates.insert(iri);
        }
    }

    Ok(sets)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn classifies_covered_gap_and_ignored() {
        let path = write_tmp(
            "gmeow_validate_coverage_basic.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:thing a gmeow:Email ;\n\
                 gmeow:addressValue \"x\" ;\n\
                 foaf:homepage ex:hp ;\n\
                 ex:unaligned ex:y ;\n\
                 owl:sameAs ex:z .\n\
             ex:other a foaf:Person, skos:Concept, owl:Class .\n",
        );
        let mut aligned = BTreeSet::new();
        aligned.insert("http://xmlns.com/foaf/0.1/Person".to_owned());
        aligned.insert("http://xmlns.com/foaf/0.1/homepage".to_owned());
        let sets = coverage_analyze(std::slice::from_ref(&path), &aligned, NS).unwrap();
        std::fs::remove_file(&path).ok();

        // Classes: gmeow:Email + foaf:Person covered; skos:Concept covered
        // (recommended); owl:Class ignored; no gap classes here.
        assert!(sets.covered_classes.contains(&format!("{NS}Email")));
        assert!(sets
            .covered_classes
            .contains("http://xmlns.com/foaf/0.1/Person"));
        assert!(sets
            .covered_classes
            .contains("http://www.w3.org/2004/02/skos/core#Concept"));
        assert!(!sets
            .covered_classes
            .iter()
            .any(|c| c.starts_with("http://www.w3.org/2002/07/owl#")));
        assert!(sets.gap_classes.is_empty());

        // Predicates: gmeow:addressValue + foaf:homepage covered; ex:unaligned
        // gap; owl:sameAs + rdf:type ignored.
        assert!(sets
            .covered_predicates
            .contains(&format!("{NS}addressValue")));
        assert!(sets
            .covered_predicates
            .contains("http://xmlns.com/foaf/0.1/homepage"));
        assert!(sets
            .gap_predicates
            .contains("https://example.org/unaligned"));
        assert!(!sets
            .covered_predicates
            .iter()
            .any(|p| p.starts_with("http://www.w3.org/2002/07/owl#")));
        assert!(!sets.gap_predicates.iter().any(|p| p.contains("sameAs")));
    }
}
