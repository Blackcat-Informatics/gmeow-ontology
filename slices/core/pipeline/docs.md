<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Pipeline

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/pipeline` · **tier: core**

The project's own build, authored as data. GMEOW is built by a directed acyclic graph of typed
**stages** that exchange an in-memory RDF dataset rather than re-parsing the GTS bundle from disk per
generator. This slice models that graph — `gmeow:Pipeline` and `gmeow:PipelineStage` — so the build is
a first-class ontological citizen: the `gmeow-pipeline` Rust executor reads these individuals back,
validates the graph, binds each stage to its Rust implementation, and runs it single-pass.

## The dogfooded build graph

`gmeow:pipeline-build` is the canonical build. Its spine flows:

```text
source-load → (statements, mappings) → reason → gts-compose → {docs-render + export leaves} → gts-sink
```

Each `gmeow:PipelineStage` is a `logic:ActionSchema` and declares:

| Property | Meaning |
|---|---|
| `gmeow:stageImpl` | The `STAGE_REGISTRY` key binding the stage to its Rust `Stage`. |
| `gmeow:dataflowConsumes` | The upstream stages whose products it reads (consumer → producer). |
| `gmeow:requiresResource` | The shared `gmeow:Resource`s it must hold exclusively while running (e.g. `gmeow:engineResource`); stages competing for one serialize. |
| `gmeow:hasCapability` | The `gmeow:StageCapability`s it holds — the executor reads these in place of a kind enum. |
| `gmeow:producesFormat` | Output-format tags (export leaves). |

Reified `gmeow:BuildDataFlow` edges (`gmeow:buildFlowFrom` / `gmeow:buildFlowTo` / `gmeow:flowEntity`)
narrow a consumer's dependency to the specific named graphs it reads, so the executor keys that
stage's cache on only those graphs' digests — artifact-level incremental rebuild.

## Stage capabilities

`gmeow:StageCapability` is the value vocabulary the executor reads instead of a kind enum:
`gmeow:sinkCapability` (the single serialization exit — the gts narrow waist; the loader HARD-fails
unless exactly one stage holds it) and `gmeow:sourceOrigin` (the authored-source loader, whose emitted
quads' provenance origin is `Source`). A stage holding no capability is a plain transform / validate /
export leaf — provenance-`Generated`, non-sink, parallel-eligible within its topological level.
Serialization treatment is not a kind either — it is a declared resource conflict. Two resources are
minted, for two different reasons. The reasoning stage `gmeow:requiresResource` `gmeow:engineResource`
because the process-wide reasoning state is shared mutable state. The two whole-dataset serialization
leaves (`gmeow:stage-export-export`, `gmeow:stage-export-yaml-ld`) `gmeow:requiresResource`
`gmeow:serializationBufferResource` because their PEAK RESIDENCY is exclusive: each turns the entire
carrier into in-memory documents (9.06 GiB and 8.37 GiB of measured peak allocation, an order of
magnitude above any other export leaf), so concurrently they add their peaks and exhaust a 16 GB build
host. Stages holding a resource serialize against every other stage holding it; every other leaf in
the level keeps full parallelism.

## Invariants (HARD-failed before any stage runs)

The loader proves, before scheduling: the DAG is **acyclic** and **complete** (no dangling
`gmeow:dataflowConsumes`); there is **exactly one** stage holding `gmeow:sinkCapability`; and every
bound stage's `capabilities` / `consumes` / `requiresResource` / typed dataflow **agree** with its RDF
declaration (single source of truth — RDF and Rust cannot disagree). None of these is optional or
repairable.

## The general runtime-DAG vocabulary

`gmeow:pipeline-build` is *one* consumer of this slice: the project's own build. The same vocabulary is
authored to be the single canon for any **scheduled enrichment DAG** — the in-rush enrichment planner
consumed downstream by `lillith_decodes`. Six families extend the pipeline surface with the runtime
facts such a DAG must declare. It is **graph + constraint vocabulary only**: no scheduler semantics are
modelled here (a deliberate non-goal). The Rust projection that reads it is the downstream consumer's
commitment, not this repo's.

1. **Edge reasons — why an edge exists.** `gmeow:FlowReason` is a closed set of three individuals
   carried by the **required** `gmeow:flowReason` on every `gmeow:BuildDataFlow` edge:
   `gmeow:flowReasonDataflow` (bytes flow), `gmeow:flowReasonAuthority` (a **first-class ordering** the
   consumer must run after the producer *even when no bytes flow* — a decision precedence a pure-dataflow
   reasoner would wrongly drop), and `gmeow:flowReasonCost` (an edge added only to serialize two stages
   sharing a scarce resource). A single required property composes with the existing reified edge — one
   edge, one typed reason — rather than a parallel `AuthorityFlow` subclass. The build DAG's own five
   edges all declare `gmeow:flowReasonDataflow`.

2. **Per-stage failure dispositions + obligation.** `gmeow:StageDisposition` is a closed set of six
   outcomes on a `gmeow:StageExecution` (one run of a stage) via `gmeow:stageDisposition`:
   `dispositionCompleted`, `dispositionTypedAbsence` (the stage **ran** and produced an explicit typed
   "nothing" — a valid answer that links a reason individual, never a failure), `dispositionRejectedCandidate`,
   `dispositionRetryable`, `dispositionFatalToCandidate`, and `dispositionTimedOut`. Independently, the
   stage-level `gmeow:stageObligation` (closed: `obligationMandatory` / `obligationDegradable`) makes
   *"this stage's absence invalidates the run"* a **declared property** rather than scheduler convention.

3. **Stability.** `gmeow:StageStability` (closed: `stabilityStablePrefix` / `stabilityPerTurnVariance`),
   on a stage's output via `gmeow:stageStability`. `stabilityStablePrefix` means **byte-identical across
   turns**, the precondition a prefix-cache ordering relies on to place stable stages first and reuse
   their result; `stabilityPerTurnVariance` marks output that varies per turn and cannot be prefix-cached.

4. **Budgets.** `gmeow:StageBudget` reifies a stage's per-run ceilings, linked by `gmeow:hasStageBudget`:
   `gmeow:budgetDeadline` (an **`xsd:duration`** — a *relative*, turn-invariant, replayable budget, chosen
   over an absolute `xsd:dateTime` that would bake one run's clock into the canon; a breach yields a
   `dispositionTimedOut`) and `gmeow:budgetMaxSteps` / `budgetMaxAnswers` / `budgetMaxInputBytes` /
   `budgetMaxMemoryBytes` (all `xsd:nonNegativeInteger`; unset means unbounded on that axis).

5. **Receipts.** `gmeow:StageReceipt` is a content-addressed record of one stage's contribution —
   `gmeow:receiptDigest` (the content address) and `gmeow:receiptOfStage` — gathered onto a
   `gmeow:PipelineRun` by `gmeow:hasStageReceipt`. Receipts serialize in deterministic **topological**
   order over the DAG *even under concurrent execution*, so an interleaved trace replays identically and
   two runs are byte-comparable stage by stage.

6. **Admission constraint.** `gmeow:RuntimeDagAdmissionConstraint` is a `logic:Constraint` + `logic:Formula`
   (authored in `logic:` only — no hand-authored SHACL) encoding four expectations of the declared graph:
   **(a) acyclicity**, **(b) connectivity** (no unreachable/dead stage), **(c)** every `gmeow:dataflowConsumes`
   satisfied by a producing stage on a preceding path, and **(d)** every **authority** edge satisfied —
   retained even when no bytes flow. The first-order-checkable core (c + d) is the `logic:integrity`
   formula; the transitive-closure arms (a + b) are certified by the `logic:DagWorkflowContract` the DAG
   runs under and disclosed honestly through `gmeow:runtimeDagAdmissionBoundary` (a second-order
   expressiveness boundary), never faked as a first-order formula (Principle 17). The worked
   `examples/in-rush-enrichment-dag.ttl` and its conformance CQ pin that the authority edge a pure-dataflow
   reduction would drop is kept.

## Consumer

The `gmeow-pipeline` crate. The migrated `meta:gate-generator-registry` queries
`gmeow:pipeline-build`'s stages instead of the retired Python generator list, so the registry gate is
itself dogfooded against this slice. The runtime-DAG vocabulary above is consumed downstream by the
`lillith_decodes` in-rush enrichment planner, which declares its scheduled DAG in GMEOW as the single
canon.
