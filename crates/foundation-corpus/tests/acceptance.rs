// SPDX-License-Identifier: AGPL-3.0-only

//! Rust acceptance-test spine for the foundation-corpus importer (#944).
//!
//! This is the parity proof: it ports the acceptance assertions the (now
//! retired) Python `tests/test_foundation_import.py` pinned, so the Rust
//! importer is held to the SAME graph-shape contract.
//!
//! The six byte-exact projections and the budget report are covered by
//! `tests/golden.rs`; this file proves the GRAPH meaning:
//!   1. the imported graph conforms to the whole-ontology SHACL shapes,
//!   2. exactly one Assessment carries a score of 0.0 (zeros are scores),
//!   3. the unknown role mints an open-vocabulary NarrativeRole value,
//!   4. the cross-children competency-demo SPARQL answers in discourse order,
//!   5. privacy: the fixture is synthetic by construction.

mod shacl_support;

use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_foundation_corpus::run_import;
use shacl_support::{ok, ttl_str_to_nt, validate_with_ontology, violations};

const CORP: &str = "https://blackcatinformatics.ca/gmeow/corpus/foundation/";

/// Run the importer against the synthetic fixture and return the written
/// `foundation.ttl` text. Reading the on-disk artifact exercises the real
/// serialization path (not just the in-memory dataset).
fn imported_ttl() -> (tempfile::TempDir, String) {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = crate_dir.join("tests/fixtures/synthetic-corpus.jsonl");
    let tmp = tempfile::tempdir().expect("tempdir");
    run_import(&fixture, tmp.path(), None).expect("run_import");
    let ttl = std::fs::read_to_string(tmp.path().join("foundation.ttl")).expect("foundation.ttl");
    (tmp, ttl)
}

/// Load the imported `foundation.ttl` into an oxigraph store (lenient parser).
fn imported_store() -> (tempfile::TempDir, Store) {
    let (tmp, ttl) = imported_ttl();
    let store = Store::new().expect("store");
    store
        .load_from_reader(
            RdfParser::from_format(RdfFormat::Turtle).lenient(),
            ttl.as_bytes(),
        )
        .expect("parse foundation.ttl");
    (tmp, store)
}

/// Evaluate a SPARQL query against `store` (non-deprecated `SparqlEvaluator`).
fn query<'a>(store: &'a Store, sparql: &str) -> QueryResults<'a> {
    SparqlEvaluator::new()
        .parse_query(sparql)
        .unwrap_or_else(|e| panic!("parse SPARQL: {e}\n{sparql}"))
        .on_store(store)
        .execute()
        .unwrap_or_else(|e| panic!("execute SPARQL: {e}"))
}

// --------------------------------------------------------------------------- //
// 1. The imported graph conforms to the closed-world shapes.
// --------------------------------------------------------------------------- //

#[test]
fn imported_graph_conforms_to_shapes() {
    let (_tmp, ttl) = imported_ttl();
    let fixture_nt = ttl_str_to_nt(&ttl);
    let report = validate_with_ontology(&fixture_nt);
    assert!(ok(&report), "SHACL violations: {:?}", violations(&report));
}

// --------------------------------------------------------------------------- //
// 2. Zeros are scores: exactly ONE Assessment has score value 0.0.
// --------------------------------------------------------------------------- //

#[test]
fn zeros_are_scores() {
    let (_tmp, store) = imported_store();
    let sparql = r#"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
        SELECT (COUNT(?a) AS ?n) WHERE {
            ?a a gmeow:Assessment ;
               gmeow:assessmentScoreValue ?v .
            FILTER(?v = "0"^^xsd:decimal)
        }
    "#;
    let QueryResults::Solutions(solutions) = query(&store, sparql) else {
        panic!("expected SELECT solutions");
    };
    let mut count: Option<i64> = None;
    for sol in solutions {
        let sol = sol.expect("solution");
        let term = sol.get("n").expect("?n bound");
        if let oxigraph::model::Term::Literal(lit) = term {
            count = Some(lit.value().parse().expect("integer count"));
        }
    }
    assert_eq!(count, Some(1), "exactly one Assessment must score 0.0");
}

// --------------------------------------------------------------------------- //
// 3. Unknown role mints an open-vocabulary NarrativeRole value.
// --------------------------------------------------------------------------- //

#[test]
fn unknown_role_mints_open_vocabulary_value() {
    let (_tmp, store) = imported_store();
    let minted = format!("{CORP}role/apprentice-sage");

    // The minted IRI is rdf:type gmeow:NarrativeRole.
    let type_query = format!(
        r#"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        ASK {{ <{minted}> a gmeow:NarrativeRole }}
        "#
    );
    let QueryResults::Boolean(is_role) = query(&store, &type_query) else {
        panic!("expected ASK boolean");
    };
    assert!(is_role, "{minted} must be a gmeow:NarrativeRole");

    // Exactly ONE subject has gmeow:narrativeRoleValue pointing to it.
    let count_query = format!(
        r#"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT (COUNT(?s) AS ?n) WHERE {{ ?s gmeow:narrativeRoleValue <{minted}> }}
        "#
    );
    let QueryResults::Solutions(solutions) = query(&store, &count_query) else {
        panic!("expected SELECT solutions");
    };
    let mut count: Option<i64> = None;
    for sol in solutions {
        let sol = sol.expect("solution");
        if let Some(oxigraph::model::Term::Literal(lit)) = sol.get("n") {
            count = Some(lit.value().parse().expect("integer count"));
        }
    }
    assert_eq!(
        count,
        Some(1),
        "exactly one claim must reference the minted role"
    );
}

// --------------------------------------------------------------------------- //
// 4. Cross-children competency demo — trajectory in discourse order.
// --------------------------------------------------------------------------- //

#[test]
fn competency_demo_trajectory_against_exemplified_principia() {
    let (_tmp, store) = imported_store();
    // Identical to the Python query (tests/test_foundation_import.py L107-124),
    // ORDER BY ?ordinal preserved.
    let sparql = r#"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        PREFIX rdfs:  <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?ordinal ?stateLabel ?criterionLabel WHERE {
            ?sample a gmeow:ArcSample ;
                gmeow:sampleSubject ?who ;
                gmeow:samplePosition ?pos ;
                gmeow:sampleState ?state .
            ?pos gmeow:positionOrdinal ?ordinal .
            ?state rdfs:label ?stateLabel .
            ?exemplar a gmeow:Exemplar ; gmeow:exemplarSubject ?who .
            ?anchor gmeow:anchorExemplar ?exemplar .
            ?criterion gmeow:hasScoreAnchor ?anchor ; rdfs:label ?criterionLabel .
            ?who gmeow:narratedIn ?segment .
            ?segment gmeow:atNarrativePosition ?pos .
        }
        ORDER BY ?ordinal
    "#;
    let QueryResults::Solutions(solutions) = query(&store, sparql) else {
        panic!("expected SELECT solutions");
    };
    let mut rows: Vec<(i64, String, String)> = Vec::new();
    for sol in solutions {
        let sol = sol.expect("solution");
        let ordinal = match sol.get("ordinal").expect("?ordinal") {
            oxigraph::model::Term::Literal(lit) => lit.value().parse::<i64>().expect("ordinal int"),
            other => panic!("unexpected ?ordinal term: {other}"),
        };
        let state = literal_value(sol.get("stateLabel").expect("?stateLabel"));
        let criterion = literal_value(sol.get("criterionLabel").expect("?criterionLabel"));
        rows.push((ordinal, state, criterion));
    }
    assert_eq!(
        rows,
        vec![
            (
                1,
                "Resolute Doubt".to_string(),
                "enforce_test_trust".to_string()
            ),
            (
                2,
                "Hard-won Calm".to_string(),
                "enforce_test_trust".to_string()
            ),
        ]
    );
}

fn literal_value(term: &oxigraph::model::Term) -> String {
    match term {
        oxigraph::model::Term::Literal(lit) => lit.value().to_string(),
        other => panic!("expected literal, got {other}"),
    }
}

// --------------------------------------------------------------------------- //
// 5. Privacy gate: the fixture is synthetic by construction.
// --------------------------------------------------------------------------- //

#[test]
fn real_corpus_never_in_repo() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = crate_dir.join("tests/fixtures/synthetic-corpus.jsonl");
    let text = std::fs::read_to_string(&fixture).expect("fixture text");
    assert!(
        !text.to_lowercase().contains("lillith"),
        "fixture must not reference the real corpus"
    );
    assert!(
        text.contains("Synthetic"),
        "fixture must declare itself synthetic"
    );
}
