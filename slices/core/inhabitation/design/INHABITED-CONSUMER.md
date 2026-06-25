<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Consumer, Placement, and the Settled Decisions

> The **configuration charter.** This document names the Principle 15 consumer, settles the placement
> question against the Principle 16 dependency rules, records the design decisions the rest of the set
> implements, and sketches the eventual slice anatomy. It is the bridge from the design set to the
> `module.ttl` / `manifest.ttl` / `examples/` that are authored from it.

## The consumer (Principle 15)

> **The GTS `ai-package` and the MCP store / recall / revise memory triad — grounded agent memory
> that survives across sessions, models, and vendors.**

Principle 14 makes that survival the flagship claim of the whole project. The inhabitation slice is
its **ontological backbone**: a memory package can only be said to "survive across sessions, models,
and vendors" if the ontology can name *which* session, *which* model, and *which* vendor a claim was
formed under, and can assert that the **subject persists** while those change. Without this slice,
"survives across models" is an unfalsifiable slogan; with it, "the same subject before and after the
upgrade" is a query (CQ 1), and "which deployment served this output" is provenance (CQ 6).

The agentic slice's own deferral names this work as its consumer: it deferred trajectory aggregates
*"until a consumer requires one,"* and the inhabitation runtime stack is that consumer
([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md)). This is not "modelled beautifully" in
search of a use; it is the named substrate of a shipping product.

## Placement (Principle 16): core + a thin slice + one selecting profile

Two tempting anti-patterns are rejected. The first hides extension dependencies (`gmeow:Persona`,
`gmeow:SoftwareProduct`, `gmeow:ToolCall`) inside a "core" slice and calls them "by reference" — but a
range axiom is a real dependency regardless of `owl:imports`, and a core slice may not depend on an
extension. The second invents profiles to *mint* the model/deployment/runtime terms — using "profile"
as a dependency escape hatch, which is the same laundering relabeled.

> **The governing principle (the authority's): a profile mints nothing.** A profile is a *pure
> selection* — a dependency-closed subset of core and extension terms forming an internally consistent
> sub-ontology for a cohesive purpose and audience. The moment a design asks *"where do this profile's
> minted terms live?"*, the profile is mis-designed: the answer is *nowhere* — the terms belong in core
> (if universal) or a thin extension (if a coherent domain), and the profile selects them. The earlier
> "a profile may mint, very rarely" allowance asymptotes to zero in any well-designed profile.

The disciplined shape is therefore a minimal core, one thin standalone slice, and one real profile that
purely selects:

```text
core/inhabitation              the inhabitation topology — subject·host·tenure·continuity, the
                               transition + TransferManifest, ControlAssessment, InhabitationClaim,
                               the OPEN configurationFacet, AND EmbodimentCarrierRole /
                               EmbodimentAssignment (every inhabitation has a surface — AI, medium,
                               actor, officer — so Embodiment is universal → core). Depends only on
                               core; no extension term, no Persona, no ToolCall.

extensions/model-serving       a THIN, STANDALONE vocabulary, reusable by anyone modelling LLM ops,
   (the thin slice)            nothing to do with inhabitation: gmeow:ModelArtifact (a Kind aligned to
                               software's Distribution BY REFERENCE, not subclassing it),
                               gmeow:ModelDeployment, gmeow:RuntimeExecution, gmeow:AgentSession, the
                               ModelCard split. Depends only on core (ai, entities, awareness, events)
                               → a DAG-legal extension (extension → core).

profile/agent-runtime          ONE real profile, a PURE SELECTION for the AI-agent / MCP-developer
                               audience: core/inhabitation + extensions/model-serving + memory +
                               claims + agentic + awareness, as one coherent, dependency-closed
                               sub-ontology. Mints NOTHING. Composition rides the open
                               configurationFacet at the instance level (an InhabitationConfiguration
                               points at a ModelDeployment via configurationFacet — no typed glue
                               property to mint). The standpoint priority rule it needs is a RUNTIME
                               policy, not an ontology term.
```

Every term lives in core or a thin extension; the profile is a selection that mints nothing.
Composition needs no glue term because `gmeow:configurationFacet` is open-range: an instance writes
`ex:config gmeow:configurationFacet ex:someDeployment` directly. There are therefore **zero
inhabitation-specific minted profile terms** — the test the principle above demands.

### Why a profile, and why only this one

A profile must be pulled into existence by a real audience (Principle 15), never pushed into existence
by a dependency problem. `agent-runtime` qualifies: AI-agent and MCP developers are a concrete
constituency who want one coherent bundle — a durable subject inhabiting a served model, with memory
and claims — loaded as a unit. Narrower "profiles" would **not** qualify: an `inhabitation-expression`
profile carrying a single binding property has no audience and no coherence, and an `inhabitation-ai`
profile that is merely "load two slices together" is motivated by where terms could legally live, not
by an audience. If another genuine bundle is wanted later (a narrative-inhabitation profile, say), it
is proposed then, on its own audience.

### Tier registry

The `agent-runtime` profile rides the project's planned profile tier: a `gmeow:tierProfile` individual
in `slices/vocabulary.ttl` (which today declares only `tierCore` and `tierExtension`) plus the DAG rule
*a profile may depend on core and extensions; nothing depends on a profile* (profiles are leaves, so
the graph stays acyclic). The **minimal `core/inhabitation` and `extensions/model-serving` do not
depend on the profile tier** and may be authored immediately; the `agent-runtime` profile is registered
when the tier ships.

## The decisions

The settled design decisions the rest of this set implements:

| # | Decision | Resolution |
|---|---|---|
| 1 | Generality | Domain-general; the AI runtime is one profile, spiritual / fictional / legal are siblings — modeled as profile mappings *with documented differences*, not a claim that they are one thing. |
| 2 | DigitalSubject typing | A `logic:RoleMixin` (spans Person/SoftwareAgent — a non-sortal), borne over a `DigitalSubjectTenure`; not a plain `Role` (a single-Kind sortal) and not `Category` (which is rigid). Self-assertion supports the status, never entails it. |
| 3 | Placement & packaging | Minimal `core/inhabitation` (incl. the universal `Embodiment`) + a thin standalone `extensions/model-serving` + one selecting `profile/agent-runtime` that mints nothing. |
| 4 | Cagle-Persona clash | Fold into `IdentityFacet`/`NameUsage`; `gmeow:Persona` (norms relator) untouched. |
| 5 | Inhabitation shape | A **situation/tenure** (`InhabitationTenure ⊑ TimeScopedRelation`) plus `InhabitationConfiguration` for the time-scoped facets — not a relator subclassing a situation. |
| 6 | Manifestation ↔ WEMI | Documentation parallel only; **no** SSSOM term mappings (the spine crosses foundational categories WEMI's Kinds do not). |
| 7 | Transition & control & Holon | Migration as `eventTypeInhabitationTransition` (lifecycle value) + `TransferManifest`; ending ≠ suppression; control is a `ControlAssessment` (not deception); `gmeow:Holon` deferred (foundation kernel #704). |
| 8 | Memory-view | Derived query (P12); promote to a signed `MemoryView` only when attested. |
| 9 | Continuity | `SubjectStage`/`SubjectLineage` + `IdentityContinuityAssessment` (a single stable node would assert sameness); cross-vendor adds a COSE signature. Never `owl:sameAs`. |

Other decisions: locus split into kind + derived tenancy; `Embodiment` split into carrier role +
assignment; contested inhabitation as an unasserted `InhabitationClaim ⊑ StandpointClaim`; role
classification by native `logic:` rule; competency statuses kept honest.

## Eventual slice anatomy

Authored from this design set, after it ratifies:

```text
slices/core/inhabitation/          # MINIMAL CORE — no extension dependencies
├── manifest.ttl        # tierCore; sliceDependsOn = kernel, entities, coreference, temporal,
│                       # lifecycle, standpoint, observations; sliceConsumer = ai-package / MCP (P15)
├── module.ttl          # DigitalSubject (RoleMixin), DigitalSubjectTenure (Situation),
│                       # Inhabitant/InhabitedSystem (RoleMixin), InhabitationTenure +
│                       # InhabitationConfiguration (Situations), InhabitationClaim (⊑ StandpointClaim),
│                       # InhabitationDescription (⊑ Proposition), SubjectStage/SubjectLineage,
│                       # IdentityContinuityAssessment + ControlAssessment (⊑ Observation),
│                       # EmbodimentCarrierRole (RoleMixin) + EmbodimentAssignment (Situation) [universal],
│                       # inhabitationLocusKind (values), eventTypeInhabitationTransition +
│                       # portalFrom/portalTo, TransferManifest, OPEN configurationFacet; role-filler props
├── docs.md, design/ (this set), examples/, tests/structural.ttl
├── shapes.ttl          # SHACL: neutrality gate (no base-graph inhabitation triple for a claim),
│                       # constant-configuration invariant, interval-carries-frame (P11),
│                       # no-gufo-inheresIn, no-owl:sameAs-on-subjects, no-primaryInhabitant

extensions/model-serving/          # THIN, STANDALONE; depends only on core (ai/entities/awareness/events)
└── module.ttl          # ModelArtifact (Kind, aligned-by-reference to software Distribution),
                        # ModelDeployment (Relator), RuntimeExecution (⊑ Activity), AgentSession
                        # (⊑ Activity), describesModelArtifact/describesModelService; reuse
                        # ModelInvocation/ToolCall at the instance level. Reusable beyond inhabitation.

profile/agent-runtime/             # ONE profile — a PURE SELECTION, mints nothing
└── manifest.ttl        # tierProfile; selects core/inhabitation + extensions/model-serving + memory
                        # + claims + agentic + awareness for the AI-agent / MCP audience. NO module.ttl
                        # (no terms to mint); composition rides the open configurationFacet.
```

The structural tests pin the corrected stereotypes: `DigitalSubject` is a `logic:RoleMixin` (not a
Role, not a Kind); `InhabitationTenure`/`Configuration` are `logic:Situation ⊑ TimeScopedRelation`
(not relators); the assessments are `logic:SubKind ⊑ Observation`; no `owl:sameAs` on subjects; the
neutrality SHACL shape proves no contested inhabitation sits in the asserted base graph; and the
`agent-runtime` profile manifest declares **no minted terms**.

### Registration (the manual wiring)

Adding a core slice requires the known hand-edits beyond the directory: the root `owl:imports` in
`ontology/gmeow.ttl`, the self-contained-slice count and entry in `metadata/gmeow-self.ttl`, and a
`CITATION.cff`. These land with the `module.ttl`, not with this design set.

## Open items for the authority

The competency gaps from [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md) that remain open:
partial-migration policy (a migration manifest — likely solver/projection, not TBox); the
derive-vs-promote boundary for `MemoryView`; and whether cross-vendor identity ever needs a third,
stronger continuity assertion beyond the two layers. Each is deferred under Principle 15 until a
consumer demands it; none blocks the slice.

## Scope and seams

This document is the configuration and decision ledger. The consumer's competency requirements are
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md); the external citations the slice will register
are [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md).
