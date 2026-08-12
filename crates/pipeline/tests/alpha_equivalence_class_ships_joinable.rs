// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The α-equivalence class is joinable OFF-LINE, from a shipped artifact alone.
//!
//! `math:alphaEquivalenceClass` exists so that "these two expressions are the same expression"
//! is a **graph node a consumer can JOIN on**, not a digest literal to string-compare. That
//! promise is only kept if the node reaches something a consumer actually holds. Before this,
//! the edge was spliced into an in-process reasoned graph that no artifact carried: a consumer
//! of `gmeow.gts` or `generated/` saw only a `Severity::Note` carrying the class IRI as text,
//! which is not joinable RDF.
//!
//! This test is the consumer-side proof of the closed gap, and it is deliberately staged in two
//! halves that do not share a value:
//!
//! 1. **Producer half** — run the REAL production emitter
//!    ([`gmeow_pipeline::stages::reason::reason_over_dataset`], the one `stage-reason` calls)
//!    over the REAL committed demonstrator corpus, loaded through the REAL loader
//!    ([`gmeow_pipeline::stages::source_load::examples_graph`], the one `stage-source-load`
//!    calls). Its `closure` string is byte-for-byte what the stage writes to
//!    `generated/logic/inferred-closure.rdf12.ttl` and folds into the bundle's default graph.
//! 2. **Consumer half** — throw the producer's world away and keep only those BYTES. Parse
//!    them cold and answer the consumer's question with one ordinary SPARQL join. No gate, no
//!    reasoner, no `check_math_expression_findings`, no `Finding`, nothing from `crates/logic`
//!    at all: exactly what someone holding the file has.
//!
//! The witness is the shipped `alpha-equivalent-twins.ttl` pair — two expressions authored
//! independently, node for node, that denote the same product. Nothing in that file declares a
//! `math:structuralKey`; the ONLY thing that can bring them together is the derived class node.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue};

use gmeow_pipeline::stages::reason::reason_over_dataset;
use gmeow_pipeline::stages::source_load::{example_files, examples_graph};

/// The two independently-authored twins the shipped corpus carries.
const FIRST_TWIN: &str =
    "https://blackcatinformatics.ca/gmeow/examples/math/alpha-twins/firstProduct";
const SECOND_TWIN: &str =
    "https://blackcatinformatics.ca/gmeow/examples/math/alpha-twins/secondProduct";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The consumer's question, as one ordinary triple pattern join: which pairs of DISTINCT
/// expressions resolve to the SAME `math:AlphaEquivalenceClass` individual? The class node is
/// both the join key and a typed subject in its own right, so the query never touches a digest
/// literal — which is the whole point of the term.
const JOIN_QUERY: &str = "\
PREFIX math: <https://blackcatinformatics.ca/math/>
SELECT ?left ?right ?class WHERE {
  ?left  math:alphaEquivalenceClass ?class .
  ?right math:alphaEquivalenceClass ?class .
  ?class a math:AlphaEquivalenceClass .
  FILTER(STR(?left) < STR(?right))
}";

fn iri_of(value: &TermValue) -> String {
    match value {
        TermValue::Iri(iri) => iri.clone(),
        other => panic!("expected an IRI binding, got {other:?}"),
    }
}

/// Produce the committed-closure BYTES exactly as `stage-reason` does, over the committed
/// demonstrator corpus exactly as `stage-source-load` loads it.
fn shipped_closure_bytes() -> String {
    let root = repo_root();
    let files = example_files(&root).expect("discover every slice's examples/*.ttl");
    assert!(
        files
            .iter()
            .any(|p| p.ends_with("alpha-equivalent-twins.ttl")),
        "the loader must reach the shipped twin demonstrator"
    );
    let corpus = examples_graph(&files).expect("load the demonstrator corpus");
    reason_over_dataset(corpus.as_ref())
        .expect("the production reasoning pass runs over the corpus")
        .closure
}

#[test]
fn a_consumer_joins_two_alpha_equivalent_expressions_on_the_shipped_class_node() {
    let closure_bytes = shipped_closure_bytes();

    // ── Consumer half. Only `closure_bytes` crosses this line. ──
    let shipped: Arc<RdfDataset> =
        purrdf::parse_dataset(closure_bytes.as_bytes(), "text/turtle", None)
            .expect("a consumer parses the committed closure artifact");
    let engine = NativeSparqlEngine::new();
    let SparqlResult::Solutions {
        variables, rows, ..
    } = engine
        .query(
            &shipped,
            SparqlRequest {
                query: JOIN_QUERY,
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("the join evaluates over the shipped bytes alone")
    else {
        panic!("a SELECT must return solutions");
    };

    let column = |name: &str| {
        variables
            .iter()
            .position(|variable| variable == name)
            .unwrap_or_else(|| panic!("the SELECT projects ?{name}"))
    };
    let (left_col, right_col, class_col) = (column("left"), column("right"), column("class"));
    let mut pairs: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let get = |index: usize, name: &str| {
                iri_of(
                    row[index]
                        .as_ref()
                        .unwrap_or_else(|| panic!("solution is missing ?{name}")),
                )
            };
            (
                get(left_col, "left"),
                get(right_col, "right"),
                get(class_col, "class"),
            )
        })
        .collect();
    pairs.sort();

    println!("consumer join over the shipped closure bytes alone:");
    for (left, right, class) in &pairs {
        println!("  {left}\n  {right}\n  ↳ share {class}\n");
    }

    let twins = pairs
        .iter()
        .find(|(left, right, _)| left == FIRST_TWIN && right == SECOND_TWIN)
        .unwrap_or_else(|| {
            panic!(
                "the two independently-authored twins must join on ONE shipped class node; \
                 pairs found: {pairs:?}"
            )
        });
    assert!(
        twins
            .2
            .starts_with("https://blackcatinformatics.ca/math/alphaClass/"),
        "the join key is a content-addressed math:AlphaEquivalenceClass individual, got {}",
        twins.2
    );
}

/// Shipping the edge in the closure puts it in `gmeow.gts`'s DEFAULT graph, which
/// `gmeow_logic::reasoning_graphs::project_object_level_edb` admits — so a consumer
/// re-deriving reasoning from a shipped bundle (`gmeow verify --deep`, `validate --deep`)
/// feeds the previous run's α edges back in as EDB. That must be a FIXED POINT, not a
/// generation-on-generation drift: an identity that changed because it was published would be
/// no identity at all.
///
/// This is the guard. Reason a second time over the corpus UNIONED with the first run's own
/// closure and require a byte-identical α section. It holds structurally — an
/// `math:AlphaEquivalenceClass` individual is not one of the five types the expression grammar
/// admits as a root, so the published edges add no expression to decide — and this pins that
/// property against any future widening of the root population.
#[test]
fn republishing_the_closure_into_the_edb_is_a_fixed_point() {
    let root = repo_root();
    let corpus = examples_graph(&example_files(&root).expect("discover examples"))
        .expect("load the demonstrator corpus");
    let first = reason_over_dataset(corpus.as_ref()).expect("first generation");
    let first_section = alpha_section(&first.closure);
    assert!(
        !first_section.is_empty(),
        "the first generation must decide at least one α-equivalence class"
    );

    let republished = purrdf::parse_dataset(first.closure.as_bytes(), "text/turtle", None)
        .expect("parse the shipped closure back");
    let second_generation_edb =
        Arc::new(RdfDataset::union(&[corpus.as_ref(), republished.as_ref()]));
    let second = reason_over_dataset(second_generation_edb.as_ref()).expect("second generation");

    assert_eq!(
        alpha_section(&second.closure),
        first_section,
        "re-reasoning over a bundle that already carries the α edges must decide the IDENTICAL \
         classes — publishing an identity may not change it"
    );
}

/// The `math:alphaEquivalenceClass` lines of a closure document, in emission order.
fn alpha_section(closure: &str) -> Vec<&str> {
    closure
        .lines()
        .filter(|line| line.contains("/math/alphaEquivalenceClass>"))
        .collect()
}

/// The joinable node is REAL RDF in the artifact, not prose in a report. Read straight off the
/// parsed bytes with no query engine: the edge is a triple whose object is a typed individual.
#[test]
fn the_shipped_artifact_carries_the_class_as_rdf_not_as_a_message() {
    const ALPHA_EDGE: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    const ALPHA_TYPE: &str = "https://blackcatinformatics.ca/math/AlphaEquivalenceClass";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    let shipped = purrdf::parse_dataset(shipped_closure_bytes().as_bytes(), "text/turtle", None)
        .expect("a consumer parses the committed closure artifact");

    let quads: Vec<purrdf::RdfQuad> = shipped.owned_quads().collect();
    let class_of = |subject: &str| -> Option<String> {
        quads.iter().find_map(
            |quad| match (&quad.subject, quad.predicate.as_str(), &quad.object) {
                (purrdf::RdfTerm::Iri(s), ALPHA_EDGE, purrdf::RdfTerm::Iri(o)) if s == subject => {
                    Some(o.clone())
                }
                _ => None,
            },
        )
    };
    let first = class_of(FIRST_TWIN).expect("the first twin carries a math:alphaEquivalenceClass");
    let second =
        class_of(SECOND_TWIN).expect("the second twin carries a math:alphaEquivalenceClass");
    assert_eq!(
        first, second,
        "two α-equivalent expressions must resolve to the IDENTICAL individual"
    );
    assert!(
        quads.iter().any(|quad| matches!(
            (&quad.subject, quad.predicate.as_str(), &quad.object),
            (purrdf::RdfTerm::Iri(s), RDF_TYPE, purrdf::RdfTerm::Iri(o))
                if s == &first && o == ALPHA_TYPE
        )),
        "the shared class individual must be typed math:AlphaEquivalenceClass in the artifact, \
         so a consumer can select the identity nodes without knowing the IRI convention"
    );
}
