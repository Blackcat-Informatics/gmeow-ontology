// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ns::logic;
use crate::onnx::encode::{
    MLP_BIAS_PAYLOAD, MLP_WEIGHT_PAYLOAD, bytes_field, message_field, mlp, string_field, truncated,
    varint_field,
};

const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

/// The flagship fixture: a real, byte-valid, minimal ONNX model.
const MLP: &[u8] = include_bytes!("../../fixtures/mlp.onnx");
/// A genuinely malformed protobuf.
const TRUNCATED: &[u8] = include_bytes!("../../fixtures/truncated.onnx");

const RDF_TYPE_LINE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

fn turtle(source: &[u8]) -> String {
    lift(source, BASE)
        .unwrap_or_else(|e| panic!("the model must lift: {e}"))
        .turtle
}

fn count(ttl: &str, needle: &str) -> usize {
    ttl.matches(needle).count()
}

/// How many subjects the graph types as `math:{class}`.
///
/// Exact rather than substring: `Operation` must not be counted by `MathematicalObject`,
/// nor `ParameterSpace` by anything sharing the word "Space".
fn typed(ttl: &str, class: &str) -> usize {
    let suffix = format!("{RDF_TYPE_LINE} <{}> .", math(class));
    ttl.lines().filter(|line| line.ends_with(&suffix)).count()
}

fn typed_gmeow(ttl: &str, class: &str) -> usize {
    let suffix = format!("{RDF_TYPE_LINE} <{}> .", gmeow(class));
    ttl.lines().filter(|line| line.ends_with(&suffix)).count()
}

fn subjects_with(ttl: &str, predicate: &str) -> BTreeSet<String> {
    let marker = format!(" <{predicate}> ");
    ttl.lines()
        .filter(|line| line.contains(&marker))
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect()
}

/// The subjects carrying `predicate` with exactly the plain literal `value`.
fn subjects_with_literal(ttl: &str, predicate: &str, value: &str) -> BTreeSet<String> {
    let marker = format!(" <{predicate}> \"{value}\" .");
    ttl.lines()
        .filter(|line| line.ends_with(&marker))
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect()
}

/// Every `xsd:integer` object of `predicate`, in the graph's canonical order.
///
/// Predicate-scoped on purpose: a bare `"0"^^` substring count cannot tell a
/// `math:slotIndex` from a `math:axisIndex`, a `math:axisExtent`, or a
/// `math:tensorRank`, and this lift now emits all four.
fn integers_of(ttl: &str, predicate: &str) -> Vec<i64> {
    let marker = format!(" <{predicate}> \"");
    ttl.lines()
        .filter_map(|line| {
            let rest = line.split_once(&marker)?.1;
            let (value, _) = rest.split_once('"')?;
            value.parse::<i64>().ok()
        })
        .collect()
}

/// A model with a graph but no `NodeProto` at all.
fn nodeless_model() -> Vec<u8> {
    let mut graph = Vec::new();
    graph.extend(string_field(2, "empty"));
    let mut opset = Vec::new();
    opset.extend(varint_field(2, 18));
    let mut model = Vec::new();
    model.extend(varint_field(1, 8));
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &opset));
    model
}

/// A graph whose two nodes are structurally identical.
fn duplicated_subgraph_model() -> Vec<u8> {
    let value_type = {
        let mut shape = Vec::new();
        shape.extend(message_field(1, &varint_field(1, 4)));
        let mut tensor = Vec::new();
        tensor.extend(varint_field(1, 1));
        tensor.extend(message_field(2, &shape));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };
    let relu = |name: &str, out_name: &str| {
        let mut node = Vec::new();
        node.extend(string_field(1, "X"));
        node.extend(string_field(2, out_name));
        node.extend(string_field(3, name));
        node.extend(string_field(4, "Relu"));
        node
    };
    let mut graph = Vec::new();
    graph.extend(message_field(1, &relu("a", "Y1")));
    graph.extend(message_field(1, &relu("b", "Y2")));
    graph.extend(string_field(2, "twice"));
    graph.extend(message_field(11, &info("X")));
    graph.extend(message_field(12, &info("Y1")));
    graph.extend(message_field(12, &info("Y2")));

    let mut opset = Vec::new();
    opset.extend(varint_field(2, 18));
    let mut model = Vec::new();
    model.extend(varint_field(1, 8));
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &opset));
    model
}

// -- the committed fixtures ----------------------------------------------

#[test]
fn the_committed_mlp_fixture_is_exactly_what_the_builder_emits() {
    // The shipped `.onnx` is a PRODUCT of `encode::mlp`, not hand-typed hex: rebuild it
    // and require a byte-for-byte match, so the fixture can never drift from the model it
    // is documented to be. (Mirrors the TSTP fixture pin in
    // `crates/conformance/src/external/tptp/lower_fol.rs`.)
    assert_eq!(
        MLP,
        mlp().as_slice(),
        "fixtures/mlp.onnx drifted from crate::onnx::encode::mlp()"
    );
}

#[test]
fn the_committed_truncated_fixture_is_exactly_what_the_builder_emits() {
    assert_eq!(
        TRUNCATED,
        truncated().as_slice(),
        "fixtures/truncated.onnx drifted from crate::onnx::encode::truncated()"
    );
}

#[test]
fn the_committed_fixture_is_a_real_decodable_onnx_model() {
    let model = ModelProto::decode(MLP).expect("the fixture is byte-valid ONNX");
    assert_eq!(model.ir_version, 8);
    assert_eq!(model.producer_name, "gmeow-math-lift");
    assert_eq!(model.opset_import.len(), 1);
    assert_eq!(model.opset_import[0].spelled_domain(), "ai.onnx");
    assert_eq!(model.opset_import[0].version, 18);
    let graph = model.graph.expect("a graph");
    assert_eq!(graph.name, "mlp");
    assert_eq!(graph.node.len(), 3);
    assert_eq!(
        graph
            .node
            .iter()
            .map(|n| n.op_type.as_str())
            .collect::<Vec<_>>(),
        vec!["MatMul", "Add", "Relu"]
    );
    assert_eq!(graph.initializer.len(), 2);
    assert_eq!(graph.initializer[0].dims, vec![4, 3]);
    assert_eq!(graph.initializer[1].dims, vec![3]);
    assert_eq!(graph.input.len(), 1);
    assert_eq!(graph.output.len(), 1);
    assert_eq!(model.metadata_props.len(), 1);
}

// -- hard failures --------------------------------------------------------

#[test]
fn the_truncated_fixture_is_a_wire_failure_with_a_byte_offset() {
    let err = lift(TRUNCATED, BASE).expect_err("a truncated protobuf must not lift");
    let text = format!("{err}");
    assert!(text.contains("byte offset"), "{text}");
    assert!(
        text.contains("truncated") || text.contains("does not close"),
        "{text}"
    );
}

#[test]
fn a_graph_with_no_node_is_unliftable_by_the_min_one_restriction() {
    let err = lift(&nodeless_model(), BASE).expect_err("a nodeless graph must not lift");
    let text = format!("{err}");
    assert!(text.contains("math:computationNode"), "{text}");
    assert!(text.contains("min-1"), "{text}");
}

#[test]
fn a_model_with_no_graph_is_unliftable() {
    let model = varint_field(1, 8);
    let err = lift(&model, BASE).expect_err("no graph, no lift");
    assert!(format!("{err}").contains("no GraphProto"), "{err}");
}

#[test]
fn a_model_with_no_opset_import_is_unliftable() {
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    let err = lift(&model, BASE).expect_err("no operator vocabulary, no lift");
    assert!(format!("{err}").contains("opset_import"), "{err}");
}

#[test]
fn a_node_reading_an_undeclared_value_refuses_rather_than_minting_a_placeholder() {
    let mut node = Vec::new();
    node.extend(string_field(1, "ghost"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "dangling"));
    let mut opset = varint_field(2, 18);
    opset.splice(0..0, Vec::<u8>::new());
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &opset));
    let err = lift(&model, BASE).expect_err("a dangling operand must not lift");
    let text = format!("{err}");
    assert!(text.contains("`ghost`"), "{text}");
    assert!(text.contains("never declares"), "{text}");
}

#[test]
fn a_node_from_an_unimported_domain_refuses() {
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "MyOp"));
    node.extend(string_field(7, "ca.blackcat.custom"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    let err = lift(&model, BASE).expect_err("an unimported domain must not lift");
    assert!(format!("{err}").contains("ca.blackcat.custom"), "{err}");
}

#[test]
fn a_control_flow_subgraph_attribute_refuses_by_name() {
    let mut attribute = Vec::new();
    attribute.extend(string_field(1, "body"));
    attribute.extend(message_field(6, &string_field(2, "loop-body")));
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Loop"));
    node.extend(message_field(5, &attribute));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    // A control-flow subgraph LIFTS. It used to refuse the whole model, which refused
    // every scripted-PyTorch / decoder / RNN export — while a custom operator from a
    // private domain, whose semantics the bridge cannot know, lifted cleanly. The run
    // already declares LossyLens / mnemomorphic false and already enumerates residue,
    // so an undecoded attribute costs it no claim it was not already making.
    let lifted = lift(&model, BASE).expect("a control-flow subgraph must lift");
    let ttl = &lifted.turtle;
    assert!(
        ttl.contains(&math("TensorComputationGraph")),
        "the graph still lifts:\n{ttl}"
    );
    // Identity is preserved, not refused: the attribute's KIND keys the operator, so
    // two Loops with different bodies cannot collapse onto one math:Operation.
    assert!(
        ttl.contains("Loop(body=<g (a control-flow subgraph)#"),
        "the undecoded arm still keys the operator:\n{ttl}"
    );
    // …and what does NOT cross is named on the witness.
    assert!(
        ttl.contains("the `g (a control-flow subgraph)` arm is not decoded"),
        "the subgraph's contents are enumerated as residue:\n{ttl}"
    );
    assert!(ttl.contains("`body` on node `Loop#0`"), "{ttl}");
}

#[test]
fn two_constants_with_different_payloads_never_merge() {
    // `TensorProto` decodes HEADER ONLY (blob-by-reference), so two `Constant` nodes
    // holding different values share a name and dims and rendered identically — they
    // interned to one node, the second was discarded, and an `Add` over both resolved
    // BOTH operand slots to the same expression. `y = a + b` was lifted as `y = a + a`:
    // not merely a dropped node, a semantically wrong graph.
    //
    // The fix digests the attribute's raw bytes, which distinguishes without
    // materializing a value — the doctrine holds and the identity is real.
    let constant = |output: &str, payload: &[u8]| {
        let mut tensor = varint_field(1, 1);
        tensor.extend(varint_field(2, 1));
        tensor.extend(string_field(8, "c"));
        tensor.extend(bytes_field(9, payload));
        let mut attribute = string_field(1, "value");
        attribute.extend(message_field(5, &tensor));
        let mut node = Vec::new();
        node.extend(string_field(2, output));
        node.extend(string_field(4, "Constant"));
        node.extend(message_field(5, &attribute));
        node
    };
    let mut add = Vec::new();
    add.extend(string_field(1, "a"));
    add.extend(string_field(1, "b"));
    add.extend(string_field(2, "y"));
    add.extend(string_field(4, "Add"));

    let value_type = {
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &Vec::new()));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };

    let mut graph = Vec::new();
    graph.extend(message_field(1, &constant("a", &[0x00, 0x00, 0x80, 0x3f])));
    graph.extend(message_field(1, &constant("b", &[0x00, 0x00, 0x00, 0x40])));
    graph.extend(message_field(1, &add));
    graph.extend(message_field(12, &info("y")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = lift(&model, BASE).expect("the model lifts").turtle;
    assert_eq!(
        count(&ttl, &format!("<{}>", math("computationNode"))),
        3,
        "three nodes in, three computation nodes out:\n{ttl}"
    );
    // The decisive check: the two operand slots must NOT name one expression.
    let operands: BTreeSet<String> = ttl
        .lines()
        .filter(|l| l.contains(&format!("<{}>", math("slotExpression"))))
        .filter_map(|l| l.split_whitespace().nth(2).map(str::to_owned))
        .collect();
    assert_eq!(
        operands.len(),
        2,
        "a + b must not be lifted as a + a:\n{ttl}"
    );
}

#[test]
fn two_nodes_in_one_graph_never_collapse_into_one() {
    // The previous version of this test compared the two RUN IRIs, which are
    // content-addressed on the source bytes and so differ for any two distinct files —
    // it asserted nothing about node identity and passed while a node was being
    // silently dropped. Two nodes must be built in ONE graph and the emitted nodes
    // counted, which is the only way the collapse is observable.
    //
    // An undecoded attribute renders as a CONSTANT, so two `Loop`s differing only in
    // their subgraph bodies are the sharpest case: everything in the key except the
    // node's own outputs is identical.
    let value_type = {
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &Vec::new()));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };
    let loop_node = |output: &str, body: &str| {
        let mut attribute = string_field(1, "body");
        attribute.extend(message_field(6, &string_field(2, body)));
        let mut node = Vec::new();
        node.extend(string_field(1, "X"));
        node.extend(string_field(2, output));
        node.extend(string_field(4, "Loop"));
        node.extend(message_field(5, &attribute));
        node
    };
    let mut graph = Vec::new();
    graph.extend(message_field(1, &loop_node("Y1", "bodyA")));
    graph.extend(message_field(1, &loop_node("Y2", "bodyBBBB")));
    graph.extend(message_field(11, &info("X")));
    graph.extend(message_field(12, &info("Y1")));
    graph.extend(message_field(12, &info("Y2")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = lift(&model, BASE).expect("two Loop nodes lift").turtle;
    assert_eq!(
        count(&ttl, &format!("<{}>", math("computationNode"))),
        2,
        "TWO nodes in, two computation nodes out — a dropped node is data loss the \
             residue cannot even name:\n{ttl}"
    );
    assert_eq!(typed(&ttl, "NeuralLayer"), 2, "both layers survive:\n{ttl}");
}

#[test]
fn an_untyped_graph_output_refuses() {
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(message_field(12, &string_field(1, "Y")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    let err = lift(&model, BASE).expect_err("an untyped slot must not lift");
    assert!(format!("{err}").contains("declares no TypeProto"), "{err}");
}

#[test]
fn an_unknown_element_type_code_refuses_rather_than_labelling_a_number() {
    let value_type = {
        let mut tensor = varint_field(1, 99);
        tensor.extend(message_field(2, &Vec::new()));
        message_field(1, &tensor)
    };
    let mut info = Vec::new();
    info.extend(string_field(1, "Y"));
    info.extend(message_field(2, &value_type));
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(message_field(12, &info));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    let err = lift(&model, BASE).expect_err("code 99 is not a data type");
    assert!(format!("{err}").contains("element type code 99"), "{err}");
}

#[test]
fn a_sequence_typed_boundary_value_refuses_by_constructor_name() {
    let value_type = message_field(4, &string_field(1, "inner"));
    let mut info = Vec::new();
    info.extend(string_field(1, "Y"));
    info.extend(message_field(2, &value_type));
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(message_field(12, &info));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    let err = lift(&model, BASE).expect_err("a sequence type must not lift");
    assert!(format!("{err}").contains("sequence_type"), "{err}");
}

// -- the flagship lift ----------------------------------------------------

#[test]
fn the_mlp_fixture_lifts_every_expected_codomain_class() {
    let lifted = lift(MLP, BASE).expect("the flagship fixture lifts");
    for class in [
        "ONNXIngestRun",
        "TensorComputationGraph",
        "LearnedModel",
        "ApplicationExpression",
        "ArgumentSlot",
        "VariableExpression",
        "VariableOccurrence",
        "FreeVariableDeclaration",
        "Operation",
        "MathematicalSymbol",
        "MathematicalTheory",
        "ExpressionType",
        "WeightTensor",
        "NeuralLayer",
        "ParameterSpace",
        "Set",
        "Axiom",
    ] {
        assert!(
            typed(&lifted.turtle, class) > 0,
            "the mlp fixture must produce a math:{class}"
        );
    }
    assert_eq!(
        typed(&lifted.turtle, "ONNXIngestRun"),
        1,
        "exactly one ingest run"
    );
    assert_eq!(
        typed(&lifted.turtle, "TensorComputationGraph"),
        1,
        "exactly one computation graph"
    );
    assert_eq!(
        typed(&lifted.turtle, "ApplicationExpression"),
        3,
        "MatMul, Add, Relu"
    );
    assert_eq!(typed(&lifted.turtle, "WeightTensor"), 2, "W and B");
    assert_eq!(typed(&lifted.turtle, "ParameterSpace"), 1);
    assert!(lifted.run_iri.contains("onnx-run-"));
    assert!(lifted.codomain_nodes > 20, "a real graph is dense");
}

#[test]
fn every_node_is_a_computation_node_of_the_graph() {
    let ttl = turtle(MLP);
    assert_eq!(
        count(&ttl, &format!("<{}>", math("computationNode"))),
        3,
        "one math:computationNode edge per NodeProto:\n{ttl}"
    );
    assert_eq!(count(&ttl, &format!("<{}>", math("architectureOf"))), 1);
}

#[test]
fn the_matmul_node_resolves_to_the_declared_math_matrix_product_individual() {
    let ttl = turtle(MLP);
    assert!(
        ttl.contains(&format!(
            "<{}> <{}> .",
            math("tensorOperation"),
            math("matrixProduct")
        )),
        "MatMul must resolve to math:matrixProduct, not merely to a minted operator:\n{ttl}"
    );
}

#[test]
fn each_operator_resolves_through_a_symbol_scoped_by_the_imported_opset() {
    let ttl = turtle(MLP);
    assert_eq!(typed(&ttl, "MathematicalTheory"), 1, "one opset import");
    assert_eq!(
        typed(&ttl, "MathematicalSymbol"),
        3,
        "MatMul, Add and Relu are three symbols"
    );
    let symbols = subjects_with(&ttl, &math("definedInTheory"));
    assert_eq!(symbols.len(), 3, "every symbol is scoped by its opset");
    assert!(ttl.contains("ONNX operator set ai.onnx v18"), "{ttl}");
}

#[test]
fn the_argument_slots_are_contiguous_and_zero_based() {
    let ttl = turtle(MLP);
    // MatMul(X, W) and Add(XW, B) have two operands each; Relu(XB) has one.
    assert_eq!(typed(&ttl, "ArgumentSlot"), 5);
    let indexes = integers_of(&ttl, &math("slotIndex"));
    assert_eq!(indexes.len(), 5);
    assert_eq!(
        indexes.iter().filter(|i| **i == 0).count(),
        3,
        "three slots at index 0"
    );
    assert_eq!(
        indexes.iter().filter(|i| **i == 1).count(),
        2,
        "two slots at index 1"
    );
}

#[test]
fn the_parameter_space_carries_every_vector_space_obligation() {
    let ttl = turtle(MLP);
    for predicate in [
        "parameterSpaceOf",
        "underlyingSet",
        "structureOperation",
        "satisfiesAxiom",
        "spaceDimension",
    ] {
        assert_eq!(
            count(&ttl, &format!("<{}>", math(predicate))),
            1,
            "math:ParameterSpace is a math:VectorSpace and owes math:{predicate}:\n{ttl}"
        );
    }
    // 4×3 weights + 3 biases = 15 parameters.
    assert!(
        ttl.contains(&format!(
            "<{}> \"15\"^^<http://www.w3.org/2001/XMLSchema#integer> .",
            math("spaceDimension")
        )),
        "the parameter space's dimension is the total element count:\n{ttl}"
    );
}

#[test]
fn every_weight_tensor_is_framed_by_exactly_one_parameter_space_and_one_layer() {
    let ttl = turtle(MLP);
    assert_eq!(
        count(&ttl, &format!("<{}>", math("inParameterSpace"))),
        2,
        "one per weight, and never more (the max-1 qualified restriction)"
    );
    assert_eq!(count(&ttl, &format!("<{}>", math("weightOf"))), 2);
    assert_eq!(typed(&ttl, "NeuralLayer"), 3, "one layer per node");
}

#[test]
fn the_boundary_values_carry_their_declared_tensor_types() {
    let ttl = turtle(MLP);
    assert!(ttl.contains("tensor(float)[1,4]"), "the input type:\n{ttl}");
    assert!(
        ttl.contains("tensor(float)[1,3]"),
        "the output type:\n{ttl}"
    );
    assert!(
        ttl.contains("tensor(float)[4,3]"),
        "the W initializer header type:\n{ttl}"
    );
    assert!(
        ttl.contains("tensor(float)[3]"),
        "the B initializer header type:\n{ttl}"
    );
    assert_eq!(
        typed(&ttl, "ExpressionType"),
        4,
        "[1,4], [1,3], [4,3] and [3] — the value_info for XW re-uses [1,3]"
    );
    assert_eq!(
        count(&ttl, &format!("<{}>", math("expressionType"))),
        5,
        "X, W, B, XW and Y are all typed tensor slots"
    );
}

/// Every triple whose subject is `subject` (given in `<iri>` form).
fn about(ttl: &str, subject: &str) -> String {
    ttl.lines()
        .filter(|line| line.starts_with(subject))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The single IRI object of `subject predicate ?o`, in `<iri>` form.
fn object_of(ttl: &str, subject: &str, predicate: &str) -> Option<String> {
    let edge = format!("{subject} <{predicate}> ");
    ttl.lines()
        .find_map(|line| line.strip_prefix(&edge))
        .map(|rest| rest.trim_end_matches(" .").to_owned())
}

/// A node's `rdfs:label`.
fn label_of(ttl: &str, subject: &str) -> Option<String> {
    let edge = format!("{subject} <{RDFS_LABEL}> \"");
    ttl.lines()
        .find_map(|line| line.split_once(&edge))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(label, _)| label.to_owned())
}

/// The `math:TensorShape` a subject names, in `<iri>` form.
fn shape_of(ttl: &str, subject: &str) -> String {
    object_of(ttl, subject, &math("tensorShape"))
        .unwrap_or_else(|| panic!("{subject} must name a math:TensorShape:\n{ttl}"))
}

/// The axes of a shape, in `math:axisIndex` order, as `(index, extent-or-symbol)`.
///
/// Reads the graph the way a consumer would — shape node → `math:shapeAxis` → axis →
/// extent — so the test proves the STRUCTURE is traversable, not merely that some
/// integers appear somewhere in the Turtle.
fn axes_of(ttl: &str, shape_iri: &str) -> Vec<(i64, String)> {
    let edge = format!("{shape_iri} <{}> ", math("shapeAxis"));
    let mut axes: Vec<(i64, String)> = ttl
        .lines()
        .filter_map(|line| line.strip_prefix(&edge))
        .map(|rest| rest.trim_end_matches(" .").to_owned())
        .map(|axis| {
            let own = about(ttl, &axis);
            let index = *integers_of(&own, &math("axisIndex"))
                .first()
                .unwrap_or_else(|| panic!("axis {axis} must carry a math:axisIndex"));
            let extent = integers_of(&own, &math("axisExtent"))
                .first()
                .map(i64::to_string)
                // A symbolic extent resolves THROUGH the free-variable declaration it
                // names; the label on that node is the symbol.
                .or_else(|| {
                    object_of(&own, &axis, &math("symbolicExtent"))
                        .and_then(|declaration| label_of(ttl, &declaration))
                })
                .unwrap_or_else(|| "?".to_owned());
            (index, extent)
        })
        .collect();
    axes.sort_by_key(|(index, _)| *index);
    axes
}

#[test]
fn every_tensor_extent_is_a_per_axis_node_carrying_its_own_index() {
    let ttl = turtle(MLP);
    // [1,4] (X), [1,3] (Y and XW share it), [4,3] (W), [3] (B) — four distinct shapes,
    // each minted once however many tensors have it.
    assert_eq!(typed(&ttl, "TensorShape"), 4, "{ttl}");
    assert_eq!(typed(&ttl, "TensorAxis"), 2 + 2 + 2 + 1, "{ttl}");

    // The W initializer is the finding's own example: `tensor(float)[4,3]` used to be a
    // label and nothing else. Read it back through the graph, per axis, in order.
    let w_type = subjects_with_literal(&ttl, RDFS_LABEL, "tensor(float)[4,3]");
    assert_eq!(w_type.len(), 1, "one math:ExpressionType for [4,3]");
    let w_type = w_type.iter().next().expect("the type node");
    assert_eq!(
        axes_of(&ttl, &shape_of(&ttl, w_type)),
        vec![(0, "4".to_owned()), (1, "3".to_owned())],
        "axis 0 has extent 4 and axis 1 extent 3 — ORDER IS THE SHAPE:\n{ttl}"
    );

    // …and the input's [1,4] is genuinely a different shape, not a set of the same two
    // numbers: reading it back gives the extents in the other order.
    let x_type = subjects_with_literal(&ttl, RDFS_LABEL, "tensor(float)[1,4]");
    let x_type = x_type.iter().next().expect("the type node");
    assert_eq!(
        axes_of(&ttl, &shape_of(&ttl, x_type)),
        vec![(0, "1".to_owned()), (1, "4".to_owned())],
    );
}

#[test]
fn every_shape_states_its_rank_including_the_scalar_case() {
    let ttl = turtle(MLP);
    let ranks = integers_of(&ttl, &math("tensorRank"));
    assert_eq!(ranks.len(), 4, "one rank per shape:\n{ttl}");
    // [1,4], [1,3], [4,3] are rank 2; [3] is rank 1.
    assert_eq!(ranks.iter().filter(|r| **r == 2).count(), 3);
    assert_eq!(ranks.iter().filter(|r| **r == 1).count(), 1);
    // Every rank agrees with the axis family it labels.
    for shape in subjects_with(&ttl, &math("tensorRank")) {
        let declared = *integers_of(&about(&ttl, &shape), &math("tensorRank"))
            .first()
            .expect("a rank");
        let axes = axes_of(&ttl, &shape);
        assert_eq!(
            i64::try_from(axes.len()).expect("a small rank"),
            declared,
            "math:tensorRank must equal the math:shapeAxis count for {shape}:\n{ttl}"
        );
    }
}

#[test]
fn a_rank_zero_scalar_initializer_is_a_shape_with_no_axis_and_rank_zero() {
    // A scalar initializer: no `dims` at all. ONNX reads that as rank 0 (one element),
    // which must be POSITIVELY assertable and never confused with an unknown rank.
    let mut init = Vec::new();
    init.extend(varint_field(2, 1));
    init.extend(string_field(8, "S"));
    init.extend(bytes_field(9, &[0, 0, 0, 0]));
    let mut node = Vec::new();
    node.extend(string_field(1, "S"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "scalar"));
    graph.extend(message_field(5, &init));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(typed(&ttl, "TensorShape"), 1, "{ttl}");
    assert_eq!(typed(&ttl, "TensorAxis"), 0, "rank 0 has no axis:\n{ttl}");
    assert_eq!(
        integers_of(&ttl, &math("tensorRank")),
        vec![0],
        "rank 0 is asserted, not left to be inferred from an absence:\n{ttl}"
    );
    assert!(ttl.contains("tensor(float)[]"), "{ttl}");
}

#[test]
fn an_unknown_rank_names_no_shape_at_all_rather_than_an_empty_one() {
    // A TypeProto.Tensor with elem_type but NO TensorShapeProto: ONNX's "rank unknown".
    let value_type = message_field(1, &varint_field(1, 1));
    let mut info = Vec::new();
    info.extend(string_field(1, "Y"));
    info.extend(message_field(2, &value_type));
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "rankless"));
    graph.extend(message_field(12, &info));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(
        typed(&ttl, "TensorShape"),
        0,
        "an unknown rank is the ABSENCE of a shape, not a shape with no axis (which is \
             rank 0, a strictly stronger claim):\n{ttl}"
    );
    assert_eq!(count(&ttl, &format!("<{}>", math("tensorShape"))), 0);
    assert!(ttl.contains("of unknown rank"), "{ttl}");
    // The element type is still stated, and still crosses.
    assert_eq!(typed(&ttl, "TensorElementType"), 1, "{ttl}");
}

#[test]
fn a_symbolic_extent_is_one_shared_declaration_rather_than_two_equal_strings() {
    // Two tensors whose leading axis is the same `dim_param "batch"`. The co-identity of
    // the two unknown lengths is the information a string on each axis would lose.
    let batched = |trailing: i64| {
        let mut shape = Vec::new();
        shape.extend(message_field(1, &string_field(2, "batch")));
        shape.extend(message_field(1, &varint_field(1, trailing)));
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &shape));
        message_field(1, &tensor)
    };
    let info = |name: &str, trailing: i64| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &batched(trailing)));
        out
    };
    let mut node = Vec::new();
    node.extend(string_field(1, "X"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "batched"));
    graph.extend(message_field(11, &info("X", 4)));
    graph.extend(message_field(12, &info("Y", 3)));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(typed(&ttl, "TensorShape"), 2, "[batch,4] and [batch,3]");
    assert_eq!(
        count(&ttl, &format!("<{}>", math("symbolicExtent"))),
        2,
        "one per symbolic axis:\n{ttl}"
    );
    let declarations: BTreeSet<String> = ttl
        .lines()
        .filter_map(|line| {
            line.split_once(&format!(" <{}> ", math("symbolicExtent")))
                .map(|(_, object)| object.trim_end_matches(" .").to_owned())
        })
        .collect();
    assert_eq!(
        declarations.len(),
        1,
        "both `batch` axes must resolve to ONE math:FreeVariableDeclaration — that shared \
             identity is exactly what a per-axis string would discard:\n{ttl}"
    );
    let extent_declaration = declarations
        .iter()
        .next()
        .expect("shared extent declaration");
    assert_eq!(
        object_of(&ttl, extent_declaration, &math("variableDomain")),
        Some(format!("<{}>", math("naturalNumbers"))),
        "symbolic lengths retain the complete free-variable domain owner:\n{ttl}"
    );
    let input_domain = leaf_value_domain(&ttl, "X");
    assert_ne!(input_domain, format!("<{}>", math("naturalNumbers")));
    assert!(ttl.contains(&format!(
        "{input_domain} {RDF_TYPE_LINE} <{}> .",
        math("Set")
    )));
    assert!(
        subjects_with(&ttl, &math("expressionValueSpace"))
            .iter()
            .any(
                |descriptor| object_of(&ttl, descriptor, &math("expressionValueSpace")).as_ref()
                    == Some(&input_domain)
            )
    );
    let x_type = subjects_with_literal(&ttl, RDFS_LABEL, "tensor(float)[batch,4]");
    let x_type = x_type.iter().next().expect("the type node");
    assert_eq!(
        axes_of(&ttl, &shape_of(&ttl, x_type)),
        vec![(0, "batch".to_owned()), (1, "4".to_owned())],
    );
}

/// Encode actual admitted ONNX tensor metadata, preserving the oneof axis arm.
fn tensor_type_bytes(element: i64, axes: Option<&[Dim]>) -> Vec<u8> {
    let mut tensor = varint_field(1, element);
    if let Some(axes) = axes {
        let mut shape = Vec::new();
        for axis in axes {
            let dimension = match axis {
                Dim::Value(value) => varint_field(1, *value),
                Dim::Param(name) => string_field(2, name),
                Dim::Unknown => Vec::new(),
            };
            shape.extend(message_field(1, &dimension));
        }
        tensor.extend(message_field(2, &shape));
    }
    message_field(1, &tensor)
}

fn tensor_info_bytes(name: &str, value_type: &[u8]) -> Vec<u8> {
    let mut info = string_field(1, name);
    info.extend(message_field(2, value_type));
    info
}

/// X -> Y Identity plus independent declared inputs and optional value annotations.
fn typed_identity_model(
    inputs: &[(&str, Vec<u8>)],
    output_type: &[u8],
    annotations: &[(&str, Vec<u8>)],
) -> Vec<u8> {
    let mut node = string_field(1, "X");
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Identity"));
    let mut graph = message_field(1, &node);
    graph.extend(string_field(2, "typed identity"));
    for (name, value_type) in inputs {
        graph.extend(message_field(11, &tensor_info_bytes(name, value_type)));
    }
    graph.extend(message_field(12, &tensor_info_bytes("Y", output_type)));
    for (name, value_type) in annotations {
        graph.extend(message_field(13, &tensor_info_bytes(name, value_type)));
    }
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    model
}

fn leaf_expression(ttl: &str, name: &str) -> String {
    let named = subjects_with_literal(ttl, RDFS_LABEL, name);
    subjects_with(ttl, &math("variableOccurrence"))
        .intersection(&named)
        .next()
        .unwrap_or_else(|| panic!("leaf expression {name}:\n{ttl}"))
        .clone()
}

fn leaf_value_domain(ttl: &str, name: &str) -> String {
    let expr = leaf_expression(ttl, name);
    let occurrence = object_of(ttl, &expr, &math("variableOccurrence")).unwrap();
    let declaration = object_of(ttl, &occurrence, &math("declaredVariable")).unwrap();
    object_of(ttl, &declaration, &math("variableDomain"))
        .unwrap_or_else(|| panic!("leaf domain {name}:\n{ttl}"))
}

#[test]
fn structural_tensor_keys_preserve_equal_labels_and_delimiter_symbols_in_both_orders() {
    for (left, right, same_label) in [
        (vec![Dim::Value(3)], vec![Dim::Param("3".into())], true),
        (vec![Dim::Unknown], vec![Dim::Param("?".into())], true),
        (
            vec![Dim::Param("a,$b".into())],
            vec![Dim::Param("a".into()), Dim::Param("b".into())],
            false,
        ),
        (
            vec![Dim::Param("a,b".into())],
            vec![Dim::Param("a".into()), Dim::Param("b".into())],
            true,
        ),
        (
            vec![Dim::Param("λ,$β".into())],
            vec![Dim::Param("λ".into()), Dim::Param("β".into())],
            false,
        ),
    ] {
        let left_type = tensor_type_bytes(1, Some(&left));
        let right_type = tensor_type_bytes(1, Some(&right));
        for reversed in [false, true] {
            let mut inputs = vec![("X", left_type.clone()), ("Z", right_type.clone())];
            if reversed {
                inputs.reverse();
            }
            let ttl = turtle(&typed_identity_model(&inputs, &left_type, &[]));
            let x_type =
                object_of(&ttl, &leaf_expression(&ttl, "X"), &math("expressionType")).unwrap();
            let z_type =
                object_of(&ttl, &leaf_expression(&ttl, "Z"), &math("expressionType")).unwrap();
            assert_ne!(x_type, z_type, "source type arms remain distinct:\n{ttl}");
            assert_eq!(
                label_of(&ttl, &x_type) == label_of(&ttl, &z_type),
                same_label
            );
            let x_shape = shape_of(&ttl, &x_type);
            let z_shape = shape_of(&ttl, &z_type);
            assert_ne!(
                x_shape, z_shape,
                "axis framing preserves distinct shapes:\n{ttl}"
            );
            assert_eq!(
                integers_of(&about(&ttl, &x_shape), &math("tensorRank")),
                vec![i64::try_from(left.len()).unwrap()]
            );
            assert_eq!(
                integers_of(&about(&ttl, &z_shape), &math("tensorRank")),
                vec![i64::try_from(right.len()).unwrap()]
            );
            assert_ne!(leaf_value_domain(&ttl, "X"), leaf_value_domain(&ttl, "Z"));
        }
    }
}

#[test]
fn tensor_leaf_domains_share_structural_types_and_keep_bool_and_string_distinct() {
    let axes = [Dim::Value(2)];
    for (left_element, right_element, same_type) in [(9, 9, true), (9, 8, false)] {
        let left = tensor_type_bytes(left_element, Some(&axes));
        let right = tensor_type_bytes(right_element, Some(&axes));
        let ttl = turtle(&typed_identity_model(
            &[("X", left.clone()), ("Z", right)],
            &left,
            &[],
        ));
        let x = object_of(&ttl, &leaf_expression(&ttl, "X"), &math("expressionType")).unwrap();
        let z = object_of(&ttl, &leaf_expression(&ttl, "Z"), &math("expressionType")).unwrap();
        let x_domain = leaf_value_domain(&ttl, "X");
        let z_domain = leaf_value_domain(&ttl, "Z");
        assert_eq!(x == z, same_type);
        assert_eq!(x_domain == z_domain, same_type);
        assert_eq!(
            object_of(&ttl, &x, &math("expressionValueSpace")),
            Some(x_domain.clone())
        );
        assert_eq!(
            object_of(&ttl, &z, &math("expressionValueSpace")),
            Some(z_domain)
        );
        assert_eq!(shape_of(&ttl, &x), shape_of(&ttl, &z));
        assert!(ttl.contains(&format!("{x_domain} {RDF_TYPE_LINE} <{}> .", math("Set"))));
        assert_ne!(x, x_domain, "descriptor and admitted values are separate");
        assert_ne!(x_domain, format!("<{}>", math("naturalNumbers")));
    }
}

#[test]
fn unknown_rank_and_scalar_value_spaces_remain_distinct() {
    let unknown = tensor_type_bytes(1, None);
    let scalar = tensor_type_bytes(1, Some(&[]));
    let ttl = turtle(&typed_identity_model(
        &[("X", unknown.clone()), ("Z", scalar)],
        &unknown,
        &[],
    ));
    let x = object_of(&ttl, &leaf_expression(&ttl, "X"), &math("expressionType")).unwrap();
    let z = object_of(&ttl, &leaf_expression(&ttl, "Z"), &math("expressionType")).unwrap();
    assert_ne!(x, z);
    assert!(object_of(&ttl, &x, &math("tensorShape")).is_none());
    let scalar_shape = shape_of(&ttl, &z);
    assert_eq!(
        integers_of(&about(&ttl, &scalar_shape), &math("tensorRank")),
        vec![0]
    );
    assert!(axes_of(&ttl, &scalar_shape).is_empty());
    assert_ne!(leaf_value_domain(&ttl, "X"), leaf_value_domain(&ttl, "Z"));
}

#[test]
fn compatible_type_refinement_keeps_the_authoritative_input_domain() {
    let unknown = tensor_type_bytes(1, None);
    let known = tensor_type_bytes(1, Some(&[Dim::Value(4)]));
    let ttl = turtle(&typed_identity_model(
        &[("X", unknown)],
        &known,
        &[("X", known.clone())],
    ));
    let domain = leaf_value_domain(&ttl, "X");
    let descriptor = subjects_with(&ttl, &math("expressionValueSpace"))
        .into_iter()
        .find(|descriptor| {
            object_of(&ttl, descriptor, &math("expressionValueSpace")).as_ref() == Some(&domain)
        })
        .unwrap();
    assert_eq!(
        label_of(&ttl, &descriptor).as_deref(),
        Some("tensor(float) of unknown rank")
    );
    assert!(object_of(&ttl, &descriptor, &math("tensorShape")).is_none());
    let expression = leaf_expression(&ttl, "X");
    assert_eq!(
        count(
            &about(&ttl, &expression),
            &format!("<{}>", math("expressionType"))
        ),
        2
    );
}

#[test]
fn conflicting_leaf_types_refuse_instead_of_replacing_a_domain() {
    let original = tensor_type_bytes(
        1,
        Some(&[Dim::Param("batch".into()), Dim::Param("batch".into())]),
    );
    for conflicting in [
        tensor_type_bytes(
            9,
            Some(&[Dim::Param("batch".into()), Dim::Param("batch".into())]),
        ),
        tensor_type_bytes(1, Some(&[Dim::Value(2)])),
        tensor_type_bytes(1, Some(&[Dim::Value(2), Dim::Value(3)])),
    ] {
        let source =
            typed_identity_model(&[("X", original.clone())], &original, &[("X", conflicting)]);
        let error = lift(&source, BASE).expect_err("a conflicting source annotation must refuse");
        assert!(
            error.to_string().contains("incompatible tensor type"),
            "{error}"
        );
    }
}

fn initializer_domain_model(legacy_input_type: Option<&[u8]>) -> Vec<u8> {
    let value_type = tensor_type_bytes(1, Some(&[Dim::Value(4)]));
    let mut node = string_field(1, "X");
    node.extend(string_field(1, "W"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Add"));
    let mut tensor = varint_field(1, 4);
    tensor.extend(varint_field(2, 1));
    tensor.extend(string_field(8, "W"));
    tensor.extend(bytes_field(9, &[0; 16]));
    let mut graph = message_field(1, &node);
    graph.extend(string_field(2, "initializer value domains"));
    graph.extend(message_field(5, &tensor));
    graph.extend(message_field(11, &tensor_info_bytes("X", &value_type)));
    if let Some(legacy_input_type) = legacy_input_type {
        graph.extend(message_field(
            11,
            &tensor_info_bytes("W", legacy_input_type),
        ));
    }
    graph.extend(message_field(12, &tensor_info_bytes("Y", &value_type)));
    let mut model = varint_field(1, if legacy_input_type.is_some() { 3 } else { 8 });
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    model
}

#[test]
fn initializer_headers_and_inputs_share_their_admitted_value_space() {
    for legacy in [None, Some(tensor_type_bytes(1, None))] {
        let ttl = turtle(&initializer_domain_model(legacy.as_deref()));
        let input_domain = leaf_value_domain(&ttl, "X");
        let weight_domain = leaf_value_domain(&ttl, "W");
        assert_eq!(
            input_domain, weight_domain,
            "same admitted tensor type shares its Set"
        );
        let descriptor = subjects_with(&ttl, &math("expressionValueSpace"))
            .into_iter()
            .find(|descriptor| {
                object_of(&ttl, descriptor, &math("expressionValueSpace")).as_ref()
                    == Some(&weight_domain)
            })
            .unwrap();
        assert_eq!(
            label_of(&ttl, &descriptor).as_deref(),
            Some("tensor(float)[4]")
        );
        assert_eq!(typed(&ttl, "WeightTensor"), 1);
        assert_eq!(
            typed(&ttl, "VariableExpression"),
            2,
            "legacy declaration does not fork initializer identity"
        );
    }
}

#[test]
fn conflicting_legacy_input_cannot_replace_an_initializer_header_domain() {
    for conflicting in [
        tensor_type_bytes(9, Some(&[Dim::Value(4)])),
        tensor_type_bytes(1, Some(&[Dim::Value(5)])),
    ] {
        let error = lift(&initializer_domain_model(Some(&conflicting)), BASE)
            .expect_err("conflicting redundant initializer input must refuse");
        assert!(
            error.to_string().contains("incompatible tensor type"),
            "{error}"
        );
    }
}

#[test]
fn compatible_symbolic_refinement_and_negative_source_extent_are_distinct() {
    let symbolic = tensor_type_bytes(
        1,
        Some(&[Dim::Param("batch".into()), Dim::Param("batch".into())]),
    );
    let concrete = tensor_type_bytes(1, Some(&[Dim::Value(2), Dim::Value(2)]));
    let ttl = turtle(&typed_identity_model(
        &[("X", symbolic)],
        &concrete,
        &[("X", concrete.clone())],
    ));
    let domain = leaf_value_domain(&ttl, "X");
    let descriptor = subjects_with(&ttl, &math("expressionValueSpace"))
        .into_iter()
        .find(|descriptor| {
            object_of(&ttl, descriptor, &math("expressionValueSpace")).as_ref() == Some(&domain)
        })
        .unwrap();
    assert_eq!(
        label_of(&ttl, &descriptor).as_deref(),
        Some("tensor(float)[batch,batch]")
    );

    let negative = tensor_type_bytes(1, Some(&[Dim::Value(-1)]));
    let error = lift(
        &typed_identity_model(&[("X", negative.clone())], &negative, &[]),
        BASE,
    )
    .expect_err("negative declared type extent must refuse at source admission");
    assert!(
        error.to_string().contains("negative concrete axis extent"),
        "{error}"
    );
}

#[test]
fn an_axis_the_producer_left_unset_counts_toward_the_rank_and_claims_no_extent() {
    // A `TensorShapeProto.Dimension` with NEITHER arm set: the rank knows the axis, the
    // extent is unstated. Distinct from an extent of 0, which is a stated empty axis.
    let mut shape = Vec::new();
    shape.extend(message_field(1, &varint_field(1, 2)));
    shape.extend(message_field(1, &Vec::new()));
    let mut tensor = varint_field(1, 1);
    tensor.extend(message_field(2, &shape));
    let value_type = message_field(1, &tensor);
    let mut info = Vec::new();
    info.extend(string_field(1, "Y"));
    info.extend(message_field(2, &value_type));
    let mut node = Vec::new();
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "partial"));
    graph.extend(message_field(12, &info));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(integers_of(&ttl, &math("tensorRank")), vec![2], "{ttl}");
    assert_eq!(typed(&ttl, "TensorAxis"), 2, "{ttl}");
    assert_eq!(
        integers_of(&ttl, &math("axisExtent")),
        vec![2],
        "only the stated axis has an extent; no placeholder 0 or -1 stands in for the \
             unstated one:\n{ttl}"
    );
    assert_eq!(count(&ttl, &format!("<{}>", math("symbolicExtent"))), 0);
    assert!(ttl.contains("tensor(float)[2,?]"), "{ttl}");
}

#[test]
fn the_element_datatype_is_a_shared_term_not_a_fragment_of_a_label() {
    let ttl = turtle(MLP);
    assert_eq!(
        typed(&ttl, "TensorElementType"),
        1,
        "every tensor in the MLP is float, so ONE element-type term is shared:\n{ttl}"
    );
    let terms = subjects_with_literal(&ttl, RDFS_LABEL, "float");
    assert_eq!(terms.len(), 1);
    // Four expression types plus two weight tensors all name it.
    assert_eq!(
        count(&ttl, &format!("<{}>", math("tensorElementType"))),
        4 + 2,
        "{ttl}"
    );
}

#[test]
fn two_element_formats_are_two_terms() {
    let typed_info = |name: &str, elem: i64| {
        let mut tensor = varint_field(1, elem);
        tensor.extend(message_field(2, &message_field(1, &varint_field(1, 4))));
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &message_field(1, &tensor)));
        out
    };
    let mut node = Vec::new();
    node.extend(string_field(1, "X"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Cast"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "cast"));
    graph.extend(message_field(11, &typed_info("X", 1)));
    graph.extend(message_field(12, &typed_info("Y", 11)));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(
        typed(&ttl, "TensorElementType"),
        2,
        "float and double:\n{ttl}"
    );
    assert_eq!(subjects_with_literal(&ttl, RDFS_LABEL, "float").len(), 1);
    assert_eq!(subjects_with_literal(&ttl, RDFS_LABEL, "double").len(), 1);
    // …but the SHAPE is the same [4] for both, so exactly one shape node is minted.
    assert_eq!(
        typed(&ttl, "TensorShape"),
        1,
        "same extents, one shape object:\n{ttl}"
    );
}

#[test]
fn a_weight_tensor_and_its_slot_name_the_same_shape_object() {
    let ttl = turtle(MLP);
    let weights = subjects_with(&ttl, &math("weightOf"));
    assert_eq!(weights.len(), 2, "W and B");
    let w_type = subjects_with_literal(&ttl, RDFS_LABEL, "tensor(float)[4,3]");
    let slot_shape = shape_of(&ttl, w_type.iter().next().expect("the type node"));
    let weight_shapes: BTreeSet<String> = weights
        .iter()
        .map(|weight| shape_of(&ttl, weight))
        .collect();
    assert!(
        weight_shapes.contains(&slot_shape),
        "the math:WeightTensor and the math:ExpressionType typing its leaf must name the \
             SAME math:TensorShape, so sameness of shape is node identity:\n{ttl}"
    );
    assert_eq!(
        count(&ttl, &format!("<{}>", math("tensorShape"))),
        4 + 2,
        "four typed slots plus two weight tensors"
    );
}

#[test]
fn the_provenance_lands_on_the_retained_source_witness() {
    let ttl = turtle(MLP);
    assert_eq!(typed_gmeow(&ttl, "SoftwareAgent"), 1);
    assert_eq!(count(&ttl, &format!("<{}>", gmeow("wasAttributedTo"))), 1);
    assert!(
        ttl.contains("gmeow-math-lift 1"),
        "the producer label:\n{ttl}"
    );
    // ONNX metadata is a producer's free-form annotation, so it rides as a reified
    // math:SourceAnnotation on the retained witness — never as gmeow:Identifier, whose
    // own definition scopes it to an EXTERNAL identifier (ORCID, LEI, ROR) that resolves
    // somewhere.
    assert_eq!(
        typed_gmeow(&ttl, "Identifier"),
        0,
        "ONNX metadata identifies nothing; typing it gmeow:Identifier would assert an \
             external-identity claim the source never made:\n{ttl}"
    );
    assert_eq!(count(&ttl, &format!("<{}>", gmeow("hasIdentifier"))), 0);
    let witness = format!("<{}>", frame_witness());
    // ir_version plus model_version plus the one metadata_props entry.
    assert_eq!(typed(&ttl, "SourceAnnotation"), 3);
    assert_eq!(
        ttl.lines()
            .filter(|l| l.starts_with(&witness)
                && l.contains(&format!("<{}>", math("sourceAnnotation"))))
            .count(),
        3,
        "every metadata annotation hangs off the math:parseSource witness:\n{ttl}"
    );
}

#[test]
fn onnx_metadata_is_a_reified_key_value_pair_rather_than_prose() {
    let ttl = turtle(MLP);
    // The key and the value are SEPARATE, queryable literals, not one `key=value` string
    // a consumer has to re-split on the first `=`.
    assert_eq!(count(&ttl, &format!("<{}>", math("annotationKey"))), 3);
    assert_eq!(count(&ttl, &format!("<{}>", math("annotationValue"))), 3);
    for (key, value) in [
        ("model_license", "AGPL-3.0-only"),
        ("onnx.ir_version", "8"),
        ("onnx.model_version", "1"),
    ] {
        let subjects = subjects_with_literal(&ttl, &math("annotationKey"), key);
        assert_eq!(subjects.len(), 1, "exactly one annotation keyed `{key}`");
        let subject = subjects.iter().next().expect("a subject");
        assert!(
            ttl.lines().any(|line| {
                line.starts_with(subject)
                    && line.contains(&format!("<{}>", math("annotationValue")))
                    && line.contains(&format!("\"{value}\""))
            }),
            "`{key}` must carry the value `{value}` on the SAME node:\n{ttl}"
        );
        // The human rendering survives beside the structure, never instead of it.
        assert!(ttl.contains(&format!("{key}={value}")), "{ttl}");
    }
    // Nothing rides as an rdfs:comment any more: the prose carrier is gone, not doubled.
    assert_eq!(
        count(&ttl, "2000/01/rdf-schema#comment"),
        0,
        "metadata is structured now; the rdfs:comment carrier was replaced, not kept \
             alongside:\n{ttl}"
    );
}

fn frame_witness() -> String {
    RunFrame::mint(BridgeKind::Onnx, BASE, MLP).source_witness_iri
}

// -- the doctrines --------------------------------------------------------

#[test]
fn no_tensor_payload_byte_reaches_the_graph() {
    let ttl = turtle(MLP);
    // The fixture really does carry `raw_data`: 15 distinctive floats.
    assert!(
        MLP.windows(4)
            .any(|w| w == MLP_WEIGHT_PAYLOAD[0].to_le_bytes()),
        "the fixture must actually contain a payload for this test to mean anything"
    );
    for value in MLP_WEIGHT_PAYLOAD.iter().chain(MLP_BIAS_PAYLOAD) {
        let decimal = format!("{value}");
        assert!(
            !ttl.contains(&decimal),
            "the weight value {decimal} reached the graph; math:WeightTensor names and \
                 frames the data, it does not embed it:\n{ttl}"
        );
    }
    // Stronger than value-spotting: the ONNX lift emits no xsd:decimal literal AT ALL,
    // so no float from any payload could have crossed under any rendering.
    assert_eq!(
        count(&ttl, "XMLSchema#decimal"),
        0,
        "an ONNX lift carries shapes and indexes, never values:\n{ttl}"
    );
    // Nor as raw bytes in any escaped form.
    let hex = purrdf_hash::hex::encode(&MLP_WEIGHT_PAYLOAD[0].to_le_bytes());
    assert!(
        !ttl.contains(&hex),
        "raw payload bytes leaked as hex:\n{ttl}"
    );
    // And the structured shape did not become a back door: every integer the lift emits
    // belongs to one of the five shape/index predicates, none of which is a tensor value.
    let integer_lines = ttl
        .lines()
        .filter(|line| line.contains("XMLSchema#integer"))
        .count();
    let accounted: usize = [
        math("slotIndex"),
        math("axisIndex"),
        math("axisExtent"),
        math("tensorRank"),
        math("spaceDimension"),
    ]
    .iter()
    .map(|predicate| integers_of(&ttl, predicate).len())
    .sum();
    assert_eq!(
        integer_lines, accounted,
        "every integer this lift emits is a shape fact or an index — never an element \
             read out of a payload:\n{ttl}"
    );
}

#[test]
fn the_declared_lossy_rung_enumerates_the_loss_it_declares() {
    let ttl = turtle(MLP);
    // The rung is a logic:LossyLens, and math:unmappedConstruct's own useWhen requires a
    // lift declaring anything weaker than ExactPreservation to name what it lost.
    let residue: Vec<&str> = ttl
        .lines()
        .filter(|line| line.contains(&format!("<{}>", math("unmappedConstruct"))))
        .collect();
    assert_eq!(
        residue.len(),
        1,
        "the MLP has initializers, so their payload is the one named loss:\n{ttl}"
    );
    assert!(residue[0].contains("raw_data"), "{}", residue[0]);
    assert!(residue[0].contains("held by reference"), "{}", residue[0]);
    // The residue rides on the math:parseSource witness, nowhere else.
    assert!(residue[0].starts_with(&format!("<{}>", frame_witness())));
}

#[test]
fn a_model_with_no_initializer_claims_no_payload_loss() {
    // Residue is conditional on the construct actually being present: a graph with no
    // initializer lost no weight bytes, and claiming otherwise would be the mirror-image
    // of failing to enumerate.
    let ttl = turtle(&duplicated_subgraph_model());
    assert_eq!(count(&ttl, &format!("<{}>", math("unmappedConstruct"))), 0);
}

#[test]
fn a_metadata_entry_with_no_key_is_enumerated_as_residue_rather_than_dropped() {
    let mut metadata = Vec::new();
    metadata.extend(string_field(2, "orphaned"));
    let mut model = mlp();
    model.extend(message_field(14, &metadata));
    let ttl = turtle(&model);
    assert!(
        ttl.contains("empty `key`"),
        "an entry the lift read and could not pair must be named, not silently \
             skipped:\n{ttl}"
    );
    // …and it did NOT become a math:SourceAnnotation with an empty key.
    assert_eq!(typed(&ttl, "SourceAnnotation"), 3, "{ttl}");
    assert_eq!(
        subjects_with_literal(&ttl, &math("annotationKey"), "").len(),
        0
    );
}

#[test]
fn a_repeated_subgraph_collapses_to_one_content_addressed_node() {
    let ttl = turtle(&duplicated_subgraph_model());
    assert_eq!(
        typed(&ttl, "ApplicationExpression"),
        1,
        "two identical Relu(X) nodes are ONE computation:\n{ttl}"
    );
    assert_eq!(
        typed(&ttl, "NeuralLayer"),
        1,
        "the layer is keyed on the node, so it collapses with it"
    );
    assert_eq!(
        count(&ttl, &format!("<{}>", math("computationNode"))),
        1,
        "the duplicate math:computationNode edge collapses in the codec"
    );
    assert_eq!(typed(&ttl, "ArgumentSlot"), 1);
}

#[test]
fn distinct_structure_does_grow_the_graph() {
    let shared = lift(&duplicated_subgraph_model(), BASE).expect("lifts");
    let distinct = lift(MLP, BASE).expect("lifts");
    assert!(
        distinct.codomain_nodes > shared.codomain_nodes,
        "the fact count grows with DISTINCT structure"
    );
}

#[test]
fn a_relift_of_the_same_source_is_byte_identical() {
    let a = lift(MLP, BASE).expect("lifts").turtle;
    let b = lift(MLP, BASE).expect("lifts").turtle;
    assert_eq!(a, b, "the lift is idempotent: no clock, no counter");
}

#[test]
fn a_different_model_mints_a_different_run() {
    let a = lift(MLP, BASE).expect("lifts");
    let b = lift(&duplicated_subgraph_model(), BASE).expect("lifts");
    assert_ne!(a.run_iri, b.run_iri);
}

#[test]
fn every_codomain_node_carries_the_back_edge_the_native_lint_reads() {
    let lifted = lift(MLP, BASE).expect("lifts");
    assert_eq!(
        count(&lifted.turtle, &format!("<{}>", gmeow("wasGeneratedBy"))),
        lifted.codomain_nodes,
        "exactly one gmeow:wasGeneratedBy per generated node"
    );
}

#[test]
fn the_run_frame_travels_with_every_lift_at_the_crisp_rung() {
    let ttl = turtle(MLP);
    for required in [
        math("ONNXIngestRun"),
        math("parseSource"),
        logic("instantiatesSchema"),
        logic("instantiatesPlan"),
        math("ingestCorrespondence"),
        logic("LossyLens"),
        logic("mnemomorphic"),
    ] {
        assert!(ttl.contains(&required), "the frame is missing `{required}`");
    }
    assert!(
        ttl.contains(&logic("Crisp")),
        "an ONNX graph is exact, not interpreted"
    );
    assert!(
        !ttl.contains(&logic("Vague")),
        "the ONNX rung is Crisp, unlike R's"
    );
}

#[test]
fn a_lifted_graph_carries_no_private_use_language_tag() {
    let lifted = lift(MLP, BASE).expect("lifts");
    assert!(
        !lifted.turtle.contains("x-gmeow-"),
        "consumer output must not leak a private-use tag"
    );
}

#[test]
fn no_operator_is_typed_as_a_function_whose_domain_the_model_never_states() {
    let ttl = turtle(MLP);
    assert_eq!(
        typed(&ttl, "ActivationFunction"),
        0,
        "Relu is not typed math:ActivationFunction: math:Function owes min-1 math:domain \
             and math:codomain for the operator, which boundary value-space metadata does not establish"
    );
    assert_eq!(count(&ttl, &format!("<{}>", math("domain"))), 0);
    assert_eq!(count(&ttl, &format!("<{}>", math("codomain"))), 0);
}

#[test]
fn an_attribute_participates_in_the_operators_identity() {
    let gemm = |trans_b: i64, out: &str| {
        let mut attribute = Vec::new();
        attribute.extend(string_field(1, "transB"));
        attribute.extend(varint_field(3, trans_b));
        attribute.extend(varint_field(20, 2));
        let mut node = Vec::new();
        node.extend(string_field(1, "X"));
        node.extend(string_field(2, out));
        node.extend(string_field(4, "Gemm"));
        node.extend(message_field(5, &attribute));
        node
    };
    let value_type = {
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &message_field(1, &varint_field(1, 4))));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };
    let mut graph = Vec::new();
    graph.extend(message_field(1, &gemm(0, "Y0")));
    graph.extend(message_field(1, &gemm(1, "Y1")));
    graph.extend(string_field(2, "gemms"));
    graph.extend(message_field(11, &info("X")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(
        typed(&ttl, "ApplicationExpression"),
        2,
        "transB=0 and transB=1 are DIFFERENT operations:\n{ttl}"
    );
    assert!(ttl.contains("Gemm(transB=0)"), "{ttl}");
    assert!(ttl.contains("Gemm(transB=1)"), "{ttl}");
    assert_eq!(
        typed(&ttl, "MathematicalSymbol"),
        1,
        "but they are the SAME opset symbol"
    );
}

#[test]
fn an_absent_optional_input_leaves_the_surviving_slots_contiguous() {
    // ONNX spells an omitted optional input as an empty name; Clip(X, "", max) is the
    // canonical case. The remaining operands must still index 0, 1, … with no gap.
    let value_type = {
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &message_field(1, &varint_field(1, 4))));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };
    let mut node = Vec::new();
    node.extend(string_field(1, "X"));
    node.extend(string_field(1, ""));
    node.extend(string_field(1, "M"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Clip"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "clipped"));
    graph.extend(message_field(11, &info("X")));
    graph.extend(message_field(11, &info("M")));
    graph.extend(message_field(12, &info("Y")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(typed(&ttl, "ArgumentSlot"), 2, "X and M, not three:\n{ttl}");
    assert_eq!(
        integers_of(&ttl, &math("slotIndex")),
        vec![0, 1],
        "no gap, no phantom slot:\n{ttl}"
    );
}

#[test]
fn a_multi_output_node_projects_each_result_distinctly() {
    let value_type = {
        let mut tensor = varint_field(1, 1);
        tensor.extend(message_field(2, &message_field(1, &varint_field(1, 4))));
        message_field(1, &tensor)
    };
    let info = |name: &str| {
        let mut out = Vec::new();
        out.extend(string_field(1, name));
        out.extend(message_field(2, &value_type));
        out
    };
    let mut split = Vec::new();
    split.extend(string_field(1, "X"));
    split.extend(string_field(2, "A"));
    split.extend(string_field(2, "B"));
    split.extend(string_field(4, "Split"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &split));
    graph.extend(string_field(2, "split"));
    graph.extend(message_field(11, &info("X")));
    graph.extend(message_field(12, &info("A")));
    graph.extend(message_field(12, &info("B")));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));

    let ttl = turtle(&model);
    assert_eq!(
        typed(&ttl, "ApplicationExpression"),
        3,
        "the Split plus one projection per output:\n{ttl}"
    );
    assert!(ttl.contains("output projection at index 0"), "{ttl}");
    assert!(ttl.contains("output projection at index 1"), "{ttl}");
}

#[test]
fn a_model_with_no_initializer_emits_no_parameter_space_rather_than_an_unframed_one() {
    let ttl = turtle(&duplicated_subgraph_model());
    assert_eq!(
        typed(&ttl, "ParameterSpace"),
        0,
        "no parameter block, no space"
    );
    assert_eq!(typed(&ttl, "WeightTensor"), 0);
    assert_eq!(count(&ttl, &format!("<{}>", math("inParameterSpace"))), 0);
    assert!(
        typed(&ttl, "TensorComputationGraph") == 1,
        "but the architecture still lifts"
    );
}

#[test]
fn a_negative_initializer_extent_refuses_rather_than_framing_a_nonsense_space() {
    let mut init = Vec::new();
    init.extend(varint_field(1, -3));
    init.extend(varint_field(2, 1));
    init.extend(string_field(8, "W"));
    init.extend(bytes_field(9, &[0, 0, 0, 0]));
    let mut node = Vec::new();
    node.extend(string_field(1, "W"));
    node.extend(string_field(2, "Y"));
    node.extend(string_field(4, "Relu"));
    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "bad"));
    graph.extend(message_field(5, &init));
    let mut model = varint_field(1, 8);
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &varint_field(2, 18)));
    let err = lift(&model, BASE).expect_err("a negative extent must not lift");
    assert!(format!("{err}").contains("negative extent"), "{err}");
}
