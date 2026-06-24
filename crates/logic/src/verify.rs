// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, Docker-free reasoned-graph verify (issue #695).
//!
//! The closed-world QC half of the hybrid OWL+SHACL architecture, in Rust. It
//! replaces the ROBOT `verify` Docker step: materialize the reasoned graph (the
//! asserted RDF-1.2 graph *unioned* with the native EL/DL derived subsumption /
//! type / equivalent-class edges), then run each `queries/verify/*.rq`
//! "bad-example" SPARQL SELECT over it. Any returned solution row is a
//! violation, surfaced as an `error` [`gmeow_diagnostics::Finding`].
//!
//! The authority lives here (Principles 17/18): closure materialization, SPARQL
//! execution, and `Finding`/`Report` construction are all native. Python only
//! discovers the query files (repo/slice layout), calls in, and writes the
//! diagnostics artifacts.

use oxigraph::model::{GraphName, NamedNode, Quad};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use gmeow_diagnostics::{Finding, Location, Report, Severity};
use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::RdfDataset;

use crate::reason::reason_all;

/// Strip a single pair of angle brackets from an IRI term, if present.
///
/// The native engine emits subjects/predicates as bare IRI strings and objects
/// already wrapped in `<...>` (mirroring `reason.py::_iri_term`); this collapses
/// both to the bare IRI so `NamedNode::new` accepts them.
fn bare_iri(value: &str) -> &str {
    value
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(value)
}

/// Derive a stable, short check name from a repo-relative `.rq` path.
///
/// `queries/verify/axis-not-disjoint.rq` → `axis-not-disjoint`. Used for the
/// `verify.<name>` finding code; the full path is kept on the finding location.
fn query_stem(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .strip_suffix(".rq")
        .unwrap_or_else(|| name.rsplit('/').next().unwrap_or(name))
}

/// Run the reasoned-graph negative tests natively over `edb`.
///
/// Materializes a flat oxigraph store = the asserted graph (flattened to the
/// default graph, literals and `owl:members` RDF lists preserved) unioned with
/// the native non-EDB derived edges from a single EL/DL chase, then evaluates
/// each `(name, sparql)` SELECT query against it. A query returning any rows is
/// a violation → an `error` finding (offending bindings in `detail`, the query
/// path as the finding location). A trailing `note` summarizes the run.
///
/// Never panics on a violation; the caller inspects [`Report::ok`]. The returned
/// report is NOT yet normalized (the PyO3 layer normalizes before serializing).
///
/// # Errors
///
/// Returns `Err(String)` if reasoning fails, if a query fails to parse/evaluate,
/// if a query is not a SELECT, or if a derived edge cannot be built as a quad.
pub fn verify(edb: &RdfDataset, queries: &[(String, String)]) -> Result<Report, String> {
    // 1. Flat asserted graph (default graph; literals + owl:members lists kept).
    //    A no-GRAPH verify query then matches it, exactly like ROBOT's single
    //    merged reasoned graph.
    let store = store_from_dataset(edb, GraphPolicy::FlattenToDefaultGraph)
        .map_err(|e| format!("flatten asserted store failed: {e}"))?;

    // 2. Native EL/DL closure; layer the derived (non-EDB) edges on top, also in
    //    the default graph. The native closure only materializes subsumption /
    //    type / equivalent-class edges, which is what the inferred-edge verify
    //    queries (class-in-two-disjoint-axes, class-without-stereotype) rely on.
    let result = reason_all(edb)?;
    for ax in &result.inferred {
        if ax.is_edb {
            continue;
        }
        let subject = NamedNode::new(bare_iri(&ax.subject))
            .map_err(|e| format!("derived subject IRI {:?}: {e}", ax.subject))?;
        let predicate = NamedNode::new(bare_iri(&ax.predicate))
            .map_err(|e| format!("derived predicate IRI {:?}: {e}", ax.predicate))?;
        let object = NamedNode::new(bare_iri(&ax.object))
            .map_err(|e| format!("derived object IRI {:?}: {e}", ax.object))?;
        let quad = Quad::new(subject, predicate, object, GraphName::DefaultGraph);
        store
            .insert(&quad)
            .map_err(|e| format!("derived-edge insert failed: {e}"))?;
    }

    // 3. Evaluate each verify query; any solution row is a violation.
    let mut report = Report::new("verify");
    let mut violations = 0usize;
    for (name, sparql) in queries {
        let stem = query_stem(name);
        let results = SparqlEvaluator::new()
            .parse_query(sparql)
            .map_err(|e| format!("verify query {name} parse error: {e}"))?
            .on_store(&store)
            .execute()
            .map_err(|e| format!("verify query {name} evaluation error: {e}"))?;

        let solutions = match results {
            QueryResults::Solutions(solutions) => solutions,
            QueryResults::Boolean(_) | QueryResults::Graph(_) => {
                return Err(format!(
                    "verify query {name} must be a SPARQL SELECT (got ASK or CONSTRUCT/DESCRIBE)"
                ));
            }
        };

        let mut rows: Vec<String> = Vec::new();
        for sol in solutions {
            let sol = sol.map_err(|e| format!("verify query {name} solution error: {e}"))?;
            let mut binding: Vec<String> = Vec::new();
            for (var, term) in sol.iter() {
                binding.push(format!("{}={term}", var.as_str()));
            }
            // Sort the per-row bindings so the joined detail is independent of the
            // query engine's variable-projection / iteration order — keeping the
            // report content hash and the GTS feedback bundle byte-deterministic.
            binding.sort();
            rows.push(binding.join(", "));
        }

        if rows.is_empty() {
            continue;
        }
        violations += 1;
        // Sort the offending rows so the finding detail (and thus the report
        // content hash / GTS bundle) is deterministic.
        rows.sort();
        let mut finding = Finding::new(
            Severity::Error,
            format!("verify.{stem}"),
            format!(
                "{stem}: {} offending row(s) on the reasoned graph",
                rows.len()
            ),
        )
        .with_tool("verify");
        finding.detail = Some(rows.join("; "));
        finding.tags = vec!["reasoned-graph".to_owned(), "negative-test".to_owned()];
        // Graph-located findings still need a physicalLocation for SARIF
        // code-scanning upload: anchor on the query's repo-relative path.
        finding.add_location(Location::new(Some(name.clone()), None, None, None));
        report.add_finding(finding);
    }

    report.add_finding(
        Finding::new(
            Severity::Note,
            "verify.native.summary",
            format!(
                "native reasoned-graph verify: {} quer{} run, {violations} with violations",
                queries.len(),
                if queries.len() == 1 { "y" } else { "ies" }
            ),
        )
        .with_tool("verify"),
    );

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn store() -> std::sync::Arc<RdfDataset> {
        // A ⊑ B ⊑ C — the native EL closure derives A ⊑ C.
        let mut builder = RdfDatasetBuilder::new();
        for quad in [quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)] {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn clean_query_yields_no_error_findings() {
        // No class is a subclass of itself → no rows → clean.
        let q = (
            "queries/verify/no-self-subclass.rq".to_owned(),
            format!("SELECT ?x WHERE {{ ?x <{SUBCLASS}> ?x }}"),
        );
        let dataset = store();
        let report = verify(dataset.as_ref(), std::slice::from_ref(&q)).expect("verify runs");
        assert!(report.ok(), "clean run must have no error findings");
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn violating_query_yields_error_finding_with_detail() {
        // Anything that is a subclass of C → A (asserted) and B (asserted) and,
        // crucially, the DERIVED A ⊑ C is also present, proving the closure layer.
        let q = (
            "queries/verify/subclass-of-c.rq".to_owned(),
            format!("SELECT ?x WHERE {{ ?x <{SUBCLASS}> <{C}> }}"),
        );
        let dataset = store();
        let report = verify(dataset.as_ref(), std::slice::from_ref(&q)).expect("verify runs");
        assert!(!report.ok(), "a returned row must fail the report");
        assert_eq!(report.error_count(), 1);
        let finding = report
            .findings
            .iter()
            .find(|f| f.severity == Severity::Error)
            .expect("error finding present");
        assert_eq!(finding.code, "verify.subclass-of-c");
        let detail = finding.detail.as_deref().unwrap_or("");
        // B ⊑ C is asserted; A ⊑ C is the derived edge — both must be caught,
        // which proves the native closure was layered onto the asserted graph.
        assert!(detail.contains(A), "derived A ⊑ C must be caught: {detail}");
        assert!(
            detail.contains(B),
            "asserted B ⊑ C must be caught: {detail}"
        );
    }

    #[test]
    fn ask_query_is_rejected() {
        let q = (
            "queries/verify/bad.rq".to_owned(),
            "ASK { ?s ?p ?o }".to_owned(),
        );
        let dataset = store();
        let err = verify(dataset.as_ref(), std::slice::from_ref(&q)).unwrap_err();
        assert!(err.contains("SELECT"), "ASK must be rejected: {err}");
    }
}
