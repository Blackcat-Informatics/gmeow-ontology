// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A minimal protobuf **encoder**, for tests and for building the committed `.onnx`
//! fixtures.
//!
//! The decoder in [`super::wire`] is the shipped artifact; this is its test-side inverse.
//! It exists so the fixtures under `crates/math-lift/fixtures/` are *derived* rather than
//! hand-typed hex: `fixtures/mlp.onnx` is byte-pinned against [`mlp`] and
//! `fixtures/truncated.onnx` against [`truncated`], so neither can silently rot away from
//! the model it is supposed to be. It is the same discipline
//! `crates/conformance/src/external/tptp/lower_fol.rs` applies to its committed TSTP
//! derivation.
//!
//! Keeping the encoder separate from the decoder also keeps the decoder honest: a test that
//! round-trips a message through one shared codec proves nothing about either. Here the two
//! directions are written independently against `onnx.proto`, so a field number wrong in
//! both would have to be wrong twice, the same way.

/// A field tag: `(number << 3) | wire_type`, varint-encoded.
pub fn tag(number: u32, wire: u32) -> Vec<u8> {
    varint(u64::from((number << 3) | wire))
}

/// A base-128 varint.
pub fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = u8::try_from(value & 0x7f).expect("seven bits");
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

/// A varint field (wire type 0).
pub fn varint_field(number: u32, value: i64) -> Vec<u8> {
    let mut out = tag(number, 0);
    #[allow(clippy::cast_sign_loss)]
    let raw = value as u64;
    out.extend(varint(raw));
    out
}

/// A length-delimited field (wire type 2) carrying arbitrary bytes.
pub fn bytes_field(number: u32, value: &[u8]) -> Vec<u8> {
    let mut out = tag(number, 2);
    out.extend(varint(value.len() as u64));
    out.extend_from_slice(value);
    out
}

/// A `string` field.
pub fn string_field(number: u32, value: &str) -> Vec<u8> {
    bytes_field(number, value.as_bytes())
}

/// An embedded-message field.
pub fn message_field(number: u32, body: &[u8]) -> Vec<u8> {
    bytes_field(number, body)
}

/// The four little-endian bytes of an IEEE-754 `f32`, with no tag.
pub fn wire_float(value: f32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

// ── The committed fixtures ────────────────────────────────────────────────────

/// The distinctive weight payload written into `fixtures/mlp.onnx`.
///
/// These values exist to be **found missing**: the blob-by-reference test asserts that no
/// rendering of them reaches the lifted graph. They are chosen so that their decimal forms
/// (`6553.25`, …) collide with nothing the lift legitimately emits — no shape extent, no
/// slot index, no parameter-space dimension.
pub const MLP_WEIGHT_PAYLOAD: &[f32] = &[
    6553.25, 6554.25, 6555.25, 6556.25, 6557.25, 6558.25, 6559.25, 6560.25, 6561.25, 6562.25,
    6563.25, 6564.25,
];

/// The distinctive bias payload written into `fixtures/mlp.onnx`.
pub const MLP_BIAS_PAYLOAD: &[f32] = &[7771.5, 7772.5, 7773.5];

/// One `TypeProto` for `tensor(elem)[dims…]`, dims given as concrete extents.
fn tensor_type(elem_type: i64, dims: &[i64]) -> Vec<u8> {
    let mut shape = Vec::new();
    for dim in dims {
        shape.extend(message_field(1, &varint_field(1, *dim)));
    }
    let mut tensor = Vec::new();
    tensor.extend(varint_field(1, elem_type));
    tensor.extend(message_field(2, &shape));
    message_field(1, &tensor)
}

/// One `ValueInfoProto`.
fn value_info(name: &str, elem_type: i64, dims: &[i64]) -> Vec<u8> {
    let mut info = Vec::new();
    info.extend(string_field(1, name));
    info.extend(message_field(2, &tensor_type(elem_type, dims)));
    info
}

/// One `TensorProto` initializer, header **and** a real `raw_data` payload.
///
/// The payload is genuinely present in the file — that is the point. A fixture whose weights
/// were empty would make the "no payload byte reaches the graph" test vacuous.
fn initializer(name: &str, dims: &[i64], payload: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    for dim in dims {
        out.extend(varint_field(1, *dim));
    }
    out.extend(varint_field(2, 1)); // data_type = FLOAT
    out.extend(string_field(8, name));
    let mut raw = Vec::new();
    for value in payload {
        raw.extend(wire_float(*value));
    }
    out.extend(bytes_field(9, &raw)); // raw_data
    out
}

/// One `NodeProto`.
fn node(name: &str, op_type: &str, inputs: &[&str], outputs: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for input in inputs {
        out.extend(string_field(1, input));
    }
    for output in outputs {
        out.extend(string_field(2, output));
    }
    out.extend(string_field(3, name));
    out.extend(string_field(4, op_type));
    out
}

/// The bytes of `fixtures/mlp.onnx`: a real, minimal, byte-valid ONNX model.
///
/// One hidden layer of a multilayer perceptron:
///
/// ```text
/// X : tensor(float)[1, 4]
/// W : tensor(float)[4, 3]   (initializer, 12 weights)
/// B : tensor(float)[3]      (initializer, 3 biases)
///
/// XW = MatMul(X, W)
/// XB = Add(XW, B)
/// Y  = Relu(XB) : tensor(float)[1, 3]
/// ```
///
/// with opset `ai.onnx` v18, a producer, and one `metadata_props` entry.
pub fn mlp() -> Vec<u8> {
    let mut graph = Vec::new();
    graph.extend(message_field(
        1,
        &node("mm", "MatMul", &["X", "W"], &["XW"]),
    ));
    graph.extend(message_field(
        1,
        &node("bias", "Add", &["XW", "B"], &["XB"]),
    ));
    graph.extend(message_field(1, &node("act", "Relu", &["XB"], &["Y"])));
    graph.extend(string_field(2, "mlp"));
    graph.extend(message_field(
        5,
        &initializer("W", &[4, 3], MLP_WEIGHT_PAYLOAD),
    ));
    graph.extend(message_field(5, &initializer("B", &[3], MLP_BIAS_PAYLOAD)));
    graph.extend(message_field(11, &value_info("X", 1, &[1, 4])));
    graph.extend(message_field(12, &value_info("Y", 1, &[1, 3])));
    graph.extend(message_field(13, &value_info("XW", 1, &[1, 3])));

    let mut opset = Vec::new();
    opset.extend(string_field(1, "")); // the default ai.onnx domain
    opset.extend(varint_field(2, 18));

    let mut metadata = Vec::new();
    metadata.extend(string_field(1, "model_license"));
    metadata.extend(string_field(2, "AGPL-3.0-only"));

    let mut model = Vec::new();
    model.extend(varint_field(1, 8)); // ir_version (IR 8 is the one opset 18 ships with)
    model.extend(string_field(2, "gmeow-math-lift")); // producer_name
    model.extend(string_field(3, "1")); // producer_version
    model.extend(varint_field(5, 1)); // model_version
    model.extend(message_field(7, &graph));
    model.extend(message_field(8, &opset));
    model.extend(message_field(14, &metadata));
    model
}

/// The bytes of `fixtures/truncated.onnx`: a genuinely malformed protobuf.
///
/// It is [`mlp`] cut off inside the `GraphProto` sub-message, so the graph's declared length
/// runs past the end of the file. Not a random byte soup: a real file that a naive reader
/// would happily decode as a short model, which is exactly the degradation the decoder must
/// refuse.
pub fn truncated() -> Vec<u8> {
    let full = mlp();
    // Cut a third of the way in — comfortably inside the graph sub-message, after the
    // ir_version/producer preamble.
    full[..full.len() / 3].to_vec()
}
