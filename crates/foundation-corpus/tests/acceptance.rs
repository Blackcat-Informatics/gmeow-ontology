// SPDX-License-Identifier: AGPL-3.0-only

//! Rust acceptance-test spine for the authenticated foundation-corpus golden.
//!
//! This is the parity proof: it ports the acceptance assertions the (now
//! retired) Python `tests/test_foundation_import.py` pinned, so the Rust
//! corpus is held to the SAME graph-shape contract without invoking its producer.
//!
//! The six byte-exact projections and the budget report are covered by
//! `tests/golden.rs`; this file proves the GRAPH meaning:
//!   1. the imported graph carries the exact reference-frame links required by
//!      the ontology contract,
//!   2. exactly one Assessment carries a score of 0.0 (zeros are scores),
//!   3. the unknown role mints an open-vocabulary NarrativeRole value,
//!   4. the cross-children competency-demo SPARQL answers in discourse order,
//!   5. privacy: the fixture is synthetic by construction.

use std::path::Path;
use std::sync::Arc;

use purrdf::parse_dataset;
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue};
use sha2::{Digest, Sha256};

const CORP: &str = "https://blackcatinformatics.ca/gmeow/corpus/foundation/";
const FOUNDATION_TTL: &str = include_str!("goldens/foundation.ttl");
const FOUNDATION_TTL_SHA256: &str =
    "5a84886bee491ee6ad74ec895791d051cba35cdf54dd531a639826cb2e0652b5";

/// Return the committed corpus only after authenticating its exact identity.
fn authenticated_ttl() -> &'static str {
    let actual = format!("{:x}", Sha256::digest(FOUNDATION_TTL.as_bytes()));
    assert_eq!(
        actual, FOUNDATION_TTL_SHA256,
        "foundation corpus identity does not match the test contract"
    );
    FOUNDATION_TTL
}

/// Load the authenticated corpus into a native frozen dataset (canonical codec).
fn authenticated_store() -> Arc<RdfDataset> {
    parse_dataset(authenticated_ttl().as_bytes(), "text/turtle", None)
        .expect("parse authenticated foundation.ttl")
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
// 1. The imported graph carries the reference-frame contract.
// --------------------------------------------------------------------------- //

#[test]
fn fixture_carries_required_reference_frame_contract() {
    let store = authenticated_store();
    let sparql = format!(
        r#"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        ASK {{
            <{CORP}book/1/discourse-frame/axis> a gmeow:Axis .
            <{CORP}book/1/expression>
                gmeow:hasReferenceFrame <{CORP}book/1/discourse-frame> .
        }}
        "#
    );
    let SparqlResult::Boolean(conforms) = query(&store, &sparql) else {
        panic!("reference-frame contract ASK must return a boolean")
    };
    assert!(
        conforms,
        "the authenticated fixture must carry its typed discourse axis and expression frame"
    );
}

// --------------------------------------------------------------------------- //
// 2. Zeros are scores: exactly ONE Assessment has score value 0.0.
// --------------------------------------------------------------------------- //

#[test]
fn zeros_are_scores() {
    let store = authenticated_store();
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
    let store = authenticated_store();
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
    let store = authenticated_store();
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
