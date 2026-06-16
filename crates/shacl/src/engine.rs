// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL Core validation engine.
//!
//! `validate` is the top-level entry point.  Resolves focus nodes for every
//! non-deactivated node shape, runs all constraints, and assembles a
//! deterministically-sorted [`ValidationReport`].

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::Term;
use oxigraph::store::Store;

use crate::model::{rdf, rdfs};
use crate::report::ValidationReport;
use crate::shapes::{Shapes, Target};

// ── Target resolution helpers ─────────────────────────────────────────────────

/// Collect distinct subjects of `(?, pred, ?)` in the default graph.
fn subjects_of(data: &Store, pred: &oxigraph::model::NamedNode) -> Vec<Term> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for quad in data
        .quads_for_pattern(None, Some(pred.as_ref()), None, None)
        .flatten()
    {
        let t = Term::from(quad.subject);
        if seen.insert(t.clone()) {
            result.push(t);
        }
    }
    result
}

/// Collect distinct objects of `(?, pred, ?)` in the default graph.
fn objects_of(data: &Store, pred: &oxigraph::model::NamedNode) -> Vec<Term> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for quad in data
        .quads_for_pattern(None, Some(pred.as_ref()), None, None)
        .flatten()
    {
        let t = quad.object;
        if seen.insert(t.clone()) {
            result.push(t);
        }
    }
    result
}

/// The transitive closure of asserted `rdfs:subClassOf` at or below `class_iri`:
/// the set containing `class_iri` itself plus every class that is a (transitive)
/// subclass of it via `rdfs:subClassOf` triples **asserted in the data graph**.
///
/// This implements SHACL class-membership semantics (§4.2.5), which honor the
/// subclass relationships present in the data. It is NOT OWL/RDFS inference: we
/// read `rdfs:subClassOf` triples that exist and materialize nothing. (The
/// "no-inference contract" means no reasoner is run, not that asserted subclass
/// edges are ignored.) See #599.
pub(crate) fn subclass_closure(
    data: &Store,
    class_iri: &oxigraph::model::NamedNode,
) -> std::collections::HashSet<Term> {
    let mut closure = std::collections::HashSet::new();
    let start = Term::NamedNode(class_iri.clone());
    closure.insert(start.clone());
    let mut frontier = vec![start];
    while let Some(superclass) = frontier.pop() {
        // Any X with `X rdfs:subClassOf superclass` is a subclass to descend into.
        for quad in data
            .quads_for_pattern(
                None,
                Some(rdfs::SUB_CLASS_OF),
                Some(superclass.as_ref()),
                None,
            )
            .flatten()
        {
            let sub = Term::from(quad.subject);
            if closure.insert(sub.clone()) {
                frontier.push(sub);
            }
        }
    }
    closure
}

/// Collect subjects that are SHACL instances of `class_iri`: nodes with an
/// `rdf:type` to `class_iri` or to any asserted (transitive) subclass of it.
fn instances_of_class(data: &Store, class_iri: &oxigraph::model::NamedNode) -> Vec<Term> {
    let classes = subclass_closure(data, class_iri);
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for class in &classes {
        for quad in data
            .quads_for_pattern(None, Some(rdf::TYPE), Some(class.as_ref()), None)
            .flatten()
        {
            let t = Term::from(quad.subject);
            if seen.insert(t.clone()) {
                result.push(t);
            }
        }
    }
    result
}

/// Resolve the focus node set for a single shape from its target declarations.
///
/// Results are deduped by `Term` identity and sorted for a stable order.
fn resolve_focus_nodes(data: &Store, targets: &[Target]) -> Vec<Term> {
    let mut seen = std::collections::HashSet::new();
    let mut nodes: Vec<Term> = Vec::new();

    for target in targets {
        let candidates: Vec<Term> = match target {
            Target::Class(class_iri) => instances_of_class(data, class_iri),
            Target::SubjectsOf(pred) => subjects_of(data, pred),
            Target::ObjectsOf(pred) => objects_of(data, pred),
            Target::Node(t) => vec![t.clone()],
            Target::ImplicitClass(t) => {
                // Same semantics as Class: subjects of (?, rdf:type, t)
                if let Term::NamedNode(nn) = t {
                    instances_of_class(data, nn)
                } else {
                    vec![]
                }
            }
            // sh:SPARQLTarget: execute the pre-validated SELECT and collect ?this.
            // Query parseability is guaranteed at shapes-parse time, so .expect() is correct.
            Target::Sparql(select) => crate::sparql::eval_target(data, select)
                .expect("SPARQLTarget query execution failed (parseability checked at parse time)"),
        };
        for node in candidates {
            if seen.insert(node.clone()) {
                nodes.push(node);
            }
        }
    }

    // Sort for a stable, deterministic ordering across iterations.
    nodes.sort_by_key(|a| a.to_string());
    nodes
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Validate `data` against `shapes`, returning a deterministic [`ValidationReport`].
///
/// For every non-deactivated node shape, the focus node set is resolved from
/// the shape's target declarations and each focus node is validated against
/// the shape via [`crate::constraints::validate_shape`].  Results are sorted
/// by `(focus_node, source_constraint_component, source_shape, result_path,
/// value)` so reports are identical across runs.
pub fn validate(data: &Store, shapes: &Shapes) -> ValidationReport {
    let mut all_results = Vec::new();

    for shape in &shapes.node_shapes {
        if shape.deactivated {
            continue;
        }

        let focus_nodes = resolve_focus_nodes(data, &shape.targets);

        for focus in &focus_nodes {
            let results = crate::constraints::validate_shape(data, focus, shape);
            all_results.extend(results);
        }
    }

    // Deterministic sort key: (focus_node, component, source_shape, path, value)
    all_results.sort_by(|a, b| {
        let ka = (
            a.focus_node.to_string(),
            a.source_constraint_component.to_string(),
            a.source_shape.to_string(),
            a.result_path
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default(),
            a.value.as_ref().map(|t| t.to_string()).unwrap_or_default(),
        );
        let kb = (
            b.focus_node.to_string(),
            b.source_constraint_component.to_string(),
            b.source_shape.to_string(),
            b.result_path
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default(),
            b.value.as_ref().map(|t| t.to_string()).unwrap_or_default(),
        );
        ka.cmp(&kb)
    });

    let conforms = all_results.is_empty();

    ValidationReport {
        conforms,
        results: all_results,
    }
}

/// Validate data (N-Triples) against shapes (Turtle), returning a [`ValidationReport`].
///
/// Creates two in-memory stores, loads the respective graphs, parses shapes
/// via [`crate::shapes::from_store`], and delegates to [`validate`].
///
/// Both graphs are loaded with the **lenient** RDF parser. A validator must be
/// able to ingest the data graph before it can validate any shapes against it,
/// and RDF lexical well-formedness is a separate concern from SHACL conformance.
/// The gmeow ontology carries private-use `@x-gmeow-*` language tags whose
/// subtag exceeds BCP-47's 8-char limit (e.g. `@x-gmeow-afrikaans`); the strict
/// parser rejects the entire file on these, which would make the real ontology
/// un-validatable. Lenient parsing skips that check so the data ingests. See #597.
///
/// # Errors
///
/// Returns an error string if either graph fails to parse.
pub fn validate_graphs(data_nt: &str, shapes_ttl: &str) -> Result<ValidationReport, String> {
    let data = Store::new().map_err(|e| format!("data store creation failed: {e}"))?;
    if !data_nt.is_empty() {
        data.load_from_reader(
            RdfParser::from_format(RdfFormat::NTriples).lenient(),
            data_nt.as_bytes(),
        )
        .map_err(|e| format!("N-Triples parse error: {e}"))?;
    }

    let shapes_store = Store::new().map_err(|e| format!("shapes store creation failed: {e}"))?;
    if !shapes_ttl.is_empty() {
        shapes_store
            .load_from_reader(
                RdfParser::from_format(RdfFormat::Turtle).lenient(),
                shapes_ttl.as_bytes(),
            )
            .map_err(|e| format!("Turtle parse error: {e}"))?;
    }

    let shapes = crate::shapes::from_store(&shapes_store)?;
    Ok(validate(&data, &shapes))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Severity;
    use crate::shapes::Shapes;

    const PREFIXES: &str = r#"
        @prefix sh:   <http://www.w3.org/ns/shacl#> .
        @prefix ex:   <http://example.org/ns#> .
        @prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
    "#;

    fn load_data_nt(nt: &str) -> Store {
        let store = Store::new().unwrap();
        if !nt.is_empty() {
            store
                .load_from_reader(RdfFormat::NTriples, nt.as_bytes())
                .unwrap();
        }
        store
    }

    fn load_shapes_ttl(ttl: &str) -> Shapes {
        let store = Store::new().unwrap();
        store
            .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
            .unwrap();
        crate::shapes::from_store(&store).expect("shapes parse must succeed")
    }

    // ── Pre-existing tests ─────────────────────────────────────────────────────

    #[test]
    fn empty_inputs_return_conforming_report() {
        let report = validate_graphs("", "").expect("empty inputs must not error");
        assert!(report.conforms, "empty report must conform");
        assert!(
            report.results.is_empty(),
            "empty report must have no results"
        );
    }

    #[test]
    fn validate_stub_always_conforms() {
        let data = Store::new().unwrap();
        let shapes = Shapes::default();
        let report = validate(&data, &shapes);
        assert!(report.conforms);
        assert!(report.results.is_empty());
    }

    // ── Task 4 tests ───────────────────────────────────────────────────────────

    // Test 1: targetClass + minCount — violating case (no ex:name on ex:alice)
    #[test]
    fn target_class_min_count_violating() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // ex:alice is a Person but has no ex:name
        let data_nt = "<http://example.org/ns#alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(!report.conforms, "must NOT conform: alice has no ex:name");
        assert_eq!(report.results.len(), 1, "exactly one result expected");
        let r = &report.results[0];
        assert!(
            r.source_constraint_component.as_str().contains("MinCount"),
            "component must be MinCountConstraintComponent, got {}",
            r.source_constraint_component.as_str()
        );
        assert_eq!(
            r.focus_node.to_string(),
            "<http://example.org/ns#alice>",
            "focus node must be ex:alice"
        );
    }

    // Test 2: conforming case — adding ex:name makes it pass
    #[test]
    fn target_class_min_count_conforming() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        let data_nt = concat!(
            "<http://example.org/ns#alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n",
            "<http://example.org/ns#alice> <http://example.org/ns#name> \"Alice\" .\n"
        );

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(report.conforms, "must conform: alice now has ex:name");
        assert!(report.results.is_empty(), "zero results expected");
    }

    // Test 3a: targetSubjectsOf — shape targets subjects of ex:knows
    #[test]
    fn target_subjects_of() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:KnowerShape a sh:NodeShape ;
                sh:targetSubjectsOf ex:knows ;
                sh:property [
                    sh:path ex:label ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // ex:alice knows ex:bob, but alice has no ex:label
        let data_nt = "<http://example.org/ns#alice> <http://example.org/ns#knows> <http://example.org/ns#bob> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(
            !report.conforms,
            "alice (subject of knows) must be a focus node and fail"
        );
        assert_eq!(report.results.len(), 1);
        assert_eq!(
            report.results[0].focus_node.to_string(),
            "<http://example.org/ns#alice>"
        );
    }

    // Test 3b: targetObjectsOf — shape targets objects of ex:knows
    #[test]
    fn target_objects_of() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:KnownShape a sh:NodeShape ;
                sh:targetObjectsOf ex:knows ;
                sh:property [
                    sh:path ex:label ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // ex:alice knows ex:bob, bob has no ex:label
        let data_nt = "<http://example.org/ns#alice> <http://example.org/ns#knows> <http://example.org/ns#bob> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(
            !report.conforms,
            "bob (object of knows) must be a focus node and fail"
        );
        assert_eq!(report.results.len(), 1);
        assert_eq!(
            report.results[0].focus_node.to_string(),
            "<http://example.org/ns#bob>"
        );
    }

    // Test 4: sh:targetClass honors ASSERTED rdfs:subClassOf (SHACL §4.2.5).
    // This is NOT OWL inference — the subclass edge is asserted in the data; we
    // read it, materialize nothing. (Inverted from the former no-subclass
    // contract; see #599.)
    #[test]
    fn target_class_honors_asserted_subclass() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // ex:bob is typed ex:Employee, and ex:Employee rdfs:subClassOf ex:Person
        // is ASSERTED → bob is a SHACL instance of ex:Person → it is a focus node
        // and, lacking ex:name, violates sh:minCount.
        let data_nt = concat!(
            "<http://example.org/ns#bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Employee> .\n",
            "<http://example.org/ns#Employee> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/ns#Person> .\n",
        );

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(
            !report.conforms,
            "ex:bob IS a focus node via asserted Employee ⊑ Person; report: {report:?}"
        );
        assert_eq!(report.results.len(), 1);
        assert_eq!(
            report.results[0].focus_node.to_string(),
            "<http://example.org/ns#bob>"
        );
    }

    // Test 4b: a class with NO asserted subClassOf edge is not reached — we
    // honor asserted edges only, inventing none.
    #[test]
    fn target_class_unasserted_subclass_not_reached() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [ sh:path ex:name ; sh:minCount 1 ; ] .
            "#
        );
        // ex:carol is an ex:Robot; no ex:Robot rdfs:subClassOf ex:Person triple
        // exists → carol is not a Person-instance → conforms.
        let data_nt = "<http://example.org/ns#carol> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Robot> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(
            report.conforms,
            "carol must NOT be reached without an asserted subClassOf edge; report: {report:?}"
        );
    }

    // Test 5: deactivated shape produces no results even with violating data
    #[test]
    fn deactivated_shape_produces_no_results() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:deactivated true ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // alice is a Person with no ex:name — would fail if shape were active
        let data_nt = "<http://example.org/ns#alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(
            report.conforms,
            "deactivated shape must produce no results; report: {report:?}"
        );
        assert!(report.results.is_empty());
    }

    // Test 6: determinism — two runs on the same input yield identical results
    #[test]
    fn determinism_same_results_twice() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // Two persons, both missing ex:name, to get multiple results
        let data_nt = concat!(
            "<http://example.org/ns#alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n",
            "<http://example.org/ns#bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n",
        );

        let data1 = load_data_nt(data_nt);
        let shapes1 = load_shapes_ttl(&shapes_ttl);
        let report1 = validate(&data1, &shapes1);

        let data2 = load_data_nt(data_nt);
        let shapes2 = load_shapes_ttl(&shapes_ttl);
        let report2 = validate(&data2, &shapes2);

        assert_eq!(report1.conforms, report2.conforms);
        assert_eq!(report1.results.len(), report2.results.len());

        // Compare result tuples in order (not just as a set) to confirm stable sort.
        let tuples1: Vec<_> = report1
            .results
            .iter()
            .map(|r| {
                (
                    r.focus_node.to_string(),
                    r.source_constraint_component.to_string(),
                    r.source_shape.to_string(),
                    r.result_path.as_ref().map(|t| t.to_string()),
                    r.value.as_ref().map(|t| t.to_string()),
                    r.severity,
                )
            })
            .collect();
        let tuples2: Vec<_> = report2
            .results
            .iter()
            .map(|r| {
                (
                    r.focus_node.to_string(),
                    r.source_constraint_component.to_string(),
                    r.source_shape.to_string(),
                    r.result_path.as_ref().map(|t| t.to_string()),
                    r.value.as_ref().map(|t| t.to_string()),
                    r.severity,
                )
            })
            .collect();

        assert_eq!(
            tuples1, tuples2,
            "result ordering must be identical across runs"
        );

        // Also verify to_ntriples() is identical
        assert_eq!(
            report1.to_ntriples(),
            report2.to_ntriples(),
            "N-Triples output must be identical across runs"
        );
    }

    // Bonus: targetNode explicit
    #[test]
    fn target_node_explicit() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:AliceShape a sh:NodeShape ;
                sh:targetNode ex:alice ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] .
            "#
        );
        // ex:alice explicitly targeted; no ex:name triple
        let data_nt = "<http://example.org/ns#alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Person> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        assert!(!report.conforms, "ex:alice has no ex:name → must fail");
        assert_eq!(report.results.len(), 1);
        assert_eq!(
            report.results[0].focus_node.to_string(),
            "<http://example.org/ns#alice>"
        );
    }

    // Severity-independence: a Warning result makes conforms=false
    #[test]
    fn warning_result_makes_report_non_conforming() {
        let shapes_ttl = format!(
            r#"{PREFIXES}
            ex:WarnShape a sh:NodeShape ;
                sh:targetClass ex:Thing ;
                sh:severity sh:Warning ;
                sh:property [
                    sh:path ex:label ;
                    sh:minCount 1 ;
                    sh:severity sh:Warning ;
                ] .
            "#
        );
        let data_nt = "<http://example.org/ns#x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Thing> .\n";

        let data = load_data_nt(data_nt);
        let shapes = load_shapes_ttl(&shapes_ttl);
        let report = validate(&data, &shapes);

        // SHACL: conforms is false if ANY result exists, regardless of severity
        assert!(
            !report.conforms,
            "Warning results must still make conforms=false"
        );
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].severity, Severity::Warning);
    }
}
