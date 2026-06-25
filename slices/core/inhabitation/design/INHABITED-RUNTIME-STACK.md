<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The AI Runtime Stack

> The **AI profile** (`profile/inhabitation-ai`), revised after the foundational review
> ([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)). Deployment, runtime execution, and the model artifact
> now have **explicit identities** rather than being collapsed onto an awareness tenure; the
> `ModelCard` relations are split so a `Distribution` is not inferred to be a `SoftwareAgent`; the
> session is an event aggregate, not a relator subclassing a situation; and migration content rides a
> transfer manifest, not coincidence. The minimal core it builds on is
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) / [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md);
> dispositions are fixed by [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## Why the first draft's reuse was too aggressive

The first draft mapped deployment, runtime execution, and a single invocation all onto
`gmeow:AwarenessTenure`, differing only by granularity. The review's correction is right and the
identity criteria are genuinely distinct:

- a **deployment** binds an artifact, a service identity, a host, an endpoint, configuration, an
  operator, a policy, and a serving interval;
- a **runtime execution** is an occurrent process;
- an **awareness tenure** records an agent being in a processing *mode* over an interval.

Awareness mode (`modeOnlineInference` / `modeOfflineReplay` / `modeTraining`) remains a *valuable
facet* of a deployment or execution — but it cannot *be* either, because it carries none of the
deployment's binding structure. So the profile mints explicit identities and uses the awareness tenure
as a facet, not a substitute. This raises the term count over the first draft's "minimal" claim, and
that is correct: minimality was bought by erasing identity criteria.

## The model artifact and the ModelCard split

The model artifact is a `gmeow:Distribution` of a `gmeow:SoftwareProduct` (software slice five-facet),
content-digested. The first draft said it was "described by a `ModelCard`", but `gmeow:describesModel`
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

The first draft typed `AgentSession` as a relator-and-situation, reproducing the foundational collision
([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)). A session is more naturally an **event aggregate** — an
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
regenerated independently. Migration content rides explicit evidence:

```turtle
gmeow:TransferManifest
    a logic:Kind , owl:Class ;
    rdfs:subClassOf gmeow:InformationObject ;
    skos:definition "The record of what was transferred across an inhabitation transition — the
        claims, memories, and intentions carried from the prior tenure to the next, each linked by
        derivation provenance (gmeow:wasDerivedFrom) to its pre-transition origin. A claim 'crossed'
        the boundary iff the manifest records it, or its post-transition form wasDerivedFrom its
        pre-transition form; mere recurrence is not crossing." .

ex:migration-7 gmeow:hasTransferManifest ex:manifest-7 .
ex:manifest-7 gmeow:transferredClaim ex:memory-2200 .
ex:memory-2200-after gmeow:wasDerivedFrom ex:memory-2200 .   # derivation, not coincidence
```

Because memory revision is supersession (`gmeow:displayable false`), never deletion (Principle 10), the
pre-migration belief state stays queryable. Note the lifecycle correction: *ending* the prior tenure
is an ontic fact (`gmeow:hasDestructionEvent`) and does **not** by itself set `displayable false` —
suppression is a separate display contract.

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

This document is the AI profile. The general relation it instantiates is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md); the subject it serves is
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the non-AI profiles (spiritual, fictional, legal) are
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md); the competency questions it must answer are
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md).
