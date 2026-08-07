// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! The `math:` expression-identity gate, driven over the substrate PRODUCTION uses.
//!
//! ## Why this file exists
//! The gate's other tests parse Turtle straight into an `RdfDataset` and call
//! [`math_expression_structural_keys`] on it. Production does not: `gmeow validate --deep`
//! and the pipeline's verify stage both REASON first and hand
//! [`check_math_expression_findings`] the reasoned closure. Those two substrates are not
//! interchangeable, and the difference is not theoretical — it hid a live defect.
//!
//! The DL existential chase lowered `minQualifiedCardinality 1` on `math:operator` with its
//! `owl:Thing` qualifier intact. That put a `?witness rdf:type owl:Thing` conjunct in the rule
//! head, nothing asserts `rdf:type owl:Thing` for anything, so the restricted chase's
//! head-satisfaction probe could never match and a witness was invented on every firing. Every
//! composite expression came back with one more `math:operator` than it asserted, so the gate
//! rejected the repository's own shipped example and `math:StructuralKeyDrift` could never fire
//! for an application or a binding — only for leaf literals. Every parse-only test passed
//! throughout, because a bare parse runs no chase.
//!
//! So: assert the gate's contract on the REASONED graph, over the shipped example, not on a
//! substrate that cannot express the failure.

use gmeow_logic::math_expression::check_math_expression_findings;
use gmeow_logic::reason::reason_all;
use gmeow_logic::verify::{ReasonedGraphOutcome, materialize_reasoned_graph};

const REFERENCE_AST_ACT: &str =
    include_str!("../../../slices/grounding/math/examples/reference-ast-act.ttl");

/// The math slice TBox. The example alone carries no restrictions, so reasoning over it in
/// isolation chases nothing and CANNOT express the defect this file exists to pin — the
/// `minQualifiedCardinality 1` on `math:operator` that drove the phantom filler lives here, not
/// in the instance data. Production always reasons over TBox + data together; so does this test.
const MATH_MODULE: &str = include_str!("../../../slices/grounding/math/module.ttl");

/// The SHIPPED twin example — two independently authored copies of one expression that share
/// only their symbols. Folded into the example corpus so the bundle's own population can
/// exercise the identity headline, which the reference example cannot: it reuses one pair of
/// occurrence nodes across both of its expressions.
const ALPHA_TWINS: &str =
    include_str!("../../../slices/grounding/math/examples/alpha-equivalent-twins.ttl");

/// The asserted EDB production hands the grammar half, beside the closure it hands the rest.
fn asserted(turtle: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    let combined = format!("{MATH_MODULE}\n{turtle}");
    purrdf::parse_dataset(combined.as_bytes(), "text/turtle", None).expect("parse fixture")
}

/// Reason over the math TBox UNIONED with `turtle`, exactly as production does.
fn reasoned(turtle: &str) -> gmeow_logic::verify::ReasonedGraph {
    let edb = asserted(turtle);
    let result = reason_all(&edb).expect("native reasoning succeeds on the fixture");
    match materialize_reasoned_graph(&edb, &result).expect("materialize the reasoned graph") {
        ReasonedGraphOutcome::Ready(graph) => graph,
        ReasonedGraphOutcome::IncompleteClosure(findings) => panic!(
            "the reference example must close completely; DL coverage gap: {:?}",
            findings
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
        ),
    }
}

/// The shipped example is CLEAN through the reasoned substrate.
///
/// It declares real, recomputed `math:structuralKey` values, so any finding here means the gate
/// disagrees with a key the repository ships as correct. Before the chase repair this failed with
/// `math:ApplicationOperatorCardinality` — "carries 2 `math:operator` values; exactly one is
/// required" — against a file that asserts exactly one.
#[test]
fn the_shipped_reference_example_raises_no_expression_identity_finding_when_reasoned() {
    let graph = reasoned(REFERENCE_AST_ACT);
    // Only VIOLATIONS matter here. The gate also emits one positive Note per decided root
    // (the expression's alpha-equivalence class), which is the population signal, not a fault.
    let findings: Vec<_> =
        check_math_expression_findings(&asserted(REFERENCE_AST_ACT), &graph.dataset)
            .into_iter()
            .filter(|f| f.severity != gmeow_errors::Severity::Note)
            .collect();
    assert!(
        findings.is_empty(),
        "the shipped reference example must be clean through the reasoned substrate, got: {:?}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
    );
}

/// Structural identity is IRI-independent on the reasoned substrate.
///
/// `ex:matrixProductAst` and `ex:matrixProductNormalForm` are the same expression — same operator,
/// same operand structure — under different subject IRIs and different `math:argumentSlot` IRIs.
/// The file declares ONE `math:structuralKey` value for both, and the test above proves the gate
/// accepts both, so the digest cannot be reading node identity. This pins that directly: a
/// content-addressed key that moved with the subject IRI would be a label, not a content key, and
/// the issue's "alpha-equivalent expressions intern to one key" clause would be false.
#[test]
fn structurally_identical_expressions_under_different_iris_share_one_key() {
    let declared: Vec<&str> = REFERENCE_AST_ACT
        .lines()
        .filter_map(|line| line.trim().strip_prefix("math:structuralKey \""))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert!(
        declared.len() >= 2,
        "the example must declare a key on BOTH structurally identical expressions, saw {declared:?}"
    );
    assert!(
        declared.windows(2).all(|w| w[0] == w[1]),
        "structurally identical expressions under different IRIs must share one key, saw {declared:?}"
    );

    // And the gate agrees with that shared key on the reasoned substrate — no VIOLATION;
    // the positive alpha-class notes are the gate's verdict, not a fault.
    let graph = reasoned(REFERENCE_AST_ACT);
    assert!(
        check_math_expression_findings(&asserted(REFERENCE_AST_ACT), &graph.dataset)
            .iter()
            .all(|f| f.severity == gmeow_errors::Severity::Note),
        "the shared key must survive the reasoned substrate"
    );
}

/// Two CONFORMING α-equivalent expressions resolve to ONE joinable node.
///
/// This is the deliverable `math:alphaEquivalenceClass` exists for, and it is the half that was
/// missing: the α-class IRI used to reach production only as a finding's `cited_iris` on the
/// DRIFT branch, so two WRONG expressions shared a node and two RIGHT ones never did — exactly
/// backwards for an identity edge. The gate now materializes the edge for every cleanly-lowered
/// root, so a consumer can JOIN on it rather than string-compare `math:structuralKey` literals.
#[test]
fn conforming_alpha_equivalent_expressions_share_one_materialized_class_node() {
    const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    let graph = reasoned(REFERENCE_AST_ACT);

    let mut by_root: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for quad in graph.dataset.flat_default_graph_quads() {
        if format!("{:?}", quad.p).contains(ALPHA_CLASS) {
            by_root.insert(format!("{:?}", quad.s), format!("{:?}", quad.o));
        }
    }
    assert!(
        by_root.len() >= 2,
        "the gate must materialize an alpha-equivalence class for each lowered root, saw {by_root:?}"
    );
    let distinct: std::collections::BTreeSet<&String> = by_root.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "the example's two structurally identical expressions must resolve to ONE class node, saw {by_root:?}"
    );
}

/// The gate's population over the SHIPPED math example corpus is non-empty.
///
/// A gate that decides nothing reports nothing, and "no findings" is indistinguishable from
/// "nothing to decide" by finding-count alone — so a vacuous population reads exactly like a
/// clean one. The math slice's examples reach the object-level reasoning EDB precisely so this
/// gate is non-vacuous over the shipped artifact; assert that positively, by counting the
/// identity edges the gate materializes, rather than inferring health from silence.
#[test]
fn the_gate_decides_a_non_empty_population_over_the_shipped_example_corpus() {
    const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    let graph = reasoned(REFERENCE_AST_ACT);
    let decided = graph
        .dataset
        .flat_default_graph_quads()
        .filter(|q| format!("{:?}", q.p).contains(ALPHA_CLASS))
        .count();
    assert!(
        decided > 0,
        "the expression-identity gate must DECIDE at least one root over the shipped example \
         corpus; a zero population makes every clean run vacuous and indistinguishable from a \
         genuinely passing one"
    );
}

/// Independently authored TWINS intern to one key ON THE REASONED SUBSTRATE.
///
/// The shipped reference example cannot express this: both of its expressions share one pair of
/// `math:SymbolReference` occurrence nodes, so the occurrence IRIs are held constant and a digest
/// keyed on them looks correct. This builds two expressions that share only their SYMBOLS and
/// name every wrapper differently — the case a content-addressed key exists for, and the case
/// under which the digest was previously a label.
#[test]
fn independently_authored_twins_over_shared_symbols_share_one_key_when_reasoned() {
    const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    let twins = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <https://example.org/twin/> .
ex:symL a math:MathematicalSymbol .
ex:symR a math:MathematicalSymbol .
ex:appA a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sA0 , ex:sA1 .
ex:sA0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refA0 .
ex:sA1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refA1 .
ex:refA0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refA1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
ex:appB a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sB0 , ex:sB1 .
ex:sB0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refB0 .
ex:sB1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refB1 .
ex:refB0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refB1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
"#;
    let graph = reasoned(twins);
    let mut classes: std::collections::BTreeMap<String, String> = Default::default();
    for quad in graph.dataset.flat_default_graph_quads() {
        if format!("{:?}", quad.p).contains(ALPHA_CLASS) {
            classes.insert(format!("{:?}", quad.s), format!("{:?}", quad.o));
        }
    }
    assert_eq!(
        classes.len(),
        2,
        "both twins must be decided by the gate, saw {classes:?}"
    );
    let distinct: std::collections::BTreeSet<&String> = classes.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "independently authored twins over the same symbols must share ONE alpha-equivalence \
         class; two classes means the digest is keyed on occurrence-wrapper IRIs and is a label, \
         not a content key: {classes:?}"
    );
}

/// The SHIPPED twin example resolves its two independently authored expressions to ONE class.
///
/// Same property as the inline twin case above, but over the file the bundle actually carries,
/// so the corpus folded into `graph/examples` can itself fail if the digest ever goes back
/// to keying on occurrence-wrapper IRIs.
#[test]
fn the_shipped_twin_example_resolves_both_authorings_to_one_class() {
    const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    let graph = reasoned(ALPHA_TWINS);
    let mut classes: std::collections::BTreeMap<String, String> = Default::default();
    for quad in graph.dataset.flat_default_graph_quads() {
        if format!("{:?}", quad.p).contains(ALPHA_CLASS) {
            classes.insert(format!("{:?}", quad.s), format!("{:?}", quad.o));
        }
    }
    assert_eq!(
        classes.len(),
        2,
        "both shipped twins must be decided by the gate, saw {classes:?}"
    );
    let distinct: std::collections::BTreeSet<&String> = classes.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "the shipped twins must share ONE alpha-equivalence class: {classes:?}"
    );
}

/// A root the GRAMMAR refutes must receive NO `math:alphaEquivalenceClass` edge — even though
/// the DL chase makes it lower cleanly.
///
/// This is the substrate trap the file's header describes, in its second form. The gate's
/// finding half was moved onto the asserted graph, but the identity MATERIALIZER kept reading
/// the closure, where the chase has already invented the operator the fixture omits. The root
/// therefore lowered, and a structural identity was published for an expression the very same
/// reasoned graph reports as rejected — computed over a Skolem witness nobody authored, and
/// disagreeing with the digest the finding itself cites.
///
/// The fixture below asserts NO `math:operator`, so `math:MalformedBindingExpression` /
/// `math:ApplicationOperatorCardinality` refutes it on the asserted graph while the chase
/// supplies a filler on the closure. Reverting the materializer to the reasoned dataset makes
/// this test red.
#[test]
fn a_root_the_grammar_refutes_gets_no_materialized_identity_edge() {
    const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
    const OPERATOR_LESS: &str = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/refuted/> .

ex:noOperator a math:ApplicationExpression ;
    math:argumentSlot ex:slot0 .

ex:slot0 a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression ex:leaf .

ex:leaf a math:NumberLiteral ;
    math:literalValue 1 .
"#;

    let graph = reasoned(OPERATOR_LESS);

    // The chase really does supply the missing operator on the closure — without this the test
    // would pass for the wrong reason (nothing to invent means nothing to over-materialize).
    let operator_edges = graph
        .dataset
        .flat_default_graph_quads()
        .filter(|q| format!("{:?}", q.s).contains("refuted/noOperator"))
        .filter(|q| format!("{:?}", q.p).ends_with("math/operator\")"))
        .count();
    assert!(
        operator_edges > 0,
        "precondition: the DL chase must invent the omitted math:operator on the closure, else \
         this test cannot observe the defect it exists to pin"
    );

    let published: Vec<String> = graph
        .dataset
        .flat_default_graph_quads()
        .filter(|q| format!("{:?}", q.p).contains(ALPHA_CLASS))
        .map(|q| format!("{:?}", q.s))
        .filter(|s| s.contains("refuted/noOperator"))
        .collect();
    assert!(
        published.is_empty(),
        "an expression the grammar refutes must carry NO materialized identity edge; the \
         materializer published {published:?}, which means it lowered the CLOSURE (where the \
         chase invented the missing operator) rather than the asserted graph"
    );
}

/// Every SHIPPED `math:` example is clean through the expression-identity gate on the
/// MULTI-GRAPH substrate production builds.
///
/// The other tests here reason a single-graph parse. `gmeow validate --deep` does not: it
/// assembles an EDB whose graphs can each carry a slice's triples, and the gate's obligations
/// are cardinality claims. Counting assertions rather than distinct triples therefore reported
/// this slice's own conforming examples as carrying two `math:operator` values where they
/// author one — `alpha-equivalent-twins.ttl`, minted to demonstrate the α-class JOIN to a
/// consumer, was rejected by the shipped validator while every in-crate test stayed green.
///
/// So drive the gate over the examples through a dataset that really does hold each triple in
/// more than one graph. A regression in the index's distinctness reds this and nothing else.
#[test]
fn shipped_examples_are_clean_over_a_multi_graph_substrate() {
    const EXAMPLES: &[(&str, &str)] = &[
        (
            "alpha-equivalent-twins.ttl",
            include_str!("../../../slices/grounding/math/examples/alpha-equivalent-twins.ttl"),
        ),
        (
            "reference-ast-act.ttl",
            include_str!("../../../slices/grounding/math/examples/reference-ast-act.ttl"),
        ),
        (
            "closed-form-functions.ttl",
            include_str!("../../../slices/grounding/math/examples/closed-form-functions.ttl"),
        ),
    ];

    for (name, ttl) in EXAMPLES {
        let single = asserted(ttl);

        // The SAME triples again under a named graph, which is what makes this a real probe of
        // the multi-graph EDB rather than a second copy of the single-graph tests above.
        let mut builder = purrdf::RdfDatasetBuilder::new();
        builder.push_dataset(&single);
        for quad in single.owned_quads() {
            builder.push_owned_quad(
                &purrdf::RdfQuad::new(
                    quad.subject.clone(),
                    quad.predicate.clone(),
                    quad.object.clone(),
                )
                .in_graph(purrdf::RdfTerm::iri(
                    "https://blackcatinformatics.ca/gmeow/graph/probe",
                )),
            );
        }
        let multi = builder
            .freeze()
            .expect("the example plus a named-graph copy of itself is a valid dataset");

        let errors: Vec<String> = check_math_expression_findings(&multi, &multi)
            .into_iter()
            .filter(|f| f.severity == gmeow_errors::Severity::Error)
            .map(|f| format!("{} {}", f.code, f.message))
            .collect();
        assert!(
            errors.is_empty(),
            "{name} is a SHIPPED conforming example, but the expression-identity gate errors \
             on it once its triples appear in more than one graph — the substrate \
             `gmeow validate --deep` actually builds: {errors:?}"
        );
    }
}
