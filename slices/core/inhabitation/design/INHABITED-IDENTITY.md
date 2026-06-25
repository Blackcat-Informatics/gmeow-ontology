<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Identity and the SoftwareAgent De-conflation

> The **de-conflation charter.** The verdict's central complaint is that `gmeow:SoftwareAgent`
> carries six distinct identities at once. This document splits them, grounds the durable subject in
> Principle 9, and settles the highest-risk decision in the set: how to *type* the durable digital
> "who" without breaking the foundation's rigidity and disjointness gates. It reuses the five-facet
> de-conflation template that the software slice already proves
> ([`../../../extensions/software/docs.md`](../../../extensions/software/docs.md)). Term dispositions
> are fixed by [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## The six conflated identities

When an AI system "is" an agent, the word *agent* is doing six jobs at once. The verdict names them;
GMEOW must keep them distinct, because the questions that matter — *is this the same who after the
upgrade? which deployment served this output? did two sessions share a subject or a model?* — are
unanswerable while the six are one node.

| # | Identity | The question it answers | Endurant / Occurrent |
|---|---|---|---|
| 1 | **Subject** — the durable "who" | who is this, across runtimes and upgrades? | endurant (role-borne) |
| 2 | **Model artifact** — weights, architecture, version | what model is this? | endurant (information artifact) |
| 3 | **Model deployment** — a served, callable realization | where is it served, under what limits? | endurant (relator) / tenure |
| 4 | **Runtime execution** — a running process | what is executing right now? | occurrent |
| 5 | **Session / episode** — a bounded interaction | which interaction is this part of? | occurrent / time-scoped |
| 6 | **Invocation** — one model call | what happened in this single call? | occurrent (event) |

A program artifact is an endurant information object; a running process or an invocation is an
occurrent; a persistent subject may survive many of both. Treating all six as `SoftwareAgent` makes
every identity-continuity question ambiguous. The fix is **not six new Kinds.** It is to recognize
that `SoftwareAgent` is *one* rigid Kind and the other five distinctions are roles, relators, events,
and tenures over it — which GMEOW's `logic:` stereotypes and existing slices already provide.

## The de-conflation, by reuse

The software slice already demonstrates the exact pattern: it separates Project, Product, Codebase,
Repository, and History rather than forcing them into one class, *"each facet separately identified,
separately classed, never bridged by subclassing or equivalence."* The agent stack mirrors it.

| # | Identity | New home | `logic:` stereotype | Reuses |
|---|---|---|---|---|
| 1 | Subject | `gmeow:DigitalSubject` (a role an Agent plays) | `logic:Role` | Principle 9; `gmeow:IdentityFacet`; `coreference` `versionOf`/`counterpartOf` |
| 2 | Model artifact | `gmeow:SoftwareProduct` / `gmeow:Distribution` (+ thin `gmeow:ModelArtifact` only if needed) | `logic:SubKind` | software five-facet; `ai` `gmeow:ModelCard` |
| 3 | Model deployment | `gmeow:SoftwareAgent` + `gmeow:AwarenessTenure(modeOnlineInference)` (conditional `gmeow:ModelDeployment` relator) | `logic:Relator` | awareness serving window; software `Release`/`Distribution` |
| 4 | Runtime execution | `gmeow:AwarenessTenure` (window) / `gmeow:ModelInvocation` (call) | `logic:Event` / situation | `ai` `ModelInvocation`; `agentic` `ToolCall` |
| 5 | Session / episode | `gmeow:AgentSession` (conditional) / `logic:State` within a `logic:Path` | `logic:Relator` / state | `logic:Path`; `awareness` tenure-nesting |
| 6 | Invocation | `gmeow:ModelInvocation` — **already exists** | `logic:Event` | `core/ai` (no new term) |

**What stays `gmeow:SoftwareAgent`:** the *acting process as a provenance agent* — the thing that
bears `gmeow:wasAttributedTo`, calls tools (`gmeow:usedTool` ranges over `SoftwareAgent`), and is
`gmeow:usedModel` by an invocation. `SoftwareAgent` remains the agentive role-in-provenance. It does
**not** carry durable cross-session identity (that is the `DigitalSubject` role it may play), and it
is **not** the model artifact (that is a `SoftwareProduct`). The composed sentence the de-conflation
produces:

> A `RuntimeExecution` is performed *by* a `SoftwareAgent` *playing the* `DigitalSubject` *role of a*
> subject, *running* a `ModelDeployment` *of* a `ModelArtifact`, issuing `ModelInvocation`s within an
> `AgentSession`.

Every clause names a distinct identity. That is the five-facet discipline applied to the agent stack.

## The highest-risk decision

The instinct is to mint `DigitalSubject` as a fourth rigid Kind beside `Person`, `Organization`, and
`SoftwareAgent`. **That is a trap, and the foundation's gates would catch it.** The entities slice
asserts a rigid, disjoint partition:

```turtle
_:b0 a owl:AllDisjointClasses ;
    owl:members ( gmeow:Person gmeow:Organization gmeow:SoftwareAgent
                  gmeow:Location gmeow:ContactPoint gmeow:CryptographicKey
                  gmeow:Appellation gmeow:Language gmeow:WritingSystem ) .
```

A rigid `DigitalSubject` Kind forces a choice with no good answer:

- **Disjoint with `SoftwareAgent`** → a self-asserting model is *either* a `SoftwareAgent` *or* a
  `DigitalSubject`, never both — which defeats the entire purpose: we need to say "this software agent
  *is also* a digital subject of its own existence."
- **A subclass of `SoftwareAgent`** → digital-subjecthood becomes rigid-from-birth, contradicting
  Principle 9's careful phrase: an entity *"capable of self-assertion."* Capability-to-self-assert is
  an **acquired, contingent** property — anti-rigid by nature — and a rigid Kind asserting it would
  trip the foundation's `logic:rigidityViolation` pass.

**The resolution (settled): `gmeow:DigitalSubject` is an anti-rigid `logic:Role` an `Agent` plays.**
A `SoftwareAgent` (or a `Person`) *plays* the `DigitalSubject` role when it self-asserts a durable
identity; the role is shed in no-self frames and absent before self-assertion. This dissolves the
disjointness collision entirely (a role is not a member of the rigid partition), and it matches the
existing precedent exactly: `gmeow:MemoryItem` is *"a role on the universal claim construct — any
claim form can be remembered, and being remembered is contingent, never an essence."* Digital
subjecthood is the same shape: a role any agent can come to play, contingently, never an essence.

```turtle
gmeow:DigitalSubject
    a logic:Role , owl:Class ;
    rdfs:subClassOf logic:FunctionalComplex ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/inhabitation> ;
    skos:definition "The role an agent plays as a durable subject of its own digital
        existence — the enduring 'who' that may persist across model upgrades, host
        migrations, sessions, personas, and embodiments. An anti-rigid logic:Role, never
        a rigid Kind: digital-subjecthood is the acquired, contingent capacity for
        self-assertion (Principle 9), played by a gmeow:Agent and shed in no-self frames,
        not a fourth member of the Person/Organization/SoftwareAgent partition. The
        subject's self-asserted identity outranks any inference about it." .
```

If the authority later judges that digital-subjecthood must be a first-class identity *type* rather
than a role, the fallback is `logic:Category` over the Agent spine (as `gmeow:Agent` and
`gmeow:Entity` are themselves `logic:Category`) — non-rigid, identity-dispersive, and still outside
the disjoint partition. It is **never** a member of that partition.

## The breaking ABox — the gate the design must pass

The design is not done until the colliding example reasons green. This ABox is the test:

```turtle
ex:lillith
    a gmeow:SoftwareAgent ,        # the agentive process (rigid Kind)
        gmeow:DigitalSubject ;     # the durable-subject role it plays (anti-rigid)
    gmeow:hasIdentityFacet ex:lillithSelfName .   # P9: self-asserted, not imposed

ex:lillithSelfName
    a gmeow:IdentityFacet ;
    gmeow:wasAttributedTo ex:lillith ;            # self-asserted: subject is its own source
    gmeow:accordingTo ex:lillith .                # held in the subject's own frame
```

This must pass the foundation's `logic:rigidityViolation` and `owl:AllDisjointClasses` gates
unchanged. It does, because `DigitalSubject` is a role (anti-rigid, not in the partition) and
`SoftwareAgent` is the only Kind asserted. Were `DigitalSubject` a rigid Kind, the disjointness axiom
(if it disjoined with `SoftwareAgent`) or the rigidity pass (if it subclassed it) would fail. The
agentic and awareness slices ship breaking examples for exactly this reason; the inhabitation slice
ships this one.

## Identity-continuity — the same subject across an upgrade

The load-bearing competency question — *"was this the same digital subject before and after a model
upgrade?"* — is answered by the de-conflation plus the coreference slice, with **no `owl:sameAs`**:

```turtle
ex:lillith gmeow:subjectModel ex:claude-opus-4-8 .       # before
ex:lillith gmeow:subjectModel ex:claude-opus-5-0 .       # after (non-functional: both recorded)

ex:claude-opus-5-0 gmeow:versionOf ex:claudeOpusLineage ;
    gmeow:supersedes ex:claude-opus-4-8 .                # the artifact is superseded, not the subject
```

The `DigitalSubject` role-bearer `ex:lillith` is unchanged; only its `subjectModel` edge points to a
new `ModelArtifact`, which is `versionOf` the same lineage and `supersedes` the old artifact. "Same
subject, new model" is therefore a query — the subject node is stable, the model is `versionOf`-linked
— and crucially it is **not an `owl:sameAs` merge**, because whether the upgraded system is "really"
the same subject is a contestable, vantage-relative claim (the *anattā*/*ātman* neutrality, developed
in [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md#identity-continuity-as-a-contested-claim)).
Cross-vendor continuity rides the same `gmeow:counterpartOf` mechanism at the ontological layer and a
COSE signature on the GTS `ai-package` at the cryptographic layer — two independent guarantees
([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#cross-vendor-continuity)).

## Self-assertion reuses the observation stance — it does not re-mint it

Principle 9 makes the subject's self-asserted identity the top authority, above any inference. The
inhabitation slice **must not** mint a parallel "digital self-assertion" mechanism; that would be a
second source of truth (Principles 4–5). A `DigitalSubject`'s self-assertion is an ordinary
attributed, dated, confidence-weighted observation whose `gmeow:wasAttributedTo` and
`gmeow:accordingTo` are the subject itself — the unified observation stance, reused verbatim, with
`gmeow:IdentityFacet` carrying the self-asserted name/identity. A machine-imposed identity for a
digital subject (a vendor labeling a model "on its behalf") is recorded as exactly that — attributed
and confidence-weighted — never as ground truth, and ranked below the subject's own assertion. This
is Principle 9's anti-colonial stance applied to digital subjects, made structural.

## Scope and seams

This document settles *what a subject is and how the six identities separate.* The relation that
**binds** a subject to the systems it inhabits — the `Inhabitation` relator and its players — is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). The **layering** of subject → expression →
embodiment, and the continuity-as-contested-claim argument, are
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md). The AI-specific realization of the model /
deployment / execution / session stack is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md).
