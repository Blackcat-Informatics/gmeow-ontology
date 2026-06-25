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

## Placement (Principle 16): one minimal core, two profiles

The first draft placed everything in one `slices/core/inhabitation` slice and called its use of
`gmeow:Persona` (norms), `gmeow:SoftwareProduct` (software), and `gmeow:ToolCall` (agentic) "by
reference, not import." The review correctly rejected that: **a range axiom targeting `gmeow:Persona`
is a semantic dependency regardless of whether `owl:imports` is written.** A core slice cannot depend
on extension terms, hidden or not. The packaging is corrected into a minimal core and two profiles:

```text
core/inhabitation              the minimal subject–host–tenure–continuity vocabulary.
                               Depends only on core (kernel, entities, coreference, temporal,
                               lifecycle, standpoint, observations). NO Persona, NO ToolCall,
                               NO SoftwareProduct references. This is the part that is core
                               "by commitment" (Principle 16): the durable digital subject and
                               contested continuity are the questions an AI faces about itself.

profile/inhabitation-expression   Persona and Embodiment integration (configurationPersona,
                                  EmbodimentCarrierRole, EmbodimentAssignment). Composes core
                                  with the norms extension.

profile/inhabitation-ai           model / deployment / runtime / session / tools / memory
                                  (ModelArtifact, ModelDeployment, RuntimeExecution, AgentSession,
                                  the ModelCard split, TransferManifest). Composes core with the
                                  ai, awareness, software, and agentic surfaces.
```

The minimal core has no extension dependency. The profiles are where Persona, ToolCall, and
SoftwareProduct are legitimately depended on — composition, not laundering. If the dependency-DAG
policy cannot yet *represent* a profile that composes a core slice with extensions, that is a gap in
the packaging architecture to raise explicitly (a `profile/` tier, or extension profiles), not a
reason to hide the dependency inside a core slice.

> **Open for the authority:** whether GMEOW grows an explicit `profile/` tier for compositions like
> these, or whether the AI and expression material lives in extension slices that depend on
> `core/inhabitation`. Either represents the dependency honestly; the first draft's "by reference"
> framing did not.

## The decisions, after the foundational review

Nine decisions were taken in the first round; the foundational review
([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)) reopened five of them. The current resolutions:

| # | Decision | Resolution (post-review) |
|---|---|---|
| 1 | Generality | **Held.** Domain-general; AI-runtime is one profile, spiritual / fictional / legal are siblings — but the non-AI cases are profile mappings *with documented differences*, not a correctness proof. |
| 2 | DigitalSubject typing | **Revised.** A `logic:RoleMixin` (spans Person/SoftwareAgent — a non-sortal), with a `DigitalSubjectTenure`; not a plain `Role` (a single-Kind sortal) and not `Category` (which is rigid). Self-assertion supports the status, never entails it. |
| 3 | Placement & packaging | **Revised.** Minimal `core/inhabitation` + `profile/inhabitation-expression` + `profile/inhabitation-ai`; extension terms (Persona, ToolCall, SoftwareProduct) live in the profiles, not hidden in core. |
| 4 | Cagle-Persona clash | **Held.** Fold into `IdentityFacet`/`NameUsage`; `gmeow:Persona` (norms relator) untouched. |
| 5 | Inhabitation shape | **Revised.** A **situation/tenure** (`InhabitationTenure ⊑ TimeScopedRelation`) plus `InhabitationConfiguration` for the time-scoped facets — not a relator subclassing a situation (which is unsatisfiable). |
| 6 | Manifestation ↔ WEMI | **Revised.** Documentation parallel only; **no** SSSOM term mappings (the spine crosses foundational categories WEMI's Kinds do not). |
| 7 | Transition & control & Holon | **Revised.** Migration as `eventTypeInhabitationTransition` (lifecycle value) + `TransferManifest`; ending ≠ suppression; control is a `ControlAssessment` (not deception); `gmeow:Holon` deferred (foundation kernel #704). |
| 8 | Memory-view | **Held.** Derived query (P12); promote to a signed `MemoryView` only when attested. |
| 9 | Continuity | **Revised.** `SubjectStage`/`SubjectLineage` + `IdentityContinuityAssessment` (a single stable node would assert sameness); cross-vendor adds a COSE signature. Never `owl:sameAs`. |

Additional review corrections folded in: locus split into kind + derived tenancy; `Embodiment` split
into carrier role + assignment; contested inhabitation as an unasserted `InhabitationClaim ⊑ Observation`;
competency statuses downgraded.

## Eventual slice anatomy

Authored from this design set, after it ratifies:

```text
slices/core/inhabitation/          # MINIMAL CORE — no extension dependencies
├── manifest.ttl        # tierCore; sliceDependsOn = kernel, entities, coreference, temporal,
│                       # lifecycle, standpoint, observations; sliceConsumer = ai-package / MCP (P15)
├── module.ttl          # DigitalSubject (RoleMixin), DigitalSubjectTenure (Situation),
│                       # Inhabitant/InhabitedSystem (RoleMixin), InhabitationTenure +
│                       # InhabitationConfiguration (Situations), InhabitationClaim (Observation),
│                       # SubjectStage/SubjectLineage, IdentityContinuityAssessment + ControlAssessment
│                       # (Observations), inhabitationLocusKind (values), eventTypeInhabitationTransition
│                       # + portalFrom/portalTo, TransferManifest; per-branch role-filler properties
├── docs.md, design/ (this set), examples/, tests/structural.ttl
├── shapes.ttl          # SHACL: neutrality gate (no base-graph inhabitation triple for a claim),
│                       # constant-configuration invariant, interval-carries-frame (P11),
│                       # no-gufo-inheresIn, no-owl:sameAs-on-subjects, no-primaryInhabitant

profile/inhabitation-expression/   # composes core + norms
└── module.ttl          # EmbodimentCarrierRole (RoleMixin), EmbodimentAssignment (Situation),
                        # configurationPersona integration with gmeow:Persona

profile/inhabitation-ai/           # composes core + ai/awareness/software/agentic
└── module.ttl          # ModelArtifact (⊑ Distribution), ModelDeployment (Relator),
                        # RuntimeExecution (⊑ Activity), AgentSession (⊑ Activity),
                        # describesModelArtifact/describesModelService; reuse ModelInvocation/ToolCall
```

The structural tests pin the corrected stereotypes: `DigitalSubject` is a `logic:RoleMixin` (not a
Role, not a Kind); `InhabitationTenure`/`Configuration` are `logic:Situation ⊑ TimeScopedRelation`
(not relators); no `owl:sameAs` on subjects; and the neutrality SHACL shape proves no contested
inhabitation sits in the asserted base graph.

### Registration (the manual wiring)

Adding a core slice requires the known hand-edits beyond the directory: the root `owl:imports` in
`ontology/gmeow.ttl`, the self-contained-slice count and entry in `metadata/gmeow-self.ttl`, and a
`CITATION.cff`. These land with the `module.ttl`, not with this design set.

## Open items for the authority (post-ratification)

The competency gaps from [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md) that remain open:
partial-migration policy (a migration manifest — likely solver/projection, not TBox); the
derive-vs-promote boundary for `MemoryView`; and whether cross-vendor identity ever needs a third,
stronger continuity assertion beyond the two layers. Each is deferred under Principle 15 until a
consumer demands it; none blocks the slice.

## Scope and seams

This document is the configuration and decision ledger. The consumer's competency requirements are
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md); the external citations the slice will register
are [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md).
