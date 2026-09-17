// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::onnx::encode::{bytes_field, message_field, string_field, varint_field, wire_float};

#[test]
fn the_model_fields_decode_at_their_documented_numbers() {
    let mut body = Vec::new();
    body.extend(varint_field(1, 9)); // ir_version
    body.extend(string_field(2, "gmeow-test")); // producer_name
    body.extend(string_field(3, "0.1")); // producer_version
    body.extend(string_field(4, "ca.blackcat")); // domain
    body.extend(varint_field(5, 3)); // model_version
    let mut opset = Vec::new();
    opset.extend(string_field(1, "ai.onnx.ml"));
    opset.extend(varint_field(2, 5));
    body.extend(message_field(8, &opset));
    let mut meta = Vec::new();
    meta.extend(string_field(1, "license"));
    meta.extend(string_field(2, "AGPL-3.0-only"));
    body.extend(message_field(14, &meta));

    let model = ModelProto::decode(&body).expect("a well-formed ModelProto");
    assert_eq!(model.ir_version, 9);
    assert_eq!(model.producer_name, "gmeow-test");
    assert_eq!(model.producer_version, "0.1");
    assert_eq!(model.domain, "ca.blackcat");
    assert_eq!(model.model_version, 3);
    assert_eq!(model.opset_import.len(), 1);
    assert_eq!(model.opset_import[0].domain, "ai.onnx.ml");
    assert_eq!(model.opset_import[0].version, 5);
    assert_eq!(model.metadata_props[0].key, "license");
    assert_eq!(model.metadata_props[0].value, "AGPL-3.0-only");
    assert!(model.graph.is_none(), "no graph field was written");
}

#[test]
fn an_unknown_model_field_number_is_skipped_without_desynchronizing() {
    let mut body = Vec::new();
    // training_info (20) and functions (25) are real ONNX fields this subset ignores.
    body.extend(message_field(20, &string_field(1, "ignored")));
    body.extend(varint_field(1, 9));
    body.extend(message_field(25, &string_field(1, "also ignored")));
    body.extend(string_field(2, "after-the-unknowns"));
    let model = ModelProto::decode(&body).expect("unknown fields step over cleanly");
    assert_eq!(model.ir_version, 9);
    assert_eq!(model.producer_name, "after-the-unknowns");
}

#[test]
fn a_graph_decodes_its_nodes_initializers_and_boundary_values() {
    let mut node = Vec::new();
    node.extend(string_field(1, "X"));
    node.extend(string_field(1, "W"));
    node.extend(string_field(2, "XW"));
    node.extend(string_field(3, "mm"));
    node.extend(string_field(4, "MatMul"));
    node.extend(string_field(7, ""));

    let mut init = Vec::new();
    init.extend(varint_field(1, 4));
    init.extend(varint_field(1, 3));
    init.extend(varint_field(2, 1));
    init.extend(string_field(8, "W"));

    let mut graph = Vec::new();
    graph.extend(message_field(1, &node));
    graph.extend(string_field(2, "mlp"));
    graph.extend(message_field(5, &init));

    let model = ModelProto::decode(&message_field(7, &graph)).expect("a graph decodes");
    let graph = model.graph.expect("field 7 is the graph");
    assert_eq!(graph.name, "mlp");
    assert_eq!(graph.node.len(), 1);
    assert_eq!(graph.node[0].op_type, "MatMul");
    assert_eq!(graph.node[0].input, vec!["X".to_owned(), "W".to_owned()]);
    assert_eq!(graph.node[0].output, vec!["XW".to_owned()]);
    assert_eq!(graph.initializer[0].name, "W");
    assert_eq!(graph.initializer[0].dims, vec![4, 3]);
    assert_eq!(graph.initializer[0].data_type, 1);
}

#[test]
fn a_tensor_payload_never_reaches_the_typed_header() {
    let mut init = Vec::new();
    init.extend(varint_field(1, 2));
    init.extend(varint_field(2, 1));
    init.extend(string_field(8, "W"));
    // raw_data (9): a payload the header must not carry.
    init.extend(bytes_field(
        9,
        &[0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33, 0x44],
    ));
    // float_data (4), packed.
    let mut floats = Vec::new();
    floats.extend(wire_float(1.5));
    floats.extend(wire_float(2.5));
    init.extend(bytes_field(4, &floats));

    let mut graph = Vec::new();
    graph.extend(message_field(5, &init));
    let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
    let tensor = &model.graph.expect("a graph").initializer[0];
    assert_eq!(tensor.name, "W");
    assert_eq!(tensor.dims, vec![2]);
    // The struct has no field that could hold the payload; the round-tripped Debug is
    // the exhaustive check that nothing crept in.
    let rendered = format!("{tensor:?}");
    assert!(
        !rendered.contains("222"),
        "0xde 0xad must not survive: {rendered}"
    );
    assert!(
        !rendered.contains("1.5"),
        "float_data must not survive: {rendered}"
    );
}

#[test]
fn packed_and_unpacked_repeated_dims_both_decode() {
    let mut unpacked = Vec::new();
    unpacked.extend(varint_field(1, 4));
    unpacked.extend(varint_field(1, 3));
    unpacked.extend(string_field(8, "W"));

    let mut packed = Vec::new();
    packed.extend(bytes_field(1, &[4, 3]));
    packed.extend(string_field(8, "W"));

    for body in [unpacked, packed] {
        let mut graph = Vec::new();
        graph.extend(message_field(5, &body));
        let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
        assert_eq!(
            model.graph.expect("a graph").initializer[0].dims,
            vec![4, 3]
        );
    }
}

#[test]
fn a_value_info_carries_its_element_type_and_shape() {
    let mut dim_a = Vec::new();
    dim_a.extend(varint_field(1, 1));
    let mut dim_b = Vec::new();
    dim_b.extend(string_field(2, "batch"));
    let mut shape = Vec::new();
    shape.extend(message_field(1, &dim_a));
    shape.extend(message_field(1, &dim_b));
    let mut tensor_type = Vec::new();
    tensor_type.extend(varint_field(1, 1));
    tensor_type.extend(message_field(2, &shape));
    let mut value_type = Vec::new();
    value_type.extend(message_field(1, &tensor_type));
    let mut info = Vec::new();
    info.extend(string_field(1, "X"));
    info.extend(message_field(2, &value_type));
    let mut graph = Vec::new();
    graph.extend(message_field(11, &info));

    let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
    let input = &model.graph.expect("a graph").input[0];
    assert_eq!(input.name, "X");
    let ty = input.value_type.as_ref().expect("a declared type");
    let tensor = ty.tensor_type.as_ref().expect("the tensor arm");
    assert_eq!(tensor.elem_type, 1);
    assert_eq!(
        tensor.shape.as_ref().expect("a shape"),
        &vec![Dim::Value(1), Dim::Param("batch".to_owned())]
    );
}

#[test]
fn a_non_tensor_type_constructor_is_recorded_by_name() {
    for (field, expected) in [
        (4_u32, "sequence_type"),
        (5, "map_type"),
        (8, "sparse_tensor_type"),
        (9, "optional_type"),
    ] {
        let value_type = message_field(field, &string_field(1, "x"));
        let mut info = Vec::new();
        info.extend(string_field(1, "X"));
        info.extend(message_field(2, &value_type));
        let graph = message_field(11, &info);
        let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
        let ty = model.graph.expect("a graph").input[0]
            .value_type
            .clone()
            .expect("a type");
        assert_eq!(ty.unstructured, Some(expected));
    }
}

#[test]
fn attribute_arms_decode_and_render_canonically() {
    let mut int_attr = Vec::new();
    int_attr.extend(string_field(1, "transB"));
    int_attr.extend(varint_field(3, 1));
    int_attr.extend(varint_field(20, 2));
    let mut float_attr = Vec::new();
    float_attr.extend(string_field(1, "alpha"));
    float_attr.extend(wire_float_field(2, 2.0));
    float_attr.extend(varint_field(20, 1));
    let mut str_attr = Vec::new();
    str_attr.extend(string_field(1, "mode"));
    str_attr.extend(bytes_field(4, b"constant"));
    str_attr.extend(varint_field(20, 3));
    let mut ints_attr = Vec::new();
    ints_attr.extend(string_field(1, "axes"));
    ints_attr.extend(bytes_field(8, &[0, 2]));
    ints_attr.extend(varint_field(20, 7));

    let mut node = Vec::new();
    node.extend(string_field(4, "Gemm"));
    for attr in [&int_attr, &float_attr, &str_attr, &ints_attr] {
        node.extend(message_field(5, attr));
    }
    let graph = message_field(1, &node);
    let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
    let attrs = &model.graph.expect("a graph").node[0].attribute;
    assert_eq!(attrs[0].render(), "transB=1");
    assert_eq!(attrs[0].attribute_type, 2);
    assert_eq!(attrs[1].render(), "alpha=2.0");
    assert_eq!(attrs[2].render(), "mode=\"constant\"");
    assert_eq!(attrs[3].render(), "axes=[0,2]");
}

#[test]
fn an_attribute_subgraph_is_recorded_by_name_rather_than_dropped() {
    let mut attr = Vec::new();
    attr.extend(string_field(1, "body"));
    attr.extend(message_field(6, &string_field(2, "loop-body")));
    let mut node = Vec::new();
    node.extend(string_field(4, "Loop"));
    node.extend(message_field(5, &attr));
    let graph = message_field(1, &node);
    let model = ModelProto::decode(&message_field(7, &graph)).expect("decodes");
    assert_eq!(
        model.graph.expect("a graph").node[0].attribute[0].unstructured,
        Some("g (a control-flow subgraph)")
    );
}

#[test]
fn the_element_count_is_the_product_of_the_extents() {
    let tensor = TensorProto {
        name: "W".to_owned(),
        dims: vec![4, 3],
        data_type: 1,
    };
    assert_eq!(tensor.element_count().expect("a count"), 12);

    let scalar = TensorProto {
        name: "b".to_owned(),
        dims: Vec::new(),
        data_type: 1,
    };
    assert_eq!(scalar.element_count().expect("a count"), 1, "empty product");
}

#[test]
fn a_negative_extent_refuses_rather_than_producing_a_nonsense_dimension() {
    let tensor = TensorProto {
        name: "W".to_owned(),
        dims: vec![4, -3],
        data_type: 1,
    };
    let err = tensor
        .element_count()
        .expect_err("a negative extent is not a shape");
    assert!(format!("{err}").contains("negative extent"), "{err}");
}

#[test]
fn an_element_count_overflowing_sixty_four_bits_refuses() {
    let tensor = TensorProto {
        name: "W".to_owned(),
        dims: vec![i64::MAX, 4],
        data_type: 1,
    };
    let err = tensor.element_count().expect_err("the product overflows");
    assert!(format!("{err}").contains("overflows"), "{err}");
}

#[test]
fn every_declared_onnx_data_type_code_has_a_name_and_nothing_else_does() {
    for code in 1..=23 {
        assert!(data_type_name(code).is_some(), "code {code} must be named");
    }
    assert!(data_type_name(0).is_none(), "UNDEFINED is not a data type");
    assert!(
        data_type_name(24).is_none(),
        "an unassigned code is not named"
    );
    assert!(data_type_name(-1).is_none());
}

#[test]
fn a_malformed_stream_fails_at_the_model_level_too() {
    // A graph field whose length runs past the end of the file.
    let err = ModelProto::decode(&[0x3a, 0x40, 0x01]).expect_err("truncated");
    assert!(
        format!("{err}").contains("math.lift.onnx.wire") || format!("{err}").contains("truncated"),
        "{err}"
    );
}

fn wire_float_field(number: u32, value: f32) -> Vec<u8> {
    let mut out = crate::onnx::encode::tag(number, 5);
    out.extend(wire_float(value));
    out
}
