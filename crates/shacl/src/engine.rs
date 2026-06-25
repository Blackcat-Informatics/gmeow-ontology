// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL Core validation engine.
//!
//! `validate` is the top-level entry point.  Resolves focus nodes for every
//! non-deactivated node shape, runs all constraints, and assembles a
//! deterministically-sorted [`ValidationReport`].

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::Term;
use oxigraph::store::Store;

use gmeow_rdf::ir::RdfDatasetBuilder;
use gmeow_rdf::{RdfDataset, RdfQuad, RdfTerm};

use crate::data::{GraphFilter, ShaclDataGraph};
use crate::model::{rdf, rdfs};
use crate::report::ValidationReport;
use crate::shapes::{Shapes, Target};

// ── Target resolution helpers ─────────────────────────────────────────────────

/// Build the oxigraph `Term` pattern for a predicate IRI.
fn predicate_pattern(pred: &oxigraph::model::NamedNode) -> Term {
    Term::NamedNode(pred.clone())
}

/// Collect distinct subjects of `(?, pred, ?)` across all graphs.
fn subjects_of<G: ShaclDataGraph>(data: &G, pred: &oxigraph::model::NamedNode) -> Vec<Term> {
    let pred_term = predicate_pattern(pred);
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for quad in data.quads_for_pattern(None, Some(&pred_term), None, GraphFilter::AnyGraph) {
        let t = Term::from(quad.subject);
        if seen.insert(t.clone()) {
            result.push(t);
        }
    }
    result
}

/// Collect distinct objects of `(?, pred, ?)` across all graphs.
fn objects_of<G: ShaclDataGraph>(data: &G, pred: &oxigraph::model::NamedNode) -> Vec<Term> {
    let pred_term = predicate_pattern(pred);
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for quad in data.quads_for_pattern(None, Some(&pred_term), None, GraphFilter::AnyGraph) {
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
pub(crate) fn subclass_closure<G: ShaclDataGraph>(
    data: &G,
    class_iri: &oxigraph::model::NamedNode,
) -> std::collections::HashSet<Term> {
    let sub_class_of = Term::NamedNode(rdfs::SUB_CLASS_OF.into_owned());
    let mut closure = std::collections::HashSet::new();
    let start = Term::NamedNode(class_iri.clone());
    closure.insert(start.clone());
    let mut frontier = vec![start];
    while let Some(superclass) = frontier.pop() {
        // Any X with `X rdfs:subClassOf superclass` is a subclass to descend into.
        for quad in data.quads_for_pattern(
            None,
            Some(&sub_class_of),
            Some(&superclass),
            GraphFilter::AnyGraph,
        ) {
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
///
/// `closure_memo` is a per-`validate_with` call cache keyed by class IRI; the
/// subclass BFS is performed at most once per distinct class across all shapes.
fn instances_of_class<G: ShaclDataGraph>(
    data: &G,
    class_iri: &oxigraph::model::NamedNode,
    closure_memo: &mut std::collections::HashMap<
        oxigraph::model::NamedNode,
        std::collections::HashSet<Term>,
    >,
) -> Vec<Term> {
    let rdf_type = Term::NamedNode(rdf::TYPE.into_owned());
    // Compute the subclass closure at most once per class IRI; clone the key only
    // on a memo miss (insert requires ownership), never on a hit.
    if !closure_memo.contains_key(class_iri) {
        let closure = subclass_closure(data, class_iri);
        closure_memo.insert(class_iri.clone(), closure);
    }
    let classes = &closure_memo[class_iri];
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    // Iterate the memoized set by reference — never clone the whole HashSet per call.
    for class in classes {
        for quad in
            data.quads_for_pattern(None, Some(&rdf_type), Some(class), GraphFilter::AnyGraph)
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
///
/// `closure_memo` is threaded through to [`instances_of_class`] so the subclass
/// BFS is performed at most once per class IRI per `validate_with` call.
fn resolve_focus_nodes<G: ShaclDataGraph>(
    data: &G,
    targets: &[Target],
    closure_memo: &mut std::collections::HashMap<
        oxigraph::model::NamedNode,
        std::collections::HashSet<Term>,
    >,
) -> Vec<Term> {
    let mut seen = std::collections::HashSet::new();
    let mut nodes: Vec<Term> = Vec::new();

    for target in targets {
        let candidates: Vec<Term> = match target {
            Target::Class(class_iri) => instances_of_class(data, class_iri, closure_memo),
            Target::SubjectsOf(pred) => subjects_of(data, pred),
            Target::ObjectsOf(pred) => objects_of(data, pred),
            Target::Node(t) => vec![t.clone()],
            Target::ImplicitClass(t) => {
                // Same semantics as Class: subjects of (?, rdf:type, t)
                if let Term::NamedNode(nn) = t {
                    instances_of_class(data, nn, closure_memo)
                } else {
                    vec![]
                }
            }
            // sh:SPARQLTarget: execute the pre-parsed SELECT and collect ?this.
            // SHACL-SPARQL needs an oxigraph query engine; obtain it via the
            // data graph's SPARQL store (cheap borrow for Store, one-time
            // materialization for the IR backend).
            // SELECT-form is enforced at shape-load (shapes.rs rejects non-SELECT), so the only Err here is an impossible-by-construction runtime failure; .expect() documents that invariant.
            Target::Sparql { parsed, .. } => {
                crate::sparql::eval_target(&data.sparql_store(), &parsed.0).expect(
                    "SPARQLTarget query execution failed (parseability checked at parse time)",
                )
            }
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
    validate_with(data, shapes)
}

/// Validate any [`ShaclDataGraph`] backend against `shapes`.
///
/// This is the single, backend-generic engine core: [`validate`] (oxigraph
/// [`Store`]) and [`validate_dataset`] (the IR backend) both bottom out here, so
/// conformance is identical by construction across backends.
pub fn validate_with<G: ShaclDataGraph>(data: &G, shapes: &Shapes) -> ValidationReport {
    let mut all_results = Vec::new();
    // Per-call subclass-closure memo: keyed by class IRI, value is the full
    // transitive closure of asserted rdfs:subClassOf edges below that class.
    // Shared across all shapes in this validation run; each distinct class IRI
    // is BFS-walked AT MOST ONCE regardless of how many shapes target it.
    let mut closure_memo: std::collections::HashMap<
        oxigraph::model::NamedNode,
        std::collections::HashSet<Term>,
    > = std::collections::HashMap::new();

    for shape in &shapes.node_shapes {
        if shape.deactivated {
            continue;
        }

        let focus_nodes = resolve_focus_nodes(data, &shape.targets, &mut closure_memo);

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

/// Validate a frozen [`gmeow_rdf::RdfDataset`] against parsed SHACL shapes, IR-natively.
///
/// The generic engine reads pattern lookups DIRECTLY from a SHACL projection of
/// the IR — there is no whole-store oxigraph materialization on this path
/// (SHACL-SPARQL constraints, if any, lazily materialize a query store on demand
/// only). Named graphs are flattened so GTS bundle partitions behave like the
/// repository's Turtle source merge, which loads all inputs into one default graph.
///
/// # Errors
///
/// Returns an error string if the SHACL projection cannot be frozen into the IR.
pub fn validate_dataset(data: &RdfDataset, shapes: &Shapes) -> Result<ValidationReport, String> {
    let dataset = shacl_dataset_from_dataset(data)?;
    // The engine reads pattern lookups directly from the frozen IR, with no
    // whole-store oxigraph materialization. SHACL-SPARQL paths materialize lazily and
    // — via `CachedIrDataGraph` — AT MOST ONCE per validation, shared across every
    // `sh:sparql` target/constraint (rather than re-materializing the whole store per
    // SPARQL call as the bare `&RdfDataset` backend would).
    let reference = crate::data::CachedIrDataGraph::new(&dataset);
    Ok(validate_with(&reference, shapes))
}

/// Build a SHACL-projection dataset from the source [`RdfDataset`], flattening
/// every quad into the default graph and materializing reifier bindings as
/// `rdf:reifies` triples and statement annotations as plain triples.
fn shacl_dataset_from_dataset(
    data: &RdfDataset,
) -> Result<std::sync::Arc<gmeow_rdf::RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();

    for mut quad in data.owned_quads() {
        // FlattenToDefaultGraph: drop the source graph name.
        quad.graph_name = None;
        builder.push_owned_quad(&quad);
    }

    // Reifiers → `(reifier, rdf:reifies, <<triple>>)` triples.
    for reifier in data.owned_reifiers() {
        builder.push_owned_quad(&RdfQuad::new(
            reifier.reifier,
            RDF_REIFIES,
            RdfTerm::triple(reifier.statement),
        ));
    }

    // Annotations → `(reifier, predicate, object)` triples.
    for annotation in data.owned_annotations() {
        builder.push_owned_quad(&RdfQuad::new(
            annotation.reifier,
            annotation.predicate,
            annotation.object,
        ));
    }

    builder.freeze().map_err(|e| e.to_string())
}

/// The `rdf:reifies` predicate IRI, used to project reifier bindings into the
/// quad table so SHACL's reifier-shape lookups can find them.
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Parse a SHACL shapes graph from a Turtle string.
///
/// Creates an in-memory store, loads the shapes graph with prefix extraction,
/// and parses it into a reusable [`Shapes`] model. The parsed model can be
/// shared across multiple data graphs via [`validate`], eliminating the cost of
/// re-parsing shapes for every validation phase.
///
/// # Errors
///
/// Returns an error string if the shapes Turtle fails to parse or contains
/// unsupported SHACL constructs.
pub fn parse_shapes(shapes_ttl: &str) -> Result<Shapes, String> {
    let shapes_store = Store::new().map_err(|e| format!("shapes store creation failed: {e}"))?;
    // Drive the Turtle parser by iterator (rather than Store::load_from_reader) so
    // we can recover the document's @prefix map — oxigraph stores do not retain
    // prefixes, but SHACL-AF sh:select queries (and pySHACL) rely on them. See #578.
    let mut doc_prefixes: Vec<(String, String)> = Vec::new();
    if !shapes_ttl.is_empty() {
        let mut parser = RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(shapes_ttl.as_bytes());
        // Accumulate EVERY syntax error rather than short-circuiting on the first.
        // oxttl drives an error-recovery state (the toolkit transitions the parser
        // into `error_recovery_state` after a malformed statement), so a single
        // pass keeps yielding past the first break and surfaces all of them. A
        // SHACL author then sees the complete list in one report instead of the
        // fix-one-rerun-find-the-next loop. See #828 (item 4). Well-formed quads
        // are still inserted, but a non-empty error set is a hard parse failure.
        let mut errors: Vec<String> = Vec::new();
        for quad in parser.by_ref() {
            match quad {
                Ok(quad) => {
                    shapes_store
                        .insert(&quad)
                        .map_err(|e| format!("shapes store insert failed: {e}"))?;
                }
                Err(e) => errors.push(format!("Turtle parse error: {e}")),
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        doc_prefixes = parser
            .prefixes()
            .map(|(prefix, namespace)| (prefix.to_owned(), namespace.to_owned()))
            .collect();
    }

    crate::shapes::from_store_with_prefixes(&shapes_store, &doc_prefixes)
}

/// Validate data (N-Triples) against shapes (Turtle), returning a [`ValidationReport`].
///
/// Creates an in-memory data store, loads the data graph, parses shapes once
/// via [`parse_shapes`], and delegates to [`validate`].
///
/// The data graph is loaded with the **lenient** RDF parser. A validator must be
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
        // Iterate (rather than `load_from_reader`, which collapses to the FIRST
        // error) so every malformed N-Triples line is reported in one pass —
        // same multi-error contract as `parse_shapes`. See #828 (item 4).
        let mut errors: Vec<String> = Vec::new();
        for quad in RdfParser::from_format(RdfFormat::NTriples)
            .lenient()
            .for_reader(data_nt.as_bytes())
        {
            match quad {
                Ok(quad) => {
                    data.insert(&quad)
                        .map_err(|e| format!("data store insert failed: {e}"))?;
                }
                Err(e) => errors.push(format!("N-Triples parse error: {e}")),
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
    }

    let shapes = parse_shapes(shapes_ttl)?;
    Ok(validate(&data, &shapes))
}

/// Validate a frozen [`gmeow_rdf::RdfDataset`] against a Turtle SHACL shapes graph.
///
/// # Errors
///
/// Returns an error string if the shapes graph fails to parse or if the SHACL
/// projection cannot be frozen.
pub fn validate_dataset_graphs(
    data: &RdfDataset,
    shapes_ttl: &str,
) -> Result<ValidationReport, String> {
    let shapes = parse_shapes(shapes_ttl)?;
    validate_dataset(data, &shapes)
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

    // ── Multi-error syntax reporting (#828 item 4) ─────────────────────────────

    #[test]
    fn parse_shapes_reports_all_syntax_errors() {
        // Two independently-malformed Turtle STATEMENTS, separated by a valid one.
        // oxttl recovers at statement granularity (resync on the `.` terminator),
        // so BOTH errors must surface in one report — proving the accumulator is
        // real, not a one-element surface. (A lexer-level break such as an
        // unterminated string literal instead consumes to EOF and yields a single
        // error; that is correct, not a regression. The recoverable case below is
        // what proves multi-error reporting works.) If this regresses to a single
        // error on recoverable input, item 4's premise has broken.
        let bad = concat!(
            "@prefix ex: <http://example.org/ns#> .\n",
            "ex:a ex:p .\n",                // missing object → recoverable error
            "ex:b ex:q ex:c .\n",           // valid, between the two errors
            "ex:d ex:r ex:s ex:t ex:u .\n", // too many terms → recoverable error
        );
        let err = parse_shapes(bad).expect_err("malformed Turtle must error");
        let n = err.matches("Turtle parse error").count();
        assert!(
            n >= 2,
            "expected >=2 accumulated Turtle errors, got {n}:\n{err}"
        );
    }

    #[test]
    fn validate_graphs_reports_all_data_syntax_errors() {
        // Multiple malformed N-Triples lines must all be reported in one pass
        // rather than short-circuiting on the first (the `load_from_reader`
        // behavior item 4 replaced).
        let bad_data = concat!(
            "this is not a triple\n",
            "<http://example.org/s> <http://example.org/p> .\n",
            "neither is this\n",
        );
        let err = validate_graphs(bad_data, "").expect_err("malformed N-Triples must error");
        let n = err.matches("N-Triples parse error").count();
        assert!(
            n >= 2,
            "expected >=2 accumulated N-Triples errors, got {n}:\n{err}"
        );
    }

    #[test]
    fn parse_shapes_clean_input_still_succeeds() {
        // The accumulator must not turn a well-formed document into a failure.
        let ok = format!("{PREFIXES}\nex:Shape a sh:NodeShape ; sh:targetClass ex:Thing .\n");
        parse_shapes(&ok).expect("well-formed shapes must parse");
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
    fn dataset_entrypoint_validates_gts_backed_graph() {
        let mut builder = RdfDatasetBuilder::new();
        let ids: Vec<_> = [
            "http://example.org/ns#a",
            "http://example.org/ns#p",
            "http://example.org/ns#b",
        ]
        .into_iter()
        .map(|value| builder.intern_iri(value.to_owned()))
        .collect();
        builder.push_quad(ids[0], ids[1], ids[2], None);
        let dataset = builder.freeze().expect("valid test dataset");

        let shapes_ttl = format!(
            "{PREFIXES}
            ex:Shape a sh:NodeShape ;
                sh:targetNode ex:a ;
                sh:property [
                    sh:path ex:missing ;
                    sh:minCount 1 ;
                ] ."
        );
        let report = validate_dataset_graphs(dataset.as_ref(), &shapes_ttl)
            .expect("GTS-backed store should validate");
        assert!(!report.conforms, "missing property must violate the shape");
        assert_eq!(report.results.len(), 1);
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
