// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Executed round-trip witness for the inverse-ingest (`put`) SPARQL leg.
//!
//! The forward `generated/queries/ml-schema.rq` down-projects a canonical gmeow instance
//! to the external `mls:` vocabulary; the inverse `generated/queries/ml-schema.put.rq`
//! lifts that `mls:` output back to gmeow, minting the honest import-derived provenance
//! envelope. This test RUNS both CONSTRUCTs through the in-tree native SPARQL engine
//! (`purrdf::sparql::NativeSparqlEngine`, the same engine the pipeline put loop uses) so
//! the emitted query is proven behaviourally, not merely by text inspection — closing the
//! "emitted but never executed" gap. It also parse-checks every committed `.put.rq`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{parse_dataset, RdfDataset, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const MLS: &str = "http://www.w3.org/ns/mls#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const EX: &str = "http://example.org/";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

fn read_query(name: &str) -> String {
    let path = repo_root().join("generated").join("queries").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Run a CONSTRUCT over `dataset`, returning the constructed default-graph triples as
/// `(subject, predicate, object)` string triples. Hard-fails on a non-graph result.
fn run_construct(dataset: &Arc<RdfDataset>, query: &str) -> Vec<(String, String, String)> {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .unwrap_or_else(|e| panic!("CONSTRUCT evaluation failed: {e}\nquery:\n{query}"));
    let SparqlResult::Graph(ds) = result else {
        panic!("CONSTRUCT did not return a graph\nquery:\n{query}");
    };
    ds.owned_quads()
        .filter(|q| q.graph_name.is_none())
        .map(|q| {
            (
                term_str(&q.subject),
                q.predicate.clone(),
                term_str(&q.object),
            )
        })
        .collect()
}

/// A comparable canonical string for a term: IRIs verbatim, blanks as `_:<opaque>`.
fn term_str(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(id) => format!("_:{id}"),
        RdfTerm::Literal(lit) => format!("\"{}\"", lit.lexical_form),
        RdfTerm::Triple(_) => "<<triple>>".to_owned(),
    }
}

fn dataset_from_triples(triples: &[(String, String, String)]) -> Arc<RdfDataset> {
    let mut ttl = String::new();
    for (s, p, o) in triples {
        // Every term in the round-trip is an IRI (class assertions + object properties).
        ttl.push_str(&format!("<{s}> <{p}> <{o}> .\n"));
    }
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse constructed triples")
}

/// Whether the triple set contains an exact `(s, p, o)` of IRIs.
fn has(triples: &[(String, String, String)], s: &str, p: &str, o: &str) -> bool {
    triples
        .iter()
        .any(|(ts, tp, to)| ts == s && tp == p && to == o)
}

#[test]
fn ml_schema_forward_then_inverse_recovers_gmeow_with_import_envelope() {
    // A tiny canonical gmeow instance. The forward `ml-schema.rq` guard is
    // `FILTER NOT EXISTS { ?x gmeow:displayable false }`, which passes when no
    // `displayable false` is asserted — so the bare instance projects.
    let seed = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix ex: <{EX}> .\n\
         ex:a a gmeow:ModelArtifact .\n\
         ex:d a gmeow:ModelDeployment .\n\
         ex:r a gmeow:RuntimeExecution .\n\
         ex:r gmeow:executionOfDeployment ex:d .\n"
    );
    let source = parse_dataset(seed.as_bytes(), "text/turtle", None).expect("parse seed");

    // ── Forward: gmeow → mls ──────────────────────────────────────────────────────
    let forward = run_construct(&source, &read_query("ml-schema.rq"));
    assert!(
        has(
            &forward,
            &format!("{EX}a"),
            RDF_TYPE,
            &format!("{MLS}Model")
        ),
        "forward must project the model artifact → mls:Model\n{forward:#?}"
    );
    assert!(
        has(
            &forward,
            &format!("{EX}d"),
            RDF_TYPE,
            &format!("{MLS}Implementation")
        ),
        "forward must project the deployment → mls:Implementation\n{forward:#?}"
    );
    assert!(
        has(&forward, &format!("{EX}r"), RDF_TYPE, &format!("{MLS}Run")),
        "forward must project the runtime execution → mls:Run\n{forward:#?}"
    );
    assert!(
        has(
            &forward,
            &format!("{EX}r"),
            &format!("{MLS}executes"),
            &format!("{EX}d")
        ),
        "forward must project executionOfDeployment → mls:executes\n{forward:#?}"
    );

    // ── Inverse: mls → gmeow (mint-with-claim) ────────────────────────────────────
    let mls_ds = dataset_from_triples(&forward);
    let lifted = run_construct(&mls_ds, &read_query("ml-schema.put.rq"));

    // (a) the class / predicate lift is recovered.
    assert!(
        has(
            &lifted,
            &format!("{EX}a"),
            RDF_TYPE,
            &format!("{GMEOW}ModelArtifact")
        ),
        "inverse must lift mls:Model → gmeow:ModelArtifact\n{lifted:#?}"
    );
    assert!(
        has(
            &lifted,
            &format!("{EX}d"),
            RDF_TYPE,
            &format!("{GMEOW}ModelDeployment")
        ),
        "inverse must lift mls:Implementation → gmeow:ModelDeployment\n{lifted:#?}"
    );
    assert!(
        has(
            &lifted,
            &format!("{EX}r"),
            RDF_TYPE,
            &format!("{GMEOW}RuntimeExecution")
        ),
        "inverse must lift mls:Run → gmeow:RuntimeExecution\n{lifted:#?}"
    );
    assert!(
        has(
            &lifted,
            &format!("{EX}r"),
            &format!("{GMEOW}executionOfDeployment"),
            &format!("{EX}d")
        ),
        "inverse must lift mls:executes → gmeow:executionOfDeployment\n{lifted:#?}"
    );

    // (b) the mint-with-claim envelope is present: each lifted subject was generated by an
    // ImportActivity and carries a mappedFrom back to its mls: source term.
    let generated_by = format!("{GMEOW}wasGeneratedBy");
    let mapped_from = format!("{GMEOW}mappedFrom");
    let import_activity = format!("{GMEOW}ImportActivity");
    for subj in [format!("{EX}a"), format!("{EX}d"), format!("{EX}r")] {
        // wasGeneratedBy an ImportActivity-typed node.
        let gen: Vec<&(String, String, String)> = lifted
            .iter()
            .filter(|(s, p, _)| s == &subj && p == &generated_by)
            .collect();
        assert!(
            !gen.is_empty(),
            "lifted subject {subj} must carry gmeow:wasGeneratedBy\n{lifted:#?}"
        );
        // The subject's OWN generating node — the object of its wasGeneratedBy triple —
        // must be typed gmeow:ImportActivity. Keyed on that exact blank-node label so the
        // assertion fails if the ImportActivity typing lands on some other node.
        let import_node = &gen[0].2;
        assert!(
            has(&lifted, import_node, RDF_TYPE, &import_activity),
            "the generating node {import_node} for {subj} must be a gmeow:ImportActivity\n{lifted:#?}"
        );
        // mappedFrom some mls: source term.
        let mapped: Vec<&(String, String, String)> = lifted
            .iter()
            .filter(|(s, p, o)| s == &subj && p == &mapped_from && o.starts_with(MLS))
            .collect();
        assert!(
            !mapped.is_empty(),
            "lifted subject {subj} must carry gmeow:mappedFrom an mls: term\n{lifted:#?}"
        );
    }
}

#[test]
fn every_committed_put_rq_parses_as_valid_sparql() {
    let dir = repo_root().join("generated").join("queries");
    let empty = parse_dataset(b"", "text/turtle", None).expect("empty dataset");
    let engine = NativeSparqlEngine::new();

    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read_dir queries") {
        let path: PathBuf = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rq") {
            continue;
        }
        if !is_put_rq(&path) {
            continue;
        }
        let query = std::fs::read_to_string(&path).expect("read .put.rq");
        // A broken CONSTRUCT fails to parse/evaluate; a well-formed one returns a graph
        // (empty over the empty dataset). Either way, execution proves the SPARQL is valid.
        let result = engine
            .query(
                &empty,
                SparqlRequest {
                    query: &query,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .unwrap_or_else(|e| panic!("{} is not valid SPARQL: {e}", path.display()));
        assert!(
            matches!(result, SparqlResult::Graph(_)),
            "{} must be a CONSTRUCT (graph result)",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 1,
        "expected at least one committed `.put.rq` to parse-check"
    );
}

fn is_put_rq(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".put.rq"))
        .unwrap_or(false)
}
