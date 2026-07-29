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

/// Reason over the math TBox UNIONED with `turtle`, exactly as production does.
fn reasoned(turtle: &str) -> gmeow_logic::verify::ReasonedGraph {
    let combined = format!("{MATH_MODULE}\n{turtle}");
    let edb =
        purrdf::parse_dataset(combined.as_bytes(), "text/turtle", None).expect("parse fixture");
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
    let findings = check_math_expression_findings(&graph.dataset);
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

    // And the gate agrees with that shared key on the reasoned substrate.
    let graph = reasoned(REFERENCE_AST_ACT);
    assert!(
        check_math_expression_findings(&graph.dataset).is_empty(),
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
