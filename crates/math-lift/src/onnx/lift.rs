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
//! | `metadata_props`, `producer_name` | provenance on the retained `math:parseSource` witness |
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
//! doctrine is discharged structurally rather than by the lift remembering to skip. What
//! *does* cross is the element count, as the `math:spaceDimension` of the parameter space —
//! a shape fact, not a value.
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
//!   minting two sets the model never declares.
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
/// `rdfs:comment` — the carrier for ONNX metadata that has no `math:`/`gmeow:` image.
/// See [`Lift::provenance`] for why these do not become structured nodes.
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";

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

    let frame = RunFrame::mint(BridgeKind::Onnx, mint_base, source);
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
    /// # Why `metadata_props` stays an annotation
    ///
    /// `ir_version`, `model_version`, `domain`, and each `metadata_props` entry ride as
    /// `rdfs:comment` strings rather than as structured nodes, because no `math:` or
    /// `gmeow:` term faithfully carries them.
    ///
    /// `gmeow:Identifier` is the tempting fit and the wrong one: its own definition scopes
    /// it to "a reified EXTERNAL-IDENTIFIER record — an ORCID, a geni profile id, a Nostr
    /// nip05, a LEI, a ROR ID, a NAICS code", and its `gmeow:avoidWhen` polices that
    /// boundary explicitly ("a name borne by an entity is a `gmeow:Appellation`, not an
    /// Identifier"). An ONNX `metadata_props` entry is a producer's free-form annotation —
    /// `author`, `license`, `converted_from` — which identifies nothing and resolves
    /// nowhere. Typing it `gmeow:Identifier` would assert an external-identity claim the
    /// source never made.
    ///
    /// `logic:ProjectionLoss` is the other near-fit and is also wrong here: `logic:lossCode`
    /// binds its values to the conversion loss ledger's own vocabulary
    /// (reifier-layer-dropped / annotation-layer-dropped / standpoint-scope-dropped), so
    /// minting an ONNX-specific code would fabricate a ledger entry.
    ///
    /// Nothing is lost by the annotation route: an ONNX metadata prop IS a string pair in
    /// the source, so carrying it as a string flattens no structure. This is not the
    /// forbidden case — the ingestion rule bars degrading a structured EXPRESSION to a
    /// string, and there is no expression here to degrade.
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

        // One comment per entry, each self-contained as `key=value`, so several entries can
        // never be cross-paired by a projection the way flat scheme/value properties would.
        for (scheme, value) in entries {
            self.sink
                .string(&witness, RDFS_COMMENT, &format!("{scheme}={value}"));
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
        let rendered = match &tensor.shape {
            Some(dims) => {
                let axes: Vec<String> = dims.iter().map(Dim::render).collect();
                format!("tensor({element})[{}]", axes.join(","))
            }
            // ONNX's own reading: no TensorShapeProto means the rank is unknown, which is a
            // weaker claim than rank 0 and is spelled as such rather than as "[]".
            None => format!("tensor({element}) of unknown rank"),
        };
        self.attach_rendered_type(expr_iri, &rendered);
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
        let axes: Vec<String> = tensor.dims.iter().map(i64::to_string).collect();
        let rendered = format!("tensor({element})[{}]", axes.join(","));
        self.attach_rendered_type(expr_iri, &rendered);
        Ok(())
    }

    /// Intern a rendered type and hang it off an expression.
    ///
    /// Content-addressed on the rendering, so two values of the same tensor type share ONE
    /// `math:ExpressionType` individual rather than minting a copy per mention.
    fn attach_rendered_type(&mut self, expr_iri: &str, rendered: &str) {
        let (iri, fresh) = self.mint("type", rendered);
        if fresh {
            self.sink.typed(&iri, &math("ExpressionType"));
            self.label(&iri, rendered);
        }
        self.sink.iri(expr_iri, &math("expressionType"), &iri);
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
        index: usize,
        node: &NodeProto,
        theory: &str,
    ) -> gmeow_errors::Result<String> {
        for attribute in &node.attribute {
            if let Some(kind) = attribute.unstructured {
                return Err(unliftable(format!(
                    "ONNX node `{}` carries the attribute `{}` in the `{kind}` arm, which this \
                     bridge does not structure. An attribute is part of the operator's identity, \
                     so lifting the node without reading it would misstate what the node \
                     computes",
                    node_label(index, node),
                    attribute.name
                )));
            }
        }

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
        let axes: Vec<String> = tensor.dims.iter().map(i64::to_string).collect();
        self.label(
            &iri,
            &format!("{} : tensor({element})[{}]", tensor.name, axes.join(",")),
        );
        if let Some(layer) = self.weight_layer.get(&tensor.name).cloned() {
            self.sink.iri(&iri, &math("weightOf"), &layer);
        }
        self.tensor_structures += 1;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ns::logic;
    use crate::onnx::encode::{
        MLP_BIAS_PAYLOAD, MLP_WEIGHT_PAYLOAD, bytes_field, message_field, mlp, string_field,
        truncated, varint_field,
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
        let err = lift(&model, BASE).expect_err("an unread attribute must not lift");
        let text = format!("{err}");
        assert!(text.contains("control-flow subgraph"), "{text}");
        assert!(text.contains("`body`"), "{text}");
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
        assert_eq!(count(&ttl, &format!("<{}>", math("slotIndex"))), 5);
        assert_eq!(count(&ttl, r#""0"^^"#), 3, "three slots at index 0");
        assert_eq!(count(&ttl, r#""1"^^"#), 2, "two slots at index 1");
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

    #[test]
    fn the_provenance_lands_on_the_retained_source_witness() {
        let ttl = turtle(MLP);
        assert_eq!(typed_gmeow(&ttl, "SoftwareAgent"), 1);
        assert_eq!(count(&ttl, &format!("<{}>", gmeow("wasAttributedTo"))), 1);
        assert!(
            ttl.contains("gmeow-math-lift 1"),
            "the producer label:\n{ttl}"
        );
        // ONNX metadata is a producer's free-form annotation, so it rides as rdfs:comment
        // on the retained witness — never as gmeow:Identifier, whose own definition scopes
        // it to an EXTERNAL identifier (ORCID, LEI, ROR) that resolves somewhere.
        assert_eq!(
            typed_gmeow(&ttl, "Identifier"),
            0,
            "ONNX metadata identifies nothing; typing it gmeow:Identifier would assert an \
             external-identity claim the source never made:\n{ttl}"
        );
        assert_eq!(count(&ttl, &format!("<{}>", gmeow("hasIdentifier"))), 0);
        assert!(ttl.contains("model_license=AGPL-3.0-only"), "{ttl}");
        let witness = format!("<{}>", frame_witness());
        // ir_version plus model_version plus the one metadata_props entry.
        assert_eq!(
            ttl.lines()
                .filter(|l| l.starts_with(&witness) && l.contains(&format!("<{RDFS_COMMENT}>")))
                .count(),
            3,
            "every metadata annotation hangs off the math:parseSource witness"
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
        let hex: String = MLP_WEIGHT_PAYLOAD[0]
            .to_le_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(
            !ttl.contains(&hex),
            "raw payload bytes leaked as hex:\n{ttl}"
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
             and math:codomain on math:Set, which an ONNX graph never states"
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
        assert_eq!(count(&ttl, r#""0"^^"#), 1);
        assert_eq!(count(&ttl, r#""1"^^"#), 1);
        assert_eq!(count(&ttl, r#""2"^^"#), 0, "no gap, no phantom slot");
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
}
