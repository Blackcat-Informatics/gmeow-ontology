<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — The Ingestion Bridges

> The **bridges charter** of the GMEOW Mathematics design set: the concrete front-ends that lift
> external artifacts into the grounding layer — the **R → `math:` bridge** (any R script), the
> **ONNX → `math:` bridge** (any tensor model), and **proof-as-process** (a proof-assistant
> dependency graph as a goal-decomposed process). Each is an instance of the parse-into doctrine of
> [`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md), and together they carry three flagships —
> R-bridge (4), AI self-structure ingestion (5), and complex proofs as process (3). Anchors are in
> [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md); gates in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's gates and the projection loss ledger — not a
> claim that any implementation already realizes X except as those gates demonstrate.

## Purpose

A bridge is a front-end that **parses an external artifact into the canon**, splitting it across
the `logic:` and `math:` grounding layers: mathematical/structural content lifts into `math:`, computation and control
lower into `logic:`. Every bridge obeys the runtime charter's hard rules — no silent fallback to a
string, `parseSource` retained, loss recorded, no optional backends — and every bridge **hard-fails
with a typed diagnostic** on anything it cannot lift, because "for *any* input" is a universality bar,
not a best-effort aspiration. The bridges are the layer's most visible demonstration that `math:` and
`logic:` are complementary: one lifter, two grounding layers engaged.

## The R → `math:` bridge — the R flagship

Core classes: `math:RIngestRun` (a `gmeow:Activity`), `math:RModelLift`, and the target `math:`
objects it produces.

The R bridge lifts an R script's mathematical and statistical content into `math:` and its
computation into `logic:`:

- data frames → `math:DatasetMatrix` (held by reference); vectors/matrices → `math:Vector`/`math:Matrix`;
- model formulas `y ~ x1 + x2` → a `math:ModelFormula` AST (the `~` is a binder, not a string);
- `lm`/`glm`/`lmer` → `math:StatisticalModel`/`math:FittedModel` with declared assumptions;
- `rnorm`/`dbinom`/… → `math:Distribution` with family and parameterization;
- arithmetic/transforms → `math:ApplicationExpression`s; control flow and general computation → `logic:`.

The natural output shape is the `broom` tidy/glance/augment triple (tidy coefficients, model-level
statistics, augmented residuals), which maps cleanly onto `math:Estimate`/`math:FittedModel`/
`math:Residual`. No "R ontology" exists — the lifter is an authored Rust `math-parse` front-end
([`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md)).

> **Flagship — R, for any script.** Answerable when the bridge lifts an arbitrary R script's
> statistical content into `math:` and its computation into `logic:`, retains the source as
> `math:parseSource`, records any loss, and **hard-fails** with a typed diagnostic on anything it
> cannot lift — never emitting a degraded or string-valued placeholder
> (`math:UnliftableIngest`, [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)).

## The ONNX → `math:` bridge — the AI-self-structure flagship (ingestion)

Core classes: `math:ONNXIngestRun` (a `gmeow:Activity`) producing a `math:TensorComputationGraph`
([`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md)).

The ONNX bridge lifts a model graph into `math:`: ONNX `NodeProto`s → tensor-operator
`math:ApplicationExpression`s; initializers → `math:WeightTensor`s in a `math:ParameterSpace`; the
opset → the operator vocabulary; graph inputs/outputs → typed tensor slots; `metadata_props` →
provenance. The result is exactly the self-structure object of the learning charter, so an AI can
describe its own architecture by lifting its own ONNX export. ONNX (Apache-2.0) is the interchange
anchor; the *meaning* of the architecture is authored, not in ONNX.

> **Flagship — AI self-structure (ingestion).** Answerable when the bridge lifts an ONNX/model graph
> into a `math:TensorComputationGraph` with weights in a declared parameter space, so the architecture
> is a first-class `math:` object the AI (and GMEOW) can reason and reflect over.

## Proof-as-process — the complex-proofs flagship

Core classes: `math:ProofIngestRun` (a `gmeow:Activity`), `math:ProofDependencyGraph`, binding the
proof layer of [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md) to `logic:`'s teleology and
transaction layers.

A proof-assistant library exposes a **declaration dependency DAG** — mathlib's declaration graph,
Metamath's axiom→theorem DAG (CC0), TPTP/TSTP proof-step DAGs. The bridge lifts such a DAG into the
`math:` proof layer (`math:Proof`/`math:ProofStep`/`math:dependsOnAxiom`) and *frames the proof effort
as a process*: each lemma is a sub-goal (`logic:` teleology), each step an action, the QED a
`math:FormalVerificationResult` held as a `gmeow:Observation`. GMEOW's workflow/orchestration is the
executable shape of a proof search, and a complex proof becomes a goal-decomposed process with
recorded dependencies and provenance.

> **Flagship — complex proofs as process.** Answerable when a proof-assistant dependency DAG lifts
> into the `math:` proof layer bound to `logic:` teleology/transaction, so a complex proof is a
> goal-decomposed process whose steps, axiom dependencies, and verification claim are all first-class.

## The shared bridge contract

Every bridge:

1. splits its input across `math:` (structure) and `logic:` (computation);
2. retains the source (`math:parseSource`) and records any loss in the ledger;
3. runs as a `gmeow:Activity`, with results returned through the process/result/claim split;
4. **hard-fails** with a typed diagnostic on the unliftable — no silent fallback, no optional backend;
5. is an authored Rust front-end (no external ontology defines these lifts).

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): a bridge run is a
`gmeow:Activity`; it retains `math:parseSource` and records loss; it splits content across `math:`/
`logic:`; and it raises `math:UnliftableIngest` (never a degraded artifact) on anything it cannot
lift.

## Competency questions

1. From which R script (and at what fidelity) was this model lifted, what did it lose, and what did it
   route to `logic:`?
2. What tensor computation graph and parameter space did this ONNX model lift into?
3. What proof dependency DAG underlies this theorem, and how does its proof decompose into sub-goals
   and steps?
4. Which ingests hard-failed as unliftable, and with what diagnostic?
5. Which bridge results are held as observations, by which activity and vantage?
