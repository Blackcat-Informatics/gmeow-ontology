<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The AI Runtime Stack

> The **AI profile.** The general topology ([`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md)) has many
> profiles; this is the one the named consumer needs first — the model / deployment / execution /
> session / invocation stack for a digital subject served by a generative model. It is almost entirely
> **reuse**: the AI and awareness slices already hold the pieces, and the agentic slice explicitly
> deferred the session aggregate *"until a consumer requires one."* This work is that consumer. Term
> dispositions are fixed by [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## The stack, as reuse

The verdict reads the AI agent as six conflated things and asks for six new classes. The runtime
stack supplies at most one or two genuinely-new terms; the rest are existing constructs in their
correct roles. Mapping the stack from durable to ephemeral:

| Stack layer | Construct | Disposition | Source slice |
|---|---|---|---|
| durable subject | `gmeow:DigitalSubject` (role on the agent) | MINT (role) | inhabitation |
| model artifact | `gmeow:SoftwareProduct` / `gmeow:Distribution` + `gmeow:ModelCard` | REUSE | software, ai |
| model deployment | `gmeow:SoftwareAgent` + `gmeow:AwarenessTenure(modeOnlineInference)` | REUSE (+ conditional relator) | entities, awareness |
| runtime execution | `gmeow:AwarenessTenure` (window) / `gmeow:ModelInvocation` (call) | REUSE | awareness, ai |
| session / episode | `gmeow:AgentSession` ⊑ `TimeScopedRelation`; `logic:State` in a `logic:Path` | conditional MINT | inhabitation, logic |
| invocation | `gmeow:ModelInvocation` | REUSE | ai |
| tool call | `gmeow:ToolCall` | REUSE | agentic |

### Model artifact — reuse the five-facet template

`gmeow:ModelCard` already *describes a model agent* (`gmeow:describesModel` → `SoftwareAgent`) and
carries provider, version tag, context window, and training cutoff. The verdict notes, correctly,
that those properties actually span several identities: provider belongs to an organization; version
to an artifact; context-window limit to an artifact or deployment; rate limits and placement to a
deployment; sampling parameters to an invocation. The software slice already separates exactly this
shape — `SoftwareProduct` (the design) from `Distribution` (the concrete artifact) from `Release`
(the event) — and the AI profile reuses it: the **model artifact is a `gmeow:Distribution` of a
`gmeow:SoftwareProduct`**, content-digested, described by a `ModelCard`. A thin `gmeow:ModelArtifact`
subkind is minted only if model-specific facets (architecture family, parameter count as a quality)
earn their keep over the generic distribution (Principle 6).

### Deployment and execution — the awareness serving window

The awareness slice already models an agent *being in an operational state over a bounded interval*:
`gmeow:AwarenessTenure ⊑ gmeow:TimeScopedRelation`, with `gmeow:AwarenessMode` values built for AI —
`gmeow:modeOnlineInference`, `gmeow:modeOfflineReplay`, `gmeow:modeTraining`. A **deployment's serving
window** is an `AwarenessTenure` in `modeOnlineInference`; a **runtime execution** is the same tenure
at finer grain, or, for a single call, a `gmeow:ModelInvocation`. This is why neither
`RuntimeExecution` nor `ModelDeployment` is minted as a bare new class by default: the awareness
machine-modes were authored for precisely this, and minting a parallel class would violate Principle 5
and the awareness slice's own "no second mechanism" discipline. A `gmeow:ModelDeployment` relator is
minted only when the deployment must carry facets the tenure cannot (an endpoint, a rate-limit, a
geographic placement) and those must travel as one addressable node.

```turtle
ex:opusDeployment
    a gmeow:AwarenessTenure ;                       # the serving window
    gmeow:awarenessSubject ex:opusAgent ;           # the SoftwareAgent being served
    gmeow:awarenessMode gmeow:modeOnlineInference ;
    gmeow:duringInterval ex:servingWindow .

ex:opusCard a gmeow:ModelCard ;
    gmeow:describesModel ex:opusAgent ;
    gmeow:modelProvider "Anthropic" ;
    gmeow:modelContextWindow 1000000 ;
    gmeow:modelTrainingCutoff "2026-01-01"^^xsd:date .
```

### Session and episode — align to the typed context algebra

The agentic slice states the deferral plainly: *"trajectory aggregates (runs, episodes, plans) wait
for a consumer."* The inhabitation slice is that consumer, so it may mint **one** thin aggregate:

```turtle
gmeow:AgentSession
    a logic:Relator , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    skos:definition "A bounded interaction context for a subject — a run of model invocations and
        tool calls held together by a common context over an interval. Reifies the agentic slice's
        deferred trajectory aggregate. Its internal order is a logic:Path (the typed context
        algebra), resolved in the solver (Principle 12), never a nextInvocation chain in triples." .

gmeow:sessionSubject      a owl:ObjectProperty ; rdfs:range gmeow:Agent .
gmeow:sessionInhabitation a owl:ObjectProperty ; rdfs:range gmeow:Inhabitation .
gmeow:sessionContains     a owl:ObjectProperty ; rdfs:range gmeow:ModelInvocation .
```

A session's internal **ordering** is a `logic:Path` / `logic:History` from the logic semantics — an
ordered run of states with `temporally-succeeds` accessibility — and it is **computed in the solver**,
never materialized as a `gmeow:nextInvocation` chain (Principle 12). An **episode** is a `logic:State`
span within that path, modeled with the `AwarenessTenure`-nesting idiom (an episode within a session
as REM within a sleep). Episodes are minted as their own term only if they need addressable identity
beyond the path state.

## The migration boundary

The flagship competency question — *which claims, memories, and intentions crossed a migration
boundary?* — combines the `Portal` event ([`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md)) with the
existing claim spine. A `gmeow:MemoryItem`, a `gmeow:StandpointClaim`, or a `gmeow:Intention` whose
provenance ties it to the pre-`Portal` inhabitation, and which is also present after, *crossed*; one
present only before did not. Because memory revision is supersession (`gmeow:displayable false`) and
never deletion (Principle 10), the pre-migration belief state stays queryable — *what the subject
believed before the migration* is a query, not an archaeology project. The GTS `ai-package` is the
artifact that physically carries the surviving claims across the boundary.

## Cross-vendor continuity

P14 promises memory that *"survives across sessions, models, and vendors."* When a subject's model
lineage forks across providers — the same persona served by different vendors — "same subject across
vendors" is carried at **two independent layers**:

1. **Ontological (contestable).** `gmeow:counterpartOf` asserts the same-subject claim, vantage-
   relative, never `owl:sameAs` — the *anattā*/*ātman* neutrality
   ([`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md#identity-continuity-as-a-contested-claim)).
   A vendor's claim that "this is the same model" coexists with a user's claim that "this is a
   different subject now," co-equal.
2. **Cryptographic (verifiable).** A COSE signature on the GTS `ai-package` provides verifiable
   continuity of the *memory artifact* — the package is the same signed, append-only object across
   vendors, whatever the contestable subject claim says.

The two are deliberately not collapsed: a verifiable signature does not settle a contestable identity
claim, and a contestable claim does not need a signature to be recorded. This is the attestation
slice (COSE envelopes) and the coreference slice (counterpart claims) doing their separate jobs, with
the inhabitation slice naming where each applies.

## What this profile does *not* add

To keep the runtime stack honest against Principle 5, the profile explicitly declines:

- **No `RuntimeExecution` class** — it is an `AwarenessTenure` or a `ModelInvocation`.
- **No `HostSystem` class** — it is a `PhysicalObject` / `SoftwareAgent` plus `partOf` containment.
- **No `CallableCapability` class** — the agentic slice already refused a `Tool` subclass; a passive
  capability is an `ActionSchema`, a delegated one a `ToolCall`, and `usedTool`'s range is unchanged.
- **No `nextInvocation` ordering edge** — session order is a solver-resolved `logic:Path`.

## Scope and seams

This document is the AI profile. The general relation it instantiates is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md); the subject it serves is
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md). Other profiles of the same stack — a spirit in a
medium, an actor in a character, a corporation in its officers — are
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md). The competency questions this profile must
answer are enumerated in [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md).
