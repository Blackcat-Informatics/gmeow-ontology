// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONNX parse tier, layer 1: the `onnx.proto` subset, as typed Rust.
//!
//! Every struct here is the decoded shape of one `onnx.proto` message, and **every field
//! carries its protobuf field number in a comment** so a reviewer can check it against the
//! schema without a generator in the loop. That is the whole reason the wire decoder is
//! hand-rolled: the field numbers are the load-bearing knowledge, and they belong somewhere
//! a human reads.
//!
//! # Blob-by-reference is enforced HERE, not downstream
//!
//! [`TensorProto`] decodes a **header only** — `dims`, `data_type`, `name`. The payload
//! fields (`float_data` 4, `int32_data` 5, `string_data` 6, `int64_data` 7, `raw_data` 9,
//! `double_data` 10, `uint64_data` 11, `external_data` 13) are stepped over by
//! [`Reader::skip_field`](super::wire::Reader::skip_field) and never materialize as values.
//! `MATHEMATICS-RUNTIME.md`'s blob-by-reference doctrine says a `math:WeightTensor` "names
//! and frames the data; it does not embed it", and the cheapest way to keep that promise
//! total is to make the payload *unreachable from the parse tier at all*: there is no field
//! on `TensorProto` that could hold a weight byte, so no later edit can leak one by
//! accident.
//!
//! # What the subset deliberately does not structure
//!
//! `TrainingInfoProto` (ModelProto 20), `FunctionProto` (ModelProto 25),
//! `SparseTensorProto`, quantization annotations, and `TypeProto`'s sequence/map/optional
//! arms are not modeled. Where their absence would change what a lift *means* — a
//! `TypeProto` that is not a tensor type, an `AttributeProto` carrying a control-flow
//! subgraph — the decoder records **which** construct it met ([`TypeProto::unstructured`],
//! [`AttributeProto::unstructured`]) so the lift can hard-fail naming it, rather than
//! quietly lifting a model whose meaning it did not read.

use gmeow_errors::Diag;

use crate::error::{OnnxUnliftable, OnnxWire};
use crate::onnx::wire::{Reader, WireType};

/// `onnx.ModelProto` — the root message of every `.onnx` file.
///
/// | field | № | note |
/// |---|---|---|
/// | `ir_version` | 1 | `int64` |
/// | `producer_name` | 2 | `string` |
/// | `producer_version` | 3 | `string` |
/// | `domain` | 4 | `string` |
/// | `model_version` | 5 | `int64` |
/// | `doc_string` | 6 | `string`, skipped |
/// | `graph` | 7 | `GraphProto` |
/// | `opset_import` | 8 | repeated `OperatorSetIdProto` |
/// | `metadata_props` | 14 | repeated `StringStringEntryProto` |
/// | `training_info` | 20 | repeated `TrainingInfoProto`, skipped |
/// | `functions` | 25 | repeated `FunctionProto`, skipped |
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelProto {
    /// The ONNX IR version the file is written against (field 1).
    pub ir_version: i64,
    /// The exporting tool (field 2).
    pub producer_name: String,
    /// The exporting tool's version (field 3).
    pub producer_version: String,
    /// The model's own namespace (field 4).
    pub domain: String,
    /// The model's own version (field 5).
    pub model_version: i64,
    /// The forward-pass graph (field 7). Absent in a model that carries only functions.
    pub graph: Option<GraphProto>,
    /// The operator sets the graph draws its operators from (field 8).
    pub opset_import: Vec<OperatorSetId>,
    /// Free-form producer metadata (field 14).
    pub metadata_props: Vec<StringStringEntry>,
}

impl ModelProto {
    /// Decode a whole `.onnx` byte stream.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on any malformation of the protobuf encoding, with the offending byte
    /// offset. There is no partial decode and no lenient mode.
    pub fn decode(bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let mut reader = Reader::new(bytes);
        let model = Self::read(&mut reader)?;
        reader.finish()?;
        Ok(model)
    }

    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.ir_version = reader.read_i64()?,
                2 => out.producer_name = reader.read_string()?.to_owned(),
                3 => out.producer_version = reader.read_string()?.to_owned(),
                4 => out.domain = reader.read_string()?.to_owned(),
                5 => out.model_version = reader.read_i64()?,
                7 => {
                    let mut nested = reader.read_message()?;
                    out.graph = Some(GraphProto::read(&mut nested)?);
                }
                8 => {
                    let mut nested = reader.read_message()?;
                    out.opset_import.push(OperatorSetId::read(&mut nested)?);
                }
                14 => {
                    let mut nested = reader.read_message()?;
                    out.metadata_props
                        .push(StringStringEntry::read(&mut nested)?);
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.OperatorSetIdProto` — one entry of `ModelProto.opset_import`.
///
/// | field | № |
/// |---|---|
/// | `domain` | 1 |
/// | `version` | 2 |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperatorSetId {
    /// The operator-set namespace (field 1). The empty string is the default `ai.onnx` set.
    pub domain: String,
    /// The operator-set version (field 2).
    pub version: i64,
}

impl OperatorSetId {
    /// The operator-set namespace, with ONNX's empty-string default spelled out.
    #[must_use]
    pub fn spelled_domain(&self) -> &str {
        if self.domain.is_empty() {
            "ai.onnx"
        } else {
            &self.domain
        }
    }

    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.domain = reader.read_string()?.to_owned(),
                2 => out.version = reader.read_i64()?,
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.GraphProto` — the computation graph itself.
///
/// | field | № | note |
/// |---|---|---|
/// | `node` | 1 | repeated `NodeProto`, in topological order |
/// | `name` | 2 | `string` |
/// | `initializer` | 5 | repeated `TensorProto` (the weights) |
/// | `doc_string` | 10 | skipped |
/// | `input` | 11 | repeated `ValueInfoProto` |
/// | `output` | 12 | repeated `ValueInfoProto` |
/// | `value_info` | 13 | repeated `ValueInfoProto` (intermediates) |
/// | `quantization_annotation` | 14 | skipped |
/// | `sparse_initializer` | 15 | skipped |
/// | `metadata_props` | 16 | skipped (the model-level one is the provenance surface) |
///
/// Field numbers 3, 4 and 6–9 are retired in `onnx.proto` and are therefore unknown here;
/// they step over cleanly through the wire decoder's `skip_field`.
///
/// Not `Eq`: an [`AttributeProto`] may carry an `f32`, and a decoder must not pretend that
/// float equality is an equivalence relation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphProto {
    /// The graph's name (field 2).
    pub name: String,
    /// The operator nodes (field 1).
    pub node: Vec<NodeProto>,
    /// The initializers — the learned weights, **header only** (field 5).
    pub initializer: Vec<TensorProto>,
    /// The typed graph inputs (field 11).
    pub input: Vec<ValueInfoProto>,
    /// The typed graph outputs (field 12).
    pub output: Vec<ValueInfoProto>,
    /// Types declared for intermediate values (field 13).
    pub value_info: Vec<ValueInfoProto>,
}

impl GraphProto {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => {
                    let mut nested = reader.read_message()?;
                    out.node.push(NodeProto::read(&mut nested)?);
                }
                2 => out.name = reader.read_string()?.to_owned(),
                5 => {
                    let mut nested = reader.read_message()?;
                    out.initializer.push(TensorProto::read(&mut nested)?);
                }
                11 => {
                    let mut nested = reader.read_message()?;
                    out.input.push(ValueInfoProto::read(&mut nested)?);
                }
                12 => {
                    let mut nested = reader.read_message()?;
                    out.output.push(ValueInfoProto::read(&mut nested)?);
                }
                13 => {
                    let mut nested = reader.read_message()?;
                    out.value_info.push(ValueInfoProto::read(&mut nested)?);
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.NodeProto` — one operator application in the graph.
///
/// | field | № |
/// |---|---|
/// | `input` | 1 | repeated `string` |
/// | `output` | 2 | repeated `string` |
/// | `name` | 3 | `string` |
/// | `op_type` | 4 | `string` |
/// | `attribute` | 5 | repeated `AttributeProto` |
/// | `doc_string` | 6 | skipped |
/// | `domain` | 7 | `string` |
/// | `overload` | 8 | skipped |
/// | `metadata_props` | 9 | skipped |
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeProto {
    /// The node's own name (field 3). Optional in ONNX and used only for labelling.
    pub name: String,
    /// The operator (field 4) — `MatMul`, `Relu`, `Conv`, …
    pub op_type: String,
    /// The operator-set domain the operator is drawn from (field 7).
    pub domain: String,
    /// The names of the values this node reads (field 1). An empty name is ONNX's spelling
    /// of "this optional input is absent".
    pub input: Vec<String>,
    /// The names this node's results are bound to (field 2).
    pub output: Vec<String>,
    /// The operator's configuration (field 5).
    pub attribute: Vec<AttributeProto>,
}

impl NodeProto {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.input.push(reader.read_string()?.to_owned()),
                2 => out.output.push(reader.read_string()?.to_owned()),
                3 => out.name = reader.read_string()?.to_owned(),
                4 => out.op_type = reader.read_string()?.to_owned(),
                5 => {
                    let mut nested = reader.read_message()?;
                    out.attribute.push(AttributeProto::read(&mut nested)?);
                }
                7 => out.domain = reader.read_string()?.to_owned(),
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.TensorProto`, **header only**.
///
/// | field | № | note |
/// |---|---|---|
/// | `dims` | 1 | repeated `int64`, packed or unpacked |
/// | `data_type` | 2 | `int32`, an `onnx.TensorProto.DataType` |
/// | `segment` | 3 | skipped |
/// | `float_data` | 4 | **payload — skipped** |
/// | `int32_data` | 5 | **payload — skipped** |
/// | `string_data` | 6 | **payload — skipped** |
/// | `int64_data` | 7 | **payload — skipped** |
/// | `name` | 8 | `string` |
/// | `raw_data` | 9 | **payload — skipped** |
/// | `double_data` | 10 | **payload — skipped** |
/// | `uint64_data` | 11 | **payload — skipped** |
/// | `doc_string` | 12 | skipped |
/// | `external_data` | 13 | skipped |
/// | `data_location` | 14 | skipped |
/// | `metadata_props` | 16 | skipped |
///
/// There is intentionally no field on this struct that could hold a weight byte.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TensorProto {
    /// The tensor's name (field 8) — the graph-scoped value it initializes.
    pub name: String,
    /// The extent of each axis (field 1).
    pub dims: Vec<i64>,
    /// The `onnx.TensorProto.DataType` code (field 2).
    pub data_type: i32,
}

impl TensorProto {
    /// The number of elements the tensor holds — the product of its extents.
    ///
    /// A rank-0 tensor holds one element (the empty product), which is ONNX's own reading
    /// of an empty `dims`.
    ///
    /// # Errors
    ///
    /// [`OnnxUnliftable`] on a negative extent (not a tensor shape) or an element count
    /// that overflows 64 bits.
    pub fn element_count(&self) -> gmeow_errors::Result<i64> {
        let mut count: i128 = 1;
        for dim in &self.dims {
            if *dim < 0 {
                return Err(Diag::of_kind(OnnxUnliftable {
                    detail: format!(
                        "initializer `{}` declares the negative extent {dim}; a tensor axis has a \
                         non-negative extent, so this is not a shape the lift can frame a \
                         math:ParameterSpace's dimension with",
                        self.name
                    ),
                }));
            }
            count *= i128::from(*dim);
        }
        i64::try_from(count).map_err(|_| {
            Diag::of_kind(OnnxUnliftable {
                detail: format!(
                    "initializer `{}` declares {count} elements, which overflows the 64-bit \
                     integer a math:spaceDimension is carried as; the lift refuses rather than \
                     emitting a truncated dimension",
                    self.name
                ),
            })
        })
    }

    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => read_repeated_i64(reader, tag.wire, &mut out.dims)?,
                2 => out.data_type = reader.read_i32()?,
                8 => out.name = reader.read_string()?.to_owned(),
                // Everything else — every payload field included — is stepped over by wire
                // type. The bytes are bounds-checked and then dropped.
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.ValueInfoProto` — a named, typed value at the graph boundary.
///
/// | field | № |
/// |---|---|
/// | `name` | 1 |
/// | `type` | 2 |
/// | `doc_string` | 3 | skipped |
/// | `metadata_props` | 4 | skipped |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueInfoProto {
    /// The value's graph-scoped name (field 1).
    pub name: String,
    /// The declared type (field 2).
    pub value_type: Option<TypeProto>,
}

impl ValueInfoProto {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.name = reader.read_string()?.to_owned(),
                2 => {
                    let mut nested = reader.read_message()?;
                    out.value_type = Some(TypeProto::read(&mut nested)?);
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.TypeProto` — the `oneof` over ONNX's type constructors.
///
/// | field | № | note |
/// |---|---|---|
/// | `tensor_type` | 1 | `TypeProto.Tensor` |
/// | `sequence_type` | 4 | recorded as [`TypeProto::unstructured`] |
/// | `map_type` | 5 | recorded as [`TypeProto::unstructured`] |
/// | `denotation` | 6 | skipped |
/// | `sparse_tensor_type` | 8 | recorded as [`TypeProto::unstructured`] |
/// | `optional_type` | 9 | recorded as [`TypeProto::unstructured`] |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeProto {
    /// The tensor arm (field 1) — the only arm this subset structures.
    pub tensor_type: Option<TensorTypeProto>,
    /// Which non-tensor type constructor was met, when one was. The lift refuses on it by
    /// name rather than lifting a value whose type it did not read.
    pub unstructured: Option<&'static str>,
}

impl TypeProto {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => {
                    let mut nested = reader.read_message()?;
                    out.tensor_type = Some(TensorTypeProto::read(&mut nested)?);
                }
                4 => {
                    out.unstructured = Some("sequence_type");
                    reader.skip_field()?;
                }
                5 => {
                    out.unstructured = Some("map_type");
                    reader.skip_field()?;
                }
                8 => {
                    out.unstructured = Some("sparse_tensor_type");
                    reader.skip_field()?;
                }
                9 => {
                    out.unstructured = Some("optional_type");
                    reader.skip_field()?;
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.TypeProto.Tensor`.
///
/// | field | № |
/// |---|---|
/// | `elem_type` | 1 |
/// | `shape` | 2 |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TensorTypeProto {
    /// The `onnx.TensorProto.DataType` code of the elements (field 1).
    pub elem_type: i32,
    /// The declared shape (field 2). `None` is ONNX's "rank unknown".
    pub shape: Option<Vec<Dim>>,
}

impl TensorTypeProto {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.elem_type = reader.read_i32()?,
                2 => {
                    let mut nested = reader.read_message()?;
                    out.shape = Some(read_shape(&mut nested)?);
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// One axis of an `onnx.TensorShapeProto`.
///
/// `onnx.TensorShapeProto` carries its axes as repeated `dim` (field 1); each
/// `TensorShapeProto.Dimension` is a `oneof` of `dim_value` (field 1, `int64`) and
/// `dim_param` (field 2, `string`), with a `denotation` (field 3) this subset skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dim {
    /// A concrete extent.
    Value(i64),
    /// A symbolic extent — `"batch"`, `"sequence"` — that the model leaves open.
    Param(String),
    /// An axis whose `oneof` arm the producer left unset: rank is known, extent is not.
    Unknown,
}

impl Dim {
    /// The axis, rendered for a `math:ExpressionType` label.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Value(n) => n.to_string(),
            Self::Param(name) => name.clone(),
            Self::Unknown => "?".to_owned(),
        }
    }
}

fn read_shape(reader: &mut Reader<'_>) -> gmeow_errors::Result<Vec<Dim>> {
    let mut dims = Vec::new();
    while let Some(tag) = reader.next_tag()? {
        match tag.number {
            1 => {
                let mut nested = reader.read_message()?;
                dims.push(read_dim(&mut nested)?);
            }
            _ => reader.skip_field()?,
        }
    }
    Ok(dims)
}

fn read_dim(reader: &mut Reader<'_>) -> gmeow_errors::Result<Dim> {
    let mut dim = Dim::Unknown;
    while let Some(tag) = reader.next_tag()? {
        match tag.number {
            1 => dim = Dim::Value(reader.read_i64()?),
            2 => dim = Dim::Param(reader.read_string()?.to_owned()),
            _ => reader.skip_field()?,
        }
    }
    Ok(dim)
}

/// `onnx.AttributeProto` — one piece of an operator's configuration.
///
/// | field | № | note |
/// |---|---|---|
/// | `name` | 1 | `string` |
/// | `f` | 2 | `float` |
/// | `i` | 3 | `int64` |
/// | `s` | 4 | `bytes` |
/// | `t` | 5 | `TensorProto` (header only) |
/// | `g` | 6 | `GraphProto` — recorded as [`AttributeProto::unstructured`] |
/// | `floats` | 7 | repeated `float`, packed or unpacked |
/// | `ints` | 8 | repeated `int64`, packed or unpacked |
/// | `strings` | 9 | repeated `bytes` |
/// | `tensors` | 10 | recorded as unstructured |
/// | `graphs` | 11 | recorded as unstructured |
/// | `doc_string` | 13 | skipped |
/// | `tp` | 14 | recorded as unstructured |
/// | `type_protos` | 15 | recorded as unstructured |
/// | `type` | 20 | `int32`, an `onnx.AttributeProto.AttributeType` |
/// | `ref_attr_name` | 21 | skipped |
/// | `sparse_tensor` | 22 | recorded as unstructured |
/// | `sparse_tensors` | 23 | recorded as unstructured |
///
/// An attribute's value participates in the operator's IDENTITY (a `Gemm` with `transB=1`
/// is a different operator from one without), so an attribute arm this subset cannot read
/// makes the node's operator unknown — which is why `unstructured` exists and why the lift
/// refuses on it instead of dropping it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttributeProto {
    /// The attribute's name (field 1).
    pub name: String,
    /// The declared `AttributeType` code (field 20).
    pub attribute_type: i32,
    /// The `float` arm (field 2).
    pub f: Option<f32>,
    /// The `int64` arm (field 3).
    pub i: Option<i64>,
    /// The `bytes` arm (field 4), required to be UTF-8 to be carried.
    pub s: Option<String>,
    /// The `TensorProto` arm (field 5) — header only, exactly as everywhere else.
    pub t: Option<TensorProto>,
    /// The `floats` arm (field 7).
    pub floats: Vec<f32>,
    /// The `ints` arm (field 8).
    pub ints: Vec<i64>,
    /// The `strings` arm (field 9).
    pub strings: Vec<String>,
    /// Which arm this subset does not structure, when one was met.
    pub unstructured: Option<&'static str>,
    /// A digest of the `t` arm's raw bytes. The header decodes but does not identify:
    /// two tensors of the same name and shape holding different values are different
    /// attributes, and without this they collapse onto one node.
    pub tensor_digest: Option<String>,
    /// A digest of that arm's raw bytes, so an undecoded attribute still DISTINGUISHES.
    /// The arm name alone is a constant; two subgraphs differing in content must not
    /// render identically or the nodes carrying them intern to one and one is lost.
    pub unstructured_digest: Option<String>,
}

impl AttributeProto {
    /// The attribute rendered as a canonical `name=value` fragment.
    ///
    /// This is part of the operator's content key, so it must be a total, deterministic
    /// function of the decoded attribute — never a `Debug` rendering.
    #[must_use]
    pub fn render(&self) -> String {
        let value = if let Some(i) = self.i {
            i.to_string()
        } else if let Some(f) = self.f {
            render_f32(f)
        } else if let Some(s) = &self.s {
            format!("\"{s}\"")
        } else if let Some(t) = &self.t {
            let dims: Vec<String> = t.dims.iter().map(i64::to_string).collect();
            let digest = self.tensor_digest.as_deref().unwrap_or("");
            format!("tensor({})[{}]#{digest}", t.name, dims.join(","))
        } else if !self.ints.is_empty() {
            let items: Vec<String> = self.ints.iter().map(i64::to_string).collect();
            format!("[{}]", items.join(","))
        } else if !self.floats.is_empty() {
            let items: Vec<String> = self.floats.iter().copied().map(render_f32).collect();
            format!("[{}]", items.join(","))
        } else if !self.strings.is_empty() {
            format!("[\"{}\"]", self.strings.join("\",\""))
        } else if let Some(kind) = self.unstructured {
            if let Some(digest) = &self.unstructured_digest {
                return format!("{}=<{kind}#{digest}>", self.name);
            }
            // An arm this subset does not decode still contributes its KIND to the
            // operator signature. Without it, an `If` with one `then_branch` and an `If`
            // with a different `then_branch` would render identically and collapse onto
            // one `math:Operation` — two different computations sharing an identity. The
            // subgraph's CONTENT does not cross (it is enumerated as residue), but the
            // presence and kind of the attribute does.
            format!("<{kind}>")
        } else {
            String::new()
        };
        format!("{}={value}", self.name)
    }

    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.name = reader.read_string()?.to_owned(),
                2 => out.f = Some(reader.read_f32()?),
                3 => out.i = Some(reader.read_i64()?),
                4 => out.s = Some(read_utf8_bytes(reader, "an AttributeProto `s`")?),
                5 => {
                    // The header decodes (blob-by-reference: no payload field exists), but
                    // the header ALONE does not identify the tensor — two `Constant` nodes
                    // holding different values share a name and dims and would render
                    // identically, intern to one node, and silently merge. `y = a + b`
                    // became `y = a + a`. Digesting the raw bytes distinguishes them
                    // WITHOUT materializing a value, so the doctrine holds and the identity
                    // is real.
                    let bytes = reader.read_bytes()?;
                    out.tensor_digest = Some(digest_of(bytes));
                    let mut nested = Reader::new(bytes);
                    out.t = Some(TensorProto::read(&mut nested)?);
                }
                6 => {
                    out.unstructured = Some("g (a control-flow subgraph)");
                    // Capture a digest of the subgraph's raw bytes. The ARM is a constant,
                    // so without this two `If`/`Loop`s differing only in their bodies render
                    // identically and intern to one node — the second silently vanishing.
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                7 => read_repeated_f32(reader, tag.wire, &mut out.floats)?,
                8 => read_repeated_i64(reader, tag.wire, &mut out.ints)?,
                9 => out.strings.push(read_utf8_bytes(
                    reader,
                    "an AttributeProto `strings` entry",
                )?),
                10 => {
                    out.unstructured = Some("tensors");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                11 => {
                    out.unstructured = Some("graphs (control-flow subgraphs)");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                14 => {
                    out.unstructured = Some("tp (a TypeProto attribute)");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                15 => {
                    out.unstructured = Some("type_protos");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                20 => out.attribute_type = reader.read_i32()?,
                22 => {
                    out.unstructured = Some("sparse_tensor");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                23 => {
                    out.unstructured = Some("sparse_tensors");
                    out.unstructured_digest = Some(digest_of(reader.read_bytes()?));
                }
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// `onnx.StringStringEntryProto` — one `metadata_props` pair.
///
/// | field | № |
/// |---|---|
/// | `key` | 1 |
/// | `value` | 2 |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringStringEntry {
    /// The metadata key (field 1).
    pub key: String,
    /// The metadata value (field 2).
    pub value: String,
}

impl StringStringEntry {
    fn read(reader: &mut Reader<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self::default();
        while let Some(tag) = reader.next_tag()? {
            match tag.number {
                1 => out.key = reader.read_string()?.to_owned(),
                2 => out.value = reader.read_string()?.to_owned(),
                _ => reader.skip_field()?,
            }
        }
        Ok(out)
    }
}

/// A repeated `int64` field, in either the packed or the unpacked encoding.
///
/// proto3 defaults scalar repeated fields to packed, but a conforming reader must accept
/// both — and ONNX exporters in the wild emit both. Accepting only one would reject valid
/// models, which is the failure mode this crate exists to avoid.
fn read_repeated_i64(
    reader: &mut Reader<'_>,
    wire: WireType,
    out: &mut Vec<i64>,
) -> gmeow_errors::Result<()> {
    if wire == WireType::LengthDelimited {
        let mut packed = reader.read_message()?;
        while !packed.is_exhausted() {
            let raw = packed.read_raw_varint()?;
            // Two's complement, exactly as the scalar path.
            #[allow(clippy::cast_possible_wrap)]
            let signed = raw as i64;
            out.push(signed);
        }
        return Ok(());
    }
    out.push(reader.read_i64()?);
    Ok(())
}

/// A repeated `float` field, in either the packed or the unpacked encoding.
fn read_repeated_f32(
    reader: &mut Reader<'_>,
    wire: WireType,
    out: &mut Vec<f32>,
) -> gmeow_errors::Result<()> {
    if wire == WireType::LengthDelimited {
        let mut packed = reader.read_message()?;
        while !packed.is_exhausted() {
            out.push(packed.read_raw_f32()?);
        }
        return Ok(());
    }
    out.push(reader.read_f32()?);
    Ok(())
}

/// A protobuf `bytes` field that ONNX documents as text.
fn read_utf8_bytes(reader: &mut Reader<'_>, what: &str) -> gmeow_errors::Result<String> {
    let bytes = reader.read_bytes()?;
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_owned()),
        Err(e) => Err(Diag::of_kind(OnnxWire {
            detail: format!(
                "{what} is not valid UTF-8 (invalid byte sequence at +{}); ONNX carries an \
                 operator's string configuration as UTF-8 text, and a lift that guessed an \
                 encoding for it would be inventing the operator's identity",
                e.valid_up_to()
            ),
        })),
    }
}

/// An `f32` rendered without exponent notation, for a content key and a label.
fn render_f32(value: f32) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains(['e', 'E', 'N', 'i']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// The `onnx.TensorProto.DataType` enumeration, spelled for a type label.
///
/// The list is the one `onnx.proto` declares. A code outside it is not "some other type" —
/// it is a model this decoder has not been taught to read, so the lift refuses on it rather
/// than labelling a tensor with a number.
#[must_use]
pub fn data_type_name(code: i32) -> Option<&'static str> {
    Some(match code {
        1 => "float",
        2 => "uint8",
        3 => "int8",
        4 => "uint16",
        5 => "int16",
        6 => "int32",
        7 => "int64",
        8 => "string",
        9 => "bool",
        10 => "float16",
        11 => "double",
        12 => "uint32",
        13 => "uint64",
        14 => "complex64",
        15 => "complex128",
        16 => "bfloat16",
        17 => "float8e4m3fn",
        18 => "float8e4m3fnuz",
        19 => "float8e5m2",
        20 => "float8e5m2fnuz",
        21 => "uint4",
        22 => "int4",
        23 => "float4e2m1",
        _ => return None,
    })
}

/// FNV-1a 64-bit hex of a byte slice — a content discriminator for an arm this subset does
/// not decode. Not a semantic identity: it distinguishes, it does not interpret.
fn digest_of(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[path = "model.tests.rs"]
#[cfg(test)]
mod tests;
