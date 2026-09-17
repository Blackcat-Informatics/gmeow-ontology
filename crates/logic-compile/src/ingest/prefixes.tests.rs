// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn registry_has_unique_prefixes_and_namespaces() {
    let mut prefixes: Vec<&str> = PREFIX_REGISTRY.iter().map(|(p, _)| *p).collect();
    let n = prefixes.len();
    prefixes.sort_unstable();
    prefixes.dedup();
    assert_eq!(prefixes.len(), n, "duplicate prefix in registry");
}

#[test]
fn registry_insertion_order_is_preserved() {
    // Order is load-bearing for CURIE tie-breaks: the first four entries are the
    // GMEOW-local grounding namespaces (gmeow, logic, lang, math), ahead of the
    // standard vocabularies (owl next). The grounding order is logic: < lang: < math:.
    assert_eq!(PREFIX_REGISTRY[0].0, "gmeow");
    assert_eq!(PREFIX_REGISTRY[1].0, "logic");
    assert_eq!(PREFIX_REGISTRY[2].0, "lang");
    assert_eq!(PREFIX_REGISTRY[3].0, "math");
    assert_eq!(PREFIX_REGISTRY[4].0, "owl");
}

#[test]
fn curie_prefers_longest_namespace() {
    // `obi` (…/obo/OBI_) must win over `bfo` (…/obo/) for an OBI IRI — the
    // descending-namespace-length sort is what guarantees the most specific CURIE.
    let table = ns_to_prefix();
    assert_eq!(
        sssom_id("http://purl.obolibrary.org/obo/OBI_0000123", table),
        "obi:0000123"
    );
    assert_eq!(
        sssom_id("http://purl.obolibrary.org/obo/BFO_0000001", table),
        "bfo:BFO_0000001"
    );
    assert_eq!(
        sssom_id("http://purl.obolibrary.org/obo/RO_0002131", table),
        "ro:0002131"
    );
}

#[test]
fn unmatched_iri_is_bare() {
    let table = ns_to_prefix();
    assert_eq!(
        sssom_id("http://unknown.example/x", table),
        "http://unknown.example/x"
    );
}
