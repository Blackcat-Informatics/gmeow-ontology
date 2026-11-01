<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Model-serving — artifact, deployment, execution, and session as distinct identities

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/model-serving` · **tier: extension**
> A thin, standalone LLM-operations vocabulary. Depends on core and canonical mathematics, without extension-to-extension dependencies.

The tempting collapse is to map deployment, runtime execution, and a single call all onto one
`gmeow:AwarenessTenure`, differing only by granularity. That erases distinct identity criteria: a
**deployment** binds an artifact·service·host·endpoint·configuration·interval; a **runtime execution**
is an occurrent process; an **awareness tenure** records an agent being in a processing *mode* over an
interval. Serving mode (`modeOnlineInference` / `modeOfflineReplay` / `modeTraining`) is a valuable
*facet* of a served agent — but it cannot *be* a deployment or an execution, because it carries none of
the binding structure. So this slice gives each an explicit identity and uses the awareness tenure as a
facet, not a substitute (`design/INHABITED-RUNTIME-STACK.md`).

Its Principle-15 consumer, declared in the manifest: **the LLM-ops / AI-agent audience** — anyone
tracing an output to the exact `ModelArtifact`·`ModelDeployment`·`RuntimeExecution`·`AgentSession`·
`ModelInvocation` that produced it. The `agent-runtime` profile is its first live consumer; the
OpenTelemetry-GenAI seam is its interchange consumer.

## The four identities

- **`gmeow:ModelArtifact`** (a `logic:Kind` ⊑ `gmeow:InformationObject`) — WHAT runs. Aligned to the
  software slice's `gmeow:Distribution` **by reference** (a sibling Kind under the shared parent
  `InformationObject`), *not* `rdfs:subClassOf gmeow:Distribution`: a range/subclass axiom is a real
  dependency regardless of `owl:imports`, and an extension may not depend on another extension
  (Principle 16). The correspondence to `gmeow:Distribution` rides `mappings/equivalences.ttl`.
- **`gmeow:ModelDeployment`** (a `logic:Kind` ⊑ `logic:Relator`) — the served, callable realization
  that binds artifact·service·host·endpoint·configuration·interval. Its serving mode is a
  `gmeow:AwarenessTenure` facet on its `gmeow:deploymentService` `SoftwareAgent` — never on the
  deployment relator, because `gmeow:awarenessSubject` ranges over `gmeow:Agent`.
- **`gmeow:RuntimeExecution`** (a `logic:Event` ⊑ `gmeow:Activity`) — a particular occurrent running of
  a deployment, within which `gmeow:ModelInvocation` events occur.
- **`gmeow:AgentSession`** (a `logic:Event` ⊑ `gmeow:Activity`) — an event aggregate, NOT a relator
  subclassing a situation. Its sub-events (invocations, tool calls, episodes) relate via
  `gmeow:subEventOf`; internal order is a `logic:Path` resolved by the solver, never an asserted
  `nextInvocation` chain (Principle 12).

## The ModelCard split

`gmeow:describesModel` is `owl:FunctionalProperty` ranging over `gmeow:SoftwareAgent`, so a
`Distribution` as its target would be inferred a `SoftwareAgent`. The relations split:
`gmeow:describesModelArtifact` (ModelCard → ModelArtifact) carries architecture / version / training
cutoff / context window; `gmeow:describesModelService` (ModelCard → ModelDeployment) carries provider /
endpoint / rate limits / placement. Each property is assigned to the entity that actually bears it.

## The de-conflation chain and its flat shortcut (the commuting diagram)

Tracing an output to its durable subject is an 8–10 hop traversal:

```text
output → invocation → execution → deployment → artifact
                    ↳ invocation → session → configuration / subject-stage
```

each hop named (`wasGeneratedBy`, `invocationInExecution`, `executionOfDeployment`,
`deploymentArtifact`; `subEventOf`, `sessionConfiguration`, `sessionSubjectStage`). The
`sessionConfiguration` / `sessionSubjectStage` targets are **open-range** so the slice stays standalone;
their inhabitation typing happens at the instance level in the `agent-runtime` profile.

The de-conflation buys correctness at the cost of path length, so the runtime projects **flat,
materialized upper-projections** for the hot path — `gmeow:generatedForSubject` and
`gmeow:generatedUnderConfiguration`, computed by Datalog rules at the projection layer (Principle 12),
generated not authored. This is a **commuting diagram**: the flat shortcut equals the collapse of the
nested path (`flat = collapse(nested)`), and a drift gate proves the two agree — real-time recall reads
the shortcut, an audit walks the full path, and neither may silently disagree (Principle 7). The
OpenTelemetry-GenAI correspondence is likewise directional: projecting GMEOW → OTel is a
`SoundUnderApproximation` (the endurant/epistemic structure is honestly dropped), while ingesting OTel →
GMEOW is *enriching* (a trace lifts to a `RuntimeExecution` acquiring the subject and provenance the
span could not express) — the Galois-connection shape of a super-ontology earning its name.

## Capability use versus delegation (CQ5)

Resolved without minting a wrapper class: a `gmeow:usedCapability` edge from an invocation / execution
to a `logic:ActionSchema` is **passive** use; a `gmeow:ToolCall` whose `gmeow:usedTool` points to a
distinct `gmeow:SoftwareAgent` is **delegation**.

This slice owns only the passive half. `gmeow:ToolCall` / `gmeow:usedTool` belong to
`extensions/agentic`, a SIBLING extension, and Principle 16 forbids a dependency in either direction —
a competency query naming them would BE that dependency, whichever lane it runs in. The executable CQ5
therefore lives in the profile that selects both extensions and mints nothing:
`slices/profile/agent-runtime/queries/competency/tool-usage.rq` over its `examples/tool-usage.ttl`,
alongside the end-to-end query that is there for the same reason.

## Mathematical artifact profiles and evaluation

The serving topology remains independent of mathematical disclosure. A tokenizer
can remain a `ModelArtifact`; selecting `LearnedModelArtifact` declares a distinct
`math:LearnedModel`, and selecting `TensorModelArtifact` additionally declares its
forward `math:TensorComputationGraph` and `math:WeightTensor` parameters. The
artifact is an information object, the learned model is a mathematical object,
and neither is a deployment, runtime execution or invocation (Principles 4 and 5).

Use `artifactLearnedModel`, `artifactComputationGraph` and `artifactWeightTensor`
for those roles. Every graph's `math:architectureOf` identifies the declared
learned model. A weight's `math:inParameterSpace` points to a `math:ParameterSpace`
whose `math:parameterSpaceOf` names that model or graph. Extra incompatible owners
are rejected. Computation nodes are `math:ApplicationExpression` nodes using the
existing operator and indexed argument-slot AST. Tensor shape and element format
reuse `math:TensorShape`, `math:TensorAxis` and `math:TensorElementType`; unknown
shape remains absent, and is not asserted as a rank-zero tensor.

Imported classes carry their owners' complete contracts. ParameterSpace inherits
AlgebraicStructure through VectorSpace and Module, requiring `math:underlyingSet`,
`math:structureOperation` and `math:satisfiesAxiom`. A declared
`artifactTrainingProblem` names an OptimizationProblem with a LossFunction
`math:objectiveFunction`; the loss inherits Function's typed Set domain and
codomain. This optional declaration records a training specification, not a
training run, and is not required for every learned artifact. The worked example
records a real commutativity law for its parameter addition, without claiming a
complete vector-space proof.

Quantitative evaluations are held `ModelMetricObservation` nodes. Each names its
`vantage`, `observedFeature` artifact, `observationMethod` protocol,
`evaluatedMetric`, and Quantity `observationResult`. The open `ModelMetric`
individual declares exactly one typed `metricDimension`; every result must carry
that same dimension and exactly one finite numeric `math:quantityValue`.
Dimensionless accuracy and time-valued latency use different metric declarations.
The concrete latency witness carries QUDT seconds through `gmeow:unit` and a
complete scalar temporal ReferenceFrame. The generic metric profile imposes no
universal frame requirement on abstract dimensionless metrics. Unit-bearing
values use the existing foundation frame discipline and the existing places
frame profile; unit functionality remains with core observations and native DL
consistency. Two object-valued units do not alone establish a contradiction under
open-world identity: functionality may identify them unless distinctness is known.
Rational-only carriers, vector, interval, unbounded and categorical results remain
ordinary Observations under their appropriate contracts; do not force them into
this finite numeric-literal subtype or manufacture approximations. This selected
scalar metric profile does not make every artifact numeric or
reinterpret categorical assessments, tensor extents or parameter counts as
physical quantities. An observation method is a protocol value, not an evaluator
implementation, invocation or result. Multiple observers and multiple results
coexist without selecting a preferred observation (Principle 9).

See [the complete mathematical example](examples/mathematical-model.ttl). All
integrity rules are authored in canonical logic and projected into validation;
the vocabulary neither runs a model nor computes a benchmark (Principles 12 and
17). Tensor and metric magnitudes follow their canonical mathematical owners;
these profiles introduce no competing scalar-property path.

These profiles check declared artifact, graph and parameter ownership and
reported metric dimensions and finite magnitudes. They neither compute a
benchmark nor certify tensor payloads, numerical operator compatibility or
arbitrary tensor-shape consistency. Shape-rank, axis-count and extent validation
remain the math owner's separate contract; the displayed tensor shape is an
ordinary declaration, not an executed conformance result.
