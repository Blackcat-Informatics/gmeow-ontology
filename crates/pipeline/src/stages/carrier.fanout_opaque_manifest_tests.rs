// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const EXTRACTS_PATH: &str = "https://blackcatinformatics.ca/gmeow/extractsPath";
const EXTRACTS_FAMILY: &str = "https://blackcatinformatics.ca/gmeow/extractsGraphFamily";
const GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn members() -> BTreeMap<String, Vec<u8>> {
    // A byte-decorated RDF (`.ttl`) member AND non-RDF members — all ride the opaque lane.
    [
        ("generated/n3/gmeow.n3", b"@prefix : <> .".as_slice()),
        ("generated/logic/inferred-closure.rdf12.ttl", b"# closure"),
        ("generated/cl/gmeow.clif", b";; clif"),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_string(), b.to_vec()))
    .collect()
}

#[test]
fn opaque_fanout_manifest_roundtrips_through_read_fanout_rules() {
    let members = members();
    let manifest = build_fanout_opaque_manifest(members.keys()).expect("manifest");

    // The manifest rides in its own meta-level graph (excluded from object-level EDB).
    assert!(!gmeow_logic::reasoning_graphs::is_object_level_named_graph(
        GRAPH_FANOUT_OPAQUE_MANIFEST
    ));
    for q in manifest.owned_quads() {
        assert_eq!(
            q.graph_name.as_ref(),
            Some(&RdfTerm::Iri(GRAPH_FANOUT_OPAQUE_MANIFEST.to_string())),
            "every manifest quad rides the fanout-opaque-manifest graph"
        );
    }

    // The superset gate's own reader must recover one opaque row per member path.
    let rules = crate::stages::superset::read_fanout_rules(manifest.as_ref()).expect("read rules");
    let opaque_paths: std::collections::BTreeSet<String> = rules
        .iter()
        .filter(|r| r.is_opaque())
        .map(|r| r.path().to_string())
        .collect();
    let want: std::collections::BTreeSet<String> = members.keys().cloned().collect();
    assert_eq!(
        opaque_paths, want,
        "one opaque row per opaque member, exactly"
    );
    assert_eq!(rules.len(), members.len(), "no non-opaque rows emitted");
}

#[test]
fn opaque_fanout_manifest_carries_the_assertional_abox_skeleton() {
    // Each row must carry the type + label + graph-provenance + boxABox skeleton the
    // whole-bundle structural lint accepts for a generated assertional individual, plus
    // the opaque family facet — mirroring the constraint-catalog Finding projection.
    let members = members();
    let manifest = build_fanout_opaque_manifest(members.keys()).expect("manifest");
    let has = |s: &str, p: &str, o_iri: Option<&str>, lit_pred: bool| {
        manifest.owned_quads().any(|q| {
            let subj = matches!(&q.subject, RdfTerm::Iri(i) if i == s);
            let pred = q.predicate == p;
            let obj = match (&q.object, o_iri, lit_pred) {
                (RdfTerm::Iri(i), Some(o), _) => i == o,
                (RdfTerm::Literal(_), None, true) => true,
                _ => false,
            };
            subj && pred && obj
        })
    };
    for path in members.keys() {
        let subject = format!("{FANOUT_OPAQUE_SUBJECT_NS}{path}");
        assert!(
            has(&subject, RDFS_LABEL, None, true),
            "row {path} must carry rdfs:label"
        );
        assert!(
            has(
                &subject,
                RDFS_IS_DEFINED_BY,
                Some(GRAPH_FANOUT_OPAQUE_MANIFEST),
                false
            ),
            "row {path} must be provenanced to the manifest graph (assertional)"
        );
        assert!(
            has(
                &subject,
                GRAPH_BOX_ROLE,
                Some("https://blackcatinformatics.ca/gmeow/boxABox"),
                false
            ),
            "row {path} must declare gmeow:graphBoxRole gmeow:boxABox"
        );
        assert!(
            has(&subject, EXTRACTS_PATH, None, true),
            "row {path} must carry gmeow:extractsPath"
        );
        assert!(
            manifest.owned_quads().any(|q| {
                q.predicate == EXTRACTS_FAMILY
                    && matches!(&q.object, RdfTerm::Literal(l) if l.lexical_form == "opaque")
            }),
            "an opaque family facet must be present"
        );
    }
}

#[test]
fn opaque_fanout_manifest_is_deterministic_regardless_of_key_order() {
    let members = members();
    let a = build_fanout_opaque_manifest(members.keys()).expect("a");
    // Feed the keys in reverse to prove the sorted emission is order-independent.
    let reversed: Vec<&String> = members.keys().rev().collect();
    let b = build_fanout_opaque_manifest(reversed.into_iter()).expect("b");
    assert_eq!(
        purrdf::canonical_flat_nquads(a.as_ref()).unwrap(),
        purrdf::canonical_flat_nquads(b.as_ref()).unwrap(),
        "the opaque manifest is a deterministic function of the member set"
    );
}
