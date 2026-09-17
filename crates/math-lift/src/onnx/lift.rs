// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONNX lift tier: a decoded `ModelProto` → `math:` structures.
//!
//! The lift map is `MATHEMATICS-BRIDGES.md`'s, discharged edge for edge:
//!
//! | ONNX | `math:` |
//! |---|---|
//! | `NodeProto` | a tensor-operator `math:ApplicationExpression`, a `math:computationNode` of the graph |
//! | `GraphProto` | `math:TensorComputationGraph`, `math:architectureOf` a `math:LearnedModel` |
//! | `initializer` (`TensorProto`) | `math:WeightTensor` with `math:weightOf` and `math:inParameterSpace` |
//! | the parameter block | one `math:ParameterSpace` — a `math:VectorSpace`, with the obligations that entails |
//! | `opset_import` | `math:MathematicalTheory` scoping a `math:MathematicalSymbol` per operator |
//! | graph `input`/`output`/`value_info` | typed tensor slots: expression leaves carrying a `math:ExpressionType` |
//! | a `TypeProto.Tensor`'s `elem_type` | a `math:TensorElementType` term, shared by every tensor of that format |
//! | a `TensorShapeProto`/`TensorProto.dims` | a `math:TensorShape` with its `math:tensorRank` and one indexed `math:TensorAxis` per axis |
//! | a `TensorShapeProto.Dimension.dim_param` | a `math:FreeVariableDeclaration` named through `math:symbolicExtent` |
//! | `metadata_props`, `ir_version`, `domain` | reified `math:SourceAnnotation` pairs on the retained `math:parseSource` witness |
//! | `producer_name` | a `gmeow:SoftwareAgent` the witness is `gmeow:wasAttributedTo` |
//!
//! # The OWL restrictions that make this a hard-fail bridge
//!
//! - `math:TensorComputationGraph` carries **min 1** `math:computationNode`
//!   (`slices/grounding/math/module.ttl:10490`, `math:MalformedTensorComputationGraph`). A
//!   model whose graph declares no node is [`OnnxUnliftable`]; an empty graph is never
//!   emitted and left for a downstream validator to reject.
//! - `math:WeightTensor` carries **max 1** `math:inParameterSpace`, qualified on
//!   `math:ParameterSpace` (`module.ttl:10494`, `math:UnframedWeightTensor`). Every weight
//!   this lift emits names exactly one, and that one is the single parameter space of the
//!   model.
//! - `math:ParameterSpace` **is a** `math:VectorSpace`, hence a `math:Module`, hence a
//!   `math:AlgebraicStructure`, which carries min-1 `math:structureOperation` (on
//!   `math:Operation`), min-1 `math:satisfiesAxiom`, and max-1 `math:underlyingSet` (on
//!   `math:Set`) — `module.ttl:10280`, `math:IncompleteAlgebraicStructure`. The space is
//!   emitted with all four (plus `math:parameterSpaceOf`), matching the hand-authored target
//!   at `slices/grounding/math/examples/bridges.ttl:157-162`, or it is not emitted at all: a
//!   model with no initializer has no parameter block, so it gets no parameter space and no
//!   weight tensors rather than an unframed one.
//!
//! # Crisp, not vague
//!
//! The rung is [`Rung::lossy_crisp_with_witness`]. An ONNX graph is an exact artifact —
//! operator types, tensor shapes, and the opset are stated, not interpreted — so its
//! determinacy is `logic:Crisp`, unlike the R bridge's `logic:Vague`. It stays a
//! `logic:LossyLens` for one reason only: the weight PAYLOADS never cross.
//!
//! # Blob-by-reference
//!
//! A `math:WeightTensor` here is a NAME, a SHAPE, and a FRAME. The parse tier cannot even
//! represent a payload byte ([`super::model::TensorProto`] has no field for one), so the
//! doctrine is discharged structurally rather than by the lift remembering to skip.
//!
//! What *does* cross is metadata: the element count as the `math:spaceDimension` of the
//! parameter space, the per-axis extents as a `math:TensorShape`, and the element format as
//! a `math:TensorElementType`. Every one of those is a fact the model STATES ABOUT a tensor,
//! and none of them is a fact IN one — that distinction is the whole doctrine, and it is why
//! the shape can be carried in full while the values are held by reference. The
//! `no_tensor_payload_byte_reaches_the_graph` test pins the boundary: not one `xsd:decimal`
//! literal is emitted by this lift under any rendering.
//!
//! # Content-addressed interning
//!
//! Every node's expression is interned into a [`TermArena`], and the resulting
//! [`ContentKey`] mints its IRI. Two nodes that apply the same operator to the same operands
//! are the same computation and collapse to one `math:ApplicationExpression`, so the graph
//! grows with distinct structure rather than with node count.
//!
//! # What this lift refuses rather than fakes
//!
//! - An operator individual is typed `math:Operation` and **never** `math:ActivationFunction`
//!   — even for `Relu`. `math:ActivationFunction` is a `math:Function`, and `math:Function`
//!   carries min-1 `math:domain` and min-1 `math:codomain` qualified on `math:Set`
//!   (`module.ttl:10251`, `math:UnframedFunction`). An ONNX graph states a tensor's *shape*,
//!   not the mathematical *set* the activation maps between, so claiming the class would mean
//!   minting two sets the model never declares. The shape it *does* state is carried in full
//!   as a `math:TensorShape`; refusing the class is a refusal to invent, not a refusal to
//!   read.
//! - A `TypeProto` that is not a tensor type (a sequence, map, optional, or sparse tensor)
//!   and an `AttributeProto` carrying a control-flow subgraph are [`OnnxUnliftable`] by name.
//!   Lifting a node whose configuration this crate did not read would misstate the operator's
//!   identity, and the operator's identity is what the whole graph means.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_term_arena::{Arena, ContentKey, StructNode, TermArena};
use purrdf::TermValue;

use crate::error::OnnxUnliftable;
use crate::frame::{BridgeKind, Lifted, RunFrame, Rung};
use crate::ns::{gmeow, math};
use crate::onnx::model::{
    AttributeProto, Dim, GraphProto, ModelProto, NodeProto, TensorProto, TypeProto, data_type_name,
};
use crate::sink::Sink;

/// `rdfs:label`.
///
/// The one non-`math:`/`gmeow:` term this lift needs, for the same reason the R bridge needs
/// it: a `math:WeightTensor` is held BY REFERENCE, so the ONNX name that frames it has to
/// travel with the node or the reference addresses nothing. The literal is PLAIN — [`Sink`]
/// exposes no language-tagged constructor, because lifted graphs leave through the shipped
/// CLI where no `x-gmeow-*` private-use tag may appear.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// ONNX operators that ARE a `math:` operator the slice already declares as an individual.
///
/// Only for the default `ai.onnx` domain: a custom-domain operator that happens to be spelled
/// `MatMul` is a different operator, and this table must not claim otherwise. The list is
/// deliberately short — an entry is a claim that the ONNX operator and the `math:` individual
/// denote the same operation, and `Gemm` (αA′B′ + βC) is not the matrix product.
const CANONICAL_TENSOR_OPERATORS: &[(&str, &str)] =
    &[("MatMul", "matrixProduct"), ("Einsum", "tensorContraction")];

/// Lift an ONNX model graph into `math:` structures.
///
/// `mint_base` must end in `/` or `#`; every codomain IRI is minted beneath the run it names,
/// so a re-lift of the same bytes under the same base is byte-identical.
///
/// # Errors
///
/// - [`OnnxWire`](crate::error::OnnxWire) when `source` is not a well-formed protobuf
///   `ModelProto`, with the offending byte offset.
/// - [`OnnxUnliftable`] when the model decodes but its graph cannot be structured into the
///   `math:` codomain: no graph, no computation node, no operator set, a node reading a value
///   the graph never declares, an untyped boundary value, or a construct whose meaning this
///   crate did not read.
pub fn lift(source: &[u8], mint_base: &str) -> gmeow_errors::Result<Lifted> {
    let model = ModelProto::decode(source)?;

    let Some(graph) = model.graph.as_ref() else {
        return Err(unliftable(
            "the ONNX model declares no GraphProto (field 7), so there is no forward \
             computation to lift; a math:TensorComputationGraph is the graph, and this model \
             has none"
                .to_owned(),
        ));
    };
    if graph.node.is_empty() {
        return Err(unliftable(format!(
            "the ONNX graph `{}` declares no NodeProto, but math:TensorComputationGraph carries \
             a min-1 math:computationNode OWL restriction (math:MalformedTensorComputationGraph); \
             a graph with no computation node is an unliftable ingest, not a lift",
            graph_label(graph)
        )));
    }
    if model.opset_import.is_empty() {
        return Err(unliftable(
            "the ONNX model declares no opset_import, so its operators are drawn from no \
             declared operator set; the opset IS the operator vocabulary the lift grounds each \
             math:MathematicalSymbol in, and inventing one would be fabricating the meaning of \
             every node"
                .to_owned(),
        ));
    }

    let mut frame = RunFrame::mint(BridgeKind::Onnx, mint_base, source);
    for construct in unmapped_constructs(&model, graph) {
        frame.record_unmapped(construct);
    }
    let frame = frame;
    let mut sink = Sink::new();
    frame.emit(&mut sink, Rung::lossy_crisp_with_witness());

    let mut lift = Lift {
        frame: &frame,
        sink,
        arena: TermArena::new(),
        emitted: BTreeSet::new(),
        env: BTreeMap::new(),
        weight_layer: BTreeMap::new(),
        tensor_structures: 0,
    };
    lift.model(&model, graph)?;

    if lift.tensor_structures == 0 {
        return Err(unliftable(format!(
            "the ONNX model `{}` decodes but produced no tensor structure for the math: \
             codomain: no computation node, no weight tensor, and no graph. A run whose only \
             product is provenance is an unliftable ingest, not a lift",
            graph_label(graph)
        )));
    }

    let codomain = lift.emitted.len();
    Lifted::seal(&frame, lift.sink, codomain)
}

// ── Lift state ────────────────────────────────────────────────────────────────

/// A value flowing through the graph: the expression node standing for it.
#[derive(Debug, Clone, Copy)]
struct Value {
    node: StructNode,
}

struct Lift<'f> {
    frame: &'f RunFrame,
    sink: Sink,
    arena: TermArena,
    emitted: BTreeSet<String>,
    /// ONNX value name → the expression that computes it.
    env: BTreeMap<String, Value>,
    /// Initializer name → the `math:NeuralLayer` of the first node that consumes it.
    weight_layer: BTreeMap<String, String>,
    /// How many genuinely TENSOR structures the run produced — nodes, weights, the graph.
    ///
    /// Separate from `emitted.len()`, which also counts provenance nodes. Without it a model
    /// carrying only `metadata_props` would seal a run whose whole codomain is bookkeeping;
    /// the ONNX bridge's job is the architecture, so the architecture is what the gate counts.
    tensor_structures: usize,
}

impl Lift<'_> {
    /// Mint (and back-link) a codomain node, reporting whether it is new.
    ///
    /// The back edge `gmeow:wasGeneratedBy` is what the native `math:UnliftableIngest` lint
    /// enumerates, so it is attached HERE, once, for every node this lift creates.
    fn mint(&mut self, role: &str, key: &str) -> (String, bool) {
        let iri = self.frame.node(role, key);
        let fresh = self.emitted.insert(iri.clone());
        if fresh {
            self.frame.generated(&mut self.sink, &iri);
        }
        (iri, fresh)
    }

    fn label(&mut self, subject: &str, text: &str) {
        self.sink.string(subject, RDFS_LABEL, text);
    }

    fn key_of(&self, node: StructNode) -> ContentKey {
        self.arena
            .key(node)
            .expect("every node was minted by this lift's own arena")
    }

    fn atom(&mut self, text: &str) -> StructNode {
        self.arena.intern_leaf(TermValue::simple_literal(text))
    }

    fn app(&mut self, operator: &str, args: &[StructNode]) -> StructNode {
        let op = self.atom(operator);
        self.arena
            .intern_app(op, args)
            .expect("every node was minted by this lift's own arena")
    }

    fn expression_iri(&mut self, node: StructNode) -> String {
        let key = self.key_of(node).into_string();
        self.frame.node("expr", &key)
    }

    // -- the whole model -----------------------------------------------------

    fn model(&mut self, model: &ModelProto, graph: &GraphProto) -> gmeow_errors::Result<()> {
        self.provenance(model);

        // The operator vocabulary: one math:MathematicalTheory per imported operator set.
        let mut theories: BTreeMap<String, String> = BTreeMap::new();
        for opset in &model.opset_import {
            let domain = opset.spelled_domain().to_owned();
            let key = format!("{domain}|{}", opset.version);
            let (iri, fresh) = self.mint("opset", &key);
            if fresh {
                self.sink.typed(&iri, &math("MathematicalTheory"));
                self.label(
                    &iri,
                    &format!("ONNX operator set {domain} v{}", opset.version),
                );
            }
            // A model may import the same domain twice; ONNX takes the first. So does this.
            theories.entry(domain).or_insert(iri);
        }

        let model_iri = self.emit_learned_model(graph);

        // The leaves: graph inputs and initializers, before any node can read them.
        let initializers: BTreeSet<&str> =
            graph.initializer.iter().map(|t| t.name.as_str()).collect();
        for info in &graph.input {
            if initializers.contains(info.name.as_str()) {
                // ONNX ≤ IR 3 lists initializers among the graph inputs. The initializer is
                // the authority on that name, so the duplicate declaration is not a second
                // leaf — it would fork the value's identity.
                continue;
            }
            let value = self.emit_leaf(&info.name);
            let expr = self.expression_iri(value.node);
            let declared =
                self.require_type(info.value_type.as_ref(), "graph input", &info.name)?;
            self.attach_type(&expr, declared)?;
            self.env.insert(info.name.clone(), value);
        }
        for tensor in &graph.initializer {
            let value = self.emit_leaf(&tensor.name);
            let expr = self.expression_iri(value.node);
            self.attach_initializer_type(&expr, tensor)?;
            self.env.insert(tensor.name.clone(), value);
        }

        // The nodes, in the topological order ONNX guarantees.
        let mut node_iris = Vec::with_capacity(graph.node.len());
        for (index, node) in graph.node.iter().enumerate() {
            let iri = self.emit_node(index, node, &theories, &initializers)?;
            node_iris.push(iri);
        }

        // The weights, now that every consuming layer is known.
        let space = self.emit_parameter_space(graph, &model_iri)?;
        for tensor in &graph.initializer {
            self.emit_weight_tensor(tensor, space.as_deref())?;
        }

        // The declared intermediate types, and the typed graph outputs.
        for info in &graph.value_info {
            let Some(value) = self.env.get(&info.name).copied() else {
                continue;
            };
            if let Some(declared) = info.value_type.as_ref() {
                let expr = self.expression_iri(value.node);
                self.attach_type(&expr, declared)?;
            }
        }
        for info in &graph.output {
            let Some(value) = self.env.get(&info.name).copied() else {
                return Err(unliftable(format!(
                    "the ONNX graph declares the output `{}`, which no node produces and no \
                     initializer supplies; a graph output that names nothing is not a typed \
                     tensor slot, and the lift will not mint a placeholder for it",
                    info.name
                )));
            };
            let expr = self.expression_iri(value.node);
            let declared =
                self.require_type(info.value_type.as_ref(), "graph output", &info.name)?;
            self.attach_type(&expr, declared)?;
        }

        self.emit_computation_graph(graph, &model_iri, &node_iris);
        Ok(())
    }

    // -- provenance ----------------------------------------------------------

    /// `producer_name`/`producer_version` and `metadata_props`, landed on the retained
    /// `math:parseSource` witness.
    ///
    /// The producer is a `gmeow:SoftwareAgent` and the witness is `gmeow:wasAttributedTo` it —
    /// attribution lands on the enduring artifact, association on the activity, and the
    /// activity here is GMEOW's own lift, not the exporter's run.
    ///
    /// # `metadata_props` is a REIFIED PAIR, not prose
    ///
    /// `ir_version`, `model_version`, `domain`, and each `metadata_props` entry become one
    /// `math:SourceAnnotation` each, carrying `math:annotationKey` and `math:annotationValue`
    /// verbatim plus an `rdfs:label` holding the `key=value` rendering a reader recognizes.
    /// Reification is what keeps several annotations from being cross-paired: flat key and
    /// value properties on the witness would produce a cross product the moment a second
    /// entry appeared.
    ///
    /// Two near-fit terms were considered for the pair and rejected, which is why
    /// `math:SourceAnnotation` exists at all:
    ///
    /// - `gmeow:Identifier` scopes itself to "a reified EXTERNAL-IDENTIFIER record — an
    ///   ORCID, a geni profile id, a Nostr nip05, a LEI, a ROR ID, a NAICS code", and its
    ///   `gmeow:avoidWhen` polices that boundary explicitly. An ONNX `metadata_props` entry
    ///   is a producer's free-form note — `author`, `license`, `converted_from` — which
    ///   identifies nothing and resolves nowhere; typing it `gmeow:Identifier` would assert
    ///   an external-identity claim the source never made.
    /// - `gmeow:ExifTag` is the only other open-keyed pair in the ontology and is the shape
    ///   `math:SourceAnnotation` is modelled on, but both its properties and its single
    ///   attachment path (`gmeow:hasExifTag`) are domain-locked to a `gmeow:MediaObject`'s
    ///   EXIF block.
    ///
    /// The annotations land on the retained `math:parseSource` witness, never on a lifted
    /// `math:` object: an annotation stamped onto the codomain would be the amnesic
    /// string-placeholder pattern the ingestion rules forbid.
    fn provenance(&mut self, model: &ModelProto) {
        let witness = self.frame.source_witness_iri.clone();

        if !model.producer_name.is_empty() {
            let key = format!("{}|{}", model.producer_name, model.producer_version);
            let (agent, fresh) = self.mint("producer", &key);
            if fresh {
                self.sink.typed(&agent, &gmeow("SoftwareAgent"));
                let label = if model.producer_version.is_empty() {
                    model.producer_name.clone()
                } else {
                    format!("{} {}", model.producer_name, model.producer_version)
                };
                self.label(&agent, &label);
            }
            self.sink.iri(&witness, &gmeow("wasAttributedTo"), &agent);
        }

        let mut entries: Vec<(String, String)> =
            vec![("onnx.ir_version".to_owned(), model.ir_version.to_string())];
        if model.model_version != 0 {
            entries.push((
                "onnx.model_version".to_owned(),
                model.model_version.to_string(),
            ));
        }
        if !model.domain.is_empty() {
            entries.push(("onnx.domain".to_owned(), model.domain.clone()));
        }
        for prop in &model.metadata_props {
            if prop.key.is_empty() {
                continue;
            }
            entries.push((prop.key.clone(), prop.value.clone()));
        }

        // One math:SourceAnnotation per entry, content-addressed on the pair so a model that
        // repeats a key/value pair names one node twice rather than minting two.
        for (key, value) in entries {
            let (iri, fresh) = self.mint("annotation", &format!("{key}\u{0}{value}"));
            if fresh {
                self.sink.typed(&iri, &math("SourceAnnotation"));
                self.sink.string(&iri, &math("annotationKey"), &key);
                // Verbatim, empty string included: an annotation present with an empty value
                // is a different fact from an annotation that is absent.
                self.sink.string(&iri, &math("annotationValue"), &value);
                self.label(&iri, &format!("{key}={value}"));
            }
            self.sink.iri(&witness, &math("sourceAnnotation"), &iri);
        }
    }

    // -- the model and its graph ---------------------------------------------

    fn emit_learned_model(&mut self, graph: &GraphProto) -> String {
        let (iri, fresh) = self.mint("model", &graph_label(graph));
        if fresh {
            self.sink.typed(&iri, &math("LearnedModel"));
            self.label(&iri, &graph_label(graph));
        }
        iri
    }

    fn emit_computation_graph(
        &mut self,
        graph: &GraphProto,
        model_iri: &str,
        node_iris: &[String],
    ) {
        let (iri, fresh) = self.mint("graph", &graph_label(graph));
        if fresh {
            self.sink.typed(&iri, &math("TensorComputationGraph"));
            self.sink.iri(&iri, &math("architectureOf"), model_iri);
            self.label(&iri, &graph_label(graph));
            self.tensor_structures += 1;
        }
        for node_iri in node_iris {
            self.sink.iri(&iri, &math("computationNode"), node_iri);
        }
    }

    // -- expression leaves ---------------------------------------------------

    /// A graph input or an initializer: the AST leaf its name stands for.
    ///
    /// Modelled exactly as the R bridge models a variable — a `math:VariableExpression` over
    /// one `math:VariableOccurrence` resolving to a `math:FreeVariableDeclaration`, because
    /// `math:VariableExpression`'s own definition insists "there is no implicit free
    /// variable": an occurrence resolving to no declaration is
    /// `math:UnscopedVariableOccurrence`.
    fn emit_leaf(&mut self, name: &str) -> Value {
        let node = self
            .arena
            .intern_free(TermValue::simple_literal(format!("onnx:value:{name}")));
        let key = self.key_of(node).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if fresh {
            self.sink.typed(&iri, &math("VariableExpression"));
            self.label(&iri, name);
            let (occurrence, _) = self.mint("occurrence", &key);
            self.sink.typed(&occurrence, &math("VariableOccurrence"));
            let (declaration, _) = self.mint("declaration", &key);
            self.sink
                .typed(&declaration, &math("FreeVariableDeclaration"));
            self.label(&declaration, name);
            self.sink
                .iri(&occurrence, &math("declaredVariable"), &declaration);
            self.sink
                .iri(&iri, &math("variableOccurrence"), &occurrence);
        }
        Value { node }
    }

    /// A boundary value's declared type must be present — that is what "typed tensor slot"
    /// means.
    fn require_type<'t>(
        &self,
        declared: Option<&'t TypeProto>,
        role: &str,
        name: &str,
    ) -> gmeow_errors::Result<&'t TypeProto> {
        declared.ok_or_else(|| {
            unliftable(format!(
                "the ONNX {role} `{name}` declares no TypeProto; a typed tensor slot without its \
                 type is not typed, and the lift will not guess an element type or a shape"
            ))
        })
    }

    /// Attach a `math:ExpressionType` to an expression leaf or node.
    fn attach_type(&mut self, expr_iri: &str, declared: &TypeProto) -> gmeow_errors::Result<()> {
        if let Some(kind) = declared.unstructured {
            return Err(unliftable(format!(
                "an ONNX value declares the `{kind}` type constructor, which this bridge does not \
                 structure; the lift map carries TENSOR slots into math:, and lifting a value \
                 whose type it did not read would misstate what flows through the graph"
            )));
        }
        let Some(tensor) = declared.tensor_type.as_ref() else {
            return Err(unliftable(
                "an ONNX value declares a TypeProto with no type constructor set at all; an \
                 untyped boundary value is not a typed tensor slot"
                    .to_owned(),
            ));
        };
        let Some(element) = data_type_name(tensor.elem_type) else {
            return Err(unliftable(format!(
                "an ONNX value declares element type code {}, which is not one of the \
                 onnx.TensorProto.DataType codes this bridge reads; labelling a tensor with a \
                 number it cannot name would be a degraded lift",
                tensor.elem_type
            )));
        };
        // ONNX's own reading: no TensorShapeProto means the rank is UNKNOWN, which is a
        // weaker claim than rank 0. It is therefore spelled as the ABSENCE of a
        // math:tensorShape (math:TensorShape's own definition demands exactly that), never
        // as a shape node carrying no axis — which would assert rank 0, a scalar.
        let axes: Option<Vec<AxisSpec>> = tensor
            .shape
            .as_ref()
            .map(|dims| dims.iter().map(AxisSpec::of_dim).collect());
        self.attach_tensor_type(expr_iri, element, axes.as_deref());
        Ok(())
    }

    /// Attach a `math:ExpressionType` to an initializer's leaf.
    ///
    /// An initializer needs no `ValueInfoProto` to be typed: its own `TensorProto` header
    /// states the element type and every extent, so the weight's leaf is a typed tensor slot
    /// on exactly the same footing as a graph input's. Reading the type off the header rather
    /// than requiring a boundary declaration is information the model genuinely carries.
    fn attach_initializer_type(
        &mut self,
        expr_iri: &str,
        tensor: &TensorProto,
    ) -> gmeow_errors::Result<()> {
        let Some(element) = data_type_name(tensor.data_type) else {
            return Err(unliftable(format!(
                "initializer `{}` declares element type code {}, which is not one of the \
                 onnx.TensorProto.DataType codes this bridge reads",
                tensor.name, tensor.data_type
            )));
        };
        // Reject a negative extent (and an overflowing element count) HERE, before any axis
        // reaches the sink: math:axisExtent's own scope note says a negative extent is not a
        // shape at all but a hard failure at ingest. The message is the one
        // `TensorProto::element_count` already words.
        tensor.element_count()?;
        let axes: Vec<AxisSpec> = tensor.dims.iter().copied().map(AxisSpec::Extent).collect();
        self.attach_tensor_type(expr_iri, element, Some(&axes));
        Ok(())
    }

    /// Attach the whole of what an ONNX tensor type states: the element format and, when the
    /// source declares one, the per-axis shape.
    ///
    /// The `math:ExpressionType` individual is content-addressed on the rendered type, so two
    /// values of the same tensor type share ONE type individual rather than minting a copy per
    /// mention — and the structured element type and shape hang off that shared node, emitted
    /// exactly once.
    fn attach_tensor_type(&mut self, expr_iri: &str, element: &str, axes: Option<&[AxisSpec]>) {
        let rendered = render_tensor_type(element, axes);
        let (iri, fresh) = self.mint("type", &rendered);
        if fresh {
            self.sink.typed(&iri, &math("ExpressionType"));
            // The human label stays: it is how a reader recognizes the type at a glance. What
            // changes is that it is no longer the ONLY place the shape lives.
            self.label(&iri, &rendered);
            let element_iri = self.emit_element_type(element);
            self.sink
                .iri(&iri, &math("tensorElementType"), &element_iri);
            if let Some(axes) = axes {
                let shape_iri = self.emit_shape(axes);
                self.sink.iri(&iri, &math("tensorShape"), &shape_iri);
            }
        }
        self.sink.iri(expr_iri, &math("expressionType"), &iri);
    }

    /// The `math:TensorElementType` term for one ONNX element format.
    ///
    /// Content-addressed on the format's spelling, so every `float` tensor in the run names
    /// ONE node — which is the point of the class: "every float16 tensor in this model" is a
    /// graph query rather than a substring match over rendered labels.
    fn emit_element_type(&mut self, element: &str) -> String {
        let (iri, fresh) = self.mint("element-type", element);
        if fresh {
            self.sink.typed(&iri, &math("TensorElementType"));
            self.label(&iri, element);
        }
        iri
    }

    /// The `math:TensorShape` for one axis list: its rank, and one indexed `math:TensorAxis`
    /// per axis.
    ///
    /// Content-addressed on the axes, so the `math:ExpressionType` typing an initializer's
    /// leaf and the `math:WeightTensor` filling it name the SAME shape object rather than two
    /// coincidentally equal ones.
    fn emit_shape(&mut self, axes: &[AxisSpec]) -> String {
        let key = shape_key(axes);
        let (iri, fresh) = self.mint("shape", &key);
        if !fresh {
            return iri;
        }
        self.sink.typed(&iri, &math("TensorShape"));
        self.label(&iri, &render_axes(axes));
        // Explicit, never left to be counted: rank 0 (a scalar) has to be positively
        // assertable, and the native constraint fragment cannot express a count.
        let rank = i64::try_from(axes.len()).unwrap_or(i64::MAX);
        self.sink.integer(&iri, &math("tensorRank"), rank);

        for (index, axis) in axes.iter().enumerate() {
            let (axis_iri, _) = self.mint("axis", &format!("{key}#{index}"));
            self.sink.typed(&axis_iri, &math("TensorAxis"));
            let position = i64::try_from(index).unwrap_or(i64::MAX);
            self.sink.integer(&axis_iri, &math("axisIndex"), position);
            self.label(&axis_iri, &axis.render());
            match axis {
                AxisSpec::Extent(extent) => {
                    self.sink.integer(&axis_iri, &math("axisExtent"), *extent);
                }
                AxisSpec::Symbolic(name) => {
                    // A DECLARATION, not a string: two axes spelled `batch` denote the same
                    // unknown length, and sharing one math:FreeVariableDeclaration is what
                    // makes that co-identity a graph fact rather than a string coincidence.
                    let (declaration, declaration_fresh) = self.mint("extent-symbol", name);
                    if declaration_fresh {
                        self.sink
                            .typed(&declaration, &math("FreeVariableDeclaration"));
                        self.label(&declaration, name);
                    }
                    self.sink
                        .iri(&axis_iri, &math("symbolicExtent"), &declaration);
                }
                // ONNX's `oneof` left unset: the rank counts this axis and the source states
                // nothing about its length. Both extent properties stay off — math:axisExtent
                // forbids a placeholder 0 or -1 standing in for "unknown".
                AxisSpec::Unstated => {}
            }
            self.sink.iri(&iri, &math("shapeAxis"), &axis_iri);
        }
        iri
    }

    // -- nodes ---------------------------------------------------------------

    fn emit_node(
        &mut self,
        index: usize,
        node: &NodeProto,
        theories: &BTreeMap<String, String>,
        initializers: &BTreeSet<&str>,
    ) -> gmeow_errors::Result<String> {
        if node.op_type.is_empty() {
            return Err(unliftable(format!(
                "ONNX node #{index} ({}) declares no op_type; a computation node with no operator \
                 cannot fill math:ApplicationExpression's exactly-one math:operator obligation \
                 (math:ApplicationOperatorCardinality)",
                node_label(index, node)
            )));
        }
        let domain = spelled_node_domain(node);
        let Some(theory) = theories.get(domain).cloned() else {
            return Err(unliftable(format!(
                "ONNX node `{}` draws its operator `{}` from the domain `{domain}`, which the \
                 model's opset_import never declares; the operator is therefore scoped by no \
                 math:MathematicalTheory, and this lift does not invent one",
                node_label(index, node),
                node.op_type
            )));
        };

        // Resolve the operands FIRST: a node reading a value the graph never declares must
        // fail before any triple about it reaches the sink.
        let mut operands = Vec::with_capacity(node.input.len());
        for input in &node.input {
            if input.is_empty() {
                // ONNX spells "this optional input is absent" as an empty name. It occupies
                // no operand position, so the surviving operands stay contiguous — which is
                // exactly what math:slotIndex requires (math:NonContiguousArgumentSlots).
                continue;
            }
            let Some(value) = self.env.get(input).copied() else {
                return Err(unliftable(format!(
                    "ONNX node `{}` reads `{input}`, which the graph never declares as an input, \
                     an initializer, or the output of an earlier node; a dangling operand has no \
                     math:slotExpression to fill, and the lift will not mint a free variable to \
                     stand in for it",
                    node_label(index, node)
                )));
            };
            operands.push(value.node);
        }

        let operator = self.emit_operator(index, node, &theory)?;
        let arena_key = format!(
            "onnx:{domain}:{}:{}",
            node.op_type,
            attribute_signature(&node.attribute)
        );
        let structure = self.app(&arena_key, &operands);
        let iri = self.emit_application(structure, &operator, &operands, node);

        // Every initializer this node consumes is the weight of THIS layer. First consumer
        // wins: a weight shared by two layers belongs to the earlier one in graph order, and
        // math:weightOf is not the place to record sharing.
        let layer = self.emit_layer(index, node, &iri);
        for input in &node.input {
            if initializers.contains(input.as_str()) {
                self.weight_layer
                    .entry(input.clone())
                    .or_insert_with(|| layer.clone());
            }
        }

        // Bind the node's results. A single-output node IS its expression; a multi-output
        // node's i-th result is a projection application over it, so the two results are
        // distinguishable rather than aliased onto one node.
        let outputs: Vec<&String> = node.output.iter().filter(|n| !n.is_empty()).collect();
        if outputs.len() == 1 {
            self.env
                .insert(outputs[0].clone(), Value { node: structure });
        } else {
            for (position, name) in outputs.iter().enumerate() {
                let projected = self.emit_projection(structure, position);
                self.env.insert((*name).clone(), projected);
            }
        }
        Ok(iri)
    }

    /// The `math:Operation` a node applies, and the opset symbol that names it.
    ///
    /// The operator individual is keyed on the domain, the op_type, AND the attributes: a
    /// `Gemm` with `transB=1` transposes its right operand and one without does not, so they
    /// are different operations and must not share an identity.
    fn emit_operator(
        &mut self,
        _index: usize,
        node: &NodeProto,
        theory: &str,
    ) -> gmeow_errors::Result<String> {
        // An attribute arm this subset does not decode — an `If`/`Loop`/`Scan` control-flow
        // subgraph — no longer refuses the node. Two facts made the old hard-fail
        // indefensible: a `MyCustomOp` from a private domain, whose semantics the bridge
        // cannot possibly know, already lifted cleanly as a `math:Operation`, so "we must
        // read it to lift it" was not the operative rule; and the run already declares
        // `LossyLens` / `ValidationOnly` / `mnemomorphic false` and already enumerates
        // residue, so an undecoded attribute costs it no claim it was not already making.
        //
        // Identity is preserved instead of refused: `AttributeProto::render` emits the arm
        // KIND for an undecoded attribute, so the attribute still keys the operator and two
        // nodes differing in one cannot collapse. What does not cross — the subgraph's
        // contents — is enumerated by `unmapped_constructs`.
        let domain = spelled_node_domain(node);
        let signature = attribute_signature(&node.attribute);
        let key = format!("{domain}|{}|{signature}", node.op_type);
        let (iri, fresh) = self.mint("operation", &key);
        if fresh {
            self.sink.typed(&iri, &math("Operation"));
            let label = if signature.is_empty() {
                node.op_type.clone()
            } else {
                format!("{}({signature})", node.op_type)
            };
            self.label(&iri, &label);

            // The opset IS the operator vocabulary: the operator resolves through one
            // math:MathematicalSymbol whose meaning is scoped by the imported operator set.
            let symbol_key = format!("{domain}|{}", node.op_type);
            let (symbol, symbol_fresh) = self.mint("symbol", &symbol_key);
            if symbol_fresh {
                self.sink.typed(&symbol, &math("MathematicalSymbol"));
                self.label(&symbol, &node.op_type);
                self.sink.iri(&symbol, &math("definedInTheory"), theory);
            }
            self.sink.iri(&iri, &math("hasMathematicalSymbol"), &symbol);
        }
        Ok(iri)
    }

    /// The `math:NeuralLayer` a node realizes.
    ///
    /// Keyed on the node's own `math:ApplicationExpression` IRI, so two structurally identical
    /// nodes are one layer — the same collapse the expression AST makes, carried through to
    /// the layer that names it.
    fn emit_layer(&mut self, index: usize, node: &NodeProto, node_iri: &str) -> String {
        let (iri, fresh) = self.mint("layer", node_iri);
        if fresh {
            self.sink.typed(&iri, &math("NeuralLayer"));
            self.label(&iri, &node_label(index, node));
        }
        iri
    }

    /// Emit a `math:ApplicationExpression`: exactly one operator, contiguous zero-based slots.
    fn emit_application(
        &mut self,
        structure: StructNode,
        operator: &str,
        operands: &[StructNode],
        node: &NodeProto,
    ) -> String {
        let key = self.key_of(structure).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if !fresh {
            return iri;
        }
        self.sink.typed(&iri, &math("ApplicationExpression"));
        self.sink.iri(&iri, &math("operator"), operator);

        // math:tensorOperation names the TENSOR operator the node applies. Where the ONNX
        // operator IS one the slice already declares as an individual, that individual is
        // named — the lift resolves the vocabulary rather than only mirroring it.
        let canonical = (spelled_node_domain(node) == "ai.onnx")
            .then(|| {
                CANONICAL_TENSOR_OPERATORS
                    .iter()
                    .find(|(op, _)| *op == node.op_type)
                    .map(|(_, local)| math(local))
            })
            .flatten();
        let tensor_operation = canonical.unwrap_or_else(|| operator.to_owned());
        self.sink
            .iri(&iri, &math("tensorOperation"), &tensor_operation);

        for (index, operand) in operands.iter().enumerate() {
            let operand_iri = self.expression_iri(*operand);
            let slot_key = format!("{key}#{index}");
            let (slot_iri, _) = self.mint("slot", &slot_key);
            self.sink.typed(&slot_iri, &math("ArgumentSlot"));
            let position = i64::try_from(index).unwrap_or(i64::MAX);
            self.sink.integer(&slot_iri, &math("slotIndex"), position);
            self.sink
                .iri(&slot_iri, &math("slotExpression"), &operand_iri);
            self.sink.iri(&iri, &math("argumentSlot"), &slot_iri);
        }
        self.tensor_structures += 1;
        iri
    }

    /// The i-th result of a multi-output node, as an explicit projection application.
    fn emit_projection(&mut self, structure: StructNode, position: usize) -> Value {
        let projection = self.app(&format!("onnx:output-projection:{position}"), &[structure]);
        let key = self.key_of(projection).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if fresh {
            let (operator, operator_fresh) =
                self.mint("operation", &format!("onnx-output-projection|{position}"));
            if operator_fresh {
                self.sink.typed(&operator, &math("Operation"));
                self.label(&operator, &format!("output projection at index {position}"));
            }
            self.sink.typed(&iri, &math("ApplicationExpression"));
            self.sink.iri(&iri, &math("operator"), &operator);
            self.sink.iri(&iri, &math("tensorOperation"), &operator);
            let source_iri = self.expression_iri(structure);
            let (slot_iri, _) = self.mint("slot", &format!("{key}#0"));
            self.sink.typed(&slot_iri, &math("ArgumentSlot"));
            self.sink.integer(&slot_iri, &math("slotIndex"), 0);
            self.sink
                .iri(&slot_iri, &math("slotExpression"), &source_iri);
            self.sink.iri(&iri, &math("argumentSlot"), &slot_iri);
            self.tensor_structures += 1;
        }
        Value { node: projection }
    }

    // -- the parameter block -------------------------------------------------

    /// The one `math:ParameterSpace` the model's weights live in — or none at all.
    ///
    /// `math:ParameterSpace` is a `math:VectorSpace`, so emitting it commits to the
    /// `math:AlgebraicStructure` obligations (a carrier set, at least one operation, at least
    /// one axiom). All four edges of `bridges.ttl:157-162` are emitted together; a model with
    /// no initializer gets no space, because an empty parameter block is not a
    /// zero-dimensional space this lift has grounds to assert.
    fn emit_parameter_space(
        &mut self,
        graph: &GraphProto,
        model_iri: &str,
    ) -> gmeow_errors::Result<Option<String>> {
        if graph.initializer.is_empty() {
            return Ok(None);
        }
        let mut dimension: i128 = 0;
        for tensor in &graph.initializer {
            dimension += i128::from(tensor.element_count()?);
        }
        let dimension = i64::try_from(dimension).map_err(|_| {
            unliftable(format!(
                "the ONNX graph `{}` declares {dimension} parameters in total, which overflows \
                 the 64-bit integer math:spaceDimension is carried as",
                graph_label(graph)
            ))
        })?;

        let (iri, fresh) = self.mint("parameter-space", &graph_label(graph));
        if fresh {
            self.sink.typed(&iri, &math("ParameterSpace"));
            self.sink.iri(&iri, &math("parameterSpaceOf"), model_iri);
            self.label(&iri, &format!("{} parameter space", graph_label(graph)));
            // math:spaceDimension is the LINEAR dimension of an explicitly linear object —
            // distinct from the physical math:hasDimension. It is a shape fact read off the
            // initializer headers, never a value read out of a payload.
            self.sink.integer(&iri, &math("spaceDimension"), dimension);

            let (carrier, _) = self.mint("parameter-set", &graph_label(graph));
            self.sink.typed(&carrier, &math("Set"));
            self.label(&carrier, "the set of parameter vectors");
            self.sink.iri(&iri, &math("underlyingSet"), &carrier);

            let (addition, _) = self.mint("parameter-operation", &graph_label(graph));
            self.sink.typed(&addition, &math("Operation"));
            self.label(&addition, "parameter-vector addition");
            self.sink.iri(&iri, &math("structureOperation"), &addition);

            let (axiom, _) = self.mint("parameter-axiom", &graph_label(graph));
            self.sink.typed(&axiom, &math("Axiom"));
            self.label(&axiom, "the vector-space axioms");
            self.sink.iri(&iri, &math("satisfiesAxiom"), &axiom);
        }
        Ok(Some(iri))
    }

    /// One initializer, as a `math:WeightTensor` held BY REFERENCE.
    fn emit_weight_tensor(
        &mut self,
        tensor: &TensorProto,
        space: Option<&str>,
    ) -> gmeow_errors::Result<()> {
        let Some(element) = data_type_name(tensor.data_type) else {
            return Err(unliftable(format!(
                "initializer `{}` declares element type code {}, which is not one of the \
                 onnx.TensorProto.DataType codes this bridge reads",
                tensor.name, tensor.data_type
            )));
        };
        let space = space.ok_or_else(|| {
            unliftable(format!(
                "initializer `{}` has no math:ParameterSpace to live in; a weight tensor without \
                 a declared parameter space is math:UnframedWeightTensor",
                tensor.name
            ))
        })?;

        let (iri, fresh) = self.mint("weight", &tensor.name);
        if !fresh {
            return Ok(());
        }
        self.sink.typed(&iri, &math("WeightTensor"));
        // Exactly one, always: the max-1 qualified restriction on math:ParameterSpace.
        self.sink.iri(&iri, &math("inParameterSpace"), space);

        // The weight's own shape and element format, as the SAME terms its leaf's
        // math:ExpressionType names — both are content-addressed, so "the slot and the tensor
        // filling it have the same shape" is node identity rather than label equality.
        let axes: Vec<AxisSpec> = tensor.dims.iter().copied().map(AxisSpec::Extent).collect();
        let element_iri = self.emit_element_type(element);
        self.sink
            .iri(&iri, &math("tensorElementType"), &element_iri);
        let shape_iri = self.emit_shape(&axes);
        self.sink.iri(&iri, &math("tensorShape"), &shape_iri);

        self.label(
            &iri,
            &format!(
                "{} : {}",
                tensor.name,
                render_tensor_type(element, Some(&axes))
            ),
        );
        if let Some(layer) = self.weight_layer.get(&tensor.name).cloned() {
            self.sink.iri(&iri, &math("weightOf"), &layer);
        }
        self.tensor_structures += 1;
        Ok(())
    }
}

// ── Tensor shape ──────────────────────────────────────────────────────────────

/// One axis of a shape the lift is about to structure.
///
/// The lift's own reading of an ONNX axis, unifying the two places an extent comes from: a
/// `TensorShapeProto.Dimension` (which may be concrete, symbolic, or unset) and a
/// `TensorProto.dims` entry (always concrete). Keeping them one type is what lets an
/// initializer's `math:ExpressionType` and its `math:WeightTensor` reach the SAME
/// content-addressed `math:TensorShape`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AxisSpec {
    /// A concrete extent.
    Extent(i64),
    /// A symbolic extent the model leaves open — ONNX's `dim_param`.
    Symbolic(String),
    /// An axis whose `oneof` arm the producer left unset: the rank counts it, the extent is
    /// unstated. Distinct from an extent of 0, which is a stated, empty axis.
    Unstated,
}

impl AxisSpec {
    fn of_dim(dim: &Dim) -> Self {
        match dim {
            Dim::Value(extent) => Self::Extent(*extent),
            Dim::Param(name) => Self::Symbolic(name.clone()),
            Dim::Unknown => Self::Unstated,
        }
    }

    /// The axis as a reader sees it in a bracketed shape.
    fn render(&self) -> String {
        match self {
            Self::Extent(extent) => extent.to_string(),
            Self::Symbolic(name) => name.clone(),
            Self::Unstated => "?".to_owned(),
        }
    }

    /// The axis as a mint key sees it.
    ///
    /// Arm-tagged, unlike [`AxisSpec::render`]: a symbolic axis literally named `3` and a
    /// concrete extent of 3 render identically but are different claims, and a shape node is
    /// content-addressed on this key.
    fn key(&self) -> String {
        match self {
            Self::Extent(extent) => format!("#{extent}"),
            Self::Symbolic(name) => format!("${name}"),
            Self::Unstated => "?".to_owned(),
        }
    }
}

/// The content-addressing key of a whole axis list.
fn shape_key(axes: &[AxisSpec]) -> String {
    let parts: Vec<String> = axes.iter().map(AxisSpec::key).collect();
    format!("[{}]", parts.join(","))
}

/// An axis list as a reader sees it: `[4,3]`, `[1,batch,?]`, `[]` for a scalar.
fn render_axes(axes: &[AxisSpec]) -> String {
    let parts: Vec<String> = axes.iter().map(AxisSpec::render).collect();
    format!("[{}]", parts.join(","))
}

/// A whole tensor type as a reader sees it — the `rdfs:label` of its `math:ExpressionType`.
///
/// `None` axes is ONNX's "no TensorShapeProto", i.e. the rank is unknown; it is spelled out
/// in words rather than as `[]`, because `[]` is rank 0, a scalar, and a strictly stronger
/// claim.
fn render_tensor_type(element: &str, axes: Option<&[AxisSpec]>) -> String {
    match axes {
        Some(axes) => format!("tensor({element}){}", render_axes(axes)),
        None => format!("tensor({element}) of unknown rank"),
    }
}

// ── Residue ───────────────────────────────────────────────────────────────────

/// What this lift READ in the decoded model and still did not carry into `math:`.
///
/// `math:unmappedConstruct`'s own `gmeow:useWhen` requires this of any lift whose rung is
/// weaker than `logic:ExactPreservation`: "so the declared loss is accompanied by its actual
/// content". The ONNX rung is a `logic:LossyLens`, so declaring the rung and enumerating
/// nothing would be asserting a loss the run cannot name.
///
/// Every entry is CONDITIONAL on the construct actually appearing in this model. A fixed list
/// would claim a loss for constructs the file never carried, which is the mirror-image lie.
fn unmapped_constructs(model: &ModelProto, graph: &GraphProto) -> Vec<String> {
    let mut out = Vec::new();

    // The one loss that makes this a lens rather than an isomorphism. It is not "skipped by
    // oversight": `TensorProto` has no field that could hold a payload byte, so the doctrine
    // is structural — but structural or not, the values ARE in the source and are NOT in the
    // codomain, which is exactly what this enumeration is for.
    if !graph.initializer.is_empty() {
        out.push(
            "onnx.TensorProto payload (float_data / int32_data / int64_data / double_data / \
             uint64_data / raw_data / external_data): initializer VALUES are held by \
             reference; the shape and element type cross, the bytes do not"
                .to_owned(),
        );
    }
    // Attribute arms the decoder does not structure — chiefly the `g` / `graphs` control-flow
    // subgraphs of `If` / `Loop` / `Scan`. The node itself lifts and the attribute keys the
    // operator by kind, but the subgraph's own nodes do not become computation nodes here, so
    // the loss is named per node+attribute rather than as one blanket row.
    for (index, node) in graph.node.iter().enumerate() {
        for attribute in &node.attribute {
            if let Some(kind) = attribute.unstructured {
                out.push(format!(
                    "onnx.AttributeProto `{}` on node `{}`: the `{kind}` arm is not decoded, so \
                     the attribute's presence and kind cross but its contents do not",
                    attribute.name,
                    node_label(index, node)
                ));
            }
        }
    }
    if graph
        .node
        .iter()
        .any(|node| node.attribute.iter().any(|attribute| attribute.t.is_some()))
    {
        out.push(
            "onnx.AttributeProto `t` payload: a tensor-valued operator attribute contributes \
             its header to the operator's identity, and its values are held by reference like \
             any other tensor's"
                .to_owned(),
        );
    }
    // An entry ONNX permits and this lift cannot pair: math:annotationKey is the name under
    // which the source filed the annotation, and an unnamed annotation is an unattributable
    // string rather than a pair.
    if model.metadata_props.iter().any(|prop| prop.key.is_empty()) {
        out.push(
            "onnx.StringStringEntryProto with an empty `key`: a math:SourceAnnotation is a \
             KEY/value pair, and an entry filed under no key cannot be one"
                .to_owned(),
        );
    }
    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// A node's domain, with ONNX's empty-string default spelled out.
fn spelled_node_domain(node: &NodeProto) -> &str {
    if node.domain.is_empty() {
        "ai.onnx"
    } else {
        &node.domain
    }
}

/// A graph's name, or a stable stand-in when the producer left it empty.
fn graph_label(graph: &GraphProto) -> String {
    if graph.name.is_empty() {
        "unnamed ONNX graph".to_owned()
    } else {
        graph.name.clone()
    }
}

/// A node's name, or its op_type and position when the producer left it empty.
fn node_label(index: usize, node: &NodeProto) -> String {
    if node.name.is_empty() {
        format!("{}#{index}", node.op_type)
    } else {
        node.name.clone()
    }
}

/// An operator's attribute configuration, canonically rendered.
///
/// Sorted by name so a producer's field order cannot change an operator's identity, and
/// deterministic so a re-lift mints the same IRI.
fn attribute_signature(attributes: &[AttributeProto]) -> String {
    let mut rendered: Vec<String> = attributes.iter().map(AttributeProto::render).collect();
    rendered.sort();
    rendered.join(",")
}

fn unliftable(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(OnnxUnliftable { detail })
}

#[path = "lift.tests.rs"]
#[cfg(test)]
mod tests;
