// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_foundational_bridging.py
//!
//! The gUFO↔BFO foundational-spine bridge aligns gUFO's *nature* categories to
//! BFO 2020 by reference — never by import. Five checks are migrated here:
//!   - the expected skos:closeMatch cells are present in the foundational SSSOM set;
//!   - every foundational (gufo→bfo) row is a skos:closeMatch, exactly the expected count;
//!   - every emitted bfo: IRI is a real owl:Class in the vendored snapshot, with the
//!     stated label (Principle 7: verify, don't assume);
//!   - the bridge is link-only — no BFO class leaks into the merged closure;
//!   - coverage is non-trivial (the grounded gUFO natures are all mapped);
//!   - BFO is registered as an import-OK upper ontology (the alignment-target
//!     registry kind plus the license-policy classifier).
//!
//! One check remains in Python (a `network`-marked live-BFO fetch — see the
//! migration manifest's single Pending row):
//!   - `test_vendored_snapshot_matches_live_bfo`: fetches the LIVE BFO ontology
//!     over the network and re-verifies each referenced IRI is still an owl:Class
//!     with the same label. Off the default gate (network); the repo's live-network
//!     lane is pytest-only, so it has no on-gate native home.

mod conformance_support;
use conformance_support::*;

const BFO: &str = "http://purl.obolibrary.org/obo/";
const BFO_CLASS_PREFIX: &str = "http://purl.obolibrary.org/obo/BFO_";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The expected gUFO-nature → BFO closeMatch cells `(gufo local, bfo local, label)`.
const EXPECTED_CELLS: [(&str, &str, &str); 7] = [
    ("Endurant", "BFO_0000002", "continuant"),
    ("Object", "BFO_0000040", "material entity"),
    ("FunctionalComplex", "BFO_0000030", "object"),
    ("Collection", "BFO_0000027", "object aggregate"),
    (
        "Relator",
        "BFO_0000020",
        "specifically dependent continuant",
    ),
    ("Quality", "BFO_0000019", "quality"),
    ("Event", "BFO_0000003", "occurrent"),
];

/// The `(subject_id, predicate_id, object_id, object_label)` CURIE rows of the
/// foundational mapping set (`generated/mappings/gmeow-foundational.sssom.tsv`),
/// skipping `#`-prefixed metadata lines and the TSV header.
fn foundational_sssom_rows() -> Vec<(String, String, String, String)> {
    let path = repo_root()
        .join("generated")
        .join("mappings")
        .join("gmeow-foundational.sssom.tsv");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("subject_id") {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 4 {
            rows.push((
                cols[0].to_owned(),
                cols[1].to_owned(),
                cols[2].to_owned(),
                cols[3].to_owned(),
            ));
        }
    }
    rows
}

/// Every gUFO→BFO row (subject in `gufo:`, object in `bfo:`). Mirrors
/// `_foundational_mappings()`.
fn foundational_mappings() -> Vec<(String, String, String, String)> {
    foundational_sssom_rows()
        .into_iter()
        .filter(|(s, _p, o, _l)| s.starts_with("gufo:") && o.starts_with("bfo:"))
        .collect()
}

/// Twin of `test_expected_cells_present_in_alignment_graph`: each expected bridge
/// cell is present in the foundational alignment set.
#[test]
fn expected_cells_present_in_alignment_graph() {
    let rows = foundational_sssom_rows();
    for (gufo_local, bfo_local, _label) in EXPECTED_CELLS {
        let subject = format!("gufo:{gufo_local}");
        let object = format!("bfo:{bfo_local}");
        assert!(
            rows.iter()
                .any(|(s, p, o, _l)| *s == subject && p == "skos:closeMatch" && *o == object),
            "missing bridge cell {gufo_local} → {bfo_local}"
        );
    }
}

/// Twin of `test_bridge_uses_closematch_only`: every foundational row is a fuzzy
/// skos:closeMatch (UFO and BFO build on different bases, so no cell may claim exact
/// equivalence), and the count matches the expected cell set.
#[test]
fn bridge_uses_closematch_only() {
    let mappings = foundational_mappings();
    for (s, p, o, _l) in &mappings {
        assert_eq!(
            p, "skos:closeMatch",
            "{s} → {o} uses {p}; foundational-spine cells must be skos:closeMatch"
        );
    }
    assert_eq!(
        mappings.len(),
        EXPECTED_CELLS.len(),
        "expected exactly {} foundational (gufo→bfo) rows",
        EXPECTED_CELLS.len()
    );
}

/// Twin of `test_every_bfo_iri_is_a_real_class_in_the_snapshot` (Principle 7): each
/// emitted BFO IRI is a declared owl:Class in the vendored snapshot, with the
/// stated label.
#[test]
fn every_bfo_iri_is_a_real_class_in_the_snapshot() {
    let snapshot = GraphStore::parse_ttl_file(&repo_root().join("imports/targets/bfo.ttl"));
    for (_gufo_local, bfo_local, label) in EXPECTED_CELLS {
        let iri = format!("{BFO}{bfo_local}");
        assert!(
            snapshot.has(Some(&iri), Some(RDF_TYPE), Some(OWL_CLASS)),
            "bfo:{bfo_local} is not a declared owl:Class in the BFO snapshot"
        );
        let labels = snapshot.objects_lex(&iri, RDFS_LABEL);
        assert!(
            labels.contains(label),
            "bfo:{bfo_local} label {label:?} does not match BFO's own labels {labels:?}"
        );
    }
}

/// Twin of `test_bridge_is_link_only_no_import` (Principle 5): no BFO class enters
/// the merged closure — the bridge is by reference only.
#[test]
fn bridge_is_link_only_no_import() {
    let g = GraphStore::ontology();
    let leaked: Vec<String> = g
        .subjects_of_type(OWL_CLASS)
        .into_iter()
        .filter(|s| s.starts_with(BFO_CLASS_PREFIX))
        .collect();
    assert!(
        leaked.is_empty(),
        "BFO classes leaked into the reasoned graph: {:?} — the foundational bridge must stay link-only",
        leaked.iter().take(3).collect::<Vec<_>>()
    );
}

/// Twin of `test_coverage_reported`: coverage is non-trivial — the gUFO nature
/// categories GMEOW actually grounds classes in are all mapped to a BFO cell.
#[test]
fn coverage_reported() {
    let mapped: std::collections::BTreeSet<String> = foundational_mappings()
        .iter()
        .map(|(s, _p, _o, _l)| s.trim_start_matches("gufo:").to_owned())
        .collect();
    for nature in ["Endurant", "Object", "Event", "Relator", "Quality"] {
        assert!(
            mapped.contains(nature),
            "grounded gUFO nature {nature} must carry a BFO cell; mapped = {mapped:?}"
        );
    }
}

/// Twin of `test_bfo_is_import_ok_upper_ontology`: BFO is registered as an
/// upper-ontology alignment target whose license clears it for import. The Python
/// read `ALIGNMENT_TARGETS["bfo"].(kind, policy)`; the native surfaces are the
/// deposit alignment-target registry (kind) and the RUST-FIRST license classifier
/// (policy). BFO is published under CC-BY-4.0, which the classifier clears to
/// `ImportOk`.
#[test]
fn bfo_is_import_ok_upper_ontology() {
    let bfo = gmeow_validate::self_desc::deposit_config::ALIGNMENT_TARGETS
        .iter()
        .find(|(key, _, _, _)| *key == "bfo")
        .expect("bfo must be a registered alignment target");
    assert_eq!(bfo.3, "upper", "BFO must be registered with kind=upper");
    assert_eq!(
        gmeow_license::policy_for_license("CC-BY-4.0"),
        gmeow_license::LicensePolicy::ImportOk,
        "BFO's CC-BY-4.0 license must classify as import-OK"
    );
}
