// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One compact authenticated publication shared by both contextual consumers.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use super::super::contextual_results::{ARTIFACT, Node, Observations};

pub(super) const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
pub(super) const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/examples";

/// Load only the exact producer-selected compact action. There is no bundle
/// hydration, authored-source access or producer fallback in this process.
pub(super) fn observations() -> &'static Observations {
    static OBSERVATIONS: OnceLock<Observations> = OnceLock::new();
    OBSERVATIONS.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(&root, ARTIFACT)
            .expect("explicit producer supplies exact contextual observations");
        serde_json::from_slice(&bytes).expect("typed contextual observations")
    })
}

/// Compare the complete object set, detecting extra, missing or mistyped values.
pub(super) fn iri(node: &Node, property: &str, expected: &str) {
    assert_eq!(
        node.properties.get(&format!("{LOGIC}{property}")),
        Some(&BTreeSet::from([format!("<{expected}>")])),
        "{property}: {node:?}"
    );
}

/// Follow a single recorded IRI without parsing any RDF document.
pub(super) fn linked<'a>(observed: &'a Observations, node: &Node, property: &str) -> &'a Node {
    let objects = node
        .properties
        .get(&format!("{LOGIC}{property}"))
        .unwrap_or_else(|| panic!("missing {property}: {node:?}"));
    assert_eq!(objects.len(), 1, "exactly one {property}: {objects:?}");
    let object = objects.first().expect("one object");
    let identity = object
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or_else(|| panic!("{property} must carry an IRI: {object}"));
    observed
        .nodes
        .get(identity)
        .unwrap_or_else(|| panic!("unrecorded {property} node {identity}"))
}

/// Pin the proof's owning derivation and its actual nonempty source citations.
pub(super) fn proof(observed: &Observations, result: &Node, property: &str) {
    let proof = linked(observed, result, property);
    let ids = proof
        .properties
        .get(&format!("{LOGIC}derivationId"))
        .expect("proof derivation identity");
    assert_eq!(ids.len(), 1, "one proof derivation: {proof:?}");
    let citations = proof
        .properties
        .get(&format!("{LOGIC}citesIri"))
        .expect("proof source citations");
    assert!(!citations.is_empty(), "proof citations: {proof:?}");
}
