// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact observations of contextual verdicts already shipped in the selected
//! bundle. This reads native result metadata; it never evaluates an assessment.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};
use serde::{Deserialize, Serialize};

/// Authenticated bundle-derived action, required by the fixture selector.
pub const ARTIFACT: &str = "contextual-corpus-observations-v1.json";
/// Canonical modal example namespace, also identifying its unasserted query.
pub const CONTEXTUAL: &str = "https://blackcatinformatics.ca/gmeow/examples/composite-modal/";
/// Canonical finite-journal namespace, also identifying its unasserted query.
pub const TEMPORAL: &str = "https://blackcatinformatics.ca/gmeow/examples/finite-journal/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

#[cfg(test)]
mod tests;

/// A diagnostic projection of native metadata objects, retaining their RDF term
/// spelling (IRI brackets, literal datatype/language/direction, or quotation).
/// Consumers compare these values directly; none are reparsed as pipeline data.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Node {
    /// Predicate IRI to the exact set of observed object terms.
    pub properties: BTreeMap<String, BTreeSet<String>>,
}

/// Selected request/result/prefix/proof metadata, bounded by thirteen requests
/// and their direct result components. No corpus graph or engine state is cached.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observations {
    /// Requests, their results, and the linked prefix/proof component nodes.
    pub nodes: BTreeMap<String, Node>,
    /// Graphs asserting each example's readiness query; quoted evidence is absent.
    pub readiness_graphs: BTreeMap<String, BTreeSet<Option<String>>>,
}

/// Observe the exact selected terminal dataset without copying it or executing
/// queries. Native subject indexes cover ordinary rows and reifier annotations;
/// a reifier's quoted triple is never promoted to an assertion.
pub fn observe(dataset: &RdfDataset) -> Observations {
    let mut observed = Observations {
        nodes: BTreeMap::new(),
        readiness_graphs: BTreeMap::new(),
    };
    let graph = dataset.term_id_by_value(&TermValue::iri(gmeow_logic::result_rdf::GRAPH_REASONING));
    for (namespace, requests) in [
        (
            CONTEXTUAL,
            &["proposalAssessment", "reviewAssessment", "nestedAssessment"][..],
        ),
        (
            TEMPORAL,
            &[
                "observedNext",
                "finalNext",
                "pendingNext",
                "closedEventuality",
                "pendingEventuality",
                "closedMaintenance",
                "pendingMaintenance",
                "closedUntil",
                "pendingUntil",
                "immediateUntilWitness",
            ][..],
        ),
    ] {
        observed
            .readiness_graphs
            .insert(namespace.to_owned(), readiness_graphs(dataset, namespace));
        for request in requests {
            let request = format!("{namespace}{request}");
            let Some(subject) = dataset.term_id_by_value(&TermValue::iri(&request)) else {
                observed.nodes.insert(request, Node::default());
                continue;
            };
            let rows = graph.map_or_else(Vec::new, |graph| metadata_rows(dataset, subject, graph));
            for result in linked(dataset, &rows, "contextualResult") {
                let result_rows =
                    graph.map_or_else(Vec::new, |graph| metadata_rows(dataset, result, graph));
                for predicate in [
                    "observedTemporalPrefix",
                    "resultProof",
                    "resultCounterproof",
                ] {
                    for component in linked(dataset, &result_rows, predicate) {
                        if let TermRef::Iri(iri) = dataset.resolve(component)
                            && observed.nodes.contains_key(iri)
                        {
                            continue;
                        }
                        let component_rows = graph.map_or_else(Vec::new, |graph| {
                            metadata_rows(dataset, component, graph)
                        });
                        insert_node(dataset, component, &component_rows, &mut observed.nodes);
                    }
                }
                insert_node(dataset, result, &result_rows, &mut observed.nodes);
            }
            observed.nodes.insert(request, project_rows(dataset, &rows));
        }
    }
    observed
}

/// Read asserted metadata in exactly the reasoning graph. The selected fields
/// never use `rdf:reifies`, so its quote-bearing table supplies no metadata rows.
fn metadata_rows(dataset: &RdfDataset, subject: TermId, graph: TermId) -> Vec<(TermId, TermId)> {
    let mut rows: Vec<_> = dataset
        .quads_for_pattern(Some(subject), None, None, GraphMatch::Named(graph))
        .map(|row| (row.p, row.o))
        .collect();
    rows.extend(
        dataset
            .annotations_of_with_graph(subject)
            .filter(|(_, _, owner)| *owner == Some(graph))
            .map(|(predicate, object, _)| (predicate, object)),
    );
    rows
}

/// Follow only native IRI links; a malformed non-IRI remains visible in its
/// parent's observation and cannot become a fabricated child identity.
fn linked(dataset: &RdfDataset, rows: &[(TermId, TermId)], local: &str) -> BTreeSet<TermId> {
    let predicate = format!("{LOGIC}{local}");
    rows.iter()
        .filter_map(|(p, o)| {
            (matches!(dataset.resolve(*p), TermRef::Iri(iri) if iri == predicate)
                && matches!(dataset.resolve(*o), TermRef::Iri(_)))
            .then_some(*o)
        })
        .collect()
}

/// Retain exact RDF object spelling while deduplicating identical assertions
/// across ordinary and annotation storage, matching RDF set semantics.
fn project_rows(dataset: &RdfDataset, rows: &[(TermId, TermId)]) -> Node {
    let mut node = Node::default();
    for (predicate, object) in rows {
        if let TermRef::Iri(predicate) = dataset.resolve(*predicate) {
            node.properties
                .entry(predicate.to_owned())
                .or_default()
                .insert(dataset.to_owned_term(*object).to_string());
        }
    }
    node
}

/// Share identical result/prefix/proof identities within this one observation.
fn insert_node(
    dataset: &RdfDataset,
    id: TermId,
    rows: &[(TermId, TermId)],
    nodes: &mut BTreeMap<String, Node>,
) {
    if let TermRef::Iri(iri) = dataset.resolve(id) {
        nodes
            .entry(iri.to_owned())
            .or_insert_with(|| project_rows(dataset, rows));
    }
}

/// Inspect only asserted readiness rows across all graphs, including annotations
/// on a reifier subject. Quotation alone contributes no matching assertion.
fn readiness_graphs(dataset: &RdfDataset, namespace: &str) -> BTreeSet<Option<String>> {
    let mut graphs = BTreeSet::new();
    let [Some(subject), Some(predicate), Some(object)] = ["procedure", "readyFor", "execution"]
        .map(|local| dataset.term_id_by_value(&TermValue::iri(format!("{namespace}{local}"))))
    else {
        return graphs;
    };
    let graph_name = |graph: Option<TermId>| graph.map(|id| dataset.to_owned_term(id).to_string());
    for row in dataset.quads_for_pattern(
        Some(subject),
        Some(predicate),
        Some(object),
        GraphMatch::Any,
    ) {
        graphs.insert(graph_name(row.g));
    }
    for (p, o, graph) in dataset.annotations_of_with_graph(subject) {
        if p == predicate && o == object {
            graphs.insert(graph_name(graph));
        }
    }
    graphs
}
