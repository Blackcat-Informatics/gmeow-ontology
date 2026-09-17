// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn xml_escape_handles_metacharacters() {
    assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
}

#[test]
fn local_name_takes_tail() {
    assert_eq!(local_name("https://x/y/Foo"), "Foo");
    assert_eq!(local_name("https://x#Bar"), "Bar");
}

fn term_with_neighbours() -> DocTerm {
    DocTerm {
        iri: "https://x/y/Centre".to_string(),
        parents: vec!["https://x/y/Parent".to_string()],
        related_terms: vec!["https://x/y/Related".to_string()],
        // self-references must be filtered out of every flank
        domain: vec!["https://x/y/Centre".to_string()],
        ..Default::default()
    }
}

#[test]
fn term_has_neighbourhood_gates_on_any_non_self_relation() {
    let term = term_with_neighbours();
    assert!(term_has_neighbourhood(&term));
    // A term whose only relation is a self-reference has no neighbourhood.
    let self_only = DocTerm {
        iri: "https://x/y/Centre".to_string(),
        domain: vec!["https://x/y/Centre".to_string()],
        ..Default::default()
    };
    assert!(!term_has_neighbourhood(&self_only));
    assert!(!term_has_neighbourhood(&DocTerm::default()));
}

#[test]
fn term_neighbourhood_svg_is_pure_and_labels_nodes() {
    let term = term_with_neighbours();
    let svg = term_neighbourhood_svg(&term);
    // Centre and both non-self neighbours are present, by local name (the self
    // reference in `domain` is dropped). purrdf renders labels as node text.
    assert!(svg.contains("Centre"));
    assert!(svg.contains("Parent"));
    assert!(svg.contains("Related"));
    // Pure: identical bytes across two calls (no per-process hash-seed drift).
    assert_eq!(svg, term_neighbourhood_svg(&term));
}
