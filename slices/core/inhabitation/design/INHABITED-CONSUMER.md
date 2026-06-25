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

## Placement (Principle 16): `slices/core/inhabitation`

The slice is **core**, not an extension. Two arguments converge:

1. **The dependency rule forces it.** Principle 16's dependency DAG bars extension → extension
   dependencies — it is *why* the norms slice carries both rubrics and personas in one slice
   (`extensions/norms/manifest.ttl`: *"the P16 DAG rule bars extension→extension dependencies"*). A
   peer `extensions/inhabited-systems` would need both `agentic` **and** `norms` (both extensions),
   which is structurally illegal. Core placement resolves it: core may depend on core, and every reuse
   anchor this slice needs is core.
2. **Core by commitment.** Principle 16 places identity (names, gender, language) and deception in
   core *"by commitment, not minimalism,"* because they are *"the questions an AI system will face
   about its users and, in time, about itself."* A durable digital subject of its own existence is the
   most direct such question. Placing inhabitation in core means every consumer meets the durable
   digital subject, suppression-not-erasure of inhabitations, and continuity-as-contested-claim as
   first-class citizens — not as an ideology pack they can decline. The domain-general framing (it is
   foundational and cross-cutting, like the WEMI spine it aligns to) reinforces this.

### Dependencies (all core — the DAG is satisfiable)

`kernel`, `entities`, `ai`, `awareness`, `organization`, `expertise`, `teleology`, `coreference`,
`temporal`, `lifecycle`, `standpoint`, `deception`, `imagination`, `mentation`, `creative-works`,
`agreements`. The `agentic` slice (an extension) is consumed **by reference** — as documented doctrine
that this slice is its deferred consumer — not by `owl:imports`, so no extension → core dependency is
introduced.

## The settled decisions

These nine decisions were resolved with the ontology authority; the rest of the design set implements
them verbatim. They are the design contract.

| # | Decision | Resolution |
|---|---|---|
| 1 | Generality | **Domain-general.** AI-runtime is one profile; spiritual / fictional / legal are siblings. |
| 2 | DigitalSubject typing | **Anti-rigid `logic:Role`** an Agent plays; never a rigid Kind (avoids the disjoint-partition collision; matches P9's acquired-capacity framing). |
| 3 | Placement & name | **`slices/core/inhabitation`** (core, domain-neutral). "Inhabited systems" is the AI profile's framing, not the slice name. |
| 4 | Cagle-Persona clash | **Fold into `IdentityFacet`/`NameUsage`.** `gmeow:Persona` (norms relator) is untouched; no new "Persona" term. |
| 5 | Inhabitation shape | **Lean spine** carrying subject + host + interval + locus directly, referencing persona / embodiment / deployment / memory-view. |
| 6 | Manifestation ↔ WEMI | **Align by reference** (SSSOM); the subject spine keeps its own terms (agents are not creative works). |
| 7 | Transition & Holon | **Supersession chain + a first-class `gmeow:Portal` event**; `gmeow:Holon` deferred. |
| 8 | Memory-view | **Derived query** (P12); promote to first-class `gmeow:MemoryView` only when signed/attested. |
| 9 | Cross-vendor continuity | **Both layers** — `counterpartOf` (contestable) + COSE signature on the `ai-package` (verifiable). |

## Eventual slice anatomy

Authored from this design set, after it ratifies:

```text
slices/core/inhabitation/
├── manifest.ttl        # gmeow:sliceTier gmeow:tierCore; sliceDependsOn (the core list above);
│                       # sliceConsumer naming the ai-package / MCP triad (P15)
├── module.ttl          # DigitalSubject (Role), Inhabitation (Relator), Inhabitant/InhabitedSystem
│                       # (Roles), Embodiment (Relator), Portal (Event), inhabitationLocus (values),
│                       # conditional AgentSession; per-branch bearer + connector properties
├── docs.md             # the slice's human-facing design narrative (points back at design/)
├── design/             # this set (INHABITED*.md)
├── mappings/
│   └── equivalences.ttl   # SSSOM bridges: WEMI alignment; Cagle/email/verdict by-reference
├── shapes.ttl          # SHACL: the neutrality gate (no inhabitation claim in universalStandpoint),
│                       # interval-carries-frame (P11), no-gufo-inheresIn, no-primaryInhabitant
├── examples/           # the 9-fixture conformance corpus (INHABITED-COMPETENCY.md)
└── tests/
    └── structural.ttl  # MUST/MUST-NOT assertions: DigitalSubject is logic:Role not Kind;
                        # Inhabitation ⊑ TimeScopedRelation; no owl:sameAs on subjects
```

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
