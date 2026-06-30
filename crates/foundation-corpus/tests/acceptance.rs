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
use std::sync::Arc;

use gmeow_rdf::parse_dataset;
use gmeow_rdf_core::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue};
use gmeow_sparql_eval::NativeSparqlEngine;

use gmeow_foundation_corpus::run_import;
use shacl_support::{ok, validate_with_ontology, violations};

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

/// Load the imported `foundation.ttl` into a native frozen dataset (canonical codec).
fn imported_store() -> (tempfile::TempDir, Arc<RdfDataset>) {
    let (tmp, ttl) = imported_ttl();
    let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse foundation.ttl");
    (tmp, dataset)
}

/// Evaluate a SPARQL query against `dataset` via the native engine.
fn query(dataset: &Arc<RdfDataset>, sparql: &str) -> SparqlResult {
    NativeSparqlEngine::new()
        .query(
            dataset,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .unwrap_or_else(|e| panic!("execute SPARQL: {e}\n{sparql}"))
}

/// The lexical form of a literal term, panicking on a non-literal.
fn literal_value(term: &TermValue) -> String {
    match term {
        TermValue::Literal { lexical_form, .. } => lexical_form.clone(),
        other => panic!("expected literal, got {other:?}"),
    }
}

// --------------------------------------------------------------------------- //
// 1. The imported graph conforms to the closed-world shapes.
// --------------------------------------------------------------------------- //

#[test]
fn imported_graph_conforms_to_shapes() {
    let (_tmp, ttl) = imported_ttl();
    let report = validate_with_ontology(&ttl);
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
    let SparqlResult::Solutions {
        variables, rows, ..
    } = query(&store, sparql)
    else {
        panic!("expected SELECT solutions");
    };
    let n_idx = variables
        .iter()
        .position(|v| v == "n")
        .expect("?n projected");
    let mut count: Option<i64> = None;
    for row in &rows {
        if let Some(TermValue::Literal { lexical_form, .. }) = &row[n_idx] {
            count = Some(lexical_form.parse().expect("integer count"));
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
    let SparqlResult::Boolean(is_role) = query(&store, &type_query) else {
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
    let SparqlResult::Solutions {
        variables, rows, ..
    } = query(&store, &count_query)
    else {
        panic!("expected SELECT solutions");
    };
    let n_idx = variables
        .iter()
        .position(|v| v == "n")
        .expect("?n projected");
    let mut count: Option<i64> = None;
    for row in &rows {
        if let Some(TermValue::Literal { lexical_form, .. }) = &row[n_idx] {
            count = Some(lexical_form.parse().expect("integer count"));
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
    let SparqlResult::Solutions {
        variables,
        rows: solutions,
        ..
    } = query(&store, sparql)
    else {
        panic!("expected SELECT solutions");
    };
    let idx = |name: &str| {
        variables
            .iter()
            .position(|v| v == name)
            .unwrap_or_else(|| panic!("?{name} projected"))
    };
    let (ord_i, state_i, crit_i) = (idx("ordinal"), idx("stateLabel"), idx("criterionLabel"));
    let mut rows: Vec<(i64, String, String)> = Vec::new();
    for sol in &solutions {
        let ordinal = match sol[ord_i].as_ref().expect("?ordinal bound") {
            TermValue::Literal { lexical_form, .. } => {
                lexical_form.parse::<i64>().expect("ordinal int")
            }
            other => panic!("unexpected ?ordinal term: {other:?}"),
        };
        let state = literal_value(sol[state_i].as_ref().expect("?stateLabel bound"));
        let criterion = literal_value(sol[crit_i].as_ref().expect("?criterionLabel bound"));
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
