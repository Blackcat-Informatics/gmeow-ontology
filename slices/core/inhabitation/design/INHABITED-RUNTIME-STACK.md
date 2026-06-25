<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The AI Runtime Stack

> The **model-serving vocabulary and the `agent-runtime` profile.** This document specifies the thin
> standalone `extensions/model-serving` slice (reusable beyond inhabitation) and how the single
> `agent-runtime` profile composes it with `core/inhabitation`. Deployment, runtime execution, and the
> model artifact have **explicit identities** rather than being collapsed onto an awareness tenure; the
> `ModelCard` relations are split so a `Distribution` is not inferred to be a `SoftwareAgent`; the
> session is an event aggregate, not a relator subclassing a situation; and migration content rides a
> transfer manifest, not coincidence. The minimal core it builds on is
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) / [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md);
> dispositions are fixed by [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## Why deployment, execution, and artifact need explicit identities

Mapping deployment, runtime execution, and a single invocation all onto `gmeow:AwarenessTenure`
(differing only by granularity) would erase distinct identity criteria:

- a **deployment** binds an artifact, a service identity, a host, an endpoint, configuration, an
  operator, a policy, and a serving interval;
- a **runtime execution** is an occurrent process;
- an **awareness tenure** records an agent being in a processing *mode* over an interval.

Awareness mode (`modeOnlineInference` / `modeOfflineReplay` / `modeTraining`) is a *valuable facet* of
a deployment or execution — but it cannot *be* either, because it carries none of the deployment's
binding structure. So the model-serving slice gives each an explicit identity and uses the awareness
tenure as a facet, not a substitute. The terms are few, and each carries a genuinely distinct identity
criterion rather than collapsing them for a smaller count.

## The model artifact and the ModelCard split

The model artifact is a `gmeow:Distribution` of a `gmeow:SoftwareProduct` (software slice five-facet),
content-digested. It is tempting to say the artifact is "described by a `ModelCard`", but `gmeow:describesModel`
is `owl:FunctionalProperty` ranging over `gmeow:SoftwareAgent` — so a `Distribution` as its target
would be inferred a `SoftwareAgent`. The relations split:

```turtle
gmeow:ModelArtifact a logic:SubKind , owl:Class ; rdfs:subClassOf gmeow:Distribution .

gmeow:describesModelArtifact a owl:ObjectProperty ;
    rdfs:domain gmeow:ModelCard ; rdfs:range gmeow:ModelArtifact ;
    skos:definition "Relates a model card to the artifact it documents — architecture, version,
        training cutoff, parameter count, context window where that is an artifact property." .

gmeow:describesModelService a owl:ObjectProperty ;
    rdfs:domain gmeow:ModelCard ; rdfs:range gmeow:ModelDeployment ;
    skos:definition "Relates a model card to the served deployment it documents — provider, endpoint,
        rate limits, geographic placement, where those are deployment properties." .
```

`gmeow:describesModel` (functional, → `SoftwareAgent`) keeps its existing meaning for the acting agent;
it is no longer asked to carry the artifact. Provider, training lineage, context limits, endpoint
limits, and sampling parameters are each assigned to the entity that actually bears them.

## Deployment, execution, and the awareness facet

```turtle
gmeow:ModelDeployment
    a logic:Kind , owl:Class ;
    rdfs:subClassOf logic:Relator ;
    skos:definition "A served, callable realization of a model artifact — the relator binding an
        artifact, a service identity, a host/runtime, an endpoint, a configuration, an operator, a
        policy, and a serving interval. Distinct from the artifact (what runs) and from any one
        execution (an occurrent). Its serving mode over an interval is recorded as a facet by a
        gmeow:AwarenessTenure(modeOnlineInference), not by the deployment being a tenure." .

gmeow:RuntimeExecution
    a logic:Event , owl:Class ;
    rdfs:subClassOf gmeow:Activity ;
    skos:definition "A particular running of a deployment — an occurrent process within which
        gmeow:ModelInvocation events occur. Distinct from the deployment (an endurant relator) and the
        invocation (a single call)." .

gmeow:deploymentArtifact a owl:ObjectProperty ; rdfs:range gmeow:ModelArtifact .
gmeow:deploymentService  a owl:ObjectProperty ; rdfs:range gmeow:SoftwareAgent .  # the acting service agent
gmeow:deploymentHost     a owl:ObjectProperty ; rdfs:range gmeow:Entity .
gmeow:deploymentEndpoint a owl:ObjectProperty .
gmeow:executionOfDeployment a owl:ObjectProperty ; rdfs:range gmeow:ModelDeployment .
```

The serving mode is recorded by an `AwarenessTenure` whose `gmeow:awarenessSubject` is the deployment's
**service `SoftwareAgent`** (`gmeow:deploymentService`) — because `awarenessSubject` ranges over
`gmeow:Agent`, it cannot attach directly to a `ModelDeployment` (a relator) or a `RuntimeExecution` (an
event):

```turtle
ex:opusServing a gmeow:AwarenessTenure ;
    gmeow:awarenessSubject ex:opusServiceAgent ;     # the SoftwareAgent, not the deployment relator
    gmeow:awarenessMode gmeow:modeOnlineInference ;
    gmeow:duringInterval ex:servingWindow .
ex:opusDeployment gmeow:deploymentService ex:opusServiceAgent .
```

A single model **call** remains `gmeow:ModelInvocation` (reused); a **tool call** remains
`gmeow:ToolCall` (reused). The de-conflation chain is explicit end to end, with each join named:

```turtle
ex:output     gmeow:wasGeneratedBy   ex:invocation-7 .
ex:invocation-7 gmeow:invocationInExecution ex:execution-3 .       # invocation → execution
ex:execution-3  gmeow:executionOfDeployment ex:opusDeployment .    # execution → deployment
ex:opusDeployment gmeow:deploymentArtifact   ex:opus-artifact .    # deployment → artifact
ex:invocation-7 gmeow:subEventOf       ex:session-7 .              # invocation → session
ex:session-7    gmeow:sessionSubjectStage ex:lillithStage-opus50 . # session → subject stage
ex:session-7    gmeow:sessionConfiguration ex:config-A .           # session → active configuration
```

> output `wasGeneratedBy` a `ModelInvocation`, `invocationInExecution` a `RuntimeExecution`
> `executionOfDeployment` a `ModelDeployment` `deploymentArtifact` a `ModelArtifact`; the invocation is
> `subEventOf` an `AgentSession`, which carries `sessionSubjectStage` and `sessionConfiguration`.

## Session and episode — an event aggregate

Typing `AgentSession` as a relator-and-situation would reproduce the foundational collision (a relator
cannot subclass a situation). A session is more naturally an **event aggregate** — an
`Activity` whose sub-events are invocations, tool calls, messages, retrievals, and episodes — using
GMEOW's existing event mereology:

```turtle
gmeow:AgentSession
    a logic:Event , owl:Class ;
    rdfs:subClassOf gmeow:Activity ;
    skos:definition "A bounded interaction context for a subject — an Activity aggregating the
        gmeow:ModelInvocation, gmeow:ToolCall, message, and retrieval events of one interaction via
        gmeow:subEventOf, over a session interval. Unblocks the agentic slice's deferred trajectory
        aggregate. Internal order is a logic:Path resolved in the solver (Principle 12), never a
        nextInvocation chain in triples." .

# an episode is a sub-aggregate:
ex:episode-3 a gmeow:Activity ;
    gmeow:hasEventType gmeow:eventTypeAgentEpisode ;
    gmeow:subEventOf ex:session-7 .
```

Invocations and tool calls relate to the session via `gmeow:subEventOf`; episodes are sub-aggregates
of the same kind. Ordering is `atTime` + `temporally-succeeds`, resolved by the solver — no asserted
`nextInvocation` edge.

## The migration boundary

The competency question *which claims, memories, and intentions crossed a migration boundary?* is
**not** answered by seeing the same claim before and after the transition — it may have been
regenerated independently. Migration content rides explicit evidence. Note: `gmeow:TransferManifest`
is a **core** term (the transition-content record, declared beside the transition event in
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md), since migration applies to every inhabitation profile,
not only AI); it is shown here because the AI memory package is its primary consumer.

```turtle
gmeow:TransferManifest
    a logic:Kind , owl:Class ;
    rdfs:subClassOf gmeow:InformationObject ;
    skos:definition "The record of what was transferred across an inhabitation transition. To avoid
        doubling the memory graph at every migration, the manifest references the transferred state at
        a COARSE grain — a gmeow:MemoryView individual or a named-graph checkpoint URI — rather than a
        per-item edge for every carried claim. Only items actually MODIFIED during the transition
        receive an explicit gmeow:wasDerivedFrom edge to their pre-transition form. A claim 'crossed'
        the boundary iff it is within the manifest's referenced view (carried unchanged) or its
        post-transition form wasDerivedFrom its pre-transition form (carried modified); mere recurrence
        is not crossing." .

ex:migration-7 gmeow:hasTransferManifest ex:manifest-7 .
ex:manifest-7 gmeow:transferredView ex:viewCheckpoint-before .   # coarse: the whole carried view
ex:memory-2200-after gmeow:wasDerivedFrom ex:memory-2200 .        # per-item ONLY for modified items
```

The coarse-by-default design keeps migration storage proportional to *changes*, not to the whole
carried memory. Because memory revision is supersession (`gmeow:displayable false`),
never deletion (Principle 10), the pre-migration belief state stays queryable. Note the lifecycle
correction: *ending* the prior tenure (closing its `gmeow:duringInterval`, recorded via
`gmeow:tenureEndedBy`) is an ontic fact and does **not** by itself set `displayable false` — and a
tenure is a situation, so `gmeow:hasDestructionEvent` (domain `Entity`) is never applied to it.

## Upper-projections: the flat shortcut (Principle 12)

The de-conflation buys correctness at the cost of **property-path length**: tracing an output to its
durable subject is an 8–10 hop traversal across nested, time-scoped situations (output → invocation →
execution → deployment → artifact; and invocation → session → configuration → tenure → subject). On a
standard SPARQL engine, evaluating nested time-interval overlaps over that path will throttle
real-time agent-memory retrieval.

The remedy is the solver boundary (Principle 12), exactly as the project applies it to standpoint and
geo computation: the deep situation-nesting is the **canonical audit form**, and the runtime projects
**flat, materialized upper-projections** for the hot paths, computed by Datalog rules at the projection
layer and drift-gated like every other generated artifact (Principle 7):

```turtle
# materialized shortcut (generated, not authored): output → durable lineage, in one hop
ex:output gmeow:generatedForSubject ex:lillithLineage .
ex:output gmeow:generatedUnderConfiguration ex:config-A .
```

`gmeow:generatedForSubject` and `gmeow:generatedUnderConfiguration` are **computed projections** of the
multi-hop path, never authored facts; the canonical nested situations remain the source of truth for
deep cryptographic and lineage audits. Real-time recall reads the flat shortcut; an audit walks the
full path. This keeps the memory engine fast without collapsing the identity criteria the nesting
exists to protect.

## Capability use versus delegation (CQ5)

The competency question *was a tool call made through a passive capability or delegated to another
agent?* is resolved **without minting a generic wrapper class** (a `CapabilityExercise` would become an
operational catch-all). It reuses the provenance "used" pattern on the invocation/execution activity:

- **Passive capability** (an internal function, library, or code path): a `gmeow:usedCapability` edge
  from the `gmeow:ModelInvocation` / `gmeow:RuntimeExecution` to the `gmeow:ActionSchema` (or code
  function) that was exercised. No new agent, no `ToolCall`.
- **Delegated capability** (an external service): a first-class `gmeow:ToolCall` whose `gmeow:usedTool`
  points to a distinct `gmeow:SoftwareAgent` (the agentic slice, unchanged).

The discriminator is therefore structural and already mostly present: a `usedCapability → ActionSchema`
edge is passive; a `ToolCall → usedTool → SoftwareAgent` is delegated. (An internally-run script that
warrants its own process gets its own `gmeow:RuntimeExecution`; whether it does is a modeling choice
the producer makes, not a forced consequence.)

## The standpoint priority rule (operational)

The neutrality gate lets competing `gmeow:DigitalSubjectTenure` and continuity assessments coexist —
correct for esoteric and theoretical standpoints, but the MCP store/recall/revise triad needs a
*deterministic* answer to "whose vantage authorizes this memory mutation?" If a deployment agent claims
*"I am the continuous subject Lillith"* (vantage A) while the host asserts *"cold-started instance, no
verified memory signatures"* (vantage B), an undirected runtime forks.

The `agent-runtime` profile therefore binds an **operational standpoint priority rule**: a
deterministic evaluation order over `gmeow:vantage` values that governs *memory-mutation authorization*
(not ontological truth — the graph still records both claims, co-equal, Principle 9). The rule is a
runtime policy (e.g. a verified COSE-signed subject claim outranks an unsigned self-assertion for
*write* authorization), declared in the `agent-runtime` profile and enforced by the MCP runtime, never an axiom that
privileges one standpoint in the canonical graph. This separates *what the graph holds* (all vantages,
co-equal) from *what the runtime is permitted to do* (a deterministic, signed-vantage-priority policy).

## Cross-vendor continuity

When a subject's model lineage forks across providers, "same subject across vendors" is carried at two
layers that are deliberately not collapsed:

1. **Ontological (contestable).** A `gmeow:IdentityContinuityAssessment`
   ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)) between the two providers' subject stages —
   vantage-relative, evidence-grounded, never `owl:sameAs`. A vendor's "same model" verdict and a
   user's "different subject" verdict coexist.
2. **Cryptographic (verifiable).** A COSE signature on the GTS `ai-package` provides verifiable
   continuity of the *memory artifact* across vendors, whatever the contestable subject verdict says.

## Scope and seams

This document specifies the `extensions/model-serving` slice and the `agent-runtime` profile. The
general relation it instantiates is [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md); the subject it
serves is [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the non-AI cases (spiritual, fictional,
legal) are [`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md); the competency questions it must answer
are
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md).
