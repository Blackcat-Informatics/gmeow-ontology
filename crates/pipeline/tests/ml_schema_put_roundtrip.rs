// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Executed round-trip witness for the inverse-ingest (`put`) SPARQL leg.
//!
//! The forward `generated/queries/ml-schema.rq` down-projects a canonical gmeow instance
//! to the external `mls:` vocabulary; the inverse `generated/queries/ml-schema.put.rq`
//! lifts that `mls:` output back to gmeow as reasoner-inert `gmeow:StatementMetadata` reified
//! claims — never asserting the interior triple — under one deterministic import activity. This
//! test RUNS both CONSTRUCTs through the in-tree native SPARQL engine
//! (`purrdf::sparql::NativeSparqlEngine`, the same engine the pipeline put loop uses) so the
//! emitted query is proven behaviourally, not merely by text inspection, and RUNS the native
//! reason lane over the lift to prove — with a positive control — that it materializes no
//! domain fact. It also parse-checks every committed `.put.rq`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult, parse_dataset};

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
fn ml_schema_forward_then_inverse_reifies_the_lift_as_an_inert_claim() {
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

    // ── Inverse: mls → gmeow (reified claim, not asserted fact) ───────────────────
    let mls_ds = dataset_from_triples(&forward);
    let lifted = run_construct(&mls_ds, &read_query("ml-schema.put.rq"));

    // (a) LOAD-BEARING (C2): the interior triple is NEVER asserted. A ValidationOnly lift is
    // a candidate preimage the source cannot itself express — it must not appear as fact.
    for (s, p, o) in [
        (
            format!("{EX}a"),
            RDF_TYPE.to_owned(),
            format!("{GMEOW}ModelArtifact"),
        ),
        (
            format!("{EX}d"),
            RDF_TYPE.to_owned(),
            format!("{GMEOW}ModelDeployment"),
        ),
        (
            format!("{EX}r"),
            RDF_TYPE.to_owned(),
            format!("{GMEOW}RuntimeExecution"),
        ),
        (
            format!("{EX}r"),
            format!("{GMEOW}executionOfDeployment"),
            format!("{EX}d"),
        ),
    ] {
        assert!(
            !has(&lifted, &s, &p, &o),
            "the ValidationOnly lift ({s} {p} {o}) must NOT be asserted as a base-graph fact\n{lifted:#?}"
        );
    }

    // (b) each lift is instead carried as a gmeow:StatementMetadata reified claim naming the
    // very subject/predicate/object, wasGeneratedBy the ONE coalesced import activity, with
    // gmeow:mappedFrom (on the annotation list) back to its mls: source term.
    assert!(
        reified_claim(
            &lifted,
            &format!("{EX}a"),
            RDF_TYPE,
            &format!("{GMEOW}ModelArtifact")
        ),
        "the mls:Model→gmeow:ModelArtifact lift must be a reified claim\n{lifted:#?}"
    );
    assert!(
        reified_claim(
            &lifted,
            &format!("{EX}d"),
            RDF_TYPE,
            &format!("{GMEOW}ModelDeployment")
        ),
        "the mls:Implementation→gmeow:ModelDeployment lift must be a reified claim\n{lifted:#?}"
    );
    assert!(
        reified_claim(
            &lifted,
            &format!("{EX}r"),
            RDF_TYPE,
            &format!("{GMEOW}RuntimeExecution")
        ),
        "the mls:Run→gmeow:RuntimeExecution lift must be a reified claim\n{lifted:#?}"
    );
    assert!(
        reified_claim(
            &lifted,
            &format!("{EX}r"),
            &format!("{GMEOW}executionOfDeployment"),
            &format!("{EX}d")
        ),
        "the mls:executes→gmeow:executionOfDeployment predicate lift must be a reified claim\n{lifted:#?}"
    );

    // (c) exactly ONE import activity node, coalesced onto the deterministic per-profile IRI
    // (C3) — never a per-solution blank — and it is the deterministic import IRI.
    let import_activity = format!("{GMEOW}ImportActivity");
    let import_iri = format!("{GMEOW}import/ml-schema");
    let activities: Vec<&(String, String, String)> = lifted
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &import_activity)
        .collect();
    assert_eq!(
        activities.len(),
        1,
        "exactly one coalesced gmeow:ImportActivity node\n{lifted:#?}"
    );
    assert_eq!(
        activities[0].0, import_iri,
        "the import activity must be the deterministic per-profile IRI\n{lifted:#?}"
    );
    // No wall-clock ingest stamp is ever emitted.
    assert!(
        !lifted
            .iter()
            .any(|(_, p, _)| p == &format!("{GMEOW}ingestedAt")),
        "the deterministic emitter must not stamp gmeow:ingestedAt\n{lifted:#?}"
    );
}

/// Whether the lifted graph carries a `gmeow:StatementMetadata` reified claim whose reified
/// subject/predicate/object are exactly `(subj, pred, obj)`, wasGeneratedBy the coalesced import
/// activity, with a `gmeow:mappedFrom` annotation back to an `mls:` term. The three reified edges
/// must land on ONE cell blank (join on the cell), so a claim assembled from unrelated cells
/// never spuriously satisfies it.
fn reified_claim(triples: &[(String, String, String)], subj: &str, pred: &str, obj: &str) -> bool {
    let q_subject = format!("{GMEOW}qSubject");
    let q_predicate = format!("{GMEOW}qPredicate");
    let q_object = format!("{GMEOW}qObject");
    let statement_metadata = format!("{GMEOW}StatementMetadata");
    let generated_by = format!("{GMEOW}wasGeneratedBy");
    let annotation = format!("{GMEOW}annotation");
    let ann_property = format!("{GMEOW}annProperty");
    let ann_value = format!("{GMEOW}annValue");
    let mapped_from = format!("{GMEOW}mappedFrom");
    let import_iri = format!("{GMEOW}import/ml-schema");

    triples.iter().any(|(cell, p, o)| {
        p == RDF_TYPE
            && o == &statement_metadata
            && has(triples, cell, &q_subject, subj)
            && has(triples, cell, &q_predicate, pred)
            && has(triples, cell, &q_object, obj)
            && has(triples, cell, &generated_by, &import_iri)
            // the mappedFrom annotation node hangs off the cell and names an mls: term.
            && triples.iter().any(|(c, ap, ann)| {
                c == cell
                    && ap == &annotation
                    && has(triples, ann, &ann_property, &mapped_from)
                    && triples
                        .iter()
                        .any(|(an, vp, val)| an == ann && vp == &ann_value && val.starts_with(MLS))
            })
    })
}

const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// Serialize one `(s, p, o)` string triple (from [`run_construct`], where IRIs are verbatim,
/// blanks are `_:id`, and literals are already `"quoted"`) back to a Turtle line.
fn ser_term(t: &str) -> String {
    if t.starts_with("_:") || t.starts_with('"') {
        t.to_owned()
    } else {
        format!("<{t}>")
    }
}

#[test]
fn ml_schema_reified_lift_never_materializes_under_the_reason_lane() {
    // LOAD-BEARING (AC2): prove the entire C2 honesty claim by EXECUTION, not assumption — a
    // gmeow:StatementMetadata reified lift is reasoner-INERT. Reproduce the lifted graph, then
    // run the native reason lane over it under a TBox of REAL gmeow axioms and observe that the
    // interior triples never materialize as domain facts — with a positive control so the
    // absence assertions cannot pass vacuously.
    let seed = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix ex: <{EX}> .\n\
         ex:a a gmeow:ModelArtifact .\n\
         ex:d a gmeow:ModelDeployment .\n\
         ex:r a gmeow:RuntimeExecution .\n\
         ex:r gmeow:executionOfDeployment ex:d .\n"
    );
    let source = parse_dataset(seed.as_bytes(), "text/turtle", None).expect("parse seed");
    let forward = run_construct(&source, &read_query("ml-schema.rq"));
    let mls_ds = dataset_from_triples(&forward);
    let lifted = run_construct(&mls_ds, &read_query("ml-schema.put.rq"));

    // EDB = the lifted graph (reified claims + the one import activity) + a small TBox of REAL
    // gmeow axioms that WOULD materialize a domain consequence IF an interior triple were a fact:
    //   gmeow:ModelArtifact  ⊑ gmeow:InformationObject  — the class lift's real superclass (probe)
    //   gmeow:ImportActivity ⊑ gmeow:Activity           — the import node's real superclass (control)
    // The native reason lane is world-scoped, so every quad is placed in one named graph W.
    let w = format!("{GMEOW}test/reason-world");
    let mut nq = String::new();
    nq.push_str(&format!(
        "<{GMEOW}ModelArtifact> <{SUBCLASS}> <{GMEOW}InformationObject> <{w}> .\n"
    ));
    nq.push_str(&format!(
        "<{GMEOW}ImportActivity> <{SUBCLASS}> <{GMEOW}Activity> <{w}> .\n"
    ));
    for (s, p, o) in &lifted {
        nq.push_str(&format!("{} <{p}> {} <{w}> .\n", ser_term(s), ser_term(o)));
    }
    let edb = parse_dataset(nq.as_bytes(), "application/n-quads", None).expect("parse reason edb");
    let closure = gmeow_logic::reason::el_closure(&edb)
        .expect("el_closure over the lifted graph")
        .inferred;

    // Stored axioms keep the object in its decoded display form `<iri>`; subject/predicate are
    // bare IRIs.
    let has = |s: &str, p: &str, o: &str| {
        let obj = format!("<{o}>");
        closure
            .iter()
            .any(|a| a.subject == s && a.predicate == p && a.object == obj)
    };
    let import = format!("{GMEOW}import/ml-schema");

    // POSITIVE CONTROL (non-vacuity): (1) the reified StatementMetadata cells ARE present in the
    // reasoned world — so the lifted graph is genuinely non-empty and loaded; (2) the asserted
    // ImportActivity DID materialize its superclass gmeow:Activity — so the subsumption rule is
    // firing. Only against this backdrop do the absences below carry weight.
    assert!(
        closure
            .iter()
            .any(|a| a.predicate == RDF_TYPE && a.object == format!("<{GMEOW}StatementMetadata>")),
        "positive control: reified StatementMetadata cells must be present in the reasoned closure\n{closure:#?}"
    );
    assert!(
        has(&import, RDF_TYPE, &format!("{GMEOW}Activity")),
        "positive control: the asserted ImportActivity MUST materialize gmeow:Activity (the rule fires)\n{closure:#?}"
    );

    // INERTNESS: no reified lift (class OR predicate) materializes ANY rdf:type domain fact on
    // its subject/object. ex:a covers the class lift; ex:d/ex:r cover the other class lifts AND
    // the executionOfDeployment predicate reification (its subject ex:r and object ex:d).
    for subj in [format!("{EX}a"), format!("{EX}d"), format!("{EX}r")] {
        let types: Vec<_> = closure
            .iter()
            .filter(|a| a.subject == subj && a.predicate == RDF_TYPE)
            .collect();
        assert!(
            types.is_empty(),
            "the reified lift for {subj} must NOT materialize any rdf:type domain fact (inertness)\n{types:#?}"
        );
    }
    // The specific EL consequence that WOULD fire if ex:a a gmeow:ModelArtifact were asserted
    // (its superclass gmeow:InformationObject) is absent — the very rule that fired for the
    // ImportActivity does NOT fire for the reified claim.
    assert!(
        !has(
            &format!("{EX}a"),
            RDF_TYPE,
            &format!("{GMEOW}InformationObject")
        ),
        "the reified ModelArtifact claim must NOT leak its subclass consequence into the closure"
    );
    assert!(
        !has(
            &format!("{EX}a"),
            RDF_TYPE,
            &format!("{GMEOW}ModelArtifact")
        ),
        "the interior class triple must never be materialized as a fact"
    );
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
