// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::Path;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_lpg_artifacts() -> BTreeMap<String, Vec<u8>> {
    crate::fixture::stage_artifacts(&repo_root(), 1, "stage-export-lpg")
        .expect("load authenticated LPG product without rebuilding corpus")
}

#[test]
fn render_from_dataset_emits_expected_package_layout() {
    let arts = authenticated_lpg_artifacts();

    assert!(!arts.is_empty(), "expected a non-empty LPG artifact map");
    assert!(arts.contains_key(&format!("{LPG_DIR}/nodes.csv")));
    assert!(arts.contains_key(&format!("{LPG_DIR}/edges.csv")));
    assert!(
        arts.keys()
            .any(|k| k.starts_with(&format!("{LPG_DIR}/open-cypher/"))),
        "expected an open-cypher/ member"
    );
    assert!(
        arts.keys()
            .any(|k| k.starts_with(&format!("{LPG_DIR}/graphml/"))),
        "expected a graphml/ member"
    );
    assert!(
        arts.keys()
            .any(|k| k.starts_with(&format!("{LPG_DIR}/neo4j/"))),
        "expected a neo4j/ member"
    );
    for key in arts.keys() {
        assert!(
            key.starts_with(&format!("{LPG_DIR}/")),
            "every artifact key must live under {LPG_DIR}/, got {key}"
        );
    }
}

#[test]
fn authenticated_product_is_stable_across_reads() {
    let first = authenticated_lpg_artifacts();
    let second = authenticated_lpg_artifacts();
    assert_eq!(
        first, second,
        "the same authenticated stage receipt must hydrate identical LPG bytes"
    );
}

/// ROUND-TRIP: the generic-CSV projection's `LpgGraph` lifts back
/// (`purrdf::lift_lpg`) to a dataset that is exactly isomorphic to the
/// SCOPED `STATEMENTS_GRAPH` dataset fed into the projection — every LPG
/// label/property/edge carries its exact RDF-1.2 sideband quad, so nothing
/// is lost lifting back. Compared via `purrdf::turtle_normalize::render`
/// (the same canonical-Turtle round-trip idiom the superset gate uses for
/// full-fidelity RDF-star folds), never a tautological self-comparison.
#[test]
fn lpg_lift_round_trips_a_synthetic_scoped_statements_graph() {
    let nq = format!(
        "<https://example.test/alice> <{RDF_TYPE}> <https://example.test/Person> <{STATEMENTS_GRAPH}> .\n\
             <https://example.test/alice> <https://example.test/knows> <https://example.test/bob> <{STATEMENTS_GRAPH}> .\n\
             <https://example.test/bob> <{RDF_TYPE}> <https://example.test/Person> <{STATEMENTS_GRAPH}> .\n"
    );
    let dataset = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .expect("parse the tiny synthetic statements graph");
    let scoped = dataset.project_named_graph(STATEMENTS_GRAPH);
    let config = lpg_config().expect("lpg_config");

    let csv = purrdf::project_lpg_csv(&scoped, &config).expect("project_lpg_csv");
    assert!(!csv.graph.nodes.is_empty(), "expected a non-empty node set");
    assert!(!csv.graph.edges.is_empty(), "expected a non-empty edge set");

    let outcome = purrdf::lift_lpg(&csv.graph, &config).expect("lift_lpg");

    let prefixes = crate::stages::superset::rdf_prefixes();
    let expected = purrdf::turtle_normalize::render(&scoped, &prefixes);
    let actual = purrdf::turtle_normalize::render(&outcome.dataset, &prefixes);
    assert_eq!(
        actual, expected,
        "lift_lpg must reproduce the exact scoped statements-graph dataset"
    );
}
