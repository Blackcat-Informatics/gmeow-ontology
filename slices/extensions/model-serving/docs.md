<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Model-serving — artifact, deployment, execution, and session as distinct identities

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/model-serving` · **tier: extension**
> A thin, standalone LLM-operations vocabulary. Depends only on core (extension → core, DAG-legal).

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
